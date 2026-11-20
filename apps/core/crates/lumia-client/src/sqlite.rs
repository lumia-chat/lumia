//! The native store: SQLite through `rusqlite` with the bundled engine.
//!
//! Raw event bytes per log and sequence are the local source of truth;
//! messages are materialized from them. [`Store::apply`] writes the events,
//! the materialized rows, and the cursor in one transaction, which is the
//! rule from docs/protocol.md section 5.

use std::path::Path;

use lumia_core::keystore::{Keystore, KeystoreError};
use lumia_core::mls::storage::MlsKv;
use lumia_core::proto::{event, AccountEvent, Attachment, Card, Event};
use lumia_core::store::{
    LogKind, OutboxEntry, Store, StoreError, StoredEvent, StoredMention, StoredMessage,
    StoredQuote, StoredReaction,
};
use prost::Message as _;
use rusqlite::{params, Connection, OptionalExtension};

const SCHEMA: &str = r"
CREATE TABLE IF NOT EXISTS meta (
  key TEXT PRIMARY KEY,
  value TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS cursors (
  kind INTEGER NOT NULL,
  log_id BLOB NOT NULL,
  seq INTEGER NOT NULL,
  PRIMARY KEY (kind, log_id)
);
CREATE TABLE IF NOT EXISTS events (
  kind INTEGER NOT NULL,
  log_id BLOB NOT NULL,
  seq INTEGER NOT NULL,
  bytes BLOB NOT NULL,
  PRIMARY KEY (kind, log_id, seq)
);
CREATE TABLE IF NOT EXISTS messages (
  message_id BLOB PRIMARY KEY,
  channel_id BLOB NOT NULL,
  seq INTEGER NOT NULL,
  author_account_id BLOB NOT NULL,
  body TEXT NOT NULL,
  created_at_micros INTEGER NOT NULL,
  deleted INTEGER NOT NULL DEFAULT 0,
  notice TEXT NOT NULL DEFAULT '',
  notice_account_id BLOB NOT NULL DEFAULT x'',
  reply_to_id BLOB NOT NULL DEFAULT x'',
  reply_author_id BLOB NOT NULL DEFAULT x'',
  reply_text TEXT NOT NULL DEFAULT '',
  reply_deleted INTEGER NOT NULL DEFAULT 0,
  mentions TEXT NOT NULL DEFAULT '[]',
  edited INTEGER NOT NULL DEFAULT 0,
  reply_silent INTEGER NOT NULL DEFAULT 0,
  attachments BLOB NOT NULL DEFAULT x'',
  cards BLOB NOT NULL DEFAULT x''
);
CREATE TABLE IF NOT EXISTS reactions (
  message_id BLOB NOT NULL,
  reaction TEXT NOT NULL,
  account_id BLOB NOT NULL,
  position INTEGER NOT NULL,
  PRIMARY KEY (message_id, reaction, account_id)
);
CREATE INDEX IF NOT EXISTS messages_channel_seq ON messages (channel_id, seq);
CREATE TABLE IF NOT EXISTS outbox (
  command_id BLOB PRIMARY KEY,
  channel_id BLOB NOT NULL,
  bytes BLOB NOT NULL,
  queued_at INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS mls_kv (
  kind TEXT NOT NULL,
  key BLOB NOT NULL,
  value BLOB NOT NULL,
  PRIMARY KEY (kind, key)
);
CREATE TABLE IF NOT EXISTS mls_list (
  kind TEXT NOT NULL,
  key BLOB NOT NULL,
  position INTEGER NOT NULL,
  value BLOB NOT NULL,
  PRIMARY KEY (kind, key, position)
);
CREATE TABLE IF NOT EXISTS secrets (
  name TEXT PRIMARY KEY,
  value BLOB NOT NULL
);
CREATE TABLE IF NOT EXISTS dm_operations (
  channel_id BLOB NOT NULL,
  seq INTEGER NOT NULL,
  sender_identity_id BLOB NOT NULL,
  sender_device_id BLOB NOT NULL,
  epoch INTEGER NOT NULL,
  status INTEGER NOT NULL,
  bytes BLOB NOT NULL,
  PRIMARY KEY (channel_id, seq)
);
CREATE TABLE IF NOT EXISTS drafts (
  channel_id BLOB PRIMARY KEY,
  body TEXT NOT NULL,
  reply TEXT NOT NULL DEFAULT '',
  updated_at_micros INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS kept_members (
  server_id TEXT NOT NULL,
  account_id TEXT NOT NULL,
  place INTEGER NOT NULL,
  member TEXT NOT NULL,
  PRIMARY KEY (server_id, account_id)
) WITHOUT ROWID;
CREATE INDEX IF NOT EXISTS kept_members_account ON kept_members (account_id);
CREATE TABLE IF NOT EXISTS kept_member_lists (
  server_id TEXT PRIMARY KEY,
  stale INTEGER NOT NULL DEFAULT 0
) WITHOUT ROWID;
";

/// How long a connection waits for another's write to end before it gives
/// up as busy.
pub const BUSY_WAIT: std::time::Duration = std::time::Duration::from_secs(10);

/// Where every server's member list was kept as one value before each
/// member had a row (`kept_members`): dropped at open, and the lists are
/// fetched again as they are asked for.
const MEMBERS_AS_ONE_VALUE: &str = "kept_members";

/// One row of a conversation's decrypted log (e2ee spec section 5).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DmRow {
    /// Position in the channel log.
    pub seq: u64,
    /// The sending identity.
    pub sender_identity_id: Vec<u8>,
    /// The sending device.
    pub sender_device_id: Vec<u8>,
    /// The epoch it was encrypted in.
    pub epoch: u64,
    /// What happened to it.
    pub status: DmStatus,
    /// The serialized `DmOperation` when decrypted, else empty.
    pub bytes: Vec<u8>,
}

/// What became of one encrypted event.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DmStatus {
    /// Decrypted; `bytes` is the operation.
    Decrypted,
    /// Sent before this device joined (spec section 5).
    BeforeJoin,
    /// Could not be decrypted in its epoch, or its leaf was rejected.
    Undecryptable,
}

impl DmStatus {
    fn code(self) -> i64 {
        match self {
            Self::Decrypted => 0,
            Self::BeforeJoin => 1,
            Self::Undecryptable => 2,
        }
    }

    fn from_code(code: i64) -> Self {
        match code {
            0 => Self::Decrypted,
            1 => Self::BeforeJoin,
            _ => Self::Undecryptable,
        }
    }
}

/// A SQLite-backed store for one instance.
#[derive(Debug)]
pub struct SqliteStore {
    conn: Connection,
}

impl SqliteStore {
    /// Fills in each message's reactions, each emoji with who reacted, in the
    /// order it was first used.
    fn with_reactions(&self, messages: &mut [StoredMessage]) -> Result<(), StoreError> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT reaction, account_id, MIN(position) OVER (PARTITION BY reaction) AS first
                 FROM reactions WHERE message_id = ?1 ORDER BY first, position",
            )
            .map_err(backend)?;
        for m in messages.iter_mut() {
            let rows = stmt
                .query_map(params![m.message_id], |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, Vec<u8>>(1)?))
                })
                .map_err(backend)?;
            let mut reactions: Vec<StoredReaction> = Vec::new();
            for row in rows {
                let (reaction, account) = row.map_err(backend)?;
                match reactions.iter_mut().find(|r| r.reaction == reaction) {
                    Some(r) => r.account_ids.push(account),
                    None => reactions.push(StoredReaction {
                        reaction,
                        account_ids: vec![account],
                    }),
                }
            }
            m.reactions = reactions;
        }
        Ok(())
    }

    /// Opens (creating) the database file and applies the schema.
    ///
    /// # Errors
    /// When the file cannot be opened or the schema fails.
    pub fn open(path: &Path) -> Result<Self, StoreError> {
        let conn = Connection::open(path).map_err(backend)?;
        Self::init(conn)
    }

    /// Opens the database on a named SQLite VFS: the OPFS pool on the web,
    /// where the file lives in the origin's private file system.
    ///
    /// # Errors
    /// When the VFS is not registered, the file cannot be opened, or the
    /// schema fails.
    pub fn open_on_vfs(path: &str, vfs: &str) -> Result<Self, StoreError> {
        Self::open_on_vfs_keyed(path, vfs, None)
    }

    /// `open_on_vfs` with the file encrypted at rest under `key` (32 raw
    /// bytes; ChaCha20-Poly1305 pages through SQLite3 Multiple Ciphers),
    /// which the build must link; without a key the file is plain.
    ///
    /// # Errors
    /// When the VFS is not registered, the file cannot be opened, the key
    /// does not open it, or the schema fails.
    pub fn open_on_vfs_keyed(
        path: &str,
        vfs: &str,
        key: Option<&[u8]>,
    ) -> Result<Self, StoreError> {
        let flags =
            rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE | rusqlite::OpenFlags::SQLITE_OPEN_CREATE;
        let conn = Connection::open_with_flags_and_vfs(path, flags, vfs).map_err(backend)?;
        if let Some(key) = key {
            // Before the first read: the cipher, then the raw key.
            conn.execute_batch(&format!(
                "PRAGMA cipher = 'chacha20'; PRAGMA hexkey = '{}';",
                hex::encode(key)
            ))
            .map_err(backend)?;
        }
        Self::init(conn)
    }

    /// An in-memory store, for tests.
    ///
    /// # Errors
    /// When the schema fails.
    pub fn in_memory() -> Result<Self, StoreError> {
        let conn = Connection::open_in_memory().map_err(backend)?;
        Self::init(conn)
    }

    fn init(conn: Connection) -> Result<Self, StoreError> {
        // Two connections share one file (the gateway's and the DM engine's);
        // writers wait for each other instead of failing.
        conn.busy_timeout(BUSY_WAIT).map_err(backend)?;
        conn.execute_batch("PRAGMA journal_mode = WAL; PRAGMA foreign_keys = ON;")
            .map_err(backend)?;
        conn.execute_batch(SCHEMA).map_err(backend)?;
        // Looked for first: an open that finds none writes nothing, and so
        // waits for no other writer.
        let as_one_value = conn
            .prepare("SELECT 1 FROM meta WHERE key = ?1")
            .and_then(|mut stmt| stmt.exists(params![MEMBERS_AS_ONE_VALUE]))
            .map_err(backend)?;
        if as_one_value {
            conn.execute(
                "DELETE FROM meta WHERE key = ?1",
                params![MEMBERS_AS_ONE_VALUE],
            )
            .map_err(backend)?;
        }
        add_missing_columns(&conn)?;
        refetch_what_was_kept_short(&conn)?;
        Ok(Self { conn })
    }

    /// Runs `f` inside a savepoint: everything it writes through `&self`
    /// commits together or not at all. Nests.
    ///
    /// # Errors
    /// The closure's error, after rolling back; or a database failure.
    pub fn transaction<T, E: From<StoreError>>(
        &self,
        f: impl FnOnce(&Self) -> Result<T, E>,
    ) -> Result<T, E> {
        // The outermost level takes the write lock up front: a deferred
        // transaction that reads first and writes later cannot wait out a
        // concurrent writer under WAL (SQLITE_BUSY_SNAPSHOT), an immediate
        // one can. Nested levels are savepoints.
        let (begin, commit, rollback) = if self.conn.is_autocommit() {
            ("BEGIN IMMEDIATE", "COMMIT", "ROLLBACK")
        } else {
            (
                "SAVEPOINT lumia",
                "RELEASE lumia",
                "ROLLBACK TO lumia; RELEASE lumia",
            )
        };
        self.conn.execute_batch(begin).map_err(backend)?;
        match f(self) {
            Ok(value) => {
                self.conn.execute_batch(commit).map_err(backend)?;
                Ok(value)
            }
            Err(e) => {
                let _ = self.conn.execute_batch(rollback);
                Err(e)
            }
        }
    }

    /// Begins a write transaction if the write lock is free at this moment,
    /// and says whether it did: `false` when another connection holds it,
    /// with nothing waited for. What then runs on this connection waits for
    /// nobody, until [`SqliteStore::end`]. For a caller that holds a lock of
    /// its own, which it must not hold through SQLite's wait: it lets go,
    /// and tries again.
    ///
    /// # Errors
    /// On a database failure other than the lock being held.
    pub fn begin_now(&self) -> Result<bool, StoreError> {
        self.conn
            .busy_timeout(std::time::Duration::ZERO)
            .map_err(backend)?;
        let begun = self.conn.execute_batch("BEGIN IMMEDIATE");
        self.conn.busy_timeout(BUSY_WAIT).map_err(backend)?;
        match begun {
            Ok(()) => Ok(true),
            Err(rusqlite::Error::SqliteFailure(e, _))
                if matches!(
                    e.code,
                    rusqlite::ErrorCode::DatabaseBusy | rusqlite::ErrorCode::DatabaseLocked
                ) =>
            {
                Ok(false)
            }
            Err(e) => Err(backend(e)),
        }
    }

    /// Ends the transaction [`SqliteStore::begin_now`] began: what was
    /// written in it is kept (`commit`) or undone.
    ///
    /// # Errors
    /// On a database failure.
    pub fn end(&self, commit: bool) -> Result<(), StoreError> {
        self.conn
            .execute_batch(if commit { "COMMIT" } else { "ROLLBACK" })
            .map_err(backend)
    }

    /// Writes an instance fact through a shared reference (for use inside
    /// [`SqliteStore::transaction`]).
    ///
    /// # Errors
    /// On a database failure.
    pub fn set_meta(&self, key: &str, value: &str) -> Result<(), StoreError> {
        self.conn
            .execute(
                "INSERT INTO meta (key, value) VALUES (?1, ?2) ON CONFLICT (key) DO UPDATE SET value = excluded.value",
                params![key, value],
            )
            .map(|_| ())
            .map_err(backend)
    }

    /// Erases everything this store holds, every row of every table, with the
    /// freed pages overwritten and given back: what a revoked device does
    /// with what it kept (identity spec 4.4). The tables stay, empty.
    ///
    /// # Errors
    /// On a database failure.
    pub fn erase(&self) -> Result<(), StoreError> {
        self.conn
            .execute_batch("PRAGMA secure_delete = ON;")
            .map_err(backend)?;
        let tables: Vec<String> = {
            let mut stmt = self
                .conn
                .prepare("SELECT name FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%'")
                .map_err(backend)?;
            let names = stmt
                .query_map([], |row| row.get::<_, String>(0))
                .map_err(backend)?
                .collect::<Result<Vec<_>, _>>()
                .map_err(backend)?;
            names
        };
        self.transaction(|store: &Self| {
            for table in &tables {
                store
                    .conn
                    .execute(&format!("DELETE FROM \"{table}\""), [])
                    .map_err(backend)?;
            }
            Ok::<_, StoreError>(())
        })?;
        // Overwriting freed pages is for this; the store is used again after.
        self.conn
            .execute_batch("PRAGMA wal_checkpoint(TRUNCATE); VACUUM; PRAGMA secure_delete = OFF;")
            .map_err(backend)
    }

    /// Forgets what an account that signed out leaves here: its account log
    /// (the cursor with the events, so the next account starts from the
    /// beginning of its own), the commands it had not sent, which would
    /// otherwise go out at the next welcome as whoever signs in next, and
    /// what it had typed and not sent. The channels' logs stay: they are the channels', not the account's, and a
    /// personal conversation's decrypted history could not be fetched again.
    ///
    /// # Errors
    /// On a database failure.
    pub fn forget_account(&self) -> Result<(), StoreError> {
        let account = kind_code(LogKind::Account);
        self.transaction(|store: &Self| {
            store
                .conn
                .execute("DELETE FROM cursors WHERE kind = ?1", params![account])
                .map_err(backend)?;
            store
                .conn
                .execute("DELETE FROM events WHERE kind = ?1", params![account])
                .map_err(backend)?;
            store
                .conn
                .execute("DELETE FROM outbox", [])
                .map_err(backend)?;
            store.forget_drafts()?;
            Ok::<_, StoreError>(())
        })
    }

    /// What was typed into a conversation's composer and not sent: the
    /// words, and the reply they were being written to as the caller
    /// encoded it (empty for none). This device's alone: nothing here is
    /// ever sent to the instance.
    ///
    /// # Errors
    /// On a database failure.
    pub fn draft(&self, channel_id: &[u8]) -> Result<Option<(String, String)>, StoreError> {
        self.conn
            .query_row(
                "SELECT body, reply FROM drafts WHERE channel_id = ?1",
                params![channel_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()
            .map_err(backend)
    }

    /// Keeps a conversation's draft in place of the one before.
    ///
    /// # Errors
    /// On a database failure.
    pub fn set_draft(
        &self,
        channel_id: &[u8],
        body: &str,
        reply: &str,
        now_micros: i64,
    ) -> Result<(), StoreError> {
        self.conn
            .execute(
                "INSERT INTO drafts (channel_id, body, reply, updated_at_micros) VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT (channel_id) DO UPDATE SET body = excluded.body, reply = excluded.reply,
                   updated_at_micros = excluded.updated_at_micros",
                params![channel_id, body, reply, now_micros],
            )
            .map(|_| ())
            .map_err(backend)
    }

    /// Forgets a conversation's draft: it was sent, or its field emptied.
    ///
    /// # Errors
    /// On a database failure.
    pub fn clear_draft(&self, channel_id: &[u8]) -> Result<(), StoreError> {
        self.conn
            .execute(
                "DELETE FROM drafts WHERE channel_id = ?1",
                params![channel_id],
            )
            .map(|_| ())
            .map_err(backend)
    }

    /// Forgets every conversation's draft: whoever typed them is gone, or
    /// may not be who is here next.
    ///
    /// # Errors
    /// On a database failure.
    pub fn forget_drafts(&self) -> Result<(), StoreError> {
        self.conn
            .execute("DELETE FROM drafts", [])
            .map(|_| ())
            .map_err(backend)
    }

    /// The members last listed for a server, each as the caller encoded
    /// it, in the order they were listed. A copy of what the instance
    /// holds, kept so a launch can name people before it answers: each
    /// member is a row, so a change to one is one row written, however
    /// large the server.
    ///
    /// # Errors
    /// On a database failure.
    pub fn kept_members(&self, server_id: &str) -> Result<Vec<String>, StoreError> {
        let mut stmt = self
            .conn
            .prepare_cached("SELECT member FROM kept_members WHERE server_id = ?1 ORDER BY place")
            .map_err(backend)?;
        let rows = stmt
            .query_map(params![server_id], |row| row.get(0))
            .map_err(backend)?;
        rows.collect::<Result<_, _>>().map_err(backend)
    }

    /// One member of a server's kept list; `None` when the list has no
    /// such member, or there is no list.
    ///
    /// # Errors
    /// On a database failure.
    pub fn kept_member(
        &self,
        server_id: &str,
        account_id: &str,
    ) -> Result<Option<String>, StoreError> {
        self.conn
            .prepare_cached(
                "SELECT member FROM kept_members WHERE server_id = ?1 AND account_id = ?2",
            )
            .and_then(|mut stmt| {
                stmt.query_row(params![server_id, account_id], |row| row.get(0))
                    .optional()
            })
            .map_err(backend)
    }

    /// An account as each kept list has it: the server and the member.
    ///
    /// # Errors
    /// On a database failure.
    pub fn kept_memberships(&self, account_id: &str) -> Result<Vec<(String, String)>, StoreError> {
        let mut stmt = self
            .conn
            .prepare_cached("SELECT server_id, member FROM kept_members WHERE account_id = ?1")
            .map_err(backend)?;
        let rows = stmt
            .query_map(params![account_id], |row| Ok((row.get(0)?, row.get(1)?)))
            .map_err(backend)?;
        rows.collect::<Result<_, _>>().map_err(backend)
    }

    /// Keeps a server's member list in place of the one before: `members`
    /// as `(account_id, member)`, in order. `read_at` is what
    /// [`SqliteStore::member_list_staleness`] answered before the list was
    /// asked for: a list said to be stale since then stays so, because
    /// what was read may be from before what made it stale.
    ///
    /// # Errors
    /// On a database failure; the list before is then still whole.
    pub fn keep_member_list(
        &self,
        server_id: &str,
        members: &[(String, String)],
        read_at: u64,
    ) -> Result<(), StoreError> {
        self.transaction(|store: &Self| {
            store
                .conn
                .execute(
                    "DELETE FROM kept_members WHERE server_id = ?1",
                    params![server_id],
                )
                .map_err(backend)?;
            let mut insert = store
                .conn
                .prepare_cached(
                    "INSERT OR REPLACE INTO kept_members (server_id, account_id, place, member)
                     VALUES (?1, ?2, ?3, ?4)",
                )
                .map_err(backend)?;
            for (place, (account_id, member)) in members.iter().enumerate() {
                insert
                    .execute(params![
                        server_id,
                        account_id,
                        i64::try_from(place).unwrap_or(i64::MAX),
                        member
                    ])
                    .map_err(backend)?;
            }
            store
                .conn
                .execute(
                    "INSERT INTO kept_member_lists (server_id, stale) VALUES (?1, 0)
                     ON CONFLICT (server_id) DO UPDATE SET
                       stale = CASE WHEN stale = ?2 THEN 0 ELSE stale END",
                    params![server_id, i64::try_from(read_at).unwrap_or(i64::MAX)],
                )
                .map_err(backend)?;
            Ok::<_, StoreError>(())
        })
    }

    /// Writes one member of a kept list over what it had of them; a member
    /// the list does not have is not added.
    ///
    /// # Errors
    /// On a database failure.
    pub fn keep_member(
        &self,
        server_id: &str,
        account_id: &str,
        member: &str,
    ) -> Result<(), StoreError> {
        self.conn
            .prepare_cached(
                "UPDATE kept_members SET member = ?3 WHERE server_id = ?1 AND account_id = ?2",
            )
            .and_then(|mut stmt| stmt.execute(params![server_id, account_id, member]))
            .map(|_| ())
            .map_err(backend)
    }

    /// Takes a member out of a server's kept list.
    ///
    /// # Errors
    /// On a database failure.
    pub fn drop_kept_member(&self, server_id: &str, account_id: &str) -> Result<(), StoreError> {
        self.conn
            .execute(
                "DELETE FROM kept_members WHERE server_id = ?1 AND account_id = ?2",
                params![server_id, account_id],
            )
            .map(|_| ())
            .map_err(backend)
    }

    /// Forgets a server's kept list: the account is no longer in it.
    ///
    /// # Errors
    /// On a database failure.
    pub fn drop_member_list(&self, server_id: &str) -> Result<(), StoreError> {
        self.transaction(|store: &Self| {
            store
                .conn
                .execute(
                    "DELETE FROM kept_members WHERE server_id = ?1",
                    params![server_id],
                )
                .map_err(backend)?;
            store
                .conn
                .execute(
                    "DELETE FROM kept_member_lists WHERE server_id = ?1",
                    params![server_id],
                )
                .map_err(backend)?;
            Ok::<_, StoreError>(())
        })
    }

    /// Says every kept member list may be out of date: the account log was
    /// taken from a snapshot, and what it would have said of their members
    /// meanwhile was never heard. Each stays so until it is listed again
    /// ([`SqliteStore::keep_member_list`]).
    ///
    /// # Errors
    /// On a database failure.
    pub fn mark_member_lists_stale(&self) -> Result<(), StoreError> {
        self.conn
            .execute("UPDATE kept_member_lists SET stale = stale + 1", [])
            .map(|_| ())
            .map_err(backend)
    }

    /// Says one server's kept member list may be out of date (a guest let
    /// into a channel there sees more of its members than it was listed).
    /// A server with no kept list is left with none.
    ///
    /// # Errors
    /// On a database failure.
    pub fn mark_member_list_stale(&self, server_id: &str) -> Result<(), StoreError> {
        self.conn
            .execute(
                "UPDATE kept_member_lists SET stale = stale + 1 WHERE server_id = ?1",
                [server_id],
            )
            .map(|_| ())
            .map_err(backend)
    }

    /// The servers whose kept member list is to be listed again.
    ///
    /// # Errors
    /// On a database failure.
    pub fn stale_member_lists(&self) -> Result<Vec<String>, StoreError> {
        let mut stmt = self
            .conn
            .prepare("SELECT server_id FROM kept_member_lists WHERE stale > 0 ORDER BY server_id")
            .map_err(backend)?;
        let rows = stmt.query_map([], |row| row.get(0)).map_err(backend)?;
        rows.collect::<Result<_, _>>().map_err(backend)
    }

    /// How many times a server's kept list has been said to be stale since
    /// it was last listed; 0 for one that is current, and for none.
    ///
    /// # Errors
    /// On a database failure.
    pub fn member_list_staleness(&self, server_id: &str) -> Result<u64, StoreError> {
        self.conn
            .query_row(
                "SELECT stale FROM kept_member_lists WHERE server_id = ?1",
                params![server_id],
                |row| row.get::<_, i64>(0),
            )
            .optional()
            .map(|stale| stale.unwrap_or(0).max(0).unsigned_abs())
            .map_err(backend)
    }

    /// Forgets every kept member list: whoever they were listed for is gone.
    ///
    /// # Errors
    /// On a database failure.
    pub fn forget_kept_members(&self) -> Result<(), StoreError> {
        self.conn
            .execute_batch("DELETE FROM kept_members; DELETE FROM kept_member_lists;")
            .map_err(backend)
    }

    /// Adds to the outbox through a shared reference.
    ///
    /// # Errors
    /// On a database failure.
    pub fn push_outbox(&self, entry: &OutboxEntry) -> Result<(), StoreError> {
        self.conn
            .execute(
                "INSERT OR REPLACE INTO outbox (command_id, channel_id, bytes, queued_at) VALUES (?1, ?2, ?3, ?4)",
                params![entry.command_id, entry.channel_id, entry.bytes, now_micros()],
            )
            .map(|_| ())
            .map_err(backend)
    }

    /// Every stored event of a log, in order.
    ///
    /// # Errors
    /// On a database failure.
    pub fn stored_events(
        &self,
        kind: LogKind,
        log_id: &[u8],
    ) -> Result<Vec<StoredEvent>, StoreError> {
        let mut stmt = self
            .conn
            .prepare("SELECT seq, bytes FROM events WHERE kind = ?1 AND log_id = ?2 ORDER BY seq")
            .map_err(backend)?;
        let rows = stmt
            .query_map(params![kind_code(kind), log_id], |row| {
                let seq: i64 = row.get(0)?;
                Ok(StoredEvent {
                    seq: seq.max(0).unsigned_abs(),
                    bytes: row.get(1)?,
                })
            })
            .map_err(backend)?;
        rows.collect::<Result<_, _>>().map_err(backend)
    }

    /// Every stored account event, in order, regardless of account id.
    ///
    /// # Errors
    /// On a database failure.
    pub fn account_events(&self) -> Result<Vec<StoredEvent>, StoreError> {
        let mut stmt = self
            .conn
            .prepare("SELECT seq, bytes FROM events WHERE kind = 1 ORDER BY seq")
            .map_err(backend)?;
        let rows = stmt
            .query_map([], |row| {
                let seq: i64 = row.get(0)?;
                Ok(StoredEvent {
                    seq: seq.max(0).unsigned_abs(),
                    bytes: row.get(1)?,
                })
            })
            .map_err(backend)?;
        rows.collect::<Result<_, _>>().map_err(backend)
    }

    /// Records what became of one encrypted event.
    ///
    /// # Errors
    /// On a database failure.
    pub fn put_dm_row(&self, channel_id: &[u8], row: &DmRow) -> Result<(), StoreError> {
        self.conn
            .execute(
                "INSERT OR REPLACE INTO dm_operations
                 (channel_id, seq, sender_identity_id, sender_device_id, epoch, status, bytes)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![
                    channel_id,
                    to_i64(row.seq),
                    row.sender_identity_id,
                    row.sender_device_id,
                    to_i64(row.epoch),
                    row.status.code(),
                    row.bytes
                ],
            )
            .map(|_| ())
            .map_err(backend)
    }

    /// A conversation's decrypted log, in seq order.
    ///
    /// # Errors
    /// On a database failure.
    pub fn dm_rows(&self, channel_id: &[u8]) -> Result<Vec<DmRow>, StoreError> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT seq, sender_identity_id, sender_device_id, epoch, status, bytes
                 FROM dm_operations WHERE channel_id = ?1 ORDER BY seq",
            )
            .map_err(backend)?;
        let rows = stmt
            .query_map(params![channel_id], |row| {
                let seq: i64 = row.get(0)?;
                let epoch: i64 = row.get(3)?;
                Ok(DmRow {
                    seq: seq.max(0).unsigned_abs(),
                    sender_identity_id: row.get(1)?,
                    sender_device_id: row.get(2)?,
                    epoch: epoch.max(0).unsigned_abs(),
                    status: DmStatus::from_code(row.get(4)?),
                    bytes: row.get(5)?,
                })
            })
            .map_err(backend)?;
        rows.collect::<Result<_, _>>().map_err(backend)
    }
}

