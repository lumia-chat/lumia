//! Read state, reactions and finding a message (architecture 6.6).
//!
//! The counts are the core's (`App::read_states`): the snapshot's, with what arrived since
//! added and what this account's other devices read taken away. They are read again after
//! every snapshot, every applied event and every position this device moves. A conversation
//! is marked read by the window when its newest message is on screen in front of the person
//! (`app::State::tick`), at most once a second each, and only when that moves something.
//!
//! The Mentions pages are `App::mentions`, one per server and one across all (Home), read a
//! page at a time. Finding a message (a quote's original, a mention opened from its page)
//! reads further back until it is among the entries, up to [`SEEK_PAGES`] pages; the window
//! scrolls to it and marks it once it is there.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use lumia_client::app::{MessageSummary, QuoteSummary, ReadStateSummary};

use super::{Conversation, Entry, Failure, Session, Update};

/// How many pages further back finding a message reads before it gives up.
pub const SEEK_PAGES: u32 = 5;
/// The least time between two marks of one conversation.
const MARK_EVERY: Duration = Duration::from_secs(1);

/// What a finished call about reading reports.
#[derive(Debug, Clone)]
pub enum ReadUpdate {
    /// A conversation marked read, as the instance then counts it.
    Marked(Result<ReadStateSummary, Failure>),
    /// A page of the Mentions of a server (`None`: every server), and whether there is more;
    /// `more` is true for a page after the first.
    Mentions {
        server_id: Option<String>,
        more: bool,
        page: Result<(Vec<MessageSummary>, bool), Failure>,
    },
    /// A reaction in a personal conversation was sent, or could not be.
    Reacted(Result<(), Failure>),
}

/// The messages that notify this account, as far as they have been read.
#[derive(Debug, Clone, Default)]
pub struct MentionsPage {
    pub messages: Vec<MessageSummary>,
    /// The instance has older ones.
    pub more: bool,
    pub loading: bool,
    pub error: Option<String>,
}

/// The read state this session keeps.
#[derive(Debug, Default)]
pub struct Reads {
    /// By channel id.
    states: HashMap<String, ReadStateSummary>,
    /// When each conversation was last marked, and through which event.
    marked: HashMap<String, (u64, Instant)>,
    /// By server id; `""` is every server's.
    pub mentions: HashMap<String, MentionsPage>,
    /// Where the open conversation had been read to when it was opened, while some of it
    /// was unread: the "New messages" line goes above the first message after it.
    pub new_since: Option<(String, u64)>,
    /// The message being found in the open conversation, and how many more pages may be read
    /// for it.
    seeking: Option<(String, u32)>,
    /// The message found, for the window to scroll to and mark; taken by it.
    pub found: Option<String>,
    /// The conversation just opened has a "New messages" line the window has not yet
    /// scrolled to; taken by it.
    pub land: bool,
}

impl Session {
    /// Reads the counts again from the core.
    pub(crate) fn refresh_reads(&mut self) {
        if let Some(app) = &self.app {
            self.reads.states = app
                .read_states()
                .into_iter()
                .map(|r| (r.channel_id.clone(), r))
                .collect();
        }
    }

    /// Takes these counts as the core's, for a test or a scene with no instance behind it.
    pub fn take_read_states(&mut self, states: &[ReadStateSummary]) {
        self.reads.states = states
            .iter()
            .map(|r| (r.channel_id.clone(), r.clone()))
            .collect();
    }

    /// A conversation's read state: how many messages others sent after this account's
    /// position, and how many of those are for it. Nothing for one not counted.
    pub fn unread(&self, channel_id: &str) -> (u32, u32) {
        self.reads
            .states
            .get(channel_id)
            .map_or((0, 0), |r| (r.unread, r.mentions))
    }

    /// Whether anything in a server is unread, and how much of it is for this account: its
    /// channels this account is in and its server DMs.
    pub fn server_unread(&self, server_id: &str) -> (bool, u32) {
        let shown = self
            .channels_of(server_id)
            .chain(self.server_dms_of(server_id));
        shown.fold((false, 0), |(any, mentions), c| {
            let (u, m) = self.unread(&c.channel_id);
            (any || u > 0, mentions + m)
        })
    }

    /// The personal conversations' messages for this account that are unread: every one,
    /// in a DM.
    pub fn personal_unread(&self) -> u32 {
        self.personal_channels()
            .map(|c| self.unread(&c.channel_id).1)
            .sum()
    }

    /// The Dock's count: everything unread that is for this account.
    pub fn badge(&self) -> u32 {
        self.reads.states.values().map(|r| r.mentions).sum()
    }

    /// Where the open conversation's "New messages" line goes: above its first unread
    /// message, the first after where this account had read to that someone else sent (as
    /// the count counts: not a notice, not this account's own).
    pub fn new_line_at(&self) -> Option<&str> {
        let here = self.conversation.as_ref()?.id();
        let (_, read_to) = self.reads.new_since.as_ref().filter(|(id, _)| id == here)?;
        self.entries
            .iter()
            .find(|e| e.seq > *read_to && e.notice.is_empty() && e.author_account_id != self.me())
            .map(|e| e.id.as_str())
    }

    /// Notes where the conversation being opened was read to, for its "New messages" line.
    pub(super) fn note_new_since(&mut self, conversation: &Conversation) {
        let id = conversation.id();
        self.reads.new_since = self
            .reads
            .states
            .get(id)
            .filter(|r| r.unread > 0)
            .map(|r| (id.to_string(), r.read_seq));
        self.reads.land = self.reads.new_since.is_some();
        self.reads.seeking = None;
        self.reads.found = None;
    }

