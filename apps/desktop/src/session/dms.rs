//! Personal conversations (architecture 6.4 and 11): the end-to-end encrypted DMs the core's
//! engine keeps, the requests that open them, and reading and sending in one. The
//! counterpart of the `dm*` half of LumiaKit's `Session.swift`.
//!
//! The snapshot lists the personal conversations the account is in; the engine says which
//! of them this device holds the group for ([`DmState::channels`]) and what it can read of
//! each ([`DmState::logs`]). Both are asked for again when the engine reports a change.
//! Every call here needs this device to hold an identity ([`Session::has_identity`]).
//!
//! Not built: editing and deleting a message, history backup, group conversations with
//! more than one other person being started from here (one that exists is read and written
//! like any other).
//!
//! A send is shown as pending until the log holds it. The engine queues it off this thread
//! and answers with the message's id, which is also its command's: the pending send takes
//! that id, the log settles it by it, and a refusal from the instance names it.

use std::collections::{HashMap, HashSet};
use std::future::Future;

use lumia_client::app::{AppEvent, ChannelSummary, DmChannelSummary, DmLogSummary, QuoteSummary};

use super::{Conversation, Entry, Failure, Pending, Session, Update};

/// What the id of a pending personal message starts with: it is minted here, not by the core.
const SEND_PREFIX: &str = "dm-send-";

/// How often a failed ask for the engine's conversations is repeated before waiting for the
/// next event to ask again.
const CHANNEL_RETRIES: u32 = 4;

/// How a request to talk is answered.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Answer {
    Accept,
    Decline,
    Block,
}

impl Answer {
    /// The word `App::dm_respond` takes.
    fn word(self) -> &'static str {
        match self {
            Self::Accept => "accept",
            Self::Decline => "decline",
            Self::Block => "block",
        }
    }
}

/// What a finished personal-conversation call reports.
#[derive(Debug, Clone)]
pub enum DmUpdate {
    /// The conversations this device's engine knows, with whether it holds each group.
    Channels(Result<Vec<DmChannelSummary>, Failure>),
    /// A conversation as this device can read it.
    Log {
        channel_id: String,
        log: Result<DmLogSummary, Failure>,
    },
    /// A send was queued, under the message id the engine gave it, or could not be.
    Sent {
        pending_id: String,
        result: Result<String, Failure>,
    },
    /// [`Session::dm_start`] answered: the conversation's id when one was opened, `None`
    /// when the instance wanted a request first and one went out.
    Started {
        account_id: String,
        result: Result<Option<String>, Failure>,
    },
    /// [`Session::dm_answer`] answered: the conversation's id when accepting opened one.
    Answered {
        request_id: String,
        answer: Answer,
        result: Result<Option<String>, Failure>,
    },
    /// [`Session::dm_rejoin`] finished.
    Rejoined {
        channel_id: String,
        result: Result<(), Failure>,
    },
}

/// The personal conversations this device knows, and what it can read of them.
#[derive(Debug, Default)]
pub struct DmState {
    /// The conversations the engine knows, with whether this device holds each group. One
    /// the snapshot lists and this does not is one the engine has not been told of yet.
    pub channels: Vec<DmChannelSummary>,
    /// Each conversation as last read, by channel id.
    pub logs: HashMap<String, DmLogSummary>,
    /// Asks for [`DmState::channels`] that failed in a row.
    channel_failures: u32,
    /// How many sends this session has minted a placeholder id for.
    sends: u64,
}

impl Session {
    // ---- Reading --------------------------------------------------------------------

    /// The personal conversation on screen, when that is what is on screen.
    pub fn dm(&self) -> Option<&str> {
        match &self.conversation {
            Some(Conversation::Dm(id)) => Some(id),
            _ => None,
        }
    }

    /// Whether this device holds the conversation's group, so it can read and send; `None`
    /// while the engine has not said.
    pub fn dm_member(&self, channel_id: &str) -> Option<bool> {
        self.dm
            .channels
            .iter()
            .find(|c| c.channel_id == channel_id)
            .map(|c| c.member)
    }

    /// The conversation as last read.
    pub fn dm_log(&self, channel_id: &str) -> Option<&DmLogSummary> {
        self.dm.logs.get(channel_id)
    }

    /// Everyone in a conversation but this account.
    pub fn others_in<'a>(&'a self, channel: &'a ChannelSummary) -> impl Iterator<Item = &'a str> {
        let me = self.me();
        channel
            .participant_account_ids
            .iter()
            .map(String::as_str)
            .filter(move |id| *id != me)
    }