/// The cursor of a log on `conn`, which may be a transaction.
fn cursor_in(conn: &Connection, kind: LogKind, log_id: &[u8]) -> Result<u64, StoreError> {
    let seq: Option<i64> = conn
        .query_row(
            "SELECT seq FROM cursors WHERE kind = ?1 AND log_id = ?2",
            params![kind_code(kind), log_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(backend)?;
    Ok(seq.unwrap_or(0).max(0).unsigned_abs())
}

impl Store for SqliteStore {
    fn cursors(&self, kind: LogKind) -> Result<Vec<(Vec<u8>, u64)>, StoreError> {
        let mut stmt = self
            .conn
            .prepare("SELECT log_id, seq FROM cursors WHERE kind = ?1")
            .map_err(backend)?;
        let rows = stmt
            .query_map(params![kind_code(kind)], |row| {
                let id: Vec<u8> = row.get(0)?;
                let seq: i64 = row.get(1)?;
                Ok((id, seq.max(0).unsigned_abs()))
            })
            .map_err(backend)?;
        rows.collect::<Result<_, _>>().map_err(backend)
    }

    fn apply(
        &mut self,
        kind: LogKind,
        log_id: &[u8],
        events: &[StoredEvent],
    ) -> Result<(), StoreError> {
        let Some(first) = events.first() else {
            return Ok(());
        };
        // The cursor is read inside the write transaction: another
        // connection to the same file (the DM engine's) cannot move it
        // between the check and the write.
        let tx = self
            .conn
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .map_err(backend)?;
        let cursor = cursor_in(&tx, kind, log_id)?;
        if first.seq != cursor + 1 {
            return Err(StoreError::NotContiguous {
                cursor,
                first: first.seq,
            });
        }
        let mut last = cursor;
        for ev in events {
            if ev.seq != last + 1 {
                return Err(StoreError::NotContiguous {
                    cursor: last,
                    first: ev.seq,
                });
            }
            tx.execute(
                "INSERT OR REPLACE INTO events (kind, log_id, seq, bytes) VALUES (?1, ?2, ?3, ?4)",
                params![kind_code(kind), log_id, to_i64(ev.seq), ev.bytes],
            )
            .map_err(backend)?;
            if kind == LogKind::Channel {
                materialize(&tx, ev)?;
            }
            last = ev.seq;
        }
        tx.execute(
            "INSERT INTO cursors (kind, log_id, seq) VALUES (?1, ?2, ?3)
             ON CONFLICT (kind, log_id) DO UPDATE SET seq = excluded.seq",
            params![kind_code(kind), log_id, to_i64(last)],
        )
        .map_err(backend)?;
        tx.commit().map_err(backend)
    }

    fn apply_snapshot(
        &mut self,
        log_id: &[u8],
        seq: u64,
        messages: &[StoredMessage],
    ) -> Result<(), StoreError> {
        let tx = self
            .conn
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .map_err(backend)?;
        tx.execute(
            "DELETE FROM events WHERE kind = 0 AND log_id = ?1",
            params![log_id],
        )
        .map_err(backend)?;
        tx.execute(
            "DELETE FROM reactions WHERE message_id IN (SELECT message_id FROM messages WHERE channel_id = ?1)",
            params![log_id],
        )
        .map_err(backend)?;
        tx.execute(
            "DELETE FROM messages WHERE channel_id = ?1",
            params![log_id],
        )
        .map_err(backend)?;
        for m in messages {
            insert_message(&tx, m)?;
        }
        tx.execute(
            "INSERT INTO cursors (kind, log_id, seq) VALUES (0, ?1, ?2)
             ON CONFLICT (kind, log_id) DO UPDATE SET seq = excluded.seq",
            params![log_id, to_i64(seq)],
        )
        .map_err(backend)?;
        tx.commit().map_err(backend)
    }

    fn apply_account_snapshot(&mut self, account_id: &[u8], seq: u64) -> Result<(), StoreError> {
        let tx = self
            .conn
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .map_err(backend)?;
        tx.execute(
            "DELETE FROM events WHERE kind = 1 AND log_id = ?1",
            params![account_id],
        )
        .map_err(backend)?;
        tx.execute(
            "INSERT INTO cursors (kind, log_id, seq) VALUES (1, ?1, ?2)
             ON CONFLICT (kind, log_id) DO UPDATE SET seq = excluded.seq",
            params![account_id, to_i64(seq)],
        )
        .map_err(backend)?;
        tx.commit().map_err(backend)
    }

    // The DM engine's per-conversation progress (`dm:processed:<channel>`)
    // is a position in a channel log; with a new generation those positions
    // restart, so it goes with the cursors. Group state, decrypted rows and
    // history watermarks stay: the conversation itself did not change.
    fn discard_all(&mut self) -> Result<(), StoreError> {
        self.conn
            .execute_batch(
                "DELETE FROM cursors; DELETE FROM events; DELETE FROM messages; DELETE FROM reactions;
                 DELETE FROM meta WHERE key LIKE 'dm:processed:%';",
            )
            .map_err(backend)
    }

    fn generation(&self) -> Result<Option<u64>, StoreError> {
        Ok(self
            .meta_get("recovery_generation")?
            .and_then(|v| v.parse().ok()))
    }

    fn set_generation(&mut self, generation: u64) -> Result<(), StoreError> {
        self.meta_set("recovery_generation", &generation.to_string())
    }

    fn messages(&self, channel_id: &[u8], limit: usize) -> Result<Vec<StoredMessage>, StoreError> {
        self.messages_before(channel_id, u64::MAX, limit)
    }

    fn messages_before(
        &self,
        channel_id: &[u8],
        before_seq: u64,
        limit: usize,
    ) -> Result<Vec<StoredMessage>, StoreError> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT message_id, channel_id, seq, author_account_id, body, created_at_micros, deleted,
                        notice, notice_account_id, reply_to_id, reply_author_id, reply_text, reply_deleted, mentions,
                        edited, reply_silent, attachments, cards
                 FROM messages WHERE channel_id = ?1 AND seq < ?2 ORDER BY seq DESC LIMIT ?3",
            )
            .map_err(backend)?;
        // `seq` is stored as a signed integer; a bound past its range means "from the head".
        let before = i64::try_from(before_seq).unwrap_or(i64::MAX);
        let rows = stmt
            .query_map(params![channel_id, before, to_i64(limit as u64)], |row| {
                let seq: i64 = row.get(2)?;
                Ok(StoredMessage {
                    message_id: row.get(0)?,
                    channel_id: row.get(1)?,
                    seq: seq.max(0).unsigned_abs(),
                    author_account_id: row.get(3)?,
                    body: row.get(4)?,
                    created_at_micros: row.get(5)?,
                    deleted: row.get::<_, i64>(6)? != 0,
                    edited: row.get::<_, i64>(14)? != 0,
                    notice: row.get(7)?,
                    notice_account_id: row.get(8)?,
                    reply: {
                        let id: Vec<u8> = row.get(9)?;
                        if id.is_empty() {
                            None
                        } else {
                            Some(StoredQuote {
                                message_id: id,
                                author_account_id: row.get(10)?,
                                text: row.get(11)?,
                                original_deleted: row.get::<_, i64>(12)? != 0,
                                silent: row.get::<_, i64>(15)? != 0,
                            })
                        }
                    },
                    mentions: mentions_from(&row.get::<_, String>(13)?),
                    reactions: Vec::new(),
                    attachments: list_from(&row.get::<_, Vec<u8>>(16)?),
                    cards: list_from(&row.get::<_, Vec<u8>>(17)?),
                })
            })
            .map_err(backend)?;
        let mut messages: Vec<StoredMessage> = rows.collect::<Result<_, _>>().map_err(backend)?;
        messages.reverse();
        self.with_reactions(&mut messages)?;
        Ok(messages)
    }

    fn outbox_push(&mut self, entry: &OutboxEntry) -> Result<(), StoreError> {
        self.conn
            .execute(
                "INSERT OR REPLACE INTO outbox (command_id, channel_id, bytes, queued_at) VALUES (?1, ?2, ?3, ?4)",
                params![entry.command_id, entry.channel_id, entry.bytes, now_micros()],
            )
            .map(|_| ())
            .map_err(backend)
    }

    fn outbox_remove(&mut self, command_id: &[u8]) -> Result<(), StoreError> {
        self.conn
            .execute(
                "DELETE FROM outbox WHERE command_id = ?1",
                params![command_id],
            )
            .map(|_| ())
            .map_err(backend)
    }

    fn outbox(&self) -> Result<Vec<OutboxEntry>, StoreError> {
        let mut stmt = self
            .conn
            .prepare("SELECT command_id, channel_id, bytes FROM outbox ORDER BY queued_at, rowid")
            .map_err(backend)?;
        let rows = stmt
            .query_map([], |row| {
                Ok(OutboxEntry {
                    command_id: row.get(0)?,
                    channel_id: row.get(1)?,
                    bytes: row.get(2)?,
                })
            })
            .map_err(backend)?;
        rows.collect::<Result<_, _>>().map_err(backend)
    }

    fn meta_get(&self, key: &str) -> Result<Option<String>, StoreError> {
        self.conn
            .query_row(
                "SELECT value FROM meta WHERE key = ?1",
                params![key],
                |row| row.get(0),
            )
            .optional()
            .map_err(backend)
    }

    fn meta_set(&mut self, key: &str, value: &str) -> Result<(), StoreError> {
        self.conn
            .execute(
                "INSERT INTO meta (key, value) VALUES (?1, ?2) ON CONFLICT (key) DO UPDATE SET value = excluded.value",
                params![key, value],
            )
            .map(|_| ())
            .map_err(backend)
    }
}

