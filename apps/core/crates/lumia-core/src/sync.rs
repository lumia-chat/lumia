//! The session state machine (docs/protocol.md sections 5 to 7,
//! docs/specs/delivery.md section 8).
//!
//! Frames in, effects out. The transport feeds every `ServerFrame` to
//! [`Session::handle`] and carries out the [`Effect`]s it returns; it also
//! reports back with [`Session::fetched`] and [`Session::snapshot_applied`]
//! when an effect completes. Nothing here blocks, allocates a socket, or
//! touches the store directly, so it is tested with scripted frames.
//!
//! Rules it enforces, per log:
//!
//! * events are applied in `seq` order, never N+1 before N;
//! * an event at or below the cursor is a duplicate and dropped;
//! * a gap is filled by a fetch before anything past it is applied;
//! * the three resume outcomes: replay, snapshot, or discard everything
//!   when the recovery generation changed.

use std::collections::{BTreeMap, HashMap};

use prost::Message;

use crate::proto::{
    client_frame, server_frame, AccountEvent, ClientFrame, Command, Cursor, Event, Hello,
    HighWaterMarkRequest, ResumeAccount, ServerFrame, Subscribe, SubscribeStatus, Welcome,
};
use crate::wire::LogId;

/// What the transport must do next.
#[derive(Debug, Clone, PartialEq)]
pub enum Effect {
    /// Send a frame on the gateway.
    Send(ClientFrame),
    /// Fetch channel events with `seq > after` and, when `before > 0`,
    /// `seq < before`, then call [`Session::fetched`].
    Fetch {
        /// The channel log.
        channel_id: LogId,
        /// Exclusive lower bound.
        after: u64,
        /// Exclusive upper bound, 0 for the head.
        before: u64,
    },
    /// Fetch account events with `seq > after`, then call [`Session::account_fetched`].
    FetchAccount {
        /// Exclusive lower bound.
        after: u64,
    },
    /// Take the channel's snapshot path, store it, then call [`Session::snapshot_applied`].
    Snapshot {
        /// The channel log.
        channel_id: LogId,
    },
    /// Take the account snapshot path, store it, then call [`Session::account_snapshot_applied`].
    AccountSnapshot,
    /// Store these channel events (in order, contiguous) and advance the cursor.
    Apply {
        /// The channel log.
        channel_id: LogId,
        /// Decoded events, in `seq` order.
        events: Vec<Event>,
    },
    /// Store these account events and advance the account cursor.
    ApplyAccount {
        /// Decoded events, in `seq` order.
        events: Vec<AccountEvent>,
    },
    /// The recovery generation changed: drop every cursor and all local
    /// events, record the new generation, then expect snapshots.
    DiscardAll {
        /// The generation to record.
        generation: u64,
    },
    /// Something for the application to know.
    Notify(Notice),
}

/// Application-level notifications.
#[derive(Debug, Clone, PartialEq)]
pub enum Notice {
    /// The handshake completed.
    Welcomed(Welcome),
    /// A command was acknowledged at `seq`.
    Acked {
        /// The command id.
        command_id: Vec<u8>,
        /// Its event's sequence number.
        seq: u64,
    },
    /// The server reported an error (possibly for a command).
    Error(crate::proto::Error),
    /// A channel subscription failed; the client should drop it.
    SubscriptionFailed {
        /// The channel log.
        channel_id: LogId,
        /// The server's reason.
        error: Option<crate::proto::Error>,
    },
    /// The server is closing the connection.
    Disconnected(crate::proto::Disconnect),
    /// Who is present in a watched server changed (protocol 6.7).
    PresenceDiff(crate::proto::PresenceDiff),
}

/// Per-log replay state.
#[derive(Debug, Clone, PartialEq, Eq)]
enum LogState {
    /// Waiting for the subscribe result.
    Pending,
    /// Live: events at cursor+1 apply directly.
    Live,
    /// Waiting for the snapshot path to complete.
    Snapshotting,
    /// A fetch is in flight for the gap after the cursor.
    Fetching,
}

#[derive(Debug, Clone)]
struct ChannelSync {
    cursor: u64,
    state: LogState,
    /// Out-of-order events held until the gap below them is filled.
    buffer: BTreeMap<u64, Event>,
}

#[derive(Debug, Clone)]
struct AccountSync {
    cursor: u64,
    state: LogState,
    buffer: BTreeMap<u64, AccountEvent>,
}

