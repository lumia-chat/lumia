//! Personal conversations (architecture 6.4 and 11): the end-to-end encrypted DMs the core's
//! engine keeps, the requests that open them, and reading and sending in one. The
//! counterpart of the `dm*` half of LumiaKit's `Session.swift`.
//!
//! The snapshot lists the personal conversations the account is in; the engine says which
//! of them this device holds the group for ([`DmState::channels`]) and what it can read of
//! each ([`DmState::logs`]). Both are asked for again when the engine reports a change.
//! Every call here needs this device to hold an identity ([`Session::has_identity`]).
//!
//! Not built: history backup, group conversations with
//! more than one other person being started from here (one that exists is read and written
//! like any other).
//!
//! A send is shown as pending until the log holds it. The engine queues it off this thread
//! and answers with the message's id, which is also its command's: the pending send takes
//! that id, the log settles it by it, and a refusal from the instance names it.

use std::collections::{HashMap, HashSet};
use std::future::Future;

use lumia_client::app::{
    AppEvent, AttachmentDraft, ChannelSummary, DmChannelSummary, DmLogSummary, QuoteSummary,
};

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
    /// The conversations this device's engine knows, with whether it holds each group, as
    /// a call that changed them read them (a rejoin), or as a test or a scene gives them.
    Channels(Result<Vec<DmChannelSummary>, Failure>),
    /// The same, in answer to [`Session::refresh_dm_channels`]: the one ask in flight.
    ChannelsAsked(Result<Vec<DmChannelSummary>, Failure>),
    /// A conversation as this device can read it.
    Log {
        channel_id: String,
        log: Result<DmLogSummary, Failure>,
    },
    /// The send handed to the engine under the ticket `pending_id` ([`DmSending`]) was
    /// queued, under the message id the engine gave it, or could not be.
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
    /// Whether the engine is being asked for `channels`, and whether they were asked for
    /// again meanwhile. Each ask answers from a thread of its own, so two at once could
    /// land the older answer over the newer, and a conversation joined in between would
    /// read as not joined until something else asked: one is asked at a time, and what was
    /// asked meanwhile is asked when the first has answered.
    asking: bool,
    ask_again: bool,
    /// Whether this device has published its key packages since the session began, which
    /// it does once connected when the instance holds few of them. Until it has, a group
    /// another device makes cannot hold this one. The engine says so only in a note; the
    /// live tests wait for it before someone else opens a conversation with the shell.
    pub published: bool,
    /// How many sends this session has minted a ticket for.
    sends: u64,
    /// The sends handed to the engine that it has not answered for: not yet in the outbox,
    /// so not yet a message on its way.
    pub sending: Vec<DmSending>,
}