// Secrets in the store itself: on the web the store is the only durable
// place and is encrypted at rest as a whole (architecture 11.4); natively
// the shells bring a platform keystore and this is for tests.
impl Keystore for SqliteStore {
    fn get(&self, name: &str) -> Result<Option<Vec<u8>>, KeystoreError> {
        self.conn
            .query_row(
                "SELECT value FROM secrets WHERE name = ?1",
                params![name],
                |row| row.get(0),
            )
            .optional()
            .map_err(|e| KeystoreError::Other(e.to_string()))
    }

    fn put(&mut self, name: &str, value: &[u8]) -> Result<(), KeystoreError> {
        self.conn
            .execute(
                "INSERT INTO secrets (name, value) VALUES (?1, ?2) ON CONFLICT (name) DO UPDATE SET value = excluded.value",
                params![name, value],
            )
            .map(|_| ())
            .map_err(|e| KeystoreError::Other(e.to_string()))
    }

    fn delete(&mut self, name: &str) -> Result<(), KeystoreError> {
        self.conn
            .execute("DELETE FROM secrets WHERE name = ?1", params![name])
            .map(|_| ())
            .map_err(|e| KeystoreError::Other(e.to_string()))
    }
}

// The OpenMLS storage provider's two shapes over the same connection, so
// MLS state commits in the caller's savepoint (e2ee spec section 7).
impl MlsKv for SqliteStore {
    type Error = StoreError;

