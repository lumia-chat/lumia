//! Drives a [`Session`] over a real WebSocket: frames in, effects carried
//! out against the store and the API (docs/protocol.md section 6).
//!
//! [`run`] is one connection. [`run_forever`] wraps it with reconnect and
//! exponential backoff, re-subscribing from the store's cursors each time,
//! and stops only when the application drops its command sender or the
//! server says not to come back (`SESSION_REVOKED`, `DEVICE_REVOKED`,
//! `PROTOCOL_ERROR`, or a refresh that the instance refuses).
//!
//! Effects are carried out in the order the session returns them: a
//! fetched page yields one `Apply` per contiguous run of events and the
//! store refuses them out of order, and `Welcome` asks for the account log
//! before the channels (protocol section 6.3).

use std::collections::VecDeque;
use std::time::Duration;

use futures_channel::mpsc;
use futures_util::{FutureExt, SinkExt, StreamExt};
use lumia_core::proto::{AccountEvent, Command, DisconnectReason, Event};
use lumia_core::store::{LogKind, OutboxEntry, Store, StoredEvent, StoredMessage};
use lumia_core::sync::{decode_frame, encode_frame, Cursors, Effect, Notice, Session};
use lumia_core::wire;
use prost::Message as _;

use crate::api::Api;
use crate::time::sleep;
use crate::transport::{connect, Frame};
use crate::ClientError;