/// One gateway session's sync state for one instance.
#[derive(Debug, Clone)]
pub struct Session {
    client: String,
    device_id: Vec<u8>,
    stored_generation: Option<u64>,
    generation: Option<u64>,
    channels: HashMap<LogId, ChannelSync>,
    account: AccountSync,
    welcomed: bool,
}

/// The starting point: cursors from the store.
#[derive(Debug, Clone, Default)]
pub struct Cursors {
    /// Channel cursors.
    pub channels: Vec<(LogId, u64)>,
    /// Account log cursor.
    pub account: u64,
    /// The recovery generation last recorded, if any.
    pub generation: Option<u64>,
}

impl Session {
    /// A session for `device_id` identified to the server as `client`,
    /// resuming from `cursors`.
    #[must_use]
    pub fn new(client: impl Into<String>, device_id: Vec<u8>, cursors: Cursors) -> Self {
        let channels = cursors
            .channels
            .into_iter()
            .map(|(id, cursor)| {
                (
                    id,
                    ChannelSync {
                        cursor,
                        state: LogState::Pending,
                        buffer: BTreeMap::new(),
                    },
                )
            })
            .collect();

        Self {
            client: client.into(),
            device_id,
            stored_generation: cursors.generation,
            generation: None,
            channels,
            account: AccountSync {
                cursor: cursors.account,
                state: LogState::Pending,
                buffer: BTreeMap::new(),
            },
            welcomed: false,
        }
    }

    /// The first frame after connecting.
    #[must_use]
    pub fn hello(&self) -> Effect {
        Effect::Send(frame(client_frame::Frame::Hello(Hello {
            client: self.client.clone(),
            device_id: self.device_id.clone(),
            recovery_generation: self.stored_generation.unwrap_or(0),
            roster_version: 0,
        })))
    }

    /// Adds a channel to this session's subscriptions (cursor 0 if unknown).
    /// Returns the subscribe effect when already welcomed.
    pub fn subscribe(&mut self, channel_id: LogId, cursor: u64) -> Vec<Effect> {
        let known = self.channels.contains_key(&channel_id);
        if !known {
            self.channels.insert(
                channel_id.clone(),
                ChannelSync {
                    cursor,
                    state: LogState::Pending,
                    buffer: BTreeMap::new(),
                },
            );
        }
        if self.welcomed {
            vec![self.subscribe_frame(&[channel_id])]
        } else {
            Vec::new()
        }
    }

    /// The cursor (highest applied `seq`) for a channel, if subscribed.
    #[must_use]
    pub fn cursor(&self, channel_id: &[u8]) -> Option<u64> {
        self.channels.get(channel_id).map(|c| c.cursor)
    }

    /// The account log cursor.
    #[must_use]
    pub fn account_cursor(&self) -> u64 {
        self.account.cursor
    }

    /// Wraps a command for sending. The transport also writes it to the
    /// outbox so a reconnect can retry it verbatim.
    #[must_use]
    pub fn command(&self, command: Command) -> Effect {
        Effect::Send(frame(client_frame::Frame::Command(command)))
    }

    /// A heartbeat frame; the transport sends one every interval from `Welcome`.
    #[must_use]
    pub fn heartbeat(&self, nonce: u64) -> Effect {
        Effect::Send(frame(client_frame::Frame::Heartbeat(
            crate::proto::Heartbeat { nonce },
        )))
    }

    /// On foreground or regained connectivity: ask for every head at once.
    #[must_use]
    pub fn high_water_mark(&self) -> Effect {
        Effect::Send(frame(client_frame::Frame::HighWaterMark(
            HighWaterMarkRequest {
                channel_ids: self.channels.keys().cloned().collect(),
                include_account_log: true,
            },
        )))
    }

    /// This client's presence and the servers whose presence it wants
    /// (protocol 6.7): the set replaces the last one.
    #[must_use]
    pub fn presence(&self, update: crate::proto::PresenceUpdate) -> Effect {
        Effect::Send(frame(client_frame::Frame::Presence(update)))
    }

