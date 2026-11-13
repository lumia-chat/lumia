//! The native store: SQLite through `rusqlite` with the bundled engine.
//!
//! Raw event bytes per log and sequence are the local source of truth;
//! messages are materialized from them. [`Store::apply`] writes the events,
//! the materialized rows, and the cursor in one transaction, which is the
//! rule from docs/protocol.md section 5.

use std::path::Path;

use lumia_core::keystore::{Keystore, KeystoreError};
use lumia_core::mls::storage::MlsKv;
use lumia_core::proto::{event, AccountEvent, Event};
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
  reply_silent INTEGER NOT NULL DEFAULT 0
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
";

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
        conn.busy_timeout(std::time::Duration::from_secs(10))
            .map_err(backend)?;
        conn.execute_batch("PRAGMA journal_mode = WAL; PRAGMA foreign_keys = ON;")
            .map_err(backend)?;
        conn.execute_batch(SCHEMA).map_err(backend)?;
        add_missing_columns(&conn)?;
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

    /// Forgets what an account that signed out leaves here: its account log
    /// (the cursor with the events, so the next account starts from the
    /// beginning of its own) and the commands it had not sent, which would
    /// otherwise go out at the next welcome as whoever signs in next. The
    /// channels' logs stay: they are the channels', not the account's, and a
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
            Ok::<_, StoreError>(())
        })
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
                        edited, reply_silent
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
            .prepare("SELECT command_id, channel_id, bytes FROM outbox ORDER BY queued_at")
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
            tx.execute(
                "UPDATE messages SET body = ?2, mentions = ?3, edited = 1 WHERE message_id = ?1 AND deleted = 0",
                params![edited.message_id, edited.body, mentions_json(&mentions)],
            )
            .map_err(backend)?;
        }
        Some(event::Payload::MessageDeleted(deleted)) => {
            tx.execute(
                "UPDATE messages SET body = '', deleted = 1 WHERE message_id = ?1",
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
                "UPDATE messages SET body = '', deleted = 1 WHERE channel_id = ?1 AND seq < ?2",
                params![event.channel_id, to_i64(purged.before_seq)],
            )
            .map_err(backend)?;
        }
        _ => {}
    }
    Ok(())
}

fn insert_message(tx: &rusqlite::Transaction<'_>, m: &StoredMessage) -> Result<(), StoreError> {
    let reply = m.reply.as_ref();
    tx.execute(
        "INSERT OR REPLACE INTO messages (message_id, channel_id, seq, author_account_id, body, created_at_micros, deleted, notice, notice_account_id,
                                          reply_to_id, reply_author_id, reply_text, reply_deleted, mentions, edited, reply_silent)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)",
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
                    })
                } else {
                    event::Payload::ReactionRemoved(ReactionRemoved {
                        message_id,
                        reaction,
                        account_id,
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
    fn a_store_made_before_notices_gains_their_columns() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE messages (
               message_id BLOB PRIMARY KEY,
               channel_id BLOB NOT NULL,
               seq INTEGER NOT NULL,
               author_account_id BLOB NOT NULL,
               body TEXT NOT NULL,
               created_at_micros INTEGER NOT NULL,
               deleted INTEGER NOT NULL DEFAULT 0
             );
             INSERT INTO messages VALUES (x'01', x'02', 1, x'03', 'old', 0, 0);",
        )
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

        store.forget_account().unwrap();

        assert!(store.cursors(LogKind::Account).unwrap().is_empty());
        assert!(store.account_events().unwrap().is_empty());
        assert!(store.outbox().unwrap().is_empty());
        assert_eq!(
            store.cursors(LogKind::Channel).unwrap(),
            vec![(channel.clone(), 1)]
        );
        assert_eq!(store.messages(&channel, 10).unwrap().len(), 1);
        // The next account's log starts at its beginning.
        store.apply(LogKind::Account, &[6u8; 16], &[ev(1)]).unwrap();
    }
}
