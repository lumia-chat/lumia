//! Drives a [`Session`] over a real WebSocket: frames in, effects carried
//! out against the store and the API (docs/protocol.md section 6).

use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use lumia_core::proto::{AccountEvent, Command, Event};
use lumia_core::store::{LogKind, OutboxEntry, Store, StoredEvent, StoredMessage};
use lumia_core::sync::{decode_frame, encode_frame, Cursors, Effect, Notice, Session};
use lumia_core::wire;
use prost::Message as _;
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::Message as WsMessage;

use crate::api::Api;
use crate::ClientError;

/// What the application receives from the gateway loop.
#[derive(Debug, Clone)]
pub enum GatewayEvent {
    /// Channel events applied to the store, in order.
    Applied(Vec<Event>),
    /// Account events applied to the store, in order.
    AccountApplied(Vec<AccountEvent>),
    /// A notice from the session.
    Notice(Notice),
    /// The loop ended.
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

/// Runs one connection until it closes. Reconnect is the caller's loop.
///
/// # Errors
/// On a transport failure before the handshake completes.
pub async fn run<S: Store + Send + 'static>(
    api: Api,
    mut store: S,
    client_name: &str,
    device_id: Vec<u8>,
    initial_channels: Vec<Vec<u8>>,
    mut commands: mpsc::Receiver<GatewayCommand>,
    events: mpsc::Sender<GatewayEvent>,
) -> Result<S, ClientError> {
    let cursors = Cursors {
        channels: store.cursors(LogKind::Channel)?,
        account: store
            .cursors(LogKind::Account)?
            .first()
            .map_or(0, |(_, s)| *s),
        generation: store.generation()?,
    };
    let mut session = Session::new(client_name, device_id, cursors);
    for id in initial_channels {
        session.subscribe(id, 0);
    }

    let (ws, _) = tokio_tungstenite::connect_async(api.gateway_url()?).await?;
    let (mut sink, mut stream) = ws.split();

    let mut pending = vec![session.hello()];
    let mut heartbeat = tokio::time::interval(Duration::from_secs(30));
    heartbeat.tick().await;
    let mut nonce = 0u64;
    let mut account_id: Vec<u8> = Vec::new();

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
                        let _ = events.send(GatewayEvent::Closed(format!("{frame:?}"))).await;
                        return Ok(store);
                    }
                    Some(Ok(_)) => {}
                    Some(Err(e)) => {
                        let _ = events.send(GatewayEvent::Closed(e.to_string())).await;
                        return Ok(store);
                    }
                    None => {
                        let _ = events.send(GatewayEvent::Closed("eof".into())).await;
                        return Ok(store);
                    }
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
                        pending.extend(session.subscribe(channel_id, cursor));
                    }
                    Some(GatewayCommand::Foreground) => pending.push(session.high_water_mark()),
                    None => return Ok(store),
                }
            }
            _ = heartbeat.tick() => {
                nonce += 1;
                pending.push(session.heartbeat(nonce));
            }
        }
    }
}
