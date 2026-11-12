//! The web shell's surface over the core (architecture section 13, plan
//! step 3 milestone 3): the sans-I/O [`Session`] as a JS class.
//!
//! The store is the JS side's: the browser's SQLite (WASM on OPFS in a
//! worker, plan step 6) owns events, cursors, and messages, and the shell
//! opens the socket and makes the HTTP calls. This crate only turns frames
//! into effects and effects into plain JS objects, so the JS loop is:
//!
//! ```text
//! const s = new Session("lumia-web/0.1", deviceId, cursors, accountCursor, generation);
//! carryOut(s.hello());
//! ws.onmessage = (m) => carryOut(...s.handle(new Uint8Array(m.data)));
//! ```
//!
//! where `carryOut` sends `{kind: "send"}` bytes on the socket, writes
//! `{kind: "apply"}` events and the cursor to the store in one transaction,
//! fetches for `{kind: "fetch"}` and calls `fetched`, and so on. Every
//! effect carries `kind`; ids are `Uint8Array` (16 bytes); sequence
//! numbers and nonces are JS numbers.
//!
//! Marshalling only. Nothing here is a decision.

#[cfg(target_arch = "wasm32")]
pub mod client;

use js_sys::{Array, Object, Reflect, Uint8Array};
use lumia_core::proto::{AccountEvent, Command, Event};
use lumia_core::sync::{decode_frame, encode_frame, Cursors, Effect, Notice};
use prost::Message;
use wasm_bindgen::prelude::*;

/// One gateway session's sync state for one instance.
#[wasm_bindgen]
#[derive(Debug)]
pub struct Session {
    inner: lumia_core::sync::Session,
}

#[wasm_bindgen]
impl Session {
    /// `cursors` is an array of `{channelId: Uint8Array, seq: number}`
    /// from the store; `generation` is the recovery generation last seen,
    /// or `undefined` on a fresh store.
    ///
    /// # Errors
    /// When `cursors` is not an array of such objects.
    #[wasm_bindgen(constructor)]
    pub fn new(
        client: &str,
        device_id: &[u8],
        cursors: &JsValue,
        account_cursor: f64,
        generation: Option<f64>,
    ) -> Result<Session, JsError> {
        let mut channels = Vec::new();
        if !cursors.is_undefined() && !cursors.is_null() {
            let array = Array::from(cursors);
            for entry in array.iter() {
                let id = Reflect::get(&entry, &"channelId".into())
                    .map_err(|_| JsError::new("cursor entry has no channelId"))?;
                let seq = Reflect::get(&entry, &"seq".into())
                    .ok()
                    .and_then(|v| v.as_f64())
                    .ok_or_else(|| JsError::new("cursor entry has no seq"))?;
                channels.push((Uint8Array::new(&id).to_vec(), to_u64(seq)));
            }
        }
        Ok(Self {
            inner: lumia_core::sync::Session::new(
                client,
                device_id.to_vec(),
                Cursors {
                    channels,
                    account: to_u64(account_cursor),
                    generation: generation.map(to_u64),
                },
            ),
        })
    }

    /// The `Hello` to send first.
    #[must_use]
    pub fn hello(&self) -> JsValue {
        effect(self.inner.hello())
    }

    /// Subscribes to a channel from `cursor`.
    #[must_use]
    pub fn subscribe(&mut self, channel_id: &[u8], cursor: f64) -> Array {
        effects(self.inner.subscribe(channel_id.to_vec(), to_u64(cursor)))
    }

    /// Sends a serialized `lumia.v1.Command`. The JS side keeps it in its
    /// outbox until `{kind: "notify", notice: {kind: "acked"}}`.
    ///
    /// # Errors
    /// When the bytes do not decode as a `Command`.
    pub fn command(&self, command: &[u8]) -> Result<JsValue, JsError> {
        let command = Command::decode(command).map_err(|e| JsError::new(&e.to_string()))?;
        Ok(effect(self.inner.command(command)))
    }

    /// A heartbeat with `nonce`.
    #[must_use]
    pub fn heartbeat(&self, nonce: f64) -> JsValue {
        effect(self.inner.heartbeat(to_u64(nonce)))
    }

