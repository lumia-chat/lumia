//! Drives a [`Session`] over a real WebSocket: frames in, effects carried
//! out against the store and the API (docs/protocol.md section 6).
//!
//! [`run`] is one connection. [`run_forever`] wraps it with reconnect and
//! exponential backoff, re-subscribing from the store's cursors each time,
//! and stops only when the application drops its command sender or the
//! server says not to come back (`SESSION_REVOKED`, `DEVICE_REVOKED`,
//! `PROTOCOL_ERROR`, or a refresh that the instance refuses).

use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use lumia_core::proto::{AccountEvent, Command, DisconnectReason, Event};
use lumia_core::store::{LogKind, OutboxEntry, Store, StoredEvent, StoredMessage};
use lumia_core::sync::{decode_frame, encode_frame, Cursors, Effect, Notice, Session};
use lumia_core::wire;
use prost::Message as _;
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::{self, Message as WsMessage};

use crate::api::Api;
use crate::ClientError;

/// First delay after a drop; doubles per attempt up to [`MAX_BACKOFF`].
const BASE_BACKOFF: Duration = Duration::from_secs(1);
/// The longest wait between attempts.
const MAX_BACKOFF: Duration = Duration::from_secs(30);

/// What the application receives from the gateway loop.
#[derive(Debug, Clone)]
pub enum GatewayEvent {
    /// Channel events applied to the store, in order.
    Applied(Vec<Event>),
    /// Account events applied to the store, in order.
    AccountApplied(Vec<AccountEvent>),
    /// A notice from the session.
    Notice(Notice),
    /// The channel took the snapshot path (delivery spec 7) up to `seq`.
    SnapshotTaken {
        /// The channel log.
        channel_id: Vec<u8>,
        /// The snapshot's sequence number.
        seq: u64,
    },
    /// The connection dropped; the loop tries again after `delay`.
    Reconnecting {
        /// Consecutive failed attempts so far, from 1.
        attempt: u32,
        /// How long the loop waits before this attempt.
        delay: Duration,
        /// Why the last connection ended.
        reason: String,
    },
    /// The loop ended for good.
    Closed(String),
}

/// Instructions to the gateway loop from the application.
#[derive(Debug, Clone)]
pub enum GatewayCommand {
    /// Send a command (also written to the outbox until acked).
    Send(Command),
    /// Subscribe to a channel from the store's cursor.
    Subscribe(Vec<u8>),
    /// The app came to the foreground: ask for every head.
    Foreground,
}

/// How one connection ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Exit {
    /// The application dropped its command sender (or stopped listening).
    Done,
    /// The socket closed.
    Closed {
        /// The transport's or server's reason.
        reason: String,
        /// The server's `Disconnect`, when it sent one.
        disconnect: Option<DisconnectReason>,
        /// The server's requested wait before reconnecting.
        reconnect_after: Duration,
        /// Whether the handshake completed on this connection.
        welcomed: bool,
    },
}

/// Runs connections until the application is done or the server says stop.
/// Returns the store so the caller keeps it.
pub async fn run_forever<S: Store>(
    api: Api,
    mut store: S,
    client_name: &str,
    device_id: Vec<u8>,
    mut channels: Vec<Vec<u8>>,
    mut commands: mpsc::Receiver<GatewayCommand>,
    events: mpsc::Sender<GatewayEvent>,
) -> S {
    let mut attempt = 0u32;
    loop {
        let outcome = run(
            &api,
            &mut store,
            client_name,
            &device_id,
            &mut channels,
            &mut commands,
            &events,
        )
        .await;
        let (reason, wait) = match outcome {
            Ok(Exit::Done) => return store,
            Ok(Exit::Closed {
                reason,
                disconnect: Some(d),
                ..
            }) if !retryable(d) => {
                let _ = events.send(GatewayEvent::Closed(reason)).await;
                return store;
            }
            Ok(Exit::Closed {
                reason,
                reconnect_after,
                welcomed,
                ..
            }) => {
                if welcomed {
                    attempt = 0;
                }
                (reason, reconnect_after)
            }
            Err(ClientError::Api { code, message, .. })
                if code == "UNAUTHENTICATED" || code == "TOKEN_EXPIRED" =>
            {
                // The socket's token was refused: rotate before trying again,
                // and stop if the instance refuses the refresh too.
                if let Err(e) = api.refresh_now().await {
                    let _ = events.send(GatewayEvent::Closed(e.to_string())).await;
                    return store;
                }
                (format!("{code}: {message}"), Duration::ZERO)
            }
            Err(e) => (e.to_string(), Duration::ZERO),
        };
        attempt += 1;
        let delay = backoff(attempt).max(wait);
        if events
            .send(GatewayEvent::Reconnecting {
                attempt,
                delay,
                reason,
            })
            .await
            .is_err()
        {
            return store;
        }
        tokio::time::sleep(delay).await;
        if commands.is_closed() {
            return store;
        }
    }
}