    /// What a personal conversation is called: the others in it.
    pub fn conversation_name(&self, channel: &ChannelSummary) -> String {
        let names: Vec<String> = self.others_in(channel).map(|id| self.name_of(id)).collect();
        if names.is_empty() {
            "Just you".into()
        } else {
            names.join(", ")
        }
    }

    // ---- What the screens do ----------------------------------------------------------

    /// Puts a personal conversation on screen: what was last read of it at once, and what
    /// the engine holds now as soon as it answers.
    pub fn open_dm(&mut self, channel_id: &str) {
        self.open_conversation(Conversation::Dm(channel_id.to_string()));
    }

    /// Asks the engine which conversations it knows ([`DmUpdate::Channels`]).
    pub fn refresh_dm_channels(&self) {
        self.refresh_dm_channels_after(std::time::Duration::ZERO);
    }

    fn refresh_dm_channels_after(&self, wait: std::time::Duration) {
        let Some(app) = self.client() else { return };
        if !app.has_identity() {
            return;
        }
        self.task_local(move || async move {
            tokio::time::sleep(wait).await;
            Update::Dm(DmUpdate::Channels(
                app.dm_channels().await.map_err(Failure::from),
            ))
        });
    }

    /// Asks the engine for a conversation as this device can read it ([`DmUpdate::Log`]).
    fn refresh_dm_log(&self, channel_id: &str) {
        let Some(app) = self.client() else { return };
        if !app.has_identity() {
            return;
        }
        let channel_id = channel_id.to_string();
        self.task_local(move || async move {
            let log = app.dm_messages(&channel_id).await.map_err(Failure::from);
            Update::Dm(DmUpdate::Log { channel_id, log })
        });
    }

    /// Opens the conversation with someone, or asks them when the instance wants a request
    /// first; the answer is [`DmUpdate::Started`], after the snapshot that lists the
    /// conversation when one was opened.
    pub fn dm_start(&self, account_id: &str) {
        let Some(app) = self.client() else { return };
        let account_id = account_id.to_string();
        self.task_local_many(move || async move {
            let result = match app.dm_open(std::slice::from_ref(&account_id)).await {
                Ok(opened) => Ok(Some(opened.channel_id)),
                Err(error) => {
                    let failure = Failure::from(error);
                    if failure.code == "FAILED_PRECONDITION" {
                        app.dm_request(&account_id)
                            .await
                            .map(|_| None)
                            .map_err(Failure::from)
                    } else {
                        Err(failure)
                    }
                }
            };
            let mut updates = Vec::new();
            if matches!(result, Ok(Some(_)))
                && let Ok(snapshot) = app.snapshot().await
            {
                updates.push(Update::Snapshot(Ok(snapshot)));
            }
            updates.push(Update::Dm(DmUpdate::Started { account_id, result }));
            updates
        });
    }

    /// Answers a request. Accepting opens the conversation and, as the accepter, creates
    /// its group. The answer is [`DmUpdate::Answered`], after the snapshot without the
    /// request (and with the conversation, when one was opened).
    pub fn dm_answer(&self, request_id: &str, answer: Answer) {
        let Some(app) = self.client() else { return };
        let request_id = request_id.to_string();
        self.task_local_many(move || async move {
            let result = app
                .dm_respond(&request_id, answer.word())
                .await
                .map(|answered| answered.ok().map(|opened| opened.channel_id))
                .map_err(Failure::from);
            let mut updates = Vec::new();
            // Also after a failure: an accept that was recorded and then could not make the
            // group has still taken the request off the list.
            if let Ok(snapshot) = app.snapshot().await {
                updates.push(Update::Snapshot(Ok(snapshot)));
            }
            updates.push(Update::Dm(DmUpdate::Answered {
                request_id,
                answer,
                result,
            }));
            updates
        });
    }

    /// Rejoins a conversation this device is not in, by external commit from the group info
    /// the instance holds (e2ee spec section 10). The answer is [`DmUpdate::Rejoined`].
    pub fn dm_rejoin(&self, channel_id: &str) {
        let Some(app) = self.client() else { return };
        let channel_id = channel_id.to_string();
        self.task_local_many(move || async move {
            let result = app.dm_rejoin(&channel_id).await.map_err(Failure::from);
            let mut updates = Vec::new();
            if let Ok(channels) = app.dm_channels().await {
                updates.push(Update::Dm(DmUpdate::Channels(Ok(channels))));
            }
            updates.push(Update::Dm(DmUpdate::Rejoined { channel_id, result }));
            updates
        });
    }