    /// Asks for every subscribed head (the app came to the foreground).
    #[must_use]
    #[wasm_bindgen(js_name = highWaterMark)]
    pub fn high_water_mark(&self) -> JsValue {
        effect(self.inner.high_water_mark())
    }

    /// Handles a binary frame from the socket.
    ///
    /// # Errors
    /// When the bytes do not decode as a `ServerFrame`.
    pub fn handle(&mut self, frame: &[u8]) -> Result<Array, JsError> {
        let frame = decode_frame(frame).map_err(|e| JsError::new(&e.to_string()))?;
        Ok(effects(self.inner.handle(frame)))
    }

    /// The events a `{kind: "fetch"}` effect asked for, as an array of
    /// serialized `lumia.v1.Event` (`Uint8Array`), in `seq` order.
    ///
    /// # Errors
    /// When an entry does not decode as an `Event`.
    pub fn fetched(&mut self, channel_id: &[u8], events: &Array) -> Result<Array, JsError> {
        let events = decode_all::<Event>(events)?;
        Ok(effects(self.inner.fetched(channel_id, events)))
    }

    /// The events a `{kind: "fetchAccount"}` effect asked for.
    ///
    /// # Errors
    /// When an entry does not decode as an `AccountEvent`.
    #[wasm_bindgen(js_name = accountFetched)]
    pub fn account_fetched(&mut self, events: &Array) -> Result<Array, JsError> {
        let events = decode_all::<AccountEvent>(events)?;
        Ok(effects(self.inner.account_fetched(events)))
    }

    /// The store took a channel's snapshot path up to `seq`.
    #[must_use]
    #[wasm_bindgen(js_name = snapshotApplied)]
    pub fn snapshot_applied(&mut self, channel_id: &[u8], seq: f64) -> Array {
        effects(self.inner.snapshot_applied(channel_id, to_u64(seq)))
    }

    /// The store took the account snapshot path up to `seq`.
    #[must_use]
    #[wasm_bindgen(js_name = accountSnapshotApplied)]
    pub fn account_snapshot_applied(&mut self, seq: f64) -> Array {
        effects(self.inner.account_snapshot_applied(to_u64(seq)))
    }

    /// The session's cursor for a channel, if subscribed.
    #[must_use]
    pub fn cursor(&self, channel_id: &[u8]) -> Option<f64> {
        self.inner.cursor(channel_id).map(to_f64)
    }

    /// The session's account cursor.
    #[must_use]
    #[wasm_bindgen(js_name = accountCursor)]
    pub fn account_cursor(&self) -> f64 {
        to_f64(self.inner.account_cursor())
    }
}

fn decode_all<M: Message + Default>(events: &Array) -> Result<Vec<M>, JsError> {
    events
        .iter()
        .map(|entry| {
            M::decode(Uint8Array::new(&entry).to_vec().as_slice())
                .map_err(|e| JsError::new(&e.to_string()))
        })
        .collect()
}

fn effects(list: Vec<Effect>) -> Array {
    list.into_iter().map(effect).collect()
}

fn effect(e: Effect) -> JsValue {
    match e {
        Effect::Send(frame) => obj(&[
            ("kind", "send".into()),
            ("bytes", bytes(&encode_frame(&frame))),
        ]),
        Effect::Fetch {
            channel_id,
            after,
            before,
        } => obj(&[
            ("kind", "fetch".into()),
            ("channelId", bytes(&channel_id)),
            ("after", to_f64(after).into()),
            ("before", to_f64(before).into()),
        ]),
        Effect::FetchAccount { after } => obj(&[
            ("kind", "fetchAccount".into()),
            ("after", to_f64(after).into()),
        ]),
        Effect::Snapshot { channel_id } => obj(&[
            ("kind", "snapshot".into()),
            ("channelId", bytes(&channel_id)),
        ]),
        Effect::AccountSnapshot => obj(&[("kind", "accountSnapshot".into())]),
        Effect::Apply { channel_id, events } => obj(&[
            ("kind", "apply".into()),
            ("channelId", bytes(&channel_id)),
            (
                "events",
                encoded(events.iter().map(|e| (e.seq, e.encode_to_vec()))),
            ),
        ]),
        Effect::ApplyAccount { events } => obj(&[
            ("kind", "applyAccount".into()),
            (
                "events",
                encoded(events.iter().map(|e| (e.seq, e.encode_to_vec()))),
            ),
        ]),
        Effect::DiscardAll { generation } => obj(&[
            ("kind", "discardAll".into()),
            ("generation", to_f64(generation).into()),
        ]),
        Effect::Notify(notice) => obj(&[("kind", "notify".into()), ("notice", notify(notice))]),
    }
}