    /// This account has read the open conversation through `seq`, its newest message on
    /// screen in front of the person ([`ReadUpdate::Marked`]). Nothing when that moves
    /// nothing, or when it was marked less than a second ago.
    pub fn mark_read(&mut self, channel_id: &str, seq: u64) {
        let Some(app) = self.client() else { return };
        let state = self.reads.states.get(channel_id);
        let moves = state.is_none_or(|r| seq > r.read_seq || r.unread > 0);
        let recent = self
            .reads
            .marked
            .get(channel_id)
            .is_some_and(|(through, at)| *through >= seq && at.elapsed() < MARK_EVERY);
        if !moves || recent || seq == 0 {
            return;
        }
        self.reads
            .marked
            .insert(channel_id.to_string(), (seq, Instant::now()));
        let channel_id = channel_id.to_string();
        self.task(async move {
            let result = app.mark_read(&channel_id, seq).await;
            Update::Read(ReadUpdate::Marked(result.map_err(Failure::from)))
        });
    }

    /// Reads the first page of a Mentions list, or the next one after what it holds
    /// ([`ReadUpdate::Mentions`]). `None` is every server's.
    pub fn load_mentions(&mut self, server_id: Option<&str>, more: bool) {
        let Some(app) = self.client() else { return };
        let key = server_id.unwrap_or_default().to_string();
        let page = self.reads.mentions.entry(key).or_default();
        if page.loading || (more && !page.more) {
            return;
        }
        page.loading = true;
        let before = if more {
            page.messages.last().map(|m| m.message_id.clone())
        } else {
            None
        };
        let server_id = server_id.map(str::to_string);
        self.task(async move {
            let page = app
                .mentions(server_id.as_deref(), before.as_deref())
                .await
                .map_err(Failure::from);
            Update::Read(ReadUpdate::Mentions {
                server_id,
                more,
                page,
            })
        });
    }

    /// A Mentions list as far as it has been read.
    pub fn mentions_page(&self, server_id: Option<&str>) -> Option<&MentionsPage> {
        self.reads.mentions.get(server_id.unwrap_or_default())
    }

    /// Adds this account's reaction to a message of the open conversation, or takes it away
    /// when it is already there.
    pub fn react(&mut self, message_id: &str, emoji: &str) {
        let (Some(app), Some(conversation)) = (self.client(), self.conversation.clone()) else {
            return;
        };
        let mine = self
            .entries
            .iter()
            .find(|e| e.id == message_id)
            .and_then(|e| e.reactions.iter().find(|r| r.reaction == emoji))
            .is_some_and(|r| r.me);
        match conversation {
            Conversation::Channel(channel_id) => {
                if let Err(error) = app.react(&channel_id, message_id, emoji, mine) {
                    self.last_error = Some(error.to_string());
                }
            }
            Conversation::Dm(channel_id) => {
                let (message_id, emoji) = (message_id.to_string(), emoji.to_string());
                self.task_local(move || async move {
                    let result = app.dm_react(&channel_id, &message_id, &emoji, mine).await;
                    Update::Read(ReadUpdate::Reacted(result.map_err(Failure::from)))
                });
            }
        }
    }

    /// Finds a message of the open conversation, reading further back for it if it is not
    /// among the entries; [`Reads::found`] names it once it is.
    pub fn reveal(&mut self, message_id: &str) {
        self.reads.seeking = Some((message_id.to_string(), SEEK_PAGES));
        self.seek();
    }

    /// Goes on finding the message being found: done when it is among the entries, another
    /// page further back while pages remain, given up otherwise.
    pub(super) fn seek(&mut self) {
        let Some((id, left)) = self.reads.seeking.clone() else {
            return;
        };
        if self.entries.iter().any(|e| e.id == id) {
            self.reads.seeking = None;
            self.reads.found = Some(id);
        } else if left == 0 || self.at_beginning() {
            self.reads.seeking = None;
        } else if !self.loading_older() {
            self.reads.seeking = Some((id, left - 1));
            self.load_older();
            // A page from the local store is read at once.
            if !self.loading_older() {
                self.seek();
            }
        }
    }

    pub(super) fn apply_read(&mut self, update: &ReadUpdate) {
        match update {
            ReadUpdate::Marked(Ok(_)) => self.refresh_reads(),
            ReadUpdate::Marked(Err(failure)) | ReadUpdate::Reacted(Err(failure)) => {
                self.last_error = Some(failure.message.clone());
            }
            ReadUpdate::Mentions {
                server_id,
                more,
                page,
            } => {
                let list = self
                    .reads
                    .mentions
                    .entry(server_id.clone().unwrap_or_default())
                    .or_default();
                list.loading = false;
                match page {
                    Ok((messages, has_more)) => {
                        if !*more {
                            list.messages.clear();
                        }
                        list.messages.extend(messages.iter().cloned());
                        list.more = *has_more;
                        list.error = None;
                    }
                    Err(failure) => list.error = Some(failure.message.clone()),
                }
                let strangers: Vec<String> = list
                    .messages
                    .iter()
                    .map(|m| m.author_account_id.clone())
                    .collect();
                for id in strangers {
                    self.look_up(&id);
                }
            }
            ReadUpdate::Reacted(Ok(())) => {}
        }
    }
}

/// The quote of an entry: the whole of its body, or the part of it chosen, trimmed and kept
/// to the 2,000 characters a quote may hold.
pub fn quote_of(entry: &Entry, part: Option<&str>) -> QuoteSummary {
    let text = part.unwrap_or(&entry.body).trim();
    QuoteSummary {
        message_id: entry.id.clone(),
        author_account_id: entry.author_account_id.clone(),
        text: text.chars().take(2_000).collect(),
        original_deleted: false,
        silent: false,
    }
}