    /// [`Session::task_local`] for a call whose answer refers to something that has to be
    /// applied first (a conversation the snapshot must list before it is opened): every
    /// update is queued, in order.
    fn task_local_many<F: Future<Output = Vec<Update>>>(
        &self,
        make: impl FnOnce() -> F + Send + 'static,
    ) {
        let mailbox = self.mailbox.clone();
        let runtime = self.runtime.handle().clone();
        self.runtime.spawn_blocking(move || {
            for update in runtime.block_on(make()) {
                mailbox.post(update);
            }
        });
    }

    // ---- What the session is told -----------------------------------------------------

    /// A snapshot was applied: the people in its personal conversations and behind its
    /// requests are looked up when no server lists them, and the engine is asked again
    /// which conversations it holds.
    pub(super) fn dm_snapshot_applied(&mut self) {
        let Some(snapshot) = &self.snapshot else {
            return;
        };
        let people: HashSet<String> = snapshot
            .channels
            .iter()
            .filter(|c| super::PERSONAL_KINDS.contains(&c.kind.as_str()))
            .flat_map(|c| c.participant_account_ids.iter())
            .chain(snapshot.pending_requests.iter().map(|r| &r.from_account_id))
            .cloned()
            .collect();
        for id in people {
            self.look_up(&id);
        }
        self.refresh_dm_channels();
    }

    pub(super) fn apply_dm(&mut self, update: &DmUpdate) {
        match update {
            DmUpdate::Channels(Ok(channels)) => {
                self.dm.channels.clone_from(channels);
                self.dm.channel_failures = 0;
            }
            // The engine is built on first use and gives up when an identity call holds the
            // identity's stores at that moment (the roster being read at launch, a link
            // being claimed). Nothing else would ask again until the next event, so this
            // does, a few times.
            DmUpdate::Channels(Err(_)) => {
                if self.dm.channel_failures < CHANNEL_RETRIES {
                    self.dm.channel_failures += 1;
                    self.refresh_dm_channels_after(std::time::Duration::from_millis(
                        250 * u64::from(self.dm.channel_failures),
                    ));
                }
            }
            DmUpdate::Log {
                channel_id,
                log: Ok(log),
            } => {
                self.settle_dm_sends(channel_id, log);
                if self.dm() == Some(channel_id.as_str()) {
                    let entries = self.dm_entries(channel_id, log);
                    self.set_entries(entries);
                    self.revision += 1;
                }
                self.dm.logs.insert(channel_id.clone(), log.clone());
            }
            DmUpdate::Log {
                channel_id,
                log: Err(failure),
            } => {
                if self.dm() == Some(channel_id.as_str()) {
                    self.last_error = Some(failure.message.clone());
                }
            }
            DmUpdate::Sent {
                pending_id,
                result: Err(failure),
            } => {
                if let Some(pending) = self.pending.iter_mut().find(|p| p.id == *pending_id) {
                    pending.failed = Some(failure.message.clone());
                    self.revision += 1;
                }
            }
            // The send has its real id now. The log may already hold it: it can be read
            // before this answer arrives.
            DmUpdate::Sent {
                pending_id,
                result: Ok(id),
            } => {
                let Some(pending) = self.pending.iter_mut().find(|p| p.id == *pending_id) else {
                    return;
                };
                if !id.is_empty() {
                    pending.id.clone_from(id);
                }
                let channel_id = pending.channel_id.clone();
                if let Some(log) = self.dm.logs.get(&channel_id).cloned() {
                    self.settle_dm_sends(&channel_id, &log);
                }
                self.revision += 1;
            }
            // The conversation exists, or its group is being made: the engine's own events
            // (`DmSubscribed`, `DmReady`) say when there is more to read.
            DmUpdate::Started { result: Ok(_), .. }
            | DmUpdate::Answered { result: Ok(_), .. }
            | DmUpdate::Rejoined { result: Ok(()), .. } => self.refresh_dm_channels(),
            DmUpdate::Started { result: Err(_), .. }
            | DmUpdate::Answered { result: Err(_), .. }
            | DmUpdate::Rejoined { result: Err(_), .. } => {}
        }
    }