/// Whether a server-initiated disconnect invites a reconnect.
#[must_use]
pub fn retryable(reason: DisconnectReason) -> bool {
    !matches!(
        reason,
        DisconnectReason::DisconnectSessionRevoked
            | DisconnectReason::DisconnectDeviceRevoked
            | DisconnectReason::DisconnectProtocolError
    )
}

/// Exponential backoff with up to a quarter of jitter, capped.
#[must_use]
pub fn backoff(attempt: u32) -> Duration {
    let exp = attempt.saturating_sub(1).min(16);
    let base = BASE_BACKOFF.saturating_mul(1u32 << exp).min(MAX_BACKOFF);
    let jitter_nanos = u64::from(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.subsec_nanos())
            .unwrap_or(0),
    );
    let quarter = base.as_nanos() / 4;
    let jitter = u64::try_from(quarter).map_or(0, |q| if q == 0 { 0 } else { jitter_nanos % q });
    base + Duration::from_nanos(jitter)
}

/// Runs one connection until it closes. Subscribes to `channels` from the
/// store's cursors; a `Subscribe` command adds to that list so a reconnect
/// picks it up.
///
/// # Errors
/// On a transport failure before the handshake, an API failure while
/// carrying out an effect, or a store failure.
pub async fn run<S: Store>(
    api: &Api,
    store: &mut S,
    client_name: &str,
    device_id: &[u8],
    channels: &mut Vec<Vec<u8>>,
    commands: &mut mpsc::Receiver<GatewayCommand>,
    events: &mpsc::Sender<GatewayEvent>,
) -> Result<Exit, ClientError> {
    let stored = store.cursors(LogKind::Channel)?;
    let cursors = Cursors {
        channels: stored.clone(),
        account: store
            .cursors(LogKind::Account)?
            .first()
            .map_or(0, |(_, s)| *s),
        generation: store.generation()?,
    };
    let mut session = Session::new(client_name, device_id.to_vec(), cursors);
    for id in channels.iter() {
        let cursor = stored.iter().find(|(c, _)| c == id).map_or(0, |(_, s)| *s);
        session.subscribe(id.clone(), cursor);
    }

    let (ws, _) = match tokio_tungstenite::connect_async(api.gateway_url()?).await {
        Ok(ok) => ok,
        Err(tungstenite::Error::Http(response)) if response.status() == 401 => {
            return Err(ClientError::Api {
                code: "UNAUTHENTICATED".into(),
                message: "gateway refused the access token".into(),
                status: 401,
                details: std::collections::HashMap::new(),
            });
        }
        Err(e) => return Err(e.into()),
    };
    let (mut sink, mut stream) = ws.split();

    let mut pending = vec![session.hello()];
    let mut heartbeat = tokio::time::interval(Duration::from_secs(30));
    heartbeat.tick().await;
    let mut nonce = 0u64;
    let mut account_id: Vec<u8> = Vec::new();
    let mut welcomed = false;
    let mut disconnect: Option<(DisconnectReason, Duration)> = None;

    let closed = |reason: String, disconnect: &Option<(DisconnectReason, Duration)>, welcomed| {
        Exit::Closed {
            reason,
            disconnect: disconnect.map(|(d, _)| d),
            reconnect_after: disconnect.map_or(Duration::ZERO, |(_, w)| w),
            welcomed,
        }
    };

    loop {
        // Carry out effects, which may produce more effects.
        while let Some(effect) = pending.pop() {
            match effect {
                Effect::Send(frame) => {
                    sink.send(WsMessage::Binary(encode_frame(&frame).into()))
                        .await?;
                }
                Effect::Fetch {
                    channel_id,
                    after,
                    before,
                } => {
                    let id = wire::display(&channel_id);
                    let page = api.channel_events(&id, after, before).await?;
                    pending.extend(session.fetched(&channel_id, page.events));
                }
                Effect::FetchAccount { after } => {
                    let page = api.account_events(after).await?;
                    pending.extend(session.account_fetched(page.events));
                }
                Effect::Snapshot { channel_id } => {
                    let id = wire::display(&channel_id);
                    let snap = api.channel_snapshot(&id).await?;
                    let messages: Vec<StoredMessage> = snap
                        .messages
                        .into_iter()
                        .map(|m| StoredMessage {
                            message_id: m.message_id,
                            channel_id: m.channel_id,
                            seq: m.seq,
                            author_account_id: m.author_account_id,
                            body: m.body,
                            created_at_micros: wire::micros(m.created_at.as_ref()).unwrap_or(0),
                            deleted: m.deleted_at.is_some(),
                        })
                        .collect();
                    store.apply_snapshot(&channel_id, snap.seq, &messages)?;
                    let _ = events
                        .send(GatewayEvent::SnapshotTaken {
                            channel_id: channel_id.clone(),
                            seq: snap.seq,
                        })
                        .await;
                    pending.extend(session.snapshot_applied(&channel_id, snap.seq));
                }
                Effect::AccountSnapshot => {
                    let snap = api.account_snapshot().await?;
                    store.apply(LogKind::Account, &account_id, &[])?;
                    pending.extend(session.account_snapshot_applied(snap.seq));
                }
                Effect::Apply {
                    channel_id,
                    events: applied,
                } => {
                    let stored: Vec<StoredEvent> = applied
                        .iter()
                        .map(|e| StoredEvent {
                            seq: e.seq,
                            bytes: e.encode_to_vec(),
                        })
                        .collect();
                    store.apply(LogKind::Channel, &channel_id, &stored)?;
                    let _ = events.send(GatewayEvent::Applied(applied)).await;
                }
                Effect::ApplyAccount { events: applied } => {
                    let stored: Vec<StoredEvent> = applied
                        .iter()
                        .map(|e| StoredEvent {
                            seq: e.seq,
                            bytes: e.encode_to_vec(),
                        })
                        .collect();
                    store.apply(LogKind::Account, &account_id, &stored)?;
                    let _ = events.send(GatewayEvent::AccountApplied(applied)).await;
                }
                Effect::DiscardAll { generation } => {
                    store.discard_all()?;
                    store.set_generation(generation)?;
                }
                Effect::Notify(notice) => {
                    match &notice {
                        Notice::Welcomed(welcome) => {
                            welcomed = true;
                            account_id.clone_from(&welcome.account_id);
                            store.set_generation(welcome.recovery_generation)?;
                            // Retry anything still in the outbox, verbatim.
                            for entry in store.outbox()? {
                                if let Ok(command) = Command::decode(entry.bytes.as_slice()) {
                                    pending.push(session.command(command));
                                }
                            }
                        }
                        Notice::Acked { command_id, .. } => store.outbox_remove(command_id)?,
                        Notice::Disconnected(d) => {
                            disconnect = Some((
                                DisconnectReason::try_from(d.reason).unwrap_or_default(),
                                Duration::from_secs(u64::from(d.reconnect_after_seconds)),
                            ));
                        }
                        _ => {}
                    }
                    let _ = events.send(GatewayEvent::Notice(notice)).await;
                }
            }
        }

        tokio::select! {
            message = stream.next() => {
                match message {
                    Some(Ok(WsMessage::Binary(bytes))) => {
                        let frame = decode_frame(&bytes)?;
                        pending.extend(session.handle(frame));
                    }
                    Some(Ok(WsMessage::Close(frame))) => {
                        return Ok(closed(format!("{frame:?}"), &disconnect, welcomed));
                    }
                    Some(Ok(_)) => {}
                    Some(Err(e)) => return Ok(closed(e.to_string(), &disconnect, welcomed)),
                    None => return Ok(closed("eof".into(), &disconnect, welcomed)),
                }
            }
            command = commands.recv() => {
                match command {
                    Some(GatewayCommand::Send(command)) => {
                        store.outbox_push(&OutboxEntry {
                            command_id: command.command_id.clone(),
                            channel_id: command.channel_id.clone(),
                            bytes: command.encode_to_vec(),
                        })?;
                        pending.push(session.command(command));
                    }
                    Some(GatewayCommand::Subscribe(channel_id)) => {
                        let cursor = store
                            .cursors(LogKind::Channel)?
                            .into_iter()
                            .find(|(id, _)| *id == channel_id)
                            .map_or(0, |(_, s)| s);
                        if !channels.contains(&channel_id) {
                            channels.push(channel_id.clone());
                        }
                        pending.extend(session.subscribe(channel_id, cursor));
                    }
                    Some(GatewayCommand::Foreground) => pending.push(session.high_water_mark()),
                    None => return Ok(Exit::Done),
                }
            }
            _ = heartbeat.tick() => {
                nonce += 1;
                pending.push(session.heartbeat(nonce));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backoff_doubles_and_caps() {
        let a = backoff(1);
        let b = backoff(2);
        let c = backoff(3);
        assert!(a >= Duration::from_secs(1) && a < Duration::from_millis(1250));
        assert!(b >= Duration::from_secs(2) && b < Duration::from_millis(2500));
        assert!(c >= Duration::from_secs(4) && c < Duration::from_millis(5000));
        let far = backoff(40);
        assert!(far >= MAX_BACKOFF && far <= MAX_BACKOFF + MAX_BACKOFF / 4);
    }

    #[test]
    fn only_revocations_and_protocol_errors_stop_the_loop() {
        assert!(retryable(DisconnectReason::DisconnectServerShutdown));
        assert!(retryable(DisconnectReason::DisconnectHeartbeatTimeout));
        assert!(retryable(DisconnectReason::DisconnectSlowConsumer));
        assert!(retryable(DisconnectReason::DisconnectRateLimited));
        assert!(!retryable(DisconnectReason::DisconnectSessionRevoked));
        assert!(!retryable(DisconnectReason::DisconnectDeviceRevoked));
        assert!(!retryable(DisconnectReason::DisconnectProtocolError));
    }
}
