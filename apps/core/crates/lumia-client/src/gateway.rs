//! Drives a [`Session`] over a real WebSocket: frames in, effects carried
//! out against the store and the API (docs/protocol.md section 6).
//!
//! [`run`] is one connection. [`run_forever`] wraps it with reconnect and
//! exponential backoff, re-subscribing from the store's cursors each time,
//! and stops only when the application drops its command sender or the
//! server says not to come back (`SESSION_REVOKED`, `DEVICE_REVOKED`,
//! `PROTOCOL_ERROR`, or a refresh that the instance refuses). A revoked
//! session the client has already left for a new one (a sign-in over a held
//! session revokes the held one) is not a reason to stop: the loop comes
//! back at once on the new one.
//!
//! The account log's cursor is the account's. A device another account
//! signed in on before holds that account's cursor; the loop resumes from
//! the cursor of the account the store last followed (`ACCOUNT_META`), and
//! a welcome that names another account starts that one's log afresh.
//!
//! Effects are carried out in the order the session returns them: a
//! fetched page yields one `Apply` per contiguous run of events and the
//! store refuses them out of order, and `Welcome` asks for the account log
//! before the channels (protocol section 6.3).

use std::collections::{HashSet, VecDeque};
use std::time::Duration;

use futures_channel::mpsc;
use futures_util::{FutureExt, SinkExt, StreamExt};
use lumia_core::proto::{AccountEvent, Command, DisconnectReason, Event};
use lumia_core::store::{LogKind, OutboxEntry, Store, StoredEvent, StoredMessage};
use lumia_core::sync::{decode_frame, encode_frame, Cursors, Effect, Notice, Session};
use lumia_core::wire;
use prost::Message as _;

use crate::api::{Api, REQUEST_TIMEOUT};
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

/// Where the store records the account whose account log it follows, as
/// hex: written at every welcome.
pub(crate) const ACCOUNT_META: &str = "account_id";

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
    /// The account log took the snapshot path up to `seq` (delivery spec
    /// 6: the cursor was more than the replay cap behind). The account
    /// events before it were never replayed, so whatever the application
    /// keeps from them (other members' roles and titles among it) is as
    /// old as its last connection and is to be read again.
    AccountSnapshotTaken {
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
    /// Send a command (also written to the outbox until acked). It is in
    /// the outbox once the loop has read this, which while the instance
    /// cannot be reached is not at once: a sender that must not lose it to
    /// the process ending writes the outbox itself and says `Outbox`.
    Send(Box<Command>),
    /// The outbox holds commands their sender wrote there through its own
    /// connection to the store: send those this connection has not sent.
    /// Before the welcome there is nothing to do, since the welcome sends
    /// the whole outbox.
    Outbox,
    /// Subscribe to a channel from the store's cursor.
    Subscribe(Vec<u8>),
    /// The app came to the foreground: ask for every head.
    Foreground,
    /// This client's status and the servers it wants presence for.
    Presence(lumia_core::proto::PresenceUpdate),
    /// This person is typing in a channel. Sent only once the channel is
    /// subscribed, and dropped otherwise.
    Typing(Vec<u8>),
}

/// How the loop ended, before `Closed` is said.
enum Told {
    /// The application stopped it.
    Done,
    /// The instance said not to come back, for this reason.
    Closed(String),
    /// The instance said this device was revoked.
    Revoked(String),
}

/// Ends the loop on an answer that says not to come back: revoked when the
/// answer is that this device was, closed otherwise.
fn end<S>(store: S, e: &ClientError) -> (S, Told) {
    let reason = e.to_string();
    if matches!(e, ClientError::Api { code, .. } if code == "DEVICE_REVOKED") {
        return (store, Told::Revoked(reason));
    }
    (store, Told::Closed(reason))
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
    store: S,
    client_name: &str,
    device_id: Vec<u8>,
    channels: Vec<Vec<u8>>,
    commands: mpsc::UnboundedReceiver<GatewayCommand>,
    events: mpsc::UnboundedSender<GatewayEvent>,
) -> S {
    let (store, revoked) = run_until_end(
        api,
        store,
        client_name,
        device_id,
        channels,
        commands,
        events.clone(),
    )
    .await;
    if let Some(reason) = revoked {
        let _ = events.unbounded_send(GatewayEvent::Closed(reason));
    }
    store
}