/// A personal message handed to the engine, which has not said whether it queued it. Until
/// it has, nothing is on its way: the composer keeps the words (and the strip the files),
/// and no row is drawn for it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DmSending {
    /// What the engine's answer names ([`DmUpdate::Sent`]).
    pub ticket: String,
    pub channel_id: String,
    pub body: String,
    pub sent_at_micros: i64,
    pub reply: Option<QuoteSummary>,
    /// The files it carries.
    drafts: Vec<AttachmentDraft>,
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

    /// Asks the engine which conversations it knows ([`DmUpdate::ChannelsAsked`]), one ask
    /// at a time: asked while one is on its way, it is asked again when that one answers.
    pub fn refresh_dm_channels(&mut self) {
        if self.dm.asking {
            self.dm.ask_again = true;
            return;
        }
        self.ask_dm_channels(std::time::Duration::ZERO);
    }

    fn ask_dm_channels(&mut self, wait: std::time::Duration) {
        let Some(app) = self.client() else { return };
        if !app.has_identity() {
            return;
        }
        self.dm.asking = true;
        self.task_local(move || async move {
            tokio::time::sleep(wait).await;
            Update::Dm(DmUpdate::ChannelsAsked(
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
            DmUpdate::Channels(Ok(channels)) => self.dm.channels.clone_from(channels),
            DmUpdate::Channels(Err(_)) => {}
            DmUpdate::ChannelsAsked(Ok(channels)) => {
                self.dm.asking = false;
                self.dm.channels.clone_from(channels);
                self.dm.channel_failures = 0;
                if std::mem::take(&mut self.dm.ask_again) {
                    self.ask_dm_channels(std::time::Duration::ZERO);
                }
            }
            // The engine is built on first use and gives up when an identity call holds the
            // identity's stores at that moment (the roster being read at launch, a link
            // being claimed). Nothing else would ask again until the next event, so this
            // does, a few times; past those, only when it was asked for meanwhile.
            DmUpdate::ChannelsAsked(Err(_)) => {
                self.dm.asking = false;
                let again = std::mem::take(&mut self.dm.ask_again);
                if self.dm.channel_failures < CHANNEL_RETRIES {
                    self.dm.channel_failures += 1;
                    self.ask_dm_channels(std::time::Duration::from_millis(
                        250 * u64::from(self.dm.channel_failures),
                    ));
                } else if again {
                    self.ask_dm_channels(std::time::Duration::ZERO);
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
            DmUpdate::Sent { pending_id, result } => {
                let Some(at) = self.dm.sending.iter().position(|s| s.ticket == *pending_id) else {
                    return;
                };
                let send = self.dm.sending.remove(at);
                self.revision += 1;
                match result {
                    // Nothing went, and nothing was taken: the words are the composer's
                    // still, and the files are in its strip.
                    Err(failure) => {
                        self.last_error = Some(format!("Not sent: {}", failure.message));
                    }
                    // It is queued, under its real id: on its way from now, and shown so.
                    // The log may already hold it: it can be read before this answer.
                    Ok(id) => {
                        self.pending.push(Pending {
                            id: if id.is_empty() {
                                send.ticket
                            } else {
                                id.clone()
                            },
                            channel_id: send.channel_id.clone(),
                            body: send.body,
                            sent_at_micros: send.sent_at_micros,
                            reply: send.reply,
                            attachments: super::names(&send.drafts),
                            failed: None,
                        });
                        self.sent_media(&send.channel_id, &send.drafts);
                        if let Some(log) = self.dm.logs.get(&send.channel_id).cloned() {
                            self.settle_dm_sends(&send.channel_id, &log);
                        }
                    }
                }
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
            AppEvent::DmNote { message } => {
                self.dm.published |= lumia_client::dm::says_published(message);
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
                    attachments: m.attachments.clone(),
                    cards: m.cards.clone(),
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

    /// Whether a send in a personal conversation is with the engine, unanswered.
    pub fn sending_in(&self, channel_id: &str) -> bool {
        self.dm.sending.iter().any(|s| s.channel_id == channel_id)
    }

    /// Hands a message for a personal conversation to the engine and answers the ticket its
    /// answer will name ([`DmUpdate::Sent`]). Until that says it is queued nothing is on its
    /// way and nothing is shown as sent: the caller keeps the words, as it does when the
    /// engine says why it could not queue them. None when it could not even be tried (this
    /// device is not in the conversation's group).
    pub(super) fn send_dm(
        &mut self,
        channel_id: &str,
        body: &str,
        now_micros: i64,
        reply: Option<&QuoteSummary>,
        drafts: &[AttachmentDraft],
        cards: &[String],
    ) -> Option<String> {
        let app = self.client()?;
        if self.dm_member(channel_id) != Some(true) {
            self.last_error =
                Some("Not sent: this device is not in the conversation's group yet.".into());
            return None;
        }
        self.dm.sends += 1;
        let pending_id = format!("{SEND_PREFIX}{}", self.dm.sends);
        self.dm.sending.push(DmSending {
            ticket: pending_id.clone(),
            channel_id: channel_id.to_string(),
            body: body.to_string(),
            sent_at_micros: now_micros,
            reply: reply.cloned(),
            drafts: drafts.to_vec(),
        });
        let ticket = pending_id.clone();
        let (channel_id, body) = (channel_id.to_string(), body.to_string());
        let reply = reply.cloned();
        let ids: Vec<String> = drafts.iter().map(|d| d.draft_id.clone()).collect();
        let cards = cards.to_vec();
        self.task_local(move || async move {
            let result = app
                .dm_send_with_media(&channel_id, &body, reply.as_ref(), &ids, &cards)
                .await
                .map_err(Failure::from);
            Update::Dm(DmUpdate::Sent { pending_id, result })
        });
        Some(ticket)
    }
}