    fn get(&self, kind: &str, key: &[u8]) -> Result<Option<Vec<u8>>, StoreError> {
        self.conn
            .query_row(
                "SELECT value FROM mls_kv WHERE kind = ?1 AND key = ?2",
                params![kind, key],
                |row| row.get(0),
            )
            .optional()
            .map_err(backend)
    }

    fn set(&self, kind: &str, key: &[u8], value: &[u8]) -> Result<(), StoreError> {
        self.conn
            .execute(
                "INSERT OR REPLACE INTO mls_kv (kind, key, value) VALUES (?1, ?2, ?3)",
                params![kind, key, value],
            )
            .map(|_| ())
            .map_err(backend)
    }

    fn delete(&self, kind: &str, key: &[u8]) -> Result<(), StoreError> {
        self.conn
            .execute(
                "DELETE FROM mls_kv WHERE kind = ?1 AND key = ?2",
                params![kind, key],
            )
            .map(|_| ())
            .map_err(backend)
    }

    fn list(&self, kind: &str, key: &[u8]) -> Result<Vec<Vec<u8>>, StoreError> {
        let mut stmt = self
            .conn
            .prepare("SELECT value FROM mls_list WHERE kind = ?1 AND key = ?2 ORDER BY position")
            .map_err(backend)?;
        let rows = stmt
            .query_map(params![kind, key], |row| row.get(0))
            .map_err(backend)?;
        rows.collect::<Result<_, _>>().map_err(backend)
    }

    fn append(&self, kind: &str, key: &[u8], value: &[u8]) -> Result<(), StoreError> {
        self.conn
            .execute(
                "INSERT INTO mls_list (kind, key, position, value)
                 VALUES (?1, ?2, (SELECT COALESCE(MAX(position), 0) + 1 FROM mls_list WHERE kind = ?1 AND key = ?2), ?3)",
                params![kind, key, value],
            )
            .map(|_| ())
            .map_err(backend)
    }