/// [`run_forever`], answering why it ended when the instance revoked this
/// device (`DEVICE_REVOKED`, or a `Disconnect` saying so), and then leaving
/// `Closed` to the caller: a device the identity no longer holds erases what
/// it kept (identity spec 4.4) before it says it has ended. A loop the shell
/// stopped because one of its own calls heard it first answers the same.
pub async fn run_until_end<S: Store>(
    api: Api,
    store: S,
    client_name: &str,
    device_id: Vec<u8>,
    channels: Vec<Vec<u8>>,
    commands: mpsc::UnboundedReceiver<GatewayCommand>,
    events: mpsc::UnboundedSender<GatewayEvent>,
) -> (S, Option<String>) {
    let heard = api.clone();
    let (store, told) = run_until_told(
        api,
        store,
        client_name,
        device_id,
        channels,
        commands,
        events.clone(),
    )
    .await;
    // A device revoked is told so once, however the loop ended.
    match told {
        Told::Revoked(reason) => (store, Some(reason)),
        _ if heard.device_revoked() => (store, Some("DEVICE_REVOKED".into())),
        Told::Closed(reason) => {
            let _ = events.unbounded_send(GatewayEvent::Closed(reason));
            (store, None)
        }
        Told::Done => (store, None),
    }
}

async fn run_until_told<S: Store>(
    api: Api,
    mut store: S,
    client_name: &str,
    device_id: Vec<u8>,
    mut channels: Vec<Vec<u8>>,
    mut commands: mpsc::UnboundedReceiver<GatewayCommand>,
    events: mpsc::UnboundedSender<GatewayEvent>,
) -> (S, Told) {
    let mut attempt = 0u32;
    let mut stash: Vec<GatewayCommand> = Vec::new();
    loop {
        let signed_in = api.sign_ins();
        let outcome = run(
            &api,
            &mut store,
            client_name,
            &device_id,
            &mut channels,
            &mut commands,
            &mut stash,
            &events,
        )
        .await;
        // The session this connection was made for was revoked because the
        // client signed in again over it: the new one is held already.
        let left_for_another = || api.sign_ins() != signed_in && api.tokens().access.is_some();
        let (reason, wait) = match outcome {
            Ok(Exit::Done) => return (store, Told::Done),
            Ok(Exit::Closed {
                disconnect: Some(DisconnectReason::DisconnectSessionRevoked),
                ..
            })
            | Err(ClientError::Api { .. })
                if left_for_another() =>
            {
                attempt = 0;
                continue;
            }
            Ok(Exit::Closed {
                reason,
                disconnect: Some(d),
                ..
            }) if !retryable(d) => {
                if d == DisconnectReason::DisconnectDeviceRevoked {
                    return (store, Told::Revoked(reason));
                }
                return (store, Told::Closed(reason));
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
                return (store, Told::Closed("not signed in".into()));
            }
            Err(e @ ClientError::Api { .. })
                if matches!(&e, ClientError::Api { code, .. }
                    if code == "DEVICE_REVOKED" || code == "SESSION_REVOKED") =>
            {
                return end(store, &e);
            }
            Err(ClientError::Api { code, message, .. })
                if code == "UNAUTHENTICATED" || code == "TOKEN_EXPIRED" =>
            {
                // The socket's token was refused. The upgrade answers the
                // same status for an expired token and a revoked session
                // (see `transport::connect`), so rotate before trying again
                // and stop only if the instance refuses the refresh too:
                // that answer is what tells a revocation apart. A refresh
                // that got no answer at all refuses nothing: the loop
                // waits and tries again, as for any other lost connection.
                if let Err(e @ (ClientError::Api { .. } | ClientError::NotSignedIn)) =
                    api.refresh_now().await
                {
                    return end(store, &e);
                }
                (format!("{code}: {message}"), Duration::ZERO)
            }
            Err(ClientError::Transport(reason)) if cfg!(target_arch = "wasm32") => {
                // The browser hides the status of a refused upgrade: rotate
                // the token when one is on hand, then try again. An instance
                // that refuses the rotation as well has ended the session,
                // and that is where the loop stops, as it does natively;
                // a rotation that got no answer is tried again next time.
                if api.tokens().refresh.is_some() {
                    if let Err(e @ ClientError::Api { .. }) = api.refresh_now().await {
                        return end(store, &e);
                    }
                }
                (reason, Duration::ZERO)
            }
            Err(e) => (e.to_string(), Duration::ZERO),
        };
        attempt += 1;
        // The server's wait is a floor, not the delay: a whole node's
        // clients told the same number would come back in one instant.
        let delay = backoff(attempt).max(wait + jitter(Duration::from_secs(1)));
        if events
            .unbounded_send(GatewayEvent::Reconnecting {
                attempt,
                delay,
                reason,
            })
            .is_err()
        {
            return (store, Told::Done);
        }
        // The wait watches the application: one that lets go meanwhile
        // (signing out, going to the background) ends the loop then and
        // not after the wait, and what it sends is kept, in order, for the
        // next connection that is made, however many attempts fail first.
        let mut waited = Box::pin(sleep(delay).fuse());
        loop {
            futures_util::select! {
                () = waited => break,
                command = commands.next().fuse() => match command {
                    Some(command) => hold(command, &mut store, &mut stash),
                    None => return (store, Told::Done),
                },
            }
        }
    }
}