    /// Feeds a server frame and returns what to do.
    pub fn handle(&mut self, frame: ServerFrame) -> Vec<Effect> {
        let Some(frame) = frame.frame else {
            return Vec::new();
        };
        match frame {
            server_frame::Frame::Welcome(welcome) => self.on_welcome(welcome),
            server_frame::Frame::SubscribeResult(result) => {
                self.on_subscribe_result(result.results)
            }
            server_frame::Frame::AccountResumeResult(result) => self.on_account_resume(result),
            server_frame::Frame::Event(event) => self.on_event(event),
            server_frame::Frame::AccountEvent(event) => self.on_account_event(event),
            server_frame::Frame::Nudge(nudge) => self.on_heads(nudge.heads, None),
            server_frame::Frame::HighWaterMark(resp) => {
                self.on_heads(resp.heads, Some(resp.account_log_seq))
            }
            server_frame::Frame::Ack(ack) => vec![Effect::Notify(Notice::Acked {
                command_id: ack.command_id,
                seq: ack.seq,
            })],
            server_frame::Frame::Error(error) => vec![Effect::Notify(Notice::Error(error))],
            server_frame::Frame::Disconnect(disconnect) => {
                vec![Effect::Notify(Notice::Disconnected(disconnect))]
            }
            server_frame::Frame::PresenceDiff(diff) => {
                vec![Effect::Notify(Notice::PresenceDiff(diff))]
            }
            server_frame::Frame::Heartbeat(_) | server_frame::Frame::Typing(_) => Vec::new(),
        }
    }

    /// The transport fetched channel events for a [`Effect::Fetch`]. A page
    /// that made progress but left buffered events beyond a gap (the server
    /// pages at 200) is followed by another fetch; an empty page is not, so
    /// a server with nothing to give cannot be asked in a loop.
    pub fn fetched(&mut self, channel_id: &[u8], events: Vec<Event>) -> Vec<Effect> {
        let mut effects = Vec::new();
        if let Some(sync) = self.channels.get_mut(channel_id) {
            sync.state = LogState::Live;
        }
        let progressed = !events.is_empty();
        for event in events {
            effects.extend(self.on_event(event));
        }
        if progressed {
            effects.extend(self.refetch_gap(channel_id));
        }
        effects
    }

    /// The transport fetched account events for a [`Effect::FetchAccount`].
    /// Pages the same way as [`Session::fetched`].
    pub fn account_fetched(&mut self, events: Vec<AccountEvent>) -> Vec<Effect> {
        self.account.state = LogState::Live;
        let progressed = !events.is_empty();
        let mut effects = Vec::new();
        for event in events {
            effects.extend(self.on_account_event(event));
        }
        if progressed && !self.account.buffer.is_empty() && self.account.state == LogState::Live {
            self.account.state = LogState::Fetching;
            effects.push(Effect::FetchAccount {
                after: self.account.cursor,
            });
        }
        effects
    }

    // Buffered events still sit beyond a gap after a page: fetch the next one.
    fn refetch_gap(&mut self, channel_id: &[u8]) -> Vec<Effect> {
        let Some(sync) = self.channels.get_mut(channel_id) else {
            return Vec::new();
        };
        match sync.buffer.keys().next().copied() {
            Some(first) if sync.state == LogState::Live => {
                sync.state = LogState::Fetching;
                vec![Effect::Fetch {
                    channel_id: channel_id.to_vec(),
                    after: sync.cursor,
                    before: first,
                }]
            }
            _ => Vec::new(),
        }
    }

    /// The transport stored a channel snapshot taken at `seq`.
    pub fn snapshot_applied(&mut self, channel_id: &[u8], seq: u64) -> Vec<Effect> {
        let Some(sync) = self.channels.get_mut(channel_id) else {
            return Vec::new();
        };
        sync.cursor = seq;
        sync.state = LogState::Live;
        sync.buffer.retain(|&s, _| s > seq);
        let id = channel_id.to_vec();
        self.drain_channel(&id)
    }

    /// The transport stored the account snapshot taken at `seq`.
    pub fn account_snapshot_applied(&mut self, seq: u64) -> Vec<Effect> {
        self.account.cursor = seq;
        self.account.state = LogState::Live;
        self.account.buffer.retain(|&s, _| s > seq);
        self.drain_account()
    }

    // ---- Handshake and subscriptions ---------------------------------------------