/// Heartbeat period until `Welcome` states one; the server disconnects
/// after missing two.
const DEFAULT_HEARTBEAT: Duration = Duration::from_secs(30);

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
    mut commands: mpsc::UnboundedReceiver<GatewayCommand>,
    events: mpsc::UnboundedSender<GatewayEvent>,
) -> S {
    let mut attempt = 0u32;
    let mut stash: Option<GatewayCommand> = None;
    loop {
        let outcome = run(
            &api,
            &mut store,
            client_name,
            &device_id,
            &mut channels,
            &mut commands,
            stash.take(),
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
                let _ = events.unbounded_send(GatewayEvent::Closed(reason));
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
            Err(ClientError::NotSignedIn) => {
                let _ = events.unbounded_send(GatewayEvent::Closed("not signed in".into()));
                return store;
            }
            Err(ClientError::Api { code, message, .. })
                if code == "DEVICE_REVOKED" || code == "SESSION_REVOKED" =>
            {
                let _ = events.unbounded_send(GatewayEvent::Closed(format!("{code}: {message}")));
                return store;
            }
            Err(ClientError::Api { code, message, .. })
                if code == "UNAUTHENTICATED" || code == "TOKEN_EXPIRED" =>
            {
                // The socket's token was refused. The upgrade answers the
                // same status for an expired token and a revoked session
                // (see `transport::connect`), so rotate before trying again
                // and stop only if the instance refuses the refresh too:
                // that answer is what tells a revocation apart.
                if let Err(e) = api.refresh_now().await {
                    let _ = events.unbounded_send(GatewayEvent::Closed(e.to_string()));
                    return store;
                }
                (format!("{code}: {message}"), Duration::ZERO)
            }
            Err(ClientError::Transport(reason)) if cfg!(target_arch = "wasm32") => {
                // The browser hides the status of a refused upgrade: rotate
                // the token when one is on hand, then try again.
                if api.tokens().refresh.is_some() {
                    let _ = api.refresh_now().await;
                }
                (reason, Duration::ZERO)
            }
            Err(e) => (e.to_string(), Duration::ZERO),
        };
        attempt += 1;
        let delay = backoff(attempt).max(wait);
        if events
            .unbounded_send(GatewayEvent::Reconnecting {
                attempt,
                delay,
                reason,
            })
            .is_err()
        {
            return store;
        }
        sleep(delay).await;
        // A connection that never reached the command loop cannot see the
        // application drop its sender: look before trying again, keeping
        // any command that arrived meanwhile for the next connection.
        // `try_next` is deprecated in favour of `try_recv`, whose error
        // does not distinguish "empty" from "closed" in this version.
        #[allow(deprecated)]
        match commands.try_next() {
            Ok(None) => return store,
            Ok(Some(command)) => stash = Some(command),
            Err(_) => {}
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
    let jitter_nanos =
        u64::try_from(crate::time::now_micros().rem_euclid(1_000_000) * 1000).unwrap_or(0);
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
// The arguments are the loop's whole state; a struct would only rename them.
#[allow(clippy::too_many_arguments)]
pub async fn run<S: Store>(
    api: &Api,
    store: &mut S,
    client_name: &str,
    device_id: &[u8],
    channels: &mut Vec<Vec<u8>>,
    commands: &mut mpsc::UnboundedReceiver<GatewayCommand>,
    stash: Option<GatewayCommand>,
    events: &mpsc::UnboundedSender<GatewayEvent>,
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

    let socket = connect(&api.gateway_url(), &api.gateway_subprotocols()?).await?;
    let (mut sink, mut stream) = socket.split();

    // A queue, not a stack: effects run in the order the session returns them.
    let mut pending: VecDeque<Effect> = VecDeque::from([session.hello()]);
    if let Some(command) = stash {
        on_command(command, store, &mut session, channels, &mut pending)?;
    }
    let mut heartbeat_period = DEFAULT_HEARTBEAT;
    let mut heartbeat = Box::pin(sleep(heartbeat_period).fuse());
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
        // Carry out effects in order; each may queue more behind it.
        while let Some(effect) = pending.pop_front() {
            match effect {
                Effect::Send(frame) => {
                    sink.send(encode_frame(&frame)).await?;
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
                    let _ = events.unbounded_send(GatewayEvent::SnapshotTaken {
                        channel_id: channel_id.clone(),
                        seq: snap.seq,
                    });
                    pending.extend(session.snapshot_applied(&channel_id, snap.seq));
                }
                Effect::AccountSnapshot => {
                    let snap = api.account_snapshot().await?;
                    // The store's account cursor must move with the
                    // session's, or the next live account event is a gap
                    // the store refuses.
                    store.apply_account_snapshot(&account_id, snap.seq)?;
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
                    let _ = events.unbounded_send(GatewayEvent::Applied(applied));
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
                    let _ = events.unbounded_send(GatewayEvent::AccountApplied(applied));
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
                            if welcome.heartbeat_interval_seconds > 0 {
                                heartbeat_period = Duration::from_secs(u64::from(
                                    welcome.heartbeat_interval_seconds,
                                ));
                                heartbeat = Box::pin(sleep(heartbeat_period).fuse());
                            }
                            // Retry anything still in the outbox, verbatim.
                            for entry in store.outbox()? {
                                if let Ok(command) = Command::decode(entry.bytes.as_slice()) {
                                    pending.push_back(session.command(command));
                                }
                            }
                        }
                        Notice::Acked { command_id, .. } => store.outbox_remove(command_id)?,
                        // A command gets exactly one Ack or Error (protocol
                        // 6.5); a refused one is not retried.
                        Notice::Error(error) if !error.command_id.is_empty() => {
                            store.outbox_remove(&error.command_id)?;
                        }
                        Notice::Disconnected(d) => {
                            disconnect = Some((
                                DisconnectReason::try_from(d.reason).unwrap_or_default(),
                                Duration::from_secs(u64::from(d.reconnect_after_seconds)),
                            ));
                        }
                        _ => {}
                    }
                    let _ = events.unbounded_send(GatewayEvent::Notice(notice));
                }
            }
        }

        futures_util::select! {
            message = stream.next().fuse() => {
                match message {
                    Some(Ok(Frame::Binary(bytes))) => {
                        let frame = decode_frame(&bytes)?;
                        pending.extend(session.handle(frame));
                    }
                    Some(Ok(Frame::Close(reason))) => {
                        return Ok(closed(reason, &disconnect, welcomed));
                    }
                    Some(Ok(Frame::Other)) => {}
                    Some(Err(e)) => return Ok(closed(e.to_string(), &disconnect, welcomed)),
                    None => return Ok(closed("eof".into(), &disconnect, welcomed)),
                }
            }
            command = commands.next().fuse() => {
                match command {
                    Some(command) => on_command(command, store, &mut session, channels, &mut pending)?,
                    None => return Ok(Exit::Done),
                }
            }
            () = heartbeat => {
                nonce += 1;
                pending.push_back(session.heartbeat(nonce));
                heartbeat = Box::pin(sleep(heartbeat_period).fuse());
            }
        }
    }
}