    fn remove_item(&self, kind: &str, key: &[u8], value: &[u8]) -> Result<(), StoreError> {
        self.conn
            .execute(
                "DELETE FROM mls_list WHERE rowid = (SELECT rowid FROM mls_list
                 WHERE kind = ?1 AND key = ?2 AND value = ?3 ORDER BY position LIMIT 1)",
                params![kind, key, value],
            )
            .map(|_| ())
            .map_err(backend)
    }

    fn clear_list(&self, kind: &str, key: &[u8]) -> Result<(), StoreError> {
        self.conn
            .execute(
                "DELETE FROM mls_list WHERE kind = ?1 AND key = ?2",
                params![kind, key],
            )
            .map(|_| ())
            .map_err(backend)
    }
}

/// Decodes a stored channel event.
///
/// # Errors
/// When the bytes are not an `Event`.
pub fn decode_event(ev: &StoredEvent) -> Result<Event, StoreError> {
    Event::decode(ev.bytes.as_slice()).map_err(|_| StoreError::Corrupt(ev.seq))
}

/// Decodes a stored account event.
///
/// # Errors
/// When the bytes are not an `AccountEvent`.
pub fn decode_account_event(ev: &StoredEvent) -> Result<AccountEvent, StoreError> {
    AccountEvent::decode(ev.bytes.as_slice()).map_err(|_| StoreError::Corrupt(ev.seq))
}

// A store made before a column existed gains it here: `CREATE TABLE IF NOT
// EXISTS` leaves an existing table as it was. Notices (architecture 6.8)
// came after the first stores.
fn add_missing_columns(conn: &Connection) -> Result<(), StoreError> {
    // Notices (architecture 6.8), then quotes and mentions (6.6).
    let columns = [
        ("notice", "TEXT NOT NULL DEFAULT ''"),
        ("notice_account_id", "BLOB NOT NULL DEFAULT x''"),
        ("reply_to_id", "BLOB NOT NULL DEFAULT x''"),
        ("reply_author_id", "BLOB NOT NULL DEFAULT x''"),
        ("reply_text", "TEXT NOT NULL DEFAULT ''"),
        ("reply_deleted", "INTEGER NOT NULL DEFAULT 0"),
        ("mentions", "TEXT NOT NULL DEFAULT '[]'"),
        // Edits, and replies sent without notifying (6.6).
        ("edited", "INTEGER NOT NULL DEFAULT 0"),
        ("reply_silent", "INTEGER NOT NULL DEFAULT 0"),
        // Attachments and link cards (specs/media.md).
        ("attachments", "BLOB NOT NULL DEFAULT x''"),
        ("cards", "BLOB NOT NULL DEFAULT x''"),
    ];
    for (name, kind) in columns {
        let has = conn
            .prepare("SELECT 1 FROM pragma_table_info('messages') WHERE name = ?1")
            .and_then(|mut stmt| stmt.exists([name]))
            .map_err(backend)?;
        if !has {
            conn.execute_batch(&format!("ALTER TABLE messages ADD COLUMN {name} {kind};"))
                .map_err(backend)?;
        }
    }
    Ok(())
}

/// What a server channel's rows keep of its events, as a number raised whenever
/// `materialize` learns to keep something an older store let go: 1 is quotes, mentions,
/// reactions and edits (architecture 6.6), 2 attachments and link cards (specs/media.md), 3
/// attachments a server let go (specs/media.md 4.5).
const MATERIALIZED: u32 = 3;

// A store kept by an older core has server channels' messages without what it did not yet
// keep, and cannot make it up: the events it kept were encoded again after decoding, which
// drops the fields that core did not know. So those channels are fetched again, as on a new
// device: their messages, reactions, events and cursors go, and the gateway, finding no
// cursor, reads them from the instance. Personal conversations are not touched; their
// events are ciphertext that cannot be read twice, and no message row is ever theirs.
fn refetch_what_was_kept_short(conn: &Connection) -> Result<(), StoreError> {
    // Read and cleared under one write lock: the gateway's connection and the DM engine's
    // open the same file, and only the first may do this.
    let tx = rusqlite::Transaction::new_unchecked(conn, rusqlite::TransactionBehavior::Immediate)
        .map_err(backend)?;
    let at = tx
        .query_row(
            "SELECT value FROM meta WHERE key = 'materialized'",
            [],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(backend)?
        .and_then(|v| v.parse::<u32>().ok())
        .unwrap_or(0);
    if at >= MATERIALIZED {
        return tx.commit().map_err(backend);
    }
    tx.execute_batch(
        "DELETE FROM events WHERE kind = 0 AND log_id IN (SELECT DISTINCT channel_id FROM messages);
         DELETE FROM cursors WHERE kind = 0 AND log_id IN (SELECT DISTINCT channel_id FROM messages);
         DELETE FROM reactions;
         DELETE FROM messages;",
    )
    .map_err(backend)?;
    tx.execute(
        "INSERT INTO meta (key, value) VALUES ('materialized', ?1)
         ON CONFLICT (key) DO UPDATE SET value = excluded.value",
        params![MATERIALIZED.to_string()],
    )
    .map_err(backend)?;
    tx.commit().map_err(backend)
}

/// Mentions as a row keeps them: `[[account id in hex, everyone, start, end]]`.
fn mentions_json(mentions: &[StoredMention]) -> String {
    let rows: Vec<(String, bool, u32, u32)> = mentions
        .iter()
        .map(|m| (hex::encode(&m.account_id), m.everyone, m.start, m.end))
        .collect();
    serde_json::to_string(&rows).unwrap_or_else(|_| "[]".into())
}

fn mentions_from(json: &str) -> Vec<StoredMention> {
    serde_json::from_str::<Vec<(String, bool, u32, u32)>>(json)
        .unwrap_or_default()
        .into_iter()
        .map(|(account, everyone, start, end)| StoredMention {
            account_id: hex::decode(account).unwrap_or_default(),
            everyone,
            start,
            end,
        })
        .collect()
}

/// A list of messages as a row keeps it: each length-delimited, one after another.
fn list_bytes<M: prost::Message>(items: &[M]) -> Vec<u8> {
    let mut out = Vec::new();
    for item in items {
        // Writing to a Vec cannot run out of room.
        let _ = item.encode_length_delimited(&mut out);
    }
    out
}

/// The list [`list_bytes`] wrote; what does not decode is left out.
fn list_from<M: prost::Message + Default>(mut bytes: &[u8]) -> Vec<M> {
    let mut out = Vec::new();
    while !bytes.is_empty() {
        match M::decode_length_delimited(&mut bytes) {
            Ok(item) => out.push(item),
            Err(_) => break,
        }
    }
    out
}

/// Drops the quoted text from the replies to the messages `quoted` names (an
/// SQL query of message ids with its parameters bound by `bind`): deleting a
/// message still deletes it where it was quoted (architecture 6.6).
fn drop_quotes(
    tx: &rusqlite::Transaction<'_>,
    quoted: &str,
    args: impl rusqlite::Params,
) -> Result<(), StoreError> {
    tx.execute(
        &format!(
            "UPDATE messages SET reply_text = '', reply_deleted = 1
             WHERE reply_to_id IN ({quoted}) AND reply_deleted = 0"
        ),
        args,
    )
    .map(|_| ())
    .map_err(backend)
}

// Messages are materialized from create, edit and delete events; everything
// else is kept as raw bytes only until its read side exists.
fn materialize(tx: &rusqlite::Transaction<'_>, ev: &StoredEvent) -> Result<(), StoreError> {
    let event = decode_event(ev)?;
    match event.payload {
        Some(event::Payload::MessageCreated(created)) => {
            if let Some(m) = created.message {
                insert_message(
                    tx,
                    &StoredMessage {
                        deleted: false,
                        ..StoredMessage::of(&m)
                    },
                )?;
            }
        }
        Some(event::Payload::MessageEdited(edited)) => {
            let mentions: Vec<_> = edited
                .mentions
                .iter()
                .filter_map(lumia_core::store::mention_of)
                .collect();
            // The attachments and cards after the edit come in full.
            tx.execute(
                "UPDATE messages SET body = ?2, mentions = ?3, edited = 1, attachments = ?4, cards = ?5
                 WHERE message_id = ?1 AND deleted = 0",
                params![
                    edited.message_id,
                    edited.body,
                    mentions_json(&mentions),
                    list_bytes::<Attachment>(&edited.attachments),
                    list_bytes::<Card>(&edited.cards)
                ],
            )
            .map_err(backend)?;
        }
        Some(event::Payload::MessageCardsSet(set)) => {
            tx.execute(
                "UPDATE messages SET cards = ?2 WHERE message_id = ?1 AND deleted = 0",
                params![set.message_id, list_bytes::<Card>(&set.cards)],
            )
            .map_err(backend)?;
        }
        Some(event::Payload::MessageDeleted(deleted)) => {
            tx.execute(
                "UPDATE messages SET body = '', deleted = 1, attachments = x'', cards = x''
                 WHERE message_id = ?1",
                params![deleted.message_id],
            )
            .map_err(backend)?;
            tx.execute(
                "DELETE FROM reactions WHERE message_id = ?1",
                params![deleted.message_id],
            )
            .map_err(backend)?;
            drop_quotes(tx, "?1", params![deleted.message_id])?;
        }
        Some(event::Payload::ReactionAdded(added)) => {
            tx.execute(
                "INSERT OR IGNORE INTO reactions (message_id, reaction, account_id, position)
                 VALUES (?1, ?2, ?3, ?4)",
                params![
                    added.message_id,
                    added.reaction,
                    added.account_id,
                    to_i64(event.seq)
                ],
            )
            .map_err(backend)?;
        }
        Some(event::Payload::ReactionRemoved(removed)) => {
            tx.execute(
                "DELETE FROM reactions WHERE message_id = ?1 AND reaction = ?2 AND account_id = ?3",
                params![removed.message_id, removed.reaction, removed.account_id],
            )
            .map_err(backend)?;
        }
        Some(event::Payload::PayloadsPurged(purged)) => {
            drop_quotes(
                tx,
                "SELECT message_id FROM messages WHERE channel_id = ?1 AND seq < ?2",
                params![event.channel_id, to_i64(purged.before_seq)],
            )?;
            tx.execute(
                "UPDATE messages SET body = '', deleted = 1, attachments = x'', cards = x''
                 WHERE channel_id = ?1 AND seq < ?2",
                params![event.channel_id, to_i64(purged.before_seq)],
            )
            .map_err(backend)?;
        }
        Some(event::Payload::AttachmentsExpired(expired)) => {
            // The messages keep their text; each attachment stays as what is drawn in its
            // place, opening nothing (specs/media.md 4.5).
            let rows = tx
                .prepare(
                    "SELECT message_id, attachments FROM messages
                     WHERE channel_id = ?1 AND seq < ?2 AND length(attachments) > 0",
                )
                .and_then(|mut stmt| {
                    stmt.query_map(
                        params![event.channel_id, to_i64(expired.before_seq)],
                        |row| Ok((row.get::<_, Vec<u8>>(0)?, row.get::<_, Vec<u8>>(1)?)),
                    )?
                    .collect::<Result<Vec<_>, _>>()
                })
                .map_err(backend)?;
            for (message_id, bytes) in rows {
                let attachments: Vec<Attachment> = list_from::<Attachment>(&bytes)
                    .into_iter()
                    .map(|a| Attachment {
                        expired: true,
                        main: None,
                        thumbnail: None,
                        ..a
                    })
                    .collect();
                tx.execute(
                    "UPDATE messages SET attachments = ?2 WHERE message_id = ?1",
                    params![message_id, list_bytes(&attachments)],
                )
                .map_err(backend)?;
            }
        }
        _ => {}
    }
    Ok(())
}

fn insert_message(tx: &rusqlite::Transaction<'_>, m: &StoredMessage) -> Result<(), StoreError> {
    let reply = m.reply.as_ref();
    tx.execute(
        "INSERT OR REPLACE INTO messages (message_id, channel_id, seq, author_account_id, body, created_at_micros, deleted, notice, notice_account_id,
                                          reply_to_id, reply_author_id, reply_text, reply_deleted, mentions, edited, reply_silent,
                                          attachments, cards)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18)",
        params![
            m.message_id, m.channel_id, to_i64(m.seq), m.author_account_id, m.body, m.created_at_micros, i64::from(m.deleted),
            m.notice, m.notice_account_id,
            reply.map_or(&[][..], |q| q.message_id.as_slice()),
            reply.map_or(&[][..], |q| q.author_account_id.as_slice()),
            reply.map_or("", |q| q.text.as_str()),
            i64::from(reply.is_some_and(|q| q.original_deleted)),
            mentions_json(&m.mentions),
            i64::from(m.edited),
            i64::from(reply.is_some_and(|q| q.silent)),
            list_bytes::<Attachment>(&m.attachments),
            list_bytes::<Card>(&m.cards),
        ],
    )
    .map_err(backend)?;
    // A snapshot's message brings who reacted with it.
    for (position, reaction) in m.reactions.iter().enumerate() {
        for account in &reaction.account_ids {
            tx.execute(
                "INSERT OR IGNORE INTO reactions (message_id, reaction, account_id, position)
                 VALUES (?1, ?2, ?3, ?4)",
                params![
                    m.message_id,
                    reaction.reaction,
                    account,
                    i64::try_from(position).unwrap_or(0) - 1_000_000
                ],
            )
            .map_err(backend)?;
        }
    }
    Ok(())
}