    fn on_welcome(&mut self, welcome: Welcome) -> Vec<Effect> {
        let mut effects = Vec::new();
        self.welcomed = true;
        self.generation = Some(welcome.recovery_generation);

        let changed = self
            .stored_generation
            .is_some_and(|g| g != welcome.recovery_generation);
        if changed {
            // Every cursor is meaningless now (delivery section 8.4).
            effects.push(Effect::DiscardAll {
                generation: welcome.recovery_generation,
            });
            self.stored_generation = Some(welcome.recovery_generation);
            for sync in self.channels.values_mut() {
                sync.cursor = 0;
                sync.buffer.clear();
                sync.state = LogState::Pending;
            }
            self.account.cursor = 0;
            self.account.buffer.clear();
        }

        effects.push(Effect::Notify(Notice::Welcomed(welcome)));
        // Account log first: it is how the client learns which channels exist.
        self.account.state = LogState::Pending;
        effects.push(Effect::Send(frame(client_frame::Frame::ResumeAccount(
            ResumeAccount {
                after_seq: self.account.cursor,
            },
        ))));
        let ids: Vec<LogId> = self.channels.keys().cloned().collect();
        if !ids.is_empty() {
            effects.push(self.subscribe_frame(&ids));
        }
        effects
    }

    fn subscribe_frame(&self, ids: &[LogId]) -> Effect {
        let cursors = ids
            .iter()
            .filter_map(|id| {
                self.channels.get(id).map(|c| Cursor {
                    log_id: id.clone(),
                    seq: c.cursor,
                })
            })
            .collect();
        Effect::Send(frame(client_frame::Frame::Subscribe(Subscribe { cursors })))
    }

    fn on_subscribe_result(
        &mut self,
        results: Vec<crate::proto::ChannelSubscribeResult>,
    ) -> Vec<Effect> {
        let mut effects = Vec::new();
        for result in results {
            let id = result.channel_id.clone();
            let Some(sync) = self.channels.get_mut(&id) else {
                continue;
            };
            match SubscribeStatus::try_from(result.status).unwrap_or(SubscribeStatus::Unspecified) {
                SubscribeStatus::Resumed => {
                    // Events from cursor+1 to last_seq arrive as frames.
                    sync.state = LogState::Live;
                }
                SubscribeStatus::SnapshotRequired => {
                    sync.state = LogState::Snapshotting;
                    effects.push(Effect::Snapshot { channel_id: id });
                }
                SubscribeStatus::Error | SubscribeStatus::Unspecified => {
                    self.channels.remove(&id);
                    effects.push(Effect::Notify(Notice::SubscriptionFailed {
                        channel_id: id,
                        error: result.error,
                    }));
                }
            }
        }
        effects
    }

    fn on_account_resume(&mut self, result: crate::proto::AccountResumeResult) -> Vec<Effect> {
        match SubscribeStatus::try_from(result.status).unwrap_or(SubscribeStatus::Unspecified) {
            SubscribeStatus::Resumed => {
                self.account.state = LogState::Live;
                Vec::new()
            }
            SubscribeStatus::SnapshotRequired => {
                self.account.state = LogState::Snapshotting;
                vec![Effect::AccountSnapshot]
            }
            SubscribeStatus::Error | SubscribeStatus::Unspecified => {
                vec![Effect::Notify(Notice::Error(
                    result.error.unwrap_or_default(),
                ))]
            }
        }
    }

    // ---- Channel events -----------------------------------------------------------

    fn on_event(&mut self, event: Event) -> Vec<Effect> {
        let id = event.channel_id.clone();
        let Some(sync) = self.channels.get_mut(&id) else {
            return Vec::new();
        };

        if event.seq <= sync.cursor {
            return Vec::new(); // duplicate
        }
        if sync.state == LogState::Snapshotting {
            // Hold everything until the snapshot sets the cursor.
            sync.buffer.insert(event.seq, event);
            return Vec::new();
        }
        if event.seq == sync.cursor + 1 {
            sync.cursor = event.seq;
            let mut applied = vec![event];
            // Drain contiguous buffered events behind it.
            while let Some(next) = sync.buffer.remove(&(sync.cursor + 1)) {
                sync.cursor = next.seq;
                applied.push(next);
            }
            return vec![Effect::Apply {
                channel_id: id,
                events: applied,
            }];
        }

        // A gap: buffer this one and fill below it, once.
        sync.buffer.insert(event.seq, event);
        if sync.state == LogState::Fetching {
            return Vec::new();
        }
        sync.state = LogState::Fetching;
        let after = sync.cursor;
        let before = sync.buffer.keys().next().copied().unwrap_or(0);
        vec![Effect::Fetch {
            channel_id: id,
            after,
            before,
        }]
    }