fn notify(n: Notice) -> JsValue {
    match n {
        Notice::Welcomed(w) => obj(&[
            ("kind", "welcomed".into()),
            ("accountId", bytes(&w.account_id)),
            ("sessionId", bytes(&w.session_id)),
            ("recoveryGeneration", to_f64(w.recovery_generation).into()),
            (
                "heartbeatIntervalSeconds",
                f64::from(w.heartbeat_interval_seconds).into(),
            ),
        ]),
        Notice::PresenceDiff(d) => obj(&[
            ("kind", "presenceDiff".into()),
            ("serverId", bytes(&d.server_id)),
            (
                "joined",
                f64::from(u32::try_from(d.joined.len()).unwrap_or(u32::MAX)).into(),
            ),
            (
                "updated",
                f64::from(u32::try_from(d.updated.len()).unwrap_or(u32::MAX)).into(),
            ),
            (
                "left",
                f64::from(u32::try_from(d.left_account_ids.len()).unwrap_or(u32::MAX)).into(),
            ),
        ]),
        Notice::Typing(t) => obj(&[
            ("kind", "typing".into()),
            ("channelId", bytes(&t.channel_id)),
            ("accountId", bytes(&t.account_id)),
        ]),
        Notice::Acked { command_id, seq } => obj(&[
            ("kind", "acked".into()),
            ("commandId", bytes(&command_id)),
            ("seq", to_f64(seq).into()),
        ]),
        Notice::Error(e) => obj(&[
            ("kind", "error".into()),
            ("code", error_code(e.code).into()),
            ("message", e.message.into()),
        ]),
        Notice::SubscriptionFailed { channel_id, error } => obj(&[
            ("kind", "subscriptionFailed".into()),
            ("channelId", bytes(&channel_id)),
            (
                "code",
                error
                    .as_ref()
                    .map_or(JsValue::UNDEFINED, |e| error_code(e.code).into()),
            ),
            (
                "message",
                error.map_or(JsValue::UNDEFINED, |e| e.message.into()),
            ),
        ]),
        Notice::Disconnected(d) => obj(&[
            ("kind", "disconnected".into()),
            (
                "reason",
                lumia_core::proto::DisconnectReason::try_from(d.reason)
                    .map(|r| r.as_str_name())
                    .unwrap_or("UNKNOWN")
                    .into(),
            ),
            (
                "reconnectAfterSeconds",
                f64::from(d.reconnect_after_seconds).into(),
            ),
        ]),
    }
}

/// `[{seq, bytes}]` for the store to write with its cursor.
fn encoded(events: impl Iterator<Item = (u64, Vec<u8>)>) -> JsValue {
    events
        .map(|(seq, b)| obj(&[("seq", to_f64(seq).into()), ("bytes", bytes(&b))]))
        .collect::<Array>()
        .into()
}

fn error_code(code: i32) -> &'static str {
    lumia_core::proto::ErrorCode::try_from(code)
        .map(|c| c.as_str_name())
        .unwrap_or("UNKNOWN")
}

fn obj(fields: &[(&str, JsValue)]) -> JsValue {
    let o = Object::new();
    for (k, v) in fields {
        // Setting a string key on a fresh object cannot fail.
        let _ = Reflect::set(&o, &JsValue::from_str(k), v);
    }
    o.into()
}

fn bytes(b: &[u8]) -> JsValue {
    Uint8Array::from(b).into()
}

/// Sequence numbers fit in a JS number far beyond any log's lifetime.
#[allow(clippy::cast_precision_loss)]
fn to_f64(n: u64) -> f64 {
    n as f64
}

#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn to_u64(n: f64) -> u64 {
    if n.is_finite() && n >= 0.0 {
        n as u64
    } else {
        0
    }
}