fn kind_code(kind: LogKind) -> i64 {
    match kind {
        LogKind::Channel => 0,
        LogKind::Account => 1,
    }
}

fn to_i64(n: u64) -> i64 {
    i64::try_from(n).unwrap_or(i64::MAX)
}

fn now_micros() -> i64 {
    crate::time::now_micros()
}

fn backend(e: rusqlite::Error) -> StoreError {
    StoreError::Backend(e.to_string())
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use lumia_core::proto::{Message, MessageCreated};

    fn created(channel: &[u8], seq: u64, body: &str) -> StoredEvent {
        let event = Event {
            channel_id: channel.to_vec(),
            seq,
            payload: Some(event::Payload::MessageCreated(MessageCreated {
                message: Some(Message {
                    message_id: vec![seq as u8; 16],
                    channel_id: channel.to_vec(),
                    seq,
                    body: body.into(),
                    ..Default::default()
                }),
            })),
            ..Default::default()
        };
        StoredEvent {
            seq,
            bytes: event.encode_to_vec(),
        }
    }

    #[test]
    fn a_notice_is_kept_with_who_it_is_about() {
        use lumia_core::proto::{channel_notice, ChannelNotice};
        let mut store = SqliteStore::in_memory().unwrap();
        let ch = vec![1u8; 16];
        let mut notice = created(&ch, 1, "");
        let mut event = Event::decode(notice.bytes.as_slice()).unwrap();
        if let Some(event::Payload::MessageCreated(MessageCreated { message: Some(m) })) =
            event.payload.as_mut()
        {
            m.notice = Some(ChannelNotice {
                kind: channel_notice::Kind::Added as i32,
                account_id: vec![7; 16],
            });
        }
        notice.bytes = event.encode_to_vec();
        store
            .apply(LogKind::Channel, &ch, &[notice, created(&ch, 2, "hi")])
            .unwrap();
        let kept = store.messages(&ch, 10).unwrap();
        assert_eq!(kept[0].notice, "added");
        assert_eq!(kept[0].notice_account_id, vec![7; 16]);
        assert_eq!(kept[1].notice, "");
        assert!(kept[1].notice_account_id.is_empty());
    }

    fn stored(channel: &[u8], seq: u64, payload: event::Payload) -> StoredEvent {
        let event = Event {
            channel_id: channel.to_vec(),
            seq,
            payload: Some(payload),
            ..Default::default()
        };
        StoredEvent {
            seq,
            bytes: event.encode_to_vec(),
        }
    }

    #[test]
    fn reactions_quotes_and_mentions_are_kept_and_a_deleted_original_drops_its_quotes() {
        use lumia_core::proto::{
            mention, Mention, MessageDeleted, QuoteReply, ReactionAdded, ReactionRemoved,
        };
        let mut store = SqliteStore::in_memory().unwrap();
        let ch = vec![1u8; 16];
        let original = vec![1u8; 16];
        let reply = stored(
            &ch,
            2,
            event::Payload::MessageCreated(MessageCreated {
                message: Some(Message {
                    message_id: vec![2; 16],
                    channel_id: ch.clone(),
                    seq: 2,
                    body: "@a yes".into(),
                    mentions: vec![Mention {
                        target: Some(mention::Target::AccountId(vec![5; 16])),
                        start: 0,
                        end: 2,
                    }],
                    reply_to: Some(QuoteReply {
                        message_id: original.clone(),
                        author_account_id: vec![5; 16],
                        text: "lunch?".into(),
                        original_deleted: false,
                        silent: false,
                    }),
                    ..Default::default()
                }),
            }),
        );
        let react = |seq, account: u8, emoji: &str, add| {
            let message_id = vec![2; 16];
            let reaction = emoji.to_string();
            let account_id = vec![account; 16];
            stored(
                &ch,
                seq,
                if add {
                    event::Payload::ReactionAdded(ReactionAdded {
                        message_id,
                        reaction,
                        account_id,
                        emoji_id: Vec::new(),
                    })
                } else {
                    event::Payload::ReactionRemoved(ReactionRemoved {
                        message_id,
                        reaction,
                        account_id,
                        emoji_id: Vec::new(),
                    })
                },
            )
        };
        store
            .apply(
                LogKind::Channel,
                &ch,
                &[
                    created(&ch, 1, "lunch?"),
                    reply,
                    react(3, 5, "👍", true),
                    react(4, 6, "🎉", true),
                    react(5, 6, "👍", true),
                    react(6, 5, "🎉", true),
                    react(7, 6, "🎉", false),
                ],
            )
            .unwrap();
        let kept = store.messages(&ch, 10).unwrap();
        let r = &kept[1];
        assert_eq!(r.mentions.len(), 1);
        assert_eq!(r.mentions[0].account_id, vec![5; 16]);
        assert_eq!(r.reply.as_ref().unwrap().text, "lunch?");
        let reactions: Vec<_> = r
            .reactions
            .iter()
            .map(|x| (x.reaction.as_str(), x.account_ids.len()))
            .collect();
        assert_eq!(reactions, vec![("👍", 2), ("🎉", 1)]);

        store
            .apply(
                LogKind::Channel,
                &ch,
                &[stored(
                    &ch,
                    8,
                    event::Payload::MessageDeleted(MessageDeleted {
                        message_id: original,
                        purge_cache: false,
                    }),
                )],
            )
            .unwrap();
        let quote = store.messages(&ch, 10).unwrap()[1].reply.clone().unwrap();
        assert!(quote.original_deleted);
        assert_eq!(quote.text, "");
    }

    #[test]
    fn an_edit_is_kept_with_its_mentions_and_marked_edited() {
        use lumia_core::proto::{mention, Mention, MessageEdited};
        let mut store = SqliteStore::in_memory().unwrap();
        let ch = vec![1u8; 16];
        store
            .apply(LogKind::Channel, &ch, &[created(&ch, 1, "@ada hi")])
            .unwrap();
        assert!(!store.messages(&ch, 10).unwrap()[0].edited);
        let edited = stored(
            &ch,
            2,
            event::Payload::MessageEdited(MessageEdited {
                message_id: vec![1; 16],
                body: "@bo hi".into(),
                mentions: vec![Mention {
                    target: Some(mention::Target::AccountId(vec![6; 16])),
                    start: 0,
                    end: 3,
                }],
                ..Default::default()
            }),
        );
        store.apply(LogKind::Channel, &ch, &[edited]).unwrap();
        let kept = &store.messages(&ch, 10).unwrap()[0];
        assert!(kept.edited);
        assert_eq!(kept.body, "@bo hi");
        assert_eq!(kept.mentions.len(), 1);
        assert_eq!(kept.mentions[0].account_id, vec![6; 16]);
    }

    #[test]
    fn attachments_and_cards_are_kept_replaced_and_dropped() {
        use lumia_core::proto::{
            Attachment, AttachmentKind, BlobRef, Card, MessageCardsSet, MessageDeleted,
            MessageEdited,
        };
        let mut store = SqliteStore::in_memory().unwrap();
        let ch = vec![1u8; 16];
        let attachment = |id: u8| Attachment {
            attachment_id: vec![id; 16],
            kind: AttachmentKind::Image as i32,
            name: format!("{id}.jpg"),
            main: Some(BlobRef {
                blob_id: vec![id; 16],
                size_bytes: 116,
                key: vec![id; 32],
                nonce: vec![id; 24],
                hash: vec![id; 32],
            }),
            ..Default::default()
        };
        let card = |url: &str| Card {
            url: url.into(),
            title: url.into(),
            ..Default::default()
        };
        let mut first = created(&ch, 1, "look https://a.example");
        let mut event = Event::decode(first.bytes.as_slice()).unwrap();
        if let Some(event::Payload::MessageCreated(MessageCreated { message: Some(m) })) =
            event.payload.as_mut()
        {
            m.attachments = vec![attachment(1), attachment(2)];
        }
        first.bytes = event.encode_to_vec();
        store.apply(LogKind::Channel, &ch, &[first]).unwrap();
        let kept = &store.messages(&ch, 10).unwrap()[0];
        assert_eq!(kept.attachments, vec![attachment(1), attachment(2)]);
        assert!(kept.cards.is_empty());

        // The card arrives after the message, then an edit drops an attachment.
        let message_id = vec![1u8; 16];
        let later = [
            stored(
                &ch,
                2,
                event::Payload::MessageCardsSet(MessageCardsSet {
                    message_id: message_id.clone(),
                    cards: vec![card("https://a.example")],
                }),
            ),
            stored(
                &ch,
                3,
                event::Payload::MessageEdited(MessageEdited {
                    message_id: message_id.clone(),
                    body: "look https://a.example".into(),
                    attachments: vec![attachment(2)],
                    cards: vec![card("https://a.example")],
                    ..Default::default()
                }),
            ),
        ];
        store.apply(LogKind::Channel, &ch, &later).unwrap();
        let kept = &store.messages(&ch, 10).unwrap()[0];
        assert_eq!(kept.attachments, vec![attachment(2)]);
        assert_eq!(kept.cards, vec![card("https://a.example")]);

        let deleted = stored(
            &ch,
            4,
            event::Payload::MessageDeleted(MessageDeleted {
                message_id,
                purge_cache: false,
            }),
        );
        store.apply(LogKind::Channel, &ch, &[deleted]).unwrap();
        let kept = &store.messages(&ch, 10).unwrap()[0];
        assert!(kept.attachments.is_empty() && kept.cards.is_empty());
    }

    #[test]
    fn attachments_a_server_let_go_stay_as_their_names_and_open_nothing() {
        use lumia_core::proto::{Attachment, AttachmentKind, AttachmentsExpired, BlobRef};
        let mut store = SqliteStore::in_memory().unwrap();
        let ch = vec![1u8; 16];
        let attachment = |id: u8| Attachment {
            attachment_id: vec![id; 16],
            kind: AttachmentKind::File as i32,
            name: format!("{id}.pdf"),
            main: Some(BlobRef {
                blob_id: vec![id; 16],
                size_bytes: 116,
                ..Default::default()
            }),
            ..Default::default()
        };
        let with = |seq: u64, id: u8| {
            let mut e = created(&ch, seq, "here");
            let mut event = Event::decode(e.bytes.as_slice()).unwrap();
            if let Some(event::Payload::MessageCreated(MessageCreated { message: Some(m) })) =
                event.payload.as_mut()
            {
                m.message_id = vec![id; 16];
                m.attachments = vec![attachment(id)];
            }
            e.bytes = event.encode_to_vec();
            e
        };
        store
            .apply(LogKind::Channel, &ch, &[with(1, 1), with(2, 2)])
            .unwrap();
        let expired = stored(
            &ch,
            3,
            event::Payload::AttachmentsExpired(AttachmentsExpired { before_seq: 2 }),
        );
        store.apply(LogKind::Channel, &ch, &[expired]).unwrap();

        let kept = store.messages(&ch, 10).unwrap();
        let of = |seq: u64| &kept.iter().find(|m| m.seq == seq).unwrap().attachments;
        assert_eq!(
            of(1),
            &vec![Attachment {
                expired: true,
                main: None,
                ..attachment(1)
            }]
        );
        assert_eq!(of(2), &vec![attachment(2)]);
        assert_eq!(kept.iter().find(|m| m.seq == 1).unwrap().body, "here");
    }

    #[test]
    fn a_store_made_before_notices_gains_their_columns() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(&format!(
            "CREATE TABLE messages (
               message_id BLOB PRIMARY KEY,
               channel_id BLOB NOT NULL,
               seq INTEGER NOT NULL,
               author_account_id BLOB NOT NULL,
               body TEXT NOT NULL,
               created_at_micros INTEGER NOT NULL,
               deleted INTEGER NOT NULL DEFAULT 0
             );
             INSERT INTO messages VALUES (x'01', x'02', 1, x'03', 'old', 0, 0);
             CREATE TABLE meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);
             INSERT INTO meta VALUES ('materialized', '{MATERIALIZED}');",
        ))
        .unwrap();
        let store = SqliteStore::init(conn).unwrap();
        let kept = store.messages(&[2], 10).unwrap();
        assert_eq!(
            (kept[0].body.as_str(), kept[0].notice.as_str()),
            ("old", "")
        );
        // Opening it again finds the columns there and leaves them.
        add_missing_columns(&store.conn).unwrap();
    }

    #[test]
    fn a_store_kept_by_an_older_core_fetches_its_server_channels_again() {
        let ch = vec![1u8; 16];
        let dm = vec![9u8; 16];
        let path =
            std::env::temp_dir().join(format!("lumia-refetch-{}.sqlite", std::process::id()));
        let _ = std::fs::remove_file(&path);
        {
            let mut store = SqliteStore::open(&path).unwrap();
            store
                .apply(
                    LogKind::Channel,
                    &ch,
                    &[created(&ch, 1, "a"), created(&ch, 2, "b")],
                )
                .unwrap();
            // A personal conversation's cursor, whose events are never message rows.
            store
                .conn
                .execute(
                    "INSERT INTO cursors (kind, log_id, seq) VALUES (0, ?1, 5)",
                    params![dm],
                )
                .unwrap();
            // As an older core left it: no mark of what its rows keep.
            store
                .conn
                .execute("DELETE FROM meta WHERE key = 'materialized'", [])
                .unwrap();
        }
        let store = SqliteStore::open(&path).unwrap();
        assert!(
            store.messages(&ch, 10).unwrap().is_empty(),
            "the old rows are gone"
        );
        assert_eq!(
            store.cursors(LogKind::Channel).unwrap(),
            vec![(dm.clone(), 5)],
            "the server channel is read again from the start; the personal one is kept"
        );
        drop(store);
        // Once marked, a later open keeps what is there.
        let mut store = SqliteStore::open(&path).unwrap();
        store
            .apply(LogKind::Channel, &ch, &[created(&ch, 1, "a")])
            .unwrap();
        drop(store);
        let store = SqliteStore::open(&path).unwrap();
        assert_eq!(store.messages(&ch, 10).unwrap().len(), 1);
        drop(store);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn apply_writes_events_messages_and_cursor_together() {
        let mut store = SqliteStore::in_memory().unwrap();
        let ch = vec![1u8; 16];
        store
            .apply(
                LogKind::Channel,
                &ch,
                &[created(&ch, 1, "a"), created(&ch, 2, "b")],
            )
            .unwrap();
        assert_eq!(
            store.cursors(LogKind::Channel).unwrap(),
            vec![(ch.clone(), 2)]
        );
        let bodies: Vec<_> = store
            .messages(&ch, 10)
            .unwrap()
            .into_iter()
            .map(|m| m.body)
            .collect();
        assert_eq!(bodies, vec!["a", "b"]);
    }

    #[test]
    fn messages_before_pages_upward_from_a_seq() {
        let mut store = SqliteStore::in_memory().unwrap();
        let ch = vec![1u8; 16];
        let events: Vec<_> = (1..=7)
            .map(|seq| created(&ch, seq, &seq.to_string()))
            .collect();
        store.apply(LogKind::Channel, &ch, &events).unwrap();
        let bodies =
            |rows: Vec<StoredMessage>| rows.into_iter().map(|m| m.body).collect::<Vec<_>>();
        // The newest three, then the three above them, then what is left; oldest first each time.
        assert_eq!(bodies(store.messages(&ch, 3).unwrap()), vec!["5", "6", "7"]);
        assert_eq!(
            bodies(store.messages_before(&ch, 5, 3).unwrap()),
            vec!["2", "3", "4"]
        );
        assert_eq!(bodies(store.messages_before(&ch, 2, 3).unwrap()), vec!["1"]);
        assert!(store.messages_before(&ch, 1, 3).unwrap().is_empty());
        // Another channel's rows are not this one's.
        assert!(store.messages_before(&[2u8; 16], 9, 3).unwrap().is_empty());
    }

    #[test]
    fn apply_refuses_a_gap() {
        let mut store = SqliteStore::in_memory().unwrap();
        let ch = vec![1u8; 16];
        store
            .apply(LogKind::Channel, &ch, &[created(&ch, 1, "a")])
            .unwrap();
        let err = store
            .apply(LogKind::Channel, &ch, &[created(&ch, 3, "c")])
            .unwrap_err();
        assert!(matches!(
            err,
            StoreError::NotContiguous {
                cursor: 1,
                first: 3
            }
        ));
        assert_eq!(store.cursors(LogKind::Channel).unwrap(), vec![(ch, 1)]);
    }

    #[test]
    fn snapshot_replaces_local_state_and_discard_clears_it() {
        let mut store = SqliteStore::in_memory().unwrap();
        let ch = vec![1u8; 16];
        store
            .apply(LogKind::Channel, &ch, &[created(&ch, 1, "old")])
            .unwrap();
        let m = StoredMessage {
            message_id: vec![9; 16],
            channel_id: ch.clone(),
            seq: 2000,
            author_account_id: vec![],
            body: "new".into(),
            created_at_micros: 0,
            deleted: false,
            edited: false,
            notice: String::new(),
            notice_account_id: Vec::new(),
            reply: None,
            mentions: Vec::new(),
            reactions: Vec::new(),
            attachments: Vec::new(),
            cards: Vec::new(),
        };
        store.apply_snapshot(&ch, 2000, &[m]).unwrap();
        assert_eq!(
            store.cursors(LogKind::Channel).unwrap(),
            vec![(ch.clone(), 2000)]
        );
        assert_eq!(store.messages(&ch, 10).unwrap()[0].body, "new");

        store.set_generation(3).unwrap();
        store.discard_all().unwrap();
        assert!(store.cursors(LogKind::Channel).unwrap().is_empty());
        assert_eq!(store.generation().unwrap(), Some(3));
    }

    #[test]
    fn a_failed_transaction_rolls_back_mls_state_and_meta_together() {
        let store = SqliteStore::in_memory().unwrap();
        MlsKv::set(&store, "k", b"a", b"1").unwrap();
        let result: Result<(), StoreError> = store.transaction(|s| {
            s.set("k", b"a", b"2")?;
            s.append("l", b"x", b"one")?;
            s.set_meta("m", "v")?;
            Err(StoreError::Backend("boom".into()))
        });
        assert!(result.is_err());
        assert_eq!(MlsKv::get(&store, "k", b"a").unwrap(), Some(b"1".to_vec()));
        assert!(store.list("l", b"x").unwrap().is_empty());
        assert_eq!(store.meta_get("m").unwrap(), None);

        let ok: Result<(), StoreError> = store.transaction(|s| {
            s.append("l", b"x", b"one")?;
            s.append("l", b"x", b"two")?;
            s.remove_item("l", b"x", b"one")
        });
        ok.unwrap();
        assert_eq!(store.list("l", b"x").unwrap(), vec![b"two".to_vec()]);
    }

    #[test]
    fn account_snapshot_moves_the_cursor_so_the_next_live_event_is_contiguous() {
        let mut store = SqliteStore::in_memory().unwrap();
        let account = vec![4u8; 16];
        let ev = |seq: u64| StoredEvent {
            seq,
            bytes: vec![seq as u8],
        };
        store.apply(LogKind::Account, &account, &[ev(1)]).unwrap();

        store.apply_account_snapshot(&account, 1500).unwrap();
        assert_eq!(
            store.cursors(LogKind::Account).unwrap(),
            vec![(account.clone(), 1500)]
        );
        assert!(store.account_events().unwrap().is_empty());

        // The first live event after the snapshot applies; a stale one does not.
        assert!(matches!(
            store.apply(LogKind::Account, &account, &[ev(2)]),
            Err(StoreError::NotContiguous { cursor: 1500, .. })
        ));
        store
            .apply(LogKind::Account, &account, &[ev(1501)])
            .unwrap();
        assert_eq!(
            store.cursors(LogKind::Account).unwrap(),
            vec![(account, 1501)]
        );
    }

    #[test]
    fn discard_all_forgets_the_dm_engine_progress_but_not_its_group_state() {
        let mut store = SqliteStore::in_memory().unwrap();
        store.set_meta("dm:processed:abcd", "7").unwrap();
        store.set_meta("dm:joined_epoch:abcd", "2").unwrap();
        store.set_meta("history:backed:abcd", "7").unwrap();
        store.set("GroupState", b"g", b"state").unwrap();

        store.discard_all().unwrap();

        assert_eq!(store.meta_get("dm:processed:abcd").unwrap(), None);
        assert_eq!(
            store.meta_get("dm:joined_epoch:abcd").unwrap(),
            Some("2".into())
        );
        assert_eq!(
            store.meta_get("history:backed:abcd").unwrap(),
            Some("7".into())
        );
        assert_eq!(
            MlsKv::get(&store, "GroupState", b"g").unwrap(),
            Some(b"state".to_vec())
        );
    }

    #[test]
    fn outbox_round_trips() {
        let mut store = SqliteStore::in_memory().unwrap();
        let entry = OutboxEntry {
            command_id: vec![1; 16],
            channel_id: vec![2; 16],
            bytes: vec![3],
        };
        store.outbox_push(&entry).unwrap();
        assert_eq!(store.outbox().unwrap(), vec![entry.clone()]);
        store.outbox_remove(&entry.command_id).unwrap();
        assert!(store.outbox().unwrap().is_empty());
    }

    /// An account signs out on a device another account then signs in on.
    /// Its account log's cursor used to stay, and the next account's
    /// gateway resumed its own log from it: the events after it were
    /// refused as a gap, for ever.
    #[test]
    fn a_draft_is_kept_by_conversation_until_it_is_cleared_or_erased() {
        let store = SqliteStore::in_memory().unwrap();
        let (here, there) = (vec![5u8; 16], vec![6u8; 16]);
        assert_eq!(store.draft(&here).unwrap(), None);
        store.set_draft(&here, "half a thought", "", 1).unwrap();
        store.set_draft(&there, "another", "{\"q\":1}", 2).unwrap();
        // The later one takes the earlier one's place, in its own conversation alone.
        store
            .set_draft(&here, "half a thought, finished", "", 3)
            .unwrap();
        assert_eq!(
            store.draft(&here).unwrap(),
            Some(("half a thought, finished".into(), String::new()))
        );
        assert_eq!(
            store.draft(&there).unwrap(),
            Some(("another".into(), "{\"q\":1}".into()))
        );
        store.clear_draft(&here).unwrap();
        assert_eq!(store.draft(&here).unwrap(), None);
        assert!(store.draft(&there).unwrap().is_some());
        // A removed device keeps none (identity spec 4.4).
        store.erase().unwrap();
        assert_eq!(store.draft(&there).unwrap(), None);
    }

    #[test]
    fn a_write_begun_now_waits_for_nobody() {
        let dir = std::env::temp_dir().join(format!("lumia-now-{}", uuid::Uuid::now_v7()));
        std::fs::create_dir_all(&dir).unwrap();
        let (one, other) = (
            SqliteStore::open(&dir.join("store.sqlite")).unwrap(),
            SqliteStore::open(&dir.join("store.sqlite")).unwrap(),
        );
        // The other connection is writing: refused at once, not after the busy wait.
        assert!(other.begin_now().unwrap());
        other.set_meta("k", "theirs").unwrap();
        let asked = std::time::Instant::now();
        assert!(!one.begin_now().unwrap());
        assert!(asked.elapsed() < std::time::Duration::from_secs(1));
        other.end(true).unwrap();

        // Free: begun, and what is written in it is kept or undone as it ends.
        assert!(one.begin_now().unwrap());
        one.set_meta("k", "undone").unwrap();
        one.end(false).unwrap();
        assert_eq!(one.meta_get("k").unwrap().as_deref(), Some("theirs"));
        assert!(one.begin_now().unwrap());
        one.set_meta("k", "kept").unwrap();
        one.end(true).unwrap();
        assert_eq!(other.meta_get("k").unwrap().as_deref(), Some("kept"));
        // The connection waits as before for what is not begun this way.
        drop((one, other));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    fn list(members: &[(&str, &str)]) -> Vec<(String, String)> {
        members
            .iter()
            .map(|(account, member)| ((*account).to_string(), (*member).to_string()))
            .collect()
    }

    #[test]
    fn a_kept_member_is_a_row_of_its_servers_list() {
        let store = SqliteStore::in_memory().unwrap();
        assert!(store.kept_members("s").unwrap().is_empty());
        store
            .keep_member_list("s", &list(&[("b", "Grace"), ("a", "Ada")]), 0)
            .unwrap();
        store
            .keep_member_list("t", &list(&[("a", "Countess")]), 0)
            .unwrap();
        // In the order listed, each server's its own.
        assert_eq!(store.kept_members("s").unwrap(), ["Grace", "Ada"]);
        assert_eq!(
            store.kept_member("t", "a").unwrap().as_deref(),
            Some("Countess")
        );
        assert_eq!(store.kept_member("t", "b").unwrap(), None);
        let mut everywhere = store.kept_memberships("a").unwrap();
        everywhere.sort();
        assert_eq!(
            everywhere,
            [
                ("s".to_string(), "Ada".to_string()),
                ("t".to_string(), "Countess".to_string())
            ]
        );

        // One member written is that row and no other; one the list has not is not added.
        store.keep_member("s", "a", "Ada L.").unwrap();
        store.keep_member("s", "c", "Nobody").unwrap();
        assert_eq!(store.kept_members("s").unwrap(), ["Grace", "Ada L."]);
        assert_eq!(store.kept_members("t").unwrap(), ["Countess"]);
        store.drop_kept_member("s", "b").unwrap();
        assert_eq!(store.kept_members("s").unwrap(), ["Ada L."]);

        // A list kept again takes the place of the one before, whole.
        store
            .keep_member_list("s", &list(&[("d", "Dana")]), 0)
            .unwrap();
        assert_eq!(store.kept_members("s").unwrap(), ["Dana"]);
        store.drop_member_list("t").unwrap();
        assert!(store.kept_members("t").unwrap().is_empty());
        // Whoever signs out, and a removed device, keep none.
        store.forget_kept_members().unwrap();
        assert!(store.kept_members("s").unwrap().is_empty());
    }

    #[test]
    fn a_stale_list_is_current_once_listed_after_what_made_it_stale() {
        let store = SqliteStore::in_memory().unwrap();
        store
            .keep_member_list("s", &list(&[("a", "Ada")]), 0)
            .unwrap();
        store
            .keep_member_list("t", &list(&[("a", "Ada")]), 0)
            .unwrap();
        assert!(store.stale_member_lists().unwrap().is_empty());
        assert_eq!(store.member_list_staleness("never listed").unwrap(), 0);

        store.mark_member_lists_stale().unwrap();
        assert_eq!(store.stale_member_lists().unwrap(), ["s", "t"]);
        // Read after it was said: current again.
        let read_at = store.member_list_staleness("s").unwrap();
        store
            .keep_member_list("s", &list(&[("a", "Ada L.")]), read_at)
            .unwrap();
        assert_eq!(store.stale_member_lists().unwrap(), ["t"]);

        // Said again while a read was on its way: what that read brings may be from
        // before, so the list is kept and stays stale for another read.
        let read_at = store.member_list_staleness("t").unwrap();
        store.mark_member_lists_stale().unwrap();
        store
            .keep_member_list("t", &list(&[("a", "Ada L.")]), read_at)
            .unwrap();
        assert_eq!(store.kept_members("t").unwrap(), ["Ada L."]);
        assert_eq!(store.stale_member_lists().unwrap(), ["s", "t"]);
    }

    #[test]
    fn one_list_said_stale_leaves_the_others_current() {
        let store = SqliteStore::in_memory().unwrap();
        for server in ["s", "t"] {
            store
                .keep_member_list(server, &list(&[("a", "Ada")]), 0)
                .unwrap();
        }
        store.mark_member_list_stale("t").unwrap();
        store.mark_member_list_stale("never listed").unwrap();
        assert_eq!(store.stale_member_lists().unwrap(), ["t"]);
    }

    #[test]
    fn forgetting_an_account_leaves_the_channels_and_nothing_of_the_account() {
        let mut store = SqliteStore::in_memory().unwrap();
        let (account, channel) = (vec![4u8; 16], vec![5u8; 16]);
        let ev = |seq: u64| StoredEvent {
            seq,
            bytes: vec![seq as u8],
        };
        store
            .apply(LogKind::Account, &account, &[ev(1), ev(2)])
            .unwrap();
        store
            .apply(LogKind::Channel, &channel, &[created(&channel, 1, "kept")])
            .unwrap();
        store
            .outbox_push(&OutboxEntry {
                command_id: vec![1; 16],
                channel_id: channel.clone(),
                bytes: vec![3],
            })
            .unwrap();

        store.set_draft(&channel, "half a thought", "", 1).unwrap();

        store.forget_account().unwrap();

        assert!(store.cursors(LogKind::Account).unwrap().is_empty());
        assert!(store.account_events().unwrap().is_empty());
        assert!(store.outbox().unwrap().is_empty());
        assert_eq!(store.draft(&channel).unwrap(), None);
        assert_eq!(
            store.cursors(LogKind::Channel).unwrap(),
            vec![(channel.clone(), 1)]
        );
        assert_eq!(store.messages(&channel, 10).unwrap().len(), 1);
        // The next account's log starts at its beginning.
        store.apply(LogKind::Account, &[6u8; 16], &[ev(1)]).unwrap();
    }
}