    fn drain_channel(&mut self, id: &LogId) -> Vec<Effect> {
        let Some(sync) = self.channels.get_mut(id) else {
            return Vec::new();
        };
        let mut applied = Vec::new();
        while let Some(next) = sync.buffer.remove(&(sync.cursor + 1)) {
            sync.cursor = next.seq;
            applied.push(next);
        }
        let mut effects = Vec::new();
        if !applied.is_empty() {
            effects.push(Effect::Apply {
                channel_id: id.clone(),
                events: applied,
            });
        }
        // Anything still buffered sits beyond a gap.
        if let Some(&first) = sync.buffer.keys().next() {
            if sync.state != LogState::Fetching {
                sync.state = LogState::Fetching;
                effects.push(Effect::Fetch {
                    channel_id: id.clone(),
                    after: sync.cursor,
                    before: first,
                });
            }
        }
        effects
    }

    fn on_heads(&mut self, heads: Vec<Cursor>, account_head: Option<u64>) -> Vec<Effect> {
        let mut effects = Vec::new();
        for head in heads {
            let Some(sync) = self.channels.get_mut(&head.log_id) else {
                continue;
            };
            if head.seq > sync.cursor && sync.state == LogState::Live {
                sync.state = LogState::Fetching;
                effects.push(Effect::Fetch {
                    channel_id: head.log_id,
                    after: sync.cursor,
                    before: 0,
                });
            }
        }
        if let Some(head) = account_head {
            if head > self.account.cursor && self.account.state == LogState::Live {
                self.account.state = LogState::Fetching;
                effects.push(Effect::FetchAccount {
                    after: self.account.cursor,
                });
            }
        }
        effects
    }

    // ---- Account events -----------------------------------------------------------

    fn on_account_event(&mut self, event: AccountEvent) -> Vec<Effect> {
        let sync = &mut self.account;
        if event.seq <= sync.cursor {
            return Vec::new();
        }
        if sync.state == LogState::Snapshotting {
            sync.buffer.insert(event.seq, event);
            return Vec::new();
        }
        if event.seq == sync.cursor + 1 {
            sync.cursor = event.seq;
            let mut applied = vec![event];
            while let Some(next) = sync.buffer.remove(&(sync.cursor + 1)) {
                sync.cursor = next.seq;
                applied.push(next);
            }
            return vec![Effect::ApplyAccount { events: applied }];
        }
        sync.buffer.insert(event.seq, event);
        if sync.state == LogState::Fetching {
            return Vec::new();
        }
        sync.state = LogState::Fetching;
        vec![Effect::FetchAccount { after: sync.cursor }]
    }

    fn drain_account(&mut self) -> Vec<Effect> {
        let sync = &mut self.account;
        let mut applied = Vec::new();
        while let Some(next) = sync.buffer.remove(&(sync.cursor + 1)) {
            sync.cursor = next.seq;
            applied.push(next);
        }
        let mut effects = Vec::new();
        if !applied.is_empty() {
            effects.push(Effect::ApplyAccount { events: applied });
        }
        if !sync.buffer.is_empty() && sync.state != LogState::Fetching {
            sync.state = LogState::Fetching;
            effects.push(Effect::FetchAccount { after: sync.cursor });
        }
        effects
    }
}

fn frame(inner: client_frame::Frame) -> ClientFrame {
    ClientFrame { frame: Some(inner) }
}

/// Decodes a binary gateway frame.
///
/// # Errors
/// Returns the prost error when the bytes are not a `ServerFrame`.
pub fn decode_frame(bytes: &[u8]) -> Result<ServerFrame, prost::DecodeError> {
    ServerFrame::decode(bytes)
}

