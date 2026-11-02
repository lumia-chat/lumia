//! Scalar conventions from docs/protocol.md section 2: UUIDs travel as 16
//! raw bytes, timestamps as `google.protobuf.Timestamp`.

use uuid::Uuid;

/// A log id (channel or account) as it appears on the wire.
pub type LogId = Vec<u8>;

/// A UUID to its 16 wire bytes.
#[must_use]
pub fn uuid_bytes(id: Uuid) -> Vec<u8> {
    id.as_bytes().to_vec()
}

/// 16 wire bytes to a UUID. Returns `None` for any other length.
#[must_use]
pub fn uuid_from(bytes: &[u8]) -> Option<Uuid> {
    Uuid::from_slice(bytes).ok()
}

/// A fresh client-minted UUIDv7, used for command and message ids.
#[must_use]
pub fn new_id() -> Vec<u8> {
    uuid_bytes(Uuid::now_v7())
}

/// Renders wire bytes as a UUID string, or hex when they are not a UUID.
#[must_use]
pub fn display(bytes: &[u8]) -> String {
    match uuid_from(bytes) {
        Some(id) => id.to_string(),
        None => bytes.iter().map(|b| format!("{b:02x}")).collect(),
    }
}

/// Microseconds since the Unix epoch for a timestamp, or `None`.
#[must_use]
pub fn micros(ts: Option<&prost_types::Timestamp>) -> Option<i64> {
    let ts = ts?;
    Some(ts.seconds * 1_000_000 + i64::from(ts.nanos) / 1_000)
}