/// Carries out one application command: the outbox entry and the frame,
/// a subscription from the store's cursor, or a head check.
fn on_command<S: Store>(
    command: GatewayCommand,
    store: &mut S,
    session: &mut Session,
    channels: &mut Vec<Vec<u8>>,
    pending: &mut VecDeque<Effect>,
) -> Result<(), ClientError> {
    match command {
        GatewayCommand::Send(command) => {
            store.outbox_push(&OutboxEntry {
                command_id: command.command_id.clone(),
                channel_id: command.channel_id.clone(),
                bytes: command.encode_to_vec(),
            })?;
            pending.push_back(session.command(command));
        }
        GatewayCommand::Subscribe(channel_id) => {
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
        GatewayCommand::Foreground => pending.push_back(session.high_water_mark()),
    }
    Ok(())
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

    /// The loop must carry out a batch of effects in the order the session
    /// returned them: a fetched page is several `Apply` effects that the
    /// store accepts only in sequence, and `Welcome` resumes the account
    /// log before subscribing channels.
    #[test]
    fn effects_are_queued_in_the_order_the_session_returns_them() {
        use lumia_core::proto::{
            client_frame, server_frame, AccountResumeResult, ChannelSubscribeResult, ClientFrame,
            ServerFrame, SubscribeResult, SubscribeStatus, Welcome,
        };
        let chan = vec![1u8; 16];
        let mut session = Session::new(
            "t",
            vec![9; 16],
            Cursors {
                channels: vec![(chan.clone(), 1)],
                account: 0,
                generation: Some(1),
            },
        );
        let mut pending: VecDeque<Effect> = VecDeque::new();
        pending.extend(session.handle(ServerFrame {
            frame: Some(server_frame::Frame::Welcome(Welcome {
                recovery_generation: 1,
                ..Default::default()
            })),
        }));
        let sends: Vec<&client_frame::Frame> = pending
            .iter()
            .filter_map(|e| match e {
                Effect::Send(ClientFrame { frame: Some(f) }) => Some(f),
                _ => None,
            })
            .collect();
        assert!(matches!(sends[0], client_frame::Frame::ResumeAccount(_)));
        assert!(matches!(sends[1], client_frame::Frame::Subscribe(_)));
        pending.clear();

        session.handle(ServerFrame {
            frame: Some(server_frame::Frame::SubscribeResult(SubscribeResult {
                results: vec![ChannelSubscribeResult {
                    channel_id: chan.clone(),
                    status: SubscribeStatus::Resumed as i32,
                    ..Default::default()
                }],
            })),
        });
        session.handle(ServerFrame {
            frame: Some(server_frame::Frame::AccountResumeResult(
                AccountResumeResult {
                    status: SubscribeStatus::Resumed as i32,
                    ..Default::default()
                },
            )),
        });
        let event = |seq| Event {
            channel_id: chan.clone(),
            seq,
            ..Default::default()
        };
        pending.extend(session.fetched(&chan, vec![event(2), event(3), event(4)]));
        let mut seqs = Vec::new();
        while let Some(Effect::Apply { events, .. }) = pending.pop_front() {
            seqs.extend(events.iter().map(|e| e.seq));
        }
        assert_eq!(seqs, vec![2, 3, 4]);
    }
}