    /// What the DM engine reported through the gateway loop.
    pub(super) fn on_dm_event(&mut self, event: &AppEvent) {
        match event {
            AppEvent::DmError { message } => self.last_error = Some(message.clone()),
            // A conversation the account can now see, or one whose group this device now
            // holds.
            AppEvent::DmSubscribed { channel_id } | AppEvent::DmReady { channel_id } => {
                self.refresh_dm_channels();
                self.dm_changed(channel_id);
            }
            AppEvent::DmOperation { channel_id, .. } => self.dm_changed(channel_id),
            // The event names the operation and not its conversation: every conversation
            // with a send waiting is read again, and the one on screen.
            AppEvent::DmDelivered { .. } => {
                let mut waiting: HashSet<String> = self
                    .pending
                    .iter()
                    .filter(|p| p.failed.is_none() && self.dm_member(&p.channel_id).is_some())
                    .map(|p| p.channel_id.clone())
                    .collect();
                waiting.extend(self.dm().map(str::to_string));
                for channel_id in waiting {
                    self.refresh_dm_log(&channel_id);
                }
            }
            // The engine's running commentary is not shown, as the other shells show none of
            // it. But a row this device could not read (sent before it joined, or not
            // decryptable) is reported only as a note, and the log counts those rows: the
            // conversation on screen is read again.
            AppEvent::DmNote { .. } => {
                if let Some(channel_id) = self.dm().map(str::to_string) {
                    self.refresh_dm_log(&channel_id);
                }
            }
            _ => {}
        }
    }

    /// A conversation has something new: read again when it is on screen, forgotten
    /// otherwise so it is read when it is opened.
    fn dm_changed(&mut self, channel_id: &str) {
        if self.dm() == Some(channel_id) {
            self.refresh_dm_log(channel_id);
        } else {
            self.dm.logs.remove(channel_id);
        }
    }

    /// A personal conversation's messages as this device last read them; the engine is
    /// asked for what it holds now, which arrives as [`DmUpdate::Log`].
    pub(super) fn read_dm_entries(&mut self, channel_id: &str) -> Result<Vec<Entry>, String> {
        self.refresh_dm_log(channel_id);
        Ok(self
            .dm
            .logs
            .get(channel_id)
            .map(|log| self.dm_entries(channel_id, log))
            .unwrap_or_default())
    }

    /// A log's messages as the timeline draws them. A one-to-one conversation has one other
    /// sender, named even when the engine has not yet tied the identity to an account.
    fn dm_entries(&self, channel_id: &str, log: &DmLogSummary) -> Vec<Entry> {
        let me = self.me();
        let others: Vec<&str> = self
            .channel_summary(channel_id)
            .map(|channel| self.others_in(channel).collect())
            .unwrap_or_default();
        log.messages
            .iter()
            .map(|m| {
                let author = if m.mine {
                    me
                } else if m.sender_account_id.is_empty() && others.len() == 1 {
                    others[0]
                } else {
                    m.sender_account_id.as_str()
                };
                Entry {
                    id: m.message_id.clone(),
                    seq: m.seq,
                    author_account_id: author.to_string(),
                    body: m.body.clone(),
                    at_micros: m.sent_at_micros,
                    deleted: m.deleted,
                    edited: m.edited,
                    notice: String::new(),
                    notice_about: String::new(),
                    reply: m.reply.clone(),
                    mentions: Vec::new(),
                    reactions: m.reactions.clone(),
                    mentions_me: false,
                }
            })
            .collect()
    }

    /// Drops the pending sends a log now holds, by the id the engine gave each.
    fn settle_dm_sends(&mut self, channel_id: &str, log: &DmLogSummary) {
        let before = self.pending.len();
        self.pending.retain(|p| {
            p.channel_id != channel_id || !log.messages.iter().any(|m| m.message_id == p.id)
        });
        if self.pending.len() != before {
            self.revision += 1;
        }
    }

    /// Sends to a personal conversation: shown as pending at once, and marked failed with
    /// the reason when the engine cannot queue it. False when it could not even be tried
    /// (this device is not in the conversation's group), so the caller keeps the draft.
    pub(super) fn send_dm(
        &mut self,
        channel_id: &str,
        body: &str,
        now_micros: i64,
        reply: Option<&QuoteSummary>,
    ) -> bool {
        let Some(app) = self.client() else {
            return false;
        };
        if self.dm_member(channel_id) != Some(true) {
            self.last_error =
                Some("Not sent: this device is not in the conversation's group yet.".into());
            return false;
        }
        self.dm.sends += 1;
        let pending_id = format!("{SEND_PREFIX}{}", self.dm.sends);
        self.pending.push(Pending {
            id: pending_id.clone(),
            channel_id: channel_id.to_string(),
            body: body.to_string(),
            sent_at_micros: now_micros,
            reply: reply.cloned(),
            failed: None,
        });
        self.revision += 1;
        let (channel_id, body) = (channel_id.to_string(), body.to_string());
        let reply = reply.cloned();
        self.task_local(move || async move {
            let result = app
                .dm_send_with(&channel_id, &body, reply.as_ref())
                .await
                .map_err(Failure::from);
            Update::Dm(DmUpdate::Sent { pending_id, result })
        });
        true
    }
}