/// Keeps what the application sent while there is no connection. A command
/// goes to the outbox now, which the next welcome sends whole: held in
/// memory it would be gone with the process, and the instance may be out
/// of reach for as long as the device is. The rest waits for the next
/// connection in the order it came.
fn hold<S: Store>(command: GatewayCommand, store: &mut S, stash: &mut Vec<GatewayCommand>) {
    match command {
        GatewayCommand::Send(command) => {
            let queued = store.outbox_push(&OutboxEntry {
                command_id: command.command_id.clone(),
                channel_id: command.channel_id.clone(),
                bytes: command.encode_to_vec(),
            });
            // A store that would not take it now is asked again when connected.
            if queued.is_err() {
                stash.push(GatewayCommand::Send(command));
            }
        }
        // Already in the outbox, and the next welcome sends that.
        GatewayCommand::Outbox => {}
        command => stash.push(command),
    }
}

/// The account whose account log the store follows, and its cursor there:
/// the account last welcomed, or for a store from before that was written
/// down, the one account it has a cursor for. A cursor of another account
/// is no cursor of this one.
fn followed_account<S: Store>(store: &S) -> Result<(Option<Vec<u8>>, u64), ClientError> {
    let cursors = store.cursors(LogKind::Account)?;
    let recorded = store
        .meta_get(ACCOUNT_META)?
        .and_then(|account| hex::decode(account).ok());
    Ok(match recorded {
        Some(account) => {
            let seq = cursors
                .iter()
                .find(|(log, _)| *log == account)
                .map_or(0, |(_, seq)| *seq);
            (Some(account), seq)
        }
        None => match cursors.as_slice() {
            [(account, seq)] => (Some(account.clone()), *seq),
            _ => (None, 0),
        },
    })
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

/// Exponential backoff, capped, drawn from the upper half of each step:
/// between half the step and the whole of it. Clients that dropped
/// together then spread over half the window rather than a quarter of
/// it, and no attempt is immediate.
#[must_use]
pub fn backoff(attempt: u32) -> Duration {
    let exp = attempt.saturating_sub(1).min(16);
    let step = BASE_BACKOFF.saturating_mul(1u32 << exp).min(MAX_BACKOFF);
    let half = step / 2;
    half + jitter(half)
}

/// A duration below `span`, from the clock's sub-second part: the
/// spread a reconnecting herd needs, not a random number anyone relies on.
fn jitter(span: Duration) -> Duration {
    let nanos = u64::try_from(crate::time::now_micros().rem_euclid(1_000_000) * 1000).unwrap_or(0);
    u64::try_from(span.as_nanos()).map_or(Duration::ZERO, |span| {
        if span == 0 {
            Duration::ZERO
        } else {
            Duration::from_nanos(nanos % span)
        }
    })
}

/// Runs one connection until it closes. Subscribes to `channels` from the
/// store's cursors; a `Subscribe` command adds to that list so a reconnect
/// picks it up. `stash` is what the application sent while there was no
/// connection: it is carried out once this one is made, and left as it is
/// when none is.
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
    stash: &mut Vec<GatewayCommand>,
    events: &mpsc::UnboundedSender<GatewayEvent>,
) -> Result<Exit, ClientError> {
    let stored = store.cursors(LogKind::Channel)?;
    let (followed, account_cursor) = followed_account(store)?;
    let cursors = Cursors {
        channels: stored.clone(),
        account: account_cursor,
        generation: store.generation()?,
    };
    let mut session = Session::new(client_name, device_id.to_vec(), cursors);
    for id in channels.iter() {
        let cursor = stored.iter().find(|(c, _)| c == id).map_or(0, |(_, s)| *s);
        session.subscribe(id.clone(), cursor);
    }

    // An upgrade nobody answers would otherwise be waited on as long as
    // TCP waits, with the application's commands, a stop among them,
    // unread meanwhile.
    let (url, subprotocols) = (api.gateway_url(), api.gateway_subprotocols()?);
    let socket = {
        let mut connecting = Box::pin(connect(&url, &subprotocols).fuse());
        let mut gave_up = Box::pin(sleep(REQUEST_TIMEOUT).fuse());
        futures_util::select! {
            socket = connecting => socket?,
            () = gave_up => {
                return Err(ClientError::Transport("the gateway did not answer".into()));
            }
        }
    };
    let (mut sink, mut stream) = socket.split();

    // A queue, not a stack: effects run in the order the session returns them.
    let mut pending: VecDeque<Effect> = VecDeque::from([session.hello()]);
    // The commands this connection has sent and not heard back about: the
    // welcome sends the whole outbox, and what is said to be in it after
    // (`GatewayCommand::Outbox`) is sent once.
    let mut sent: HashSet<Vec<u8>> = HashSet::new();
    // Taken only now that there is a connection, and one at a time: an
    // attempt that failed above, or fails here, leaves the rest for the next.
    while !stash.is_empty() {
        let command = stash.remove(0);
        on_command(
            command,
            store,
            &mut session,
            channels,
            &mut pending,
            &mut sent,
            false,
        )?;
    }
    let mut heartbeat_period = DEFAULT_HEARTBEAT;
    let mut heartbeat = Box::pin(sleep(heartbeat_period).fuse());
    let mut nonce = 0u64;
    let mut account_id: Vec<u8> = Vec::new();
    let mut welcomed = false;
    // How far the server's clock ran ahead of this one at the welcome.
    let mut server_ahead = 0i64;
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
                        .map(|m| StoredMessage::of(&m))
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
                    // The events up to it were skipped, not applied: the
                    // application is told, since nothing else will say
                    // what they would have.
                    let _ =
                        events.unbounded_send(GatewayEvent::AccountSnapshotTaken { seq: snap.seq });
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
                Effect::Notify(mut notice) => {
                    // What is reported is in this device's clock.
                    if let Notice::Typing(typing) = &mut notice {
                        let now = crate::time::now_micros();
                        let until = wire::micros(typing.until.as_ref()).unwrap_or(now);
                        typing.until =
                            Some(crate::time::ts(typing_until(until, server_ahead, now)));
                    }
                    match &notice {
                        Notice::Welcomed(welcome) => {
                            store.meta_set(ACCOUNT_META, &hex::encode(&welcome.account_id))?;
                            if followed.as_ref().is_some_and(|f| *f != welcome.account_id) {
                                // The store last followed another account. Its
                                // unsent commands are not this one's to send,
                                // and the account log was resumed from its
                                // cursor: start again from this account's.
                                for entry in store.outbox()? {
                                    store.outbox_remove(&entry.command_id)?;
                                }
                                if account_cursor > 0 {
                                    return Ok(closed(
                                        "the store followed another account".into(),
                                        &disconnect,
                                        true,
                                    ));
                                }
                            }
                            welcomed = true;
                            server_ahead = wire::micros(welcome.server_time.as_ref())
                                .map_or(0, |server| server - crate::time::now_micros());
                            account_id.clone_from(&welcome.account_id);
                            store.set_generation(welcome.recovery_generation)?;
                            if welcome.heartbeat_interval_seconds > 0 {
                                heartbeat_period = Duration::from_secs(u64::from(
                                    welcome.heartbeat_interval_seconds,
                                ));
                                heartbeat = Box::pin(sleep(heartbeat_period).fuse());
                            }
                            // Retry anything still in the outbox, verbatim.
                            send_outbox(store, &session, &mut pending, &mut sent)?;
                        }
                        Notice::Acked { command_id, .. } => {
                            store.outbox_remove(command_id)?;
                            sent.remove(command_id);
                        }
                        // A command gets exactly one Ack or Error (protocol
                        // 6.5); a refused one is not retried.
                        Notice::Error(error) if !error.command_id.is_empty() => {
                            store.outbox_remove(&error.command_id)?;
                            sent.remove(&error.command_id);
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
                    Some(command) => on_command(
                        command,
                        store,
                        &mut session,
                        channels,
                        &mut pending,
                        &mut sent,
                        welcomed,
                    )?,
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

/// Queues every command of the outbox this connection has not sent, in the
/// order they were put there, and notes them as sent.
fn send_outbox<S: Store>(
    store: &S,
    session: &Session,
    pending: &mut VecDeque<Effect>,
    sent: &mut HashSet<Vec<u8>>,
) -> Result<(), ClientError> {
    for entry in store.outbox()? {
        if !sent.insert(entry.command_id) {
            continue;
        }
        if let Ok(command) = Command::decode(entry.bytes.as_slice()) {
            pending.push_back(session.command(command));
        }
    }
    Ok(())
}

/// Carries out one application command: the outbox entry and the frame,
/// what the outbox holds unsent, a subscription from the store's cursor,
/// or a head check. `sent` is what this connection has sent of the outbox
/// and `welcomed` whether its handshake is done.
fn on_command<S: Store>(
    command: GatewayCommand,
    store: &mut S,
    session: &mut Session,
    channels: &mut Vec<Vec<u8>>,
    pending: &mut VecDeque<Effect>,
    sent: &mut HashSet<Vec<u8>>,
    welcomed: bool,
) -> Result<(), ClientError> {
    match command {
        GatewayCommand::Send(command) => {
            store.outbox_push(&OutboxEntry {
                command_id: command.command_id.clone(),
                channel_id: command.channel_id.clone(),
                bytes: command.encode_to_vec(),
            })?;
            // Before the welcome it waits in the outbox, which the welcome
            // sends whole and in order: sent now as well, it went twice.
            if welcomed {
                sent.insert(command.command_id.clone());
                pending.push_back(session.command(*command));
            }
        }
        GatewayCommand::Outbox => {
            if welcomed {
                send_outbox(store, session, pending, sent)?;
            }
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
        GatewayCommand::Presence(update) => pending.push_back(session.presence(update)),
        GatewayCommand::Typing(channel_id) => pending.extend(session.typing(&channel_id)),
    }
    Ok(())
}

/// How long a typing indicator is shown at most, whatever its `until` says.
/// Servers set `until` about 8 seconds ahead (protocol 6.7).
const TYPING_MAX: Duration = Duration::from_secs(10);

/// A typing indicator's `until` on this device's clock, in microseconds.
/// The server stamped it by its own clock; `server_ahead` is how far that
/// ran ahead of this one at the welcome. Never further off than
/// [`TYPING_MAX`], so a clock that jumped cannot leave someone typing for
/// an hour.
fn typing_until(until_micros: i64, server_ahead: i64, now_micros: i64) -> i64 {
    let max = i64::try_from(TYPING_MAX.as_micros()).unwrap_or(i64::MAX);
    now_micros + (until_micros - server_ahead - now_micros).clamp(0, max)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    #[allow(clippy::unwrap_used)]
    fn the_cursor_resumed_is_the_followed_accounts() {
        use crate::sqlite::SqliteStore;

        let mut store = SqliteStore::in_memory().unwrap();
        let (one, two) = (vec![1u8; 16], vec![2u8; 16]);
        let ev = |seq: u64| StoredEvent {
            seq,
            bytes: vec![seq as u8],
        };
        // Nothing yet: from the start, following nobody in particular.
        assert_eq!(followed_account(&store).unwrap(), (None, 0));
        // A store from before the account was written down: its one cursor.
        store
            .apply(LogKind::Account, &one, &[ev(1), ev(2)])
            .unwrap();
        assert_eq!(followed_account(&store).unwrap(), (Some(one.clone()), 2));
        // Written down: that account's cursor, and none of another's.
        store.meta_set(ACCOUNT_META, &hex::encode(&two)).unwrap();
        assert_eq!(followed_account(&store).unwrap(), (Some(two.clone()), 0));
        store.apply(LogKind::Account, &two, &[ev(1)]).unwrap();
        assert_eq!(followed_account(&store).unwrap(), (Some(two), 1));
        store.meta_set(ACCOUNT_META, &hex::encode(&one)).unwrap();
        assert_eq!(followed_account(&store).unwrap(), (Some(one), 2));
    }

    /// What the application sends while the instance cannot be reached is
    /// not lost with the next attempt that fails: it used to be handed to
    /// that attempt, and went with it. A command is in the outbox as soon
    /// as it is read, and the rest waits for a connection that is made.
    #[cfg(not(target_arch = "wasm32"))]
    #[tokio::test]
    #[allow(clippy::unwrap_used)]
    async fn what_is_sent_while_unreachable_is_kept_for_the_next_connection() {
        use crate::api::Tokens;
        use crate::sqlite::SqliteStore;

        // An address nothing listens on: every connection is refused.
        let port = {
            let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            listener.local_addr().unwrap().port()
        };
        let api = Api::new(&format!("http://127.0.0.1:{port}")).with_tokens(Tokens {
            access: Some("a".into()),
            refresh: None,
        });
        let mut store = SqliteStore::in_memory().unwrap();
        let (tx, mut rx) = mpsc::unbounded();
        let (etx, mut erx) = mpsc::unbounded();
        let command = Command {
            command_id: vec![7; 16],
            channel_id: vec![1; 16],
            ..Default::default()
        };

        // One attempt, holding a subscription asked for meanwhile: it fails,
        // and the subscription is still held for the next.
        let mut stash = vec![GatewayCommand::Subscribe(vec![2; 16])];
        let attempt = run(
            &api,
            &mut store,
            "t",
            &[9; 16],
            &mut Vec::new(),
            &mut rx,
            &mut stash,
            &etx,
        )
        .await;
        assert!(attempt.is_err());
        assert!(matches!(
            stash.as_slice(),
            [GatewayCommand::Subscribe(channel)] if *channel == vec![2u8; 16]
        ));

        let looping = tokio::spawn(run_until_told(
            api,
            store,
            "t",
            vec![9; 16],
            Vec::new(),
            rx,
            etx,
        ));
        let mut attempts = 0;
        while let Some(event) = erx.next().await {
            if let GatewayEvent::Reconnecting { attempt, .. } = event {
                if attempt == 1 {
                    // Sent during the first wait, and so held through the
                    // second attempt, which fails too.
                    tx.unbounded_send(GatewayCommand::Send(Box::new(command.clone())))
                        .unwrap();
                }
                attempts = attempt;
                if attempt == 3 {
                    break;
                }
            }
        }
        assert_eq!(attempts, 3);
        drop(tx);
        let (store, _) = looping.await.unwrap();
        let queued: Vec<Vec<u8>> = store
            .outbox()
            .unwrap()
            .into_iter()
            .map(|e| e.command_id)
            .collect();
        assert_eq!(queued, [vec![7u8; 16]]);
    }

    /// A command goes once on a connection, however the loop hears of it:
    /// read from the application, found in the outbox at the welcome, or
    /// said to be in the outbox by whoever wrote it there.
    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    #[allow(clippy::unwrap_used)]
    fn a_command_goes_once_on_a_connection() {
        use crate::sqlite::SqliteStore;
        use lumia_core::proto::{client_frame, ClientFrame};

        /// The loop's state for one connection.
        struct Connection {
            store: SqliteStore,
            session: Session,
            pending: VecDeque<Effect>,
            sent: HashSet<Vec<u8>>,
        }

        impl Connection {
            fn hears(&mut self, command: GatewayCommand, welcomed: bool) {
                on_command(
                    command,
                    &mut self.store,
                    &mut self.session,
                    &mut Vec::new(),
                    &mut self.pending,
                    &mut self.sent,
                    welcomed,
                )
                .unwrap();
            }

            /// As the application writes one through its own connection.
            fn written(&mut self, n: u8) {
                self.store
                    .outbox_push(&OutboxEntry {
                        command_id: vec![n; 16],
                        channel_id: vec![1; 16],
                        bytes: command(n).encode_to_vec(),
                    })
                    .unwrap();
            }

            /// The commands queued to go since the last look, by their number.
            fn gone(&mut self) -> Vec<u8> {
                self.pending
                    .drain(..)
                    .filter_map(|effect| match effect {
                        Effect::Send(ClientFrame {
                            frame: Some(client_frame::Frame::Command(c)),
                        }) => Some(c.command_id[0]),
                        _ => None,
                    })
                    .collect()
            }
        }

        fn command(n: u8) -> Command {
            Command {
                command_id: vec![n; 16],
                channel_id: vec![1; 16],
                ..Default::default()
            }
        }

        let mut connection = Connection {
            store: SqliteStore::in_memory().unwrap(),
            session: Session::new("t", vec![9; 16], Cursors::default()),
            pending: VecDeque::new(),
            sent: HashSet::new(),
        };

        // Before the welcome nothing is sent: one read from the application
        // is kept, and one said to be in the outbox is left there.
        connection.hears(GatewayCommand::Send(Box::new(command(1))), false);
        connection.written(2);
        connection.hears(GatewayCommand::Outbox, false);
        assert!(connection.gone().is_empty());
        assert_eq!(connection.store.outbox().unwrap().len(), 2);

        // The welcome sends the outbox whole, in the order it was written.
        send_outbox(
            &connection.store,
            &connection.session,
            &mut connection.pending,
            &mut connection.sent,
        )
        .unwrap();
        assert_eq!(connection.gone(), [1, 2]);

        // Said again, they do not go again; one written since goes alone.
        connection.hears(GatewayCommand::Outbox, true);
        assert!(connection.gone().is_empty());
        connection.written(3);
        connection.hears(GatewayCommand::Outbox, true);
        assert_eq!(connection.gone(), [3]);

        // After the welcome one read from the application goes at once, and
        // is not sent again for being in the outbox.
        connection.hears(GatewayCommand::Send(Box::new(command(4))), true);
        assert_eq!(connection.gone(), [4]);
        connection.hears(GatewayCommand::Outbox, true);
        assert!(connection.gone().is_empty());
        assert_eq!(connection.store.outbox().unwrap().len(), 4);
    }

    #[test]
    fn backoff_doubles_and_caps() {
        let a = backoff(1);
        let b = backoff(2);
        let c = backoff(3);
        assert!(a >= Duration::from_millis(500) && a < Duration::from_secs(1));
        assert!(b >= Duration::from_secs(1) && b < Duration::from_secs(2));
        assert!(c >= Duration::from_secs(2) && c < Duration::from_secs(4));
        let far = backoff(40);
        assert!(far >= MAX_BACKOFF / 2 && far < MAX_BACKOFF);
    }

    #[test]
    fn a_typing_indicator_ends_by_this_devices_clock() {
        const SECOND: i64 = 1_000_000;
        let now = 1_000 * SECOND;
        // Clocks agree: eight seconds ahead is eight seconds ahead.
        assert_eq!(typing_until(now + 8 * SECOND, 0, now), now + 8 * SECOND);
        // The server's clock runs a minute ahead, or five behind: still eight.
        let ahead = 60 * SECOND;
        assert_eq!(
            typing_until(now + ahead + 8 * SECOND, ahead, now),
            now + 8 * SECOND
        );
        let behind = -300 * SECOND;
        assert_eq!(
            typing_until(now + behind + 8 * SECOND, behind, now),
            now + 8 * SECOND
        );
        // One that arrives after its time is over at once; one from a clock
        // that has moved since the welcome lasts ten seconds, not an hour.
        assert_eq!(typing_until(now - SECOND, 0, now), now);
        assert_eq!(
            typing_until(now + 3_600 * SECOND, 0, now),
            now + 10 * SECOND
        );
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
