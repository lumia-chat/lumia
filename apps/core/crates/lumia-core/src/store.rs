//! The local-store contract (architecture 13, protocol section 5).
//!
//! The store is the client's source of truth: raw event bytes per log and
//! sequence number, the cursor per log, materialized messages for display,
//! and the outbox of commands not yet acknowledged. The one rule that
//! matters is in [`Store::apply`]: events and the cursor commit together.

use crate::wire::LogId;

/// Which kind of log an id names.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LogKind {
    /// A channel or DM log carrying `lumia.v1.Event`.
    Channel,
    /// The account log carrying `lumia.v1.AccountEvent`.
    Account,
}

/// One event as stored: its sequence number and the exact wire bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredEvent {
    /// Position in the log.
    pub seq: u64,
    /// Serialized `Event` or `AccountEvent`, as received.
    pub bytes: Vec<u8>,
}

/// A message row materialized from events, for display.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredMessage {
    /// The message id (also the create command's id).
    pub message_id: Vec<u8>,
    /// The channel it belongs to.
    pub channel_id: LogId,
    /// Sequence number of its create event.
    pub seq: u64,
    /// Author account id.
    pub author_account_id: Vec<u8>,
    /// Current body (after edits); empty when deleted.
    pub body: String,
    /// Creation time, microseconds since the epoch.
    pub created_at_micros: i64,
    /// Whether the message has been deleted.
    pub deleted: bool,
    /// Whether its author has edited it.
    pub edited: bool,
    /// A notice's kind as the protocol names it in lower case (`joined`,
    /// `added`, `left`, `removed`); empty for a message (architecture 6.8).
    pub notice: String,
    /// Who a notice is about; empty for a message.
    pub notice_account_id: Vec<u8>,
    /// What it quotes (architecture 6.6).
    pub reply: Option<StoredQuote>,
    /// Whom it mentions, and where in its body.
    pub mentions: Vec<StoredMention>,
    /// Its reactions, each emoji with who reacted, in the order first used.
    pub reactions: Vec<StoredReaction>,
}

/// A quote of another message of the conversation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredQuote {
    /// The message quoted.
    pub message_id: Vec<u8>,
    /// Its author.
    pub author_account_id: Vec<u8>,
    /// What of it was quoted; empty once it is deleted.
    pub text: String,
    /// The message quoted was deleted, and the text with it.
    pub original_deleted: bool,
    /// Sent without notifying the author quoted.
    pub silent: bool,
}

/// A mention: an account (empty for everyone) and its byte range in the body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredMention {
    /// The account mentioned; empty when it is everyone.
    pub account_id: Vec<u8>,
    /// Whether it mentions everyone.
    pub everyone: bool,
    /// Byte offsets into the body, half-open.
    pub start: u32,
    /// End of the range.
    pub end: u32,
}

/// One emoji's reactions to a message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredReaction {
    /// The emoji.
    pub reaction: String,
    /// Who reacted with it.
    pub account_ids: Vec<Vec<u8>>,
}

impl StoredMessage {
    /// A message as the instance gave it, in an event or a snapshot.
    #[must_use]
    pub fn of(m: &crate::proto::Message) -> Self {
        let (notice, notice_account_id) = notice_of(m);
        Self {
            message_id: m.message_id.clone(),
            channel_id: m.channel_id.clone(),
            seq: m.seq,
            author_account_id: m.author_account_id.clone(),
            body: m.body.clone(),
            created_at_micros: crate::wire::micros(m.created_at.as_ref()).unwrap_or(0),
            deleted: m.deleted_at.is_some(),
            edited: m.edited_at.is_some(),
            notice,
            notice_account_id,
            reply: m.reply_to.as_ref().map(|q| StoredQuote {
                message_id: q.message_id.clone(),
                author_account_id: q.author_account_id.clone(),
                text: q.text.clone(),
                original_deleted: q.original_deleted,
                silent: q.silent,
            }),
            mentions: m.mentions.iter().filter_map(mention_of).collect(),
            reactions: m
                .reactions
                .iter()
                .map(|r| StoredReaction {
                    reaction: r.reaction.clone(),
                    account_ids: r.account_ids.clone(),
                })
                .collect(),
        }
    }
}

/// A mention as the store keeps it; none for one of a role.
#[must_use]
pub fn mention_of(m: &crate::proto::Mention) -> Option<StoredMention> {
    use crate::proto::mention::Target;
    let (account_id, everyone) = match m.target.as_ref()? {
        Target::AccountId(id) => (id.clone(), false),
        Target::Everyone(true) => (Vec::new(), true),
        _ => return None,
    };
    Some(StoredMention {
        account_id,
        everyone,
        start: m.start,
        end: m.end,
    })
}