/// Encodes a client frame for the wire.
#[must_use]
pub fn encode_frame(frame: &ClientFrame) -> Vec<u8> {
    frame.encode_to_vec()
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::proto::{
        AccountResumeResult, Ack, ChannelSubscribeResult, Disconnect, HighWaterMarkResponse, Nudge,
        SubscribeResult,
    };

    fn chan() -> LogId {
        vec![1; 16]
    }

    fn welcome(generation: u64) -> ServerFrame {
        ServerFrame {
            frame: Some(server_frame::Frame::Welcome(Welcome {
                recovery_generation: generation,
                ..Default::default()
            })),
        }
    }

    fn resumed(id: &LogId, head: u64) -> ServerFrame {
        ServerFrame {
            frame: Some(server_frame::Frame::SubscribeResult(SubscribeResult {
                results: vec![ChannelSubscribeResult {
                    channel_id: id.clone(),
                    status: SubscribeStatus::Resumed as i32,
                    last_seq: head,
                    ..Default::default()
                }],
            })),
        }
    }

    fn event(id: &LogId, seq: u64) -> Event {
        Event {
            channel_id: id.clone(),
            seq,
            ..Default::default()
        }
    }

    fn event_frame(id: &LogId, seq: u64) -> ServerFrame {
        ServerFrame {
            frame: Some(server_frame::Frame::Event(event(id, seq))),
        }
    }

    fn sends(effects: &[Effect]) -> Vec<&client_frame::Frame> {
        effects
            .iter()
            .filter_map(|e| match e {
                Effect::Send(f) => f.frame.as_ref(),
                _ => None,
            })
            .collect()
    }

    fn applied(effects: &[Effect]) -> Vec<u64> {
        effects
            .iter()
            .flat_map(|e| match e {
                Effect::Apply { events, .. } => events.iter().map(|ev| ev.seq).collect(),
                _ => vec![],
            })
            .collect()
    }

    fn live_session(cursor: u64) -> Session {
        let mut s = Session::new(
            "test",
            vec![9; 16],
            Cursors {
                channels: vec![(chan(), cursor)],
                account: 0,
                generation: Some(1),
            },
        );
        s.handle(welcome(1));
        s.handle(resumed(&chan(), cursor));
        s.handle(ServerFrame {
            frame: Some(server_frame::Frame::AccountResumeResult(
                AccountResumeResult {
                    status: SubscribeStatus::Resumed as i32,
                    ..Default::default()
                },
            )),
        });
        s
    }

    #[test]
    fn hello_carries_the_stored_generation() {
        let s = Session::new(
            "c",
            vec![9; 16],
            Cursors {
                generation: Some(7),
                ..Default::default()
            },
        );
        match s.hello() {
            Effect::Send(ClientFrame {
                frame: Some(client_frame::Frame::Hello(h)),
            }) => assert_eq!(h.recovery_generation, 7),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn welcome_resumes_account_first_then_subscribes_with_cursors() {
        let mut s = Session::new(
            "c",
            vec![9; 16],
            Cursors {
                channels: vec![(chan(), 41)],
                account: 5,
                generation: Some(1),
            },
        );
        let effects = s.handle(welcome(1));
        let frames = sends(&effects);
        assert!(matches!(
            frames[0],
            client_frame::Frame::ResumeAccount(ResumeAccount { after_seq: 5 })
        ));
        match frames[1] {
            client_frame::Frame::Subscribe(sub) => assert_eq!(sub.cursors[0].seq, 41),
            other => panic!("{other:?}"),
        }
        assert!(!effects
            .iter()
            .any(|e| matches!(e, Effect::DiscardAll { .. })));
    }

    #[test]
    fn generation_change_discards_everything_and_resumes_from_zero() {
        let mut s = Session::new(
            "c",
            vec![9; 16],
            Cursors {
                channels: vec![(chan(), 41)],
                account: 5,
                generation: Some(1),
            },
        );
        let effects = s.handle(welcome(2));
        assert!(matches!(effects[0], Effect::DiscardAll { generation: 2 }));
        let frames = sends(&effects);
        assert!(matches!(
            frames[0],
            client_frame::Frame::ResumeAccount(ResumeAccount { after_seq: 0 })
        ));
        match frames[1] {
            client_frame::Frame::Subscribe(sub) => assert_eq!(sub.cursors[0].seq, 0),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn in_order_events_apply_one_by_one() {
        let mut s = live_session(0);
        assert_eq!(applied(&s.handle(event_frame(&chan(), 1))), vec![1]);
        assert_eq!(applied(&s.handle(event_frame(&chan(), 2))), vec![2]);
        assert_eq!(s.cursor(&chan()), Some(2));
    }

    #[test]
    fn duplicates_are_dropped() {
        let mut s = live_session(3);
        assert!(s.handle(event_frame(&chan(), 2)).is_empty());
        assert!(s.handle(event_frame(&chan(), 3)).is_empty());
        assert_eq!(s.cursor(&chan()), Some(3));
    }

    #[test]
    fn a_gap_fetches_before_applying_anything_past_it() {
        let mut s = live_session(1);
        let effects = s.handle(event_frame(&chan(), 4));
        assert_eq!(
            effects,
            vec![Effect::Fetch {
                channel_id: chan(),
                after: 1,
                before: 4
            }]
        );
        assert_eq!(s.cursor(&chan()), Some(1));

        // A second out-of-order event while fetching does not fetch again.
        assert!(s.handle(event_frame(&chan(), 5)).is_empty());

        let effects = s.fetched(&chan(), vec![event(&chan(), 2), event(&chan(), 3)]);
        assert_eq!(applied(&effects), vec![2, 3, 4, 5]);
        assert_eq!(s.cursor(&chan()), Some(5));
    }

    #[test]
    fn a_short_page_is_followed_by_another_fetch_until_the_gap_closes() {
        let mut s = live_session(0);
        // A live event far ahead opens a gap of 5.
        assert_eq!(
            s.handle(event_frame(&chan(), 6)),
            vec![Effect::Fetch {
                channel_id: chan(),
                after: 0,
                before: 6
            }]
        );
        // The server pages: two events come back, the gap is still open.
        let effects = s.fetched(&chan(), vec![event(&chan(), 1), event(&chan(), 2)]);
        assert_eq!(applied(&effects), vec![1, 2]);
        assert_eq!(
            effects.last(),
            Some(&Effect::Fetch {
                channel_id: chan(),
                after: 2,
                before: 6
            })
        );
        // The rest closes it and drains the buffered event; no further fetch.
        let effects = s.fetched(
            &chan(),
            vec![event(&chan(), 3), event(&chan(), 4), event(&chan(), 5)],
        );
        assert_eq!(applied(&effects), vec![3, 4, 5, 6]);
        assert!(!effects.iter().any(|e| matches!(e, Effect::Fetch { .. })));
        assert_eq!(s.cursor(&chan()), Some(6));
    }

    #[test]
    fn an_empty_page_does_not_fetch_again() {
        let mut s = live_session(0);
        s.handle(event_frame(&chan(), 6));
        assert!(s.fetched(&chan(), vec![]).is_empty());
        // The next live event reopens the question.
        assert_eq!(
            s.handle(event_frame(&chan(), 7)),
            vec![Effect::Fetch {
                channel_id: chan(),
                after: 0,
                before: 6
            }]
        );
    }

    #[test]
    fn a_short_account_page_is_followed_by_another_fetch() {
        let mut s = live_session(0);
        let account_event = |seq| AccountEvent {
            seq,
            ..Default::default()
        };
        let gap = ServerFrame {
            frame: Some(server_frame::Frame::AccountEvent(account_event(4))),
        };
        assert_eq!(s.handle(gap), vec![Effect::FetchAccount { after: 0 }]);
        let effects = s.account_fetched(vec![account_event(1)]);
        assert_eq!(effects.last(), Some(&Effect::FetchAccount { after: 1 }));
        let effects = s.account_fetched(vec![account_event(2), account_event(3)]);
        assert!(!effects
            .iter()
            .any(|e| matches!(e, Effect::FetchAccount { .. })));
        assert_eq!(s.account_cursor(), 4);
    }

    #[test]
    fn snapshot_required_takes_the_snapshot_path_and_holds_live_events() {
        let mut s = Session::new(
            "c",
            vec![9; 16],
            Cursors {
                channels: vec![(chan(), 0)],
                account: 0,
                generation: Some(1),
            },
        );
        s.handle(welcome(1));
        let result = ServerFrame {
            frame: Some(server_frame::Frame::SubscribeResult(SubscribeResult {
                results: vec![ChannelSubscribeResult {
                    channel_id: chan(),
                    status: SubscribeStatus::SnapshotRequired as i32,
                    last_seq: 2000,
                    ..Default::default()
                }],
            })),
        };
        assert_eq!(
            s.handle(result),
            vec![Effect::Snapshot { channel_id: chan() }]
        );

        // Live events arriving meanwhile are held.
        assert!(s.handle(event_frame(&chan(), 2001)).is_empty());
        assert!(s.handle(event_frame(&chan(), 2002)).is_empty());

        let effects = s.snapshot_applied(&chan(), 2000);
        assert_eq!(applied(&effects), vec![2001, 2002]);
        assert_eq!(s.cursor(&chan()), Some(2002));
    }

    #[test]
    fn a_nudge_past_the_cursor_fetches_and_an_old_one_is_ignored() {
        let mut s = live_session(10);
        let nudge = |seq| ServerFrame {
            frame: Some(server_frame::Frame::Nudge(Nudge {
                heads: vec![Cursor {
                    log_id: chan(),
                    seq,
                }],
            })),
        };
        assert!(s.handle(nudge(10)).is_empty());
        assert_eq!(
            s.handle(nudge(12)),
            vec![Effect::Fetch {
                channel_id: chan(),
                after: 10,
                before: 0
            }]
        );
        // Already fetching: no second fetch.
        assert!(s.handle(nudge(13)).is_empty());
    }

    #[test]
    fn high_water_mark_response_covers_the_account_log() {
        let mut s = live_session(0);
        let resp = ServerFrame {
            frame: Some(server_frame::Frame::HighWaterMark(HighWaterMarkResponse {
                heads: vec![],
                account_log_seq: 3,
            })),
        };
        assert_eq!(s.handle(resp), vec![Effect::FetchAccount { after: 0 }]);
        let fetched = s.account_fetched(vec![
            AccountEvent {
                seq: 1,
                ..Default::default()
            },
            AccountEvent {
                seq: 2,
                ..Default::default()
            },
            AccountEvent {
                seq: 3,
                ..Default::default()
            },
        ]);
        let total: usize = fetched
            .iter()
            .map(|e| match e {
                Effect::ApplyAccount { events } => events.len(),
                _ => 0,
            })
            .sum();
        assert_eq!(total, 3);
        assert_eq!(s.account_cursor(), 3);
    }

    #[test]
    fn subscription_errors_drop_the_channel_and_notify() {
        let mut s = Session::new(
            "c",
            vec![9; 16],
            Cursors {
                channels: vec![(chan(), 0)],
                account: 0,
                generation: Some(1),
            },
        );
        s.handle(welcome(1));
        let result = ServerFrame {
            frame: Some(server_frame::Frame::SubscribeResult(SubscribeResult {
                results: vec![ChannelSubscribeResult {
                    channel_id: chan(),
                    status: SubscribeStatus::Error as i32,
                    ..Default::default()
                }],
            })),
        };
        let effects = s.handle(result);
        assert!(matches!(
            &effects[0],
            Effect::Notify(Notice::SubscriptionFailed { .. })
        ));
        assert_eq!(s.cursor(&chan()), None);
        assert!(s.handle(event_frame(&chan(), 1)).is_empty());
    }

    #[test]
    fn acks_errors_and_disconnects_are_notices() {
        let mut s = live_session(0);
        let ack = ServerFrame {
            frame: Some(server_frame::Frame::Ack(Ack {
                command_id: vec![7; 16],
                seq: 9,
                ..Default::default()
            })),
        };
        assert_eq!(
            s.handle(ack),
            vec![Effect::Notify(Notice::Acked {
                command_id: vec![7; 16],
                seq: 9
            })]
        );
        let bye = ServerFrame {
            frame: Some(server_frame::Frame::Disconnect(Disconnect::default())),
        };
        assert!(matches!(
            &s.handle(bye)[0],
            Effect::Notify(Notice::Disconnected(_))
        ));
    }

    #[test]
    fn subscribing_after_welcome_sends_immediately_and_before_waits() {
        let mut s = Session::new("c", vec![9; 16], Cursors::default());
        assert!(s.subscribe(chan(), 0).is_empty());
        let effects = s.handle(welcome(1));
        assert!(sends(&effects)
            .iter()
            .any(|f| matches!(f, client_frame::Frame::Subscribe(_))));
        let other = vec![2; 16];
        assert!(matches!(
            sends(&s.subscribe(other, 4))[0],
            client_frame::Frame::Subscribe(_)
        ));
    }

    #[test]
    fn frames_round_trip_through_the_codec() {
        let f = frame(client_frame::Frame::Heartbeat(crate::proto::Heartbeat {
            nonce: 5,
        }));
        let bytes = encode_frame(&f);
        assert_eq!(ClientFrame::decode(bytes.as_slice()).unwrap(), f);
        assert!(decode_frame(b"\xff\xff").is_err());
    }
}
