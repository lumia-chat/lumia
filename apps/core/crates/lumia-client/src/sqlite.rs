//! The native store: SQLite through `rusqlite` with the bundled engine.
//!
//! Raw event bytes per log and sequence are the local source of truth;
//! messages are materialized from them. [`Store::apply`] writes the events,
//! the materialized rows, and the cursor in one transaction, which is the
//! rule from docs/protocol.md section 5.

use std::path::Path;

use lumia_core::mls::storage::MlsKv;
use lumia_core::proto::{event, AccountEvent, Event};
use lumia_core::store::{LogKind, OutboxEntry, Store, StoreError, StoredEvent, StoredMessage};
use lumia_core::wire;
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
  deleted INTEGER NOT NULL DEFAULT 0
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
    /// Opens (creating) the database file and applies the schema.
    ///
    /// # Errors
    /// When the file cannot be opened or the schema fails.
    pub fn open(path: &Path) -> Result<Self, StoreError> {
        let conn = Connection::open(path).map_err(backend)?;
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

    fn cursor_of(&self, kind: LogKind, log_id: &[u8]) -> Result<u64, StoreError> {
        let seq: Option<i64> = self
            .conn
            .query_row(
                "SELECT seq FROM cursors WHERE kind = ?1 AND log_id = ?2",
                params![kind_code(kind), log_id],
                |row| row.get(0),
            )
            .optional()
            .map_err(backend)?;
        Ok(seq.unwrap_or(0).max(0).unsigned_abs())
    }
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
        let cursor = self.cursor_of(kind, log_id)?;
        if first.seq != cursor + 1 {
            return Err(StoreError::NotContiguous {
                cursor,
                first: first.seq,
            });
        }

        let tx = self
            .conn
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .map_err(backend)?;
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

    fn discard_all(&mut self) -> Result<(), StoreError> {
        self.conn
            .execute_batch("DELETE FROM cursors; DELETE FROM events; DELETE FROM messages;")
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
        let mut stmt = self
            .conn
            .prepare(
                "SELECT message_id, channel_id, seq, author_account_id, body, created_at_micros, deleted
                 FROM messages WHERE channel_id = ?1 ORDER BY seq DESC LIMIT ?2",
            )
            .map_err(backend)?;
        let rows = stmt
            .query_map(params![channel_id, to_i64(limit as u64)], |row| {
                let seq: i64 = row.get(2)?;
                Ok(StoredMessage {
                    message_id: row.get(0)?,
                    channel_id: row.get(1)?,
                    seq: seq.max(0).unsigned_abs(),
                    author_account_id: row.get(3)?,
                    body: row.get(4)?,
                    created_at_micros: row.get(5)?,
                    deleted: row.get::<_, i64>(6)? != 0,
                })
            })
            .map_err(backend)?;
        let mut messages: Vec<StoredMessage> = rows.collect::<Result<_, _>>().map_err(backend)?;
        messages.reverse();
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
                        message_id: m.message_id,
                        channel_id: m.channel_id,
                        seq: m.seq,
                        author_account_id: m.author_account_id,
                        body: m.body,
                        created_at_micros: wire::micros(m.created_at.as_ref()).unwrap_or(0),
                        deleted: false,
                    },
                )?;
            }
        }
        Some(event::Payload::MessageEdited(edited)) => {
            tx.execute(
                "UPDATE messages SET body = ?2 WHERE message_id = ?1",
                params![edited.message_id, edited.body],
            )
            .map_err(backend)?;
        }
        Some(event::Payload::MessageDeleted(deleted)) => {
            tx.execute(
                "UPDATE messages SET body = '', deleted = 1 WHERE message_id = ?1",
                params![deleted.message_id],
            )
            .map_err(backend)?;
        }
        Some(event::Payload::PayloadsPurged(purged)) => {
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
    tx.execute(
        "INSERT OR REPLACE INTO messages (message_id, channel_id, seq, author_account_id, body, created_at_micros, deleted)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![m.message_id, m.channel_id, to_i64(m.seq), m.author_account_id, m.body, m.created_at_micros, i64::from(m.deleted)],
    )
    .map(|_| ())
    .map_err(backend)
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
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| i64::try_from(d.as_micros()).unwrap_or(i64::MAX))
        .unwrap_or(0)
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
        store.set("k", b"a", b"1").unwrap();
        let result: Result<(), StoreError> = store.transaction(|s| {
            s.set("k", b"a", b"2")?;
            s.append("l", b"x", b"one")?;
            s.set_meta("m", "v")?;
            Err(StoreError::Backend("boom".into()))
        });
        assert!(result.is_err());
        assert_eq!(store.get("k", b"a").unwrap(), Some(b"1".to_vec()));
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
}