/// A message's notice, as [`StoredMessage`] keeps it: its kind in lower
/// case and who it is about, or two empties for a message.
#[must_use]
pub fn notice_of(m: &crate::proto::Message) -> (String, Vec<u8>) {
    match &m.notice {
        Some(n) => (
            crate::proto::channel_notice::Kind::try_from(n.kind)
                .map(|k| k.as_str_name().to_lowercase())
                .unwrap_or_default(),
            n.account_id.clone(),
        ),
        None => (String::new(), Vec::new()),
    }
}

/// A command waiting for its acknowledgment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutboxEntry {
    /// The client-minted command id.
    pub command_id: Vec<u8>,
    /// The channel the command targets.
    pub channel_id: LogId,
    /// Serialized `lumia.v1.Command`, resent verbatim on retry.
    pub bytes: Vec<u8>,
}

/// The local store for one instance.
pub trait Store {
    /// The cursor (highest applied `seq`) for each known log.
    fn cursors(&self, kind: LogKind) -> Result<Vec<(LogId, u64)>, StoreError>;

    /// Applies events to a log and advances its cursor to the last one, in
    /// one transaction. Events must be in `seq` order and contiguous with
    /// the cursor; the implementation materializes messages as it goes.
    fn apply(
        &mut self,
        kind: LogKind,
        log_id: &[u8],
        events: &[StoredEvent],
    ) -> Result<(), StoreError>;

    /// Replaces a channel's state from a snapshot: drops local events and
    /// messages, stores the given messages, and sets the cursor to `seq`.
    fn apply_snapshot(
        &mut self,
        log_id: &[u8],
        seq: u64,
        messages: &[StoredMessage],
    ) -> Result<(), StoreError>;

    /// Replaces the account log's state from a snapshot: drops local account
    /// events and sets the account cursor to `seq`. The snapshot's content
    /// (servers, memberships, channels) is the application's to keep; the
    /// store only has to agree with the session on where the log stands,
    /// or the next live account event is a gap it refuses.
    fn apply_account_snapshot(&mut self, account_id: &[u8], seq: u64) -> Result<(), StoreError>;

    /// Drops every cursor and every event (recovery generation changed).
    /// Implementations also drop any per-log progress they keep elsewhere,
    /// since sequence numbers restart with the new generation.
    fn discard_all(&mut self) -> Result<(), StoreError>;

    /// The recovery generation last seen for this instance.
    fn generation(&self) -> Result<Option<u64>, StoreError>;
    /// Records the recovery generation.
    fn set_generation(&mut self, generation: u64) -> Result<(), StoreError>;

    /// The newest messages in a channel, oldest first, up to `limit`.
    fn messages(&self, channel_id: &[u8], limit: usize) -> Result<Vec<StoredMessage>, StoreError>;

    /// The messages of a channel with `seq` strictly below `before_seq`, the
    /// newest `limit` of them, oldest first: the page above one already read.
    fn messages_before(
        &self,
        channel_id: &[u8],
        before_seq: u64,
        limit: usize,
    ) -> Result<Vec<StoredMessage>, StoreError>;

    /// Adds a command to the outbox.
    fn outbox_push(&mut self, entry: &OutboxEntry) -> Result<(), StoreError>;
    /// Removes an acknowledged command from the outbox.
    fn outbox_remove(&mut self, command_id: &[u8]) -> Result<(), StoreError>;
    /// Every command still waiting, oldest first.
    fn outbox(&self) -> Result<Vec<OutboxEntry>, StoreError>;

    /// A small key-value area for instance facts (host, account id).
    fn meta_get(&self, key: &str) -> Result<Option<String>, StoreError>;
    /// Writes an instance fact.
    fn meta_set(&mut self, key: &str, value: &str) -> Result<(), StoreError>;
}

/// Why a store operation failed.
#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    /// The events were not contiguous with the cursor.
    #[error("events not contiguous: cursor {cursor}, first seq {first}")]
    NotContiguous {
        /// The cursor before the call.
        cursor: u64,
        /// The first sequence number offered.
        first: u64,
    },
    /// Stored bytes did not decode as the expected message.
    #[error("corrupt stored event at seq {0}")]
    Corrupt(u64),
    /// The underlying database failed.
    #[error("store error: {0}")]
    Backend(String),
}
