//! The client as a shell sees it: one [`App`] per instance that opens the
//! stores, signs in, runs the gateway loop with the personal-DM engine
//! attached, and answers the screens' questions with plain summaries.
//! Both binding crates (`lumia-wasm` for the web worker, `lumia-ffi` for
//! the Swift and Kotlin shells) are thin wrappers over this, so the
//! logic lives once (architecture section 13).
//!
//! What differs per shell comes in through [`Opener`] (where the SQLite
//! stores live and under which key) and [`EventSink`] (how events reach
//! the UI); what differs per target (spawning, `Send` bounds) is confined
//! to this module's `cfg` lines.

use std::collections::HashMap;
use std::future::Future;
use std::sync::{Arc, Mutex, PoisonError};

use futures_util::lock::Mutex as AsyncMutex;
use futures_util::StreamExt as _;
use lumia_core::keystore::Keystore;
use lumia_core::mls::projection::Projection;
use lumia_core::proto::{
    command, dm_operation, event, mention, ChannelKind, Command, DeviceKind, DmOperation,
    DmRequest, DmRequestState, Event, PushKind, PushSubscription, SendMessage, TokenPair,
};
use lumia_core::store::{LogKind, Store, StoreError, StoredEvent};
use lumia_core::sync::Notice;
use lumia_core::wire;
use prost::Message as _;
use serde::Serialize;

use crate::api::{Api, Tokens};
use crate::dm::{DmEngine, Outcome};
use crate::futures_channel::mpsc;
use crate::gateway::{self, GatewayCommand, GatewayEvent};
use crate::identity::Device;
use crate::sqlite::{DmStatus, SqliteStore};
use crate::writer::{WriterGuard, WriterLock};
use crate::ClientError;

/// Which store a shell is asked to open (`Opener`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StoreName {
    /// This instance's events, messages, cursors, MLS state.
    Instance,
    /// This instance's secrets (tokens).
    Keys,
    /// The identity's roster and meta, shared by every instance on this device.
    IdentityStore,
    /// The identity's secrets (device key, backup key, link secret).
    IdentityKeys,
}

/// Opens a store by name; the shell decides the path, the VFS and the
/// at-rest key. Called more than once for `Instance` (the gateway loop,
/// a reader, the DM engine each hold a connection), and from the token
/// rotation callback, which is why it is `Send + Sync` on every target
/// (a web opener captures only paths and the key bytes).
pub type Opener = Arc<dyn Fn(StoreName) -> Result<SqliteStore, StoreError> + Send + Sync>;

/// Where events go: called in order, from a runtime thread natively and
/// from the worker's task on the web.
#[cfg(not(target_arch = "wasm32"))]
pub type EventSink = Arc<dyn Fn(AppEvent) + Send + Sync>;
/// Where events go (single-threaded on the web).
#[cfg(target_arch = "wasm32")]
pub type EventSink = Arc<dyn Fn(AppEvent)>;

#[cfg(not(target_arch = "wasm32"))]
fn spawn<F: Future<Output = ()> + Send + 'static>(f: F) {
    tokio::spawn(f);
}

#[cfg(target_arch = "wasm32")]
fn spawn<F: Future<Output = ()> + 'static>(f: F) {
    wasm_bindgen_futures::spawn_local(f);
}

// ---- Summaries, as the screens see them ------------------------------------------

/// The trust card (`GET /instance`).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstanceSummary {
    pub host: String,
    pub operator_name: String,
    pub operator_url: String,
    pub terms_url: String,
    pub privacy_url: String,
    pub software_version: String,
    pub identity_proof_sign_in_enabled: bool,
    pub identity_proof_create_enabled: bool,
    pub open_registration: bool,
    /// The VAPID public key for Web Push, base64url; empty when off.
    pub web_push_public_key: String,
    /// Whether the instance sends magic-link sign-in mail.
    pub magic_link_enabled: bool,
    /// The OIDC provider's name for the sign-in button; empty without one.
    pub oidc_name: String,
}

/// A server the account belongs to.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerSummary {
    pub server_id: String,
    pub name: String,
}

/// What an invite shows before joining (`GET /invites/{code}`): the
/// server, the instance's trust card in short, and what joining means.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InvitePreviewSummary {
    pub code: String,
    pub server_id: String,
    pub server_name: String,
    /// The channel a connected-channel invite names; empty otherwise.
    pub channel_name: String,
    pub member_count: u32,
    pub as_guest: bool,
    pub host: String,
    pub operator_name: String,
    pub open_registration: bool,
    pub identity_proof_create_enabled: bool,
}

/// An invite this account minted: its code and the link that carries it.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InviteSummary {
    pub code: String,
    /// `https://{host}/invite/{code}`, on the instance's own address.
    pub url: String,
    pub server_id: String,
    /// Unix microseconds; `None` for an invite that does not expire.
    pub expires_at_micros: Option<i64>,
    /// 0 is unlimited.
    pub max_uses: u32,
    pub uses: u32,
}

/// The identity's signed profile, as this device keeps it.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfileSummary {
    pub version: u64,
    pub display_name: String,
    pub pronouns: String,
    pub bio: String,
    pub avatar_url: String,
}

fn profile_summary(
    signed: &lumia_core::proto::SignedProfile,
) -> Result<ProfileSummary, ClientError> {
    let body = lumia_core::proto::ProfileBody::decode(signed.body.as_slice())?;
    Ok(ProfileSummary {
        version: body.version,
        display_name: body.display_name,
        pronouns: body.pronouns,
        bio: body.bio,
        avatar_url: body.avatar_url,
    })
}

/// A WebAuthn ceremony's start: the challenge's id and the options as the
/// browser's JSON, for the platform's own call.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PasskeyChallengeSummary {
    pub challenge_id: String,
    pub options_json: String,
}

/// One of the account's passkeys.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PasskeySummary {
    pub passkey_id: String,
    pub label: String,
    pub created_at_micros: Option<i64>,
    pub last_used_at_micros: Option<i64>,
}

/// `text` as a URL query value: everything but the unreserved characters
/// percent-encoded.
fn query_escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for byte in text.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(char::from(byte));
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

/// The token inside a magic-link mail's link, or the text itself when it
/// is the bare code: `https://host/sign-in?magic=abc` and `abc` both give `abc`.
#[must_use]
pub fn magic_token_of(text: &str) -> String {
    let text = text.trim();
    match text.split_once("magic=") {
        Some((_, rest)) => rest
            .split(['&', '#'])
            .next()
            .unwrap_or_default()
            .to_string(),
        None => text.to_string(),
    }
}

/// The code inside an invite link, or the text itself when it is a bare
/// code: `https://host/invite/abc` and `abc` both give `abc`.
#[must_use]
pub fn invite_code_of(text: &str) -> String {
    let text = text.trim();
    match text.rsplit_once("/invite/") {
        Some((_, rest)) => rest
            .split(['/', '?', '#'])
            .next()
            .unwrap_or_default()
            .to_string(),
        None => text.trim_matches('/').to_string(),
    }
}

/// A channel the account can view.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChannelSummary {
    pub channel_id: String,
    pub server_id: String,
    pub name: String,
    /// `ChannelKind` as its name (`TEXT`, `PERSONAL_DM`, ...).
    pub kind: String,
    pub last_seq: u64,
    /// DM kinds: who is in it.
    pub participant_account_ids: Vec<String>,
    /// Personal DMs: the hosting instance.
    pub host: String,
}

/// A DM request waiting for an answer.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DmRequestSummary {
    pub request_id: String,
    pub from_account_id: String,
    pub to_account_id: String,
    pub state: String,
    pub shared_server_ids: Vec<String>,
    pub mutual_friend_count: u32,
}

/// The account snapshot: who, which servers, which channels, what waits.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotSummary {
    pub account_id: String,
    pub handle: String,
    pub display_name: String,
    pub servers: Vec<ServerSummary>,
    pub channels: Vec<ChannelSummary>,
    pub pending_requests: Vec<DmRequestSummary>,
}

/// A person: a member of a server, or a public profile.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MemberSummary {
    pub account_id: String,
    pub handle: String,
    pub display_name: String,
    pub nickname: String,
}

/// A message as the local store holds it.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MessageSummary {
    pub message_id: String,
    pub channel_id: String,
    pub seq: u64,
    pub author_account_id: String,
    pub body: String,
    pub created_at_micros: i64,
    pub deleted: bool,
}

/// One applied channel event.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EventSummary {
    pub seq: u64,
    pub kind: String,
    pub message: Option<MessageSummary>,
}

/// A device of the identity's roster.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceSummary {
    pub device_id: String,
    pub kind: String,
    pub label: String,
    pub web_origin: String,
    pub added_at_micros: i64,
    pub this_device: bool,
    pub revoked: bool,
}

/// The identity's roster.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RosterSummary {
    pub identity_id: String,
    pub version: u64,
    pub devices: Vec<DeviceSummary>,
}

/// A personal conversation this device knows.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DmChannelSummary {
    pub channel_id: String,
    pub participant_account_ids: Vec<String>,
    /// Whether this device holds the group (can read and send).
    pub member: bool,
}

/// A message of a personal conversation, as projected.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DmMessageSummary {
    pub message_id: String,
    pub seq: u64,
    pub sender_identity_id: String,
    pub sender_device_id: String,
    /// The sender's account on this instance, when known.
    pub sender_account_id: String,
    pub body: String,
    pub sent_at_micros: i64,
    pub edited: bool,
    pub deleted: bool,
    pub mine: bool,
}

/// A personal conversation as this device can read it.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DmLogSummary {
    pub messages: Vec<DmMessageSummary>,
    /// Rows sent before this device joined (unreadable by design).
    pub before_join: u32,
    /// Rows that could not be decrypted.
    pub undecryptable: u32,
}

/// What opening a conversation found or did.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DmOpened {
    pub channel_id: String,
    pub existed: bool,
    pub member: bool,
    /// This device is creating the group; `DmReady` follows.
    pub creating: bool,
}

/// What the gateway loop and the DM engine report, in order.
#[derive(Debug, Clone, Serialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum AppEvent {
    Welcomed {
        account_id: String,
    },
    Applied {
        channel_id: String,
        events: Vec<EventSummary>,
    },
    AccountApplied {
        count: usize,
    },
    Acked {
        command_id: String,
        seq: u64,
    },
    Error {
        code: String,
        message: String,
        command_id: String,
    },
    SubscriptionFailed {
        channel_id: String,
        message: String,
    },
    SnapshotTaken {
        channel_id: String,
        seq: u64,
    },
    Disconnected {
        reason: String,
        reconnect_after_seconds: u32,
    },
    Reconnecting {
        attempt: u32,
        delay_millis: u64,
        reason: String,
    },
    Closed {
        reason: String,
    },
    /// The DM engine said something worth showing (a note, not an error).
    DmNote {
        message: String,
    },
    /// The engine failed on an event; the conversation may be stale.
    DmError {
        message: String,
    },
    /// A personal DM the account can now see was subscribed.
    DmSubscribed {
        channel_id: String,
    },
    /// A decrypted operation landed at `seq`; re-read the conversation.
    DmOperation {
        channel_id: String,
        seq: u64,
        sender_identity_id: String,
        sender_device_id: String,
    },
    /// The group for this conversation exists and its welcome was acked.
    DmReady {
        channel_id: String,
    },
    /// This device's operation was acked.
    DmDelivered {
        operation_id: String,
    },
    /// Who is present in a watched server changed: a first diff lists
    /// everyone there as joined.
    PresenceDiff {
        server_id: String,
        joined: Vec<PresenceSummary>,
        updated: Vec<PresenceSummary>,
        left_account_ids: Vec<String>,
    },
}

/// One account's presence in a server.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PresenceSummary {
    pub account_id: String,
    /// `ONLINE`, `AWAY` or `DO_NOT_DISTURB`.
    pub status: String,
    pub custom_status: String,
}

fn presence_summary(p: &lumia_core::proto::Presence) -> PresenceSummary {
    PresenceSummary {
        account_id: wire::display(&p.account_id),
        status: lumia_core::proto::PresenceStatus::try_from(p.status)
            .map(|s| s.as_str_name().to_string())
            .unwrap_or_else(|_| "ONLINE".into()),
        custom_status: p.custom_status.clone(),
    }
}

// ---- Conversions --------------------------------------------------------------

fn parse_uuid(s: &str) -> Result<Vec<u8>, ClientError> {
    uuid::Uuid::parse_str(s)
        .map(wire::uuid_bytes)
        .map_err(|_| ClientError::State(format!("not a uuid: {s}")))
}

fn tokens(keys: &SqliteStore) -> Result<Tokens, ClientError> {
    let read = |name: &str| -> Result<Option<String>, ClientError> {
        Ok(keys.get(name)?.and_then(|b| String::from_utf8(b).ok()))
    };
    Ok(Tokens {
        access: read("access_token")?,
        refresh: read("refresh_token")?,
    })
}

fn request_summary(r: &DmRequest) -> DmRequestSummary {
    DmRequestSummary {
        request_id: wire::display(&r.request_id),
        from_account_id: wire::display(&r.from_account_id),
        to_account_id: wire::display(&r.to_account_id),
        state: DmRequestState::try_from(r.state)
            .map(|s| s.as_str_name().to_string())
            .unwrap_or_default(),
        shared_server_ids: r
            .shared_server_ids
            .iter()
            .map(|id| wire::display(id))
            .collect(),
        mutual_friend_count: r.mutual_friend_count,
    }
}

fn message_summary(m: &lumia_core::proto::Message) -> MessageSummary {
    MessageSummary {
        message_id: wire::display(&m.message_id),
        channel_id: wire::display(&m.channel_id),
        seq: m.seq,
        author_account_id: wire::display(&m.author_account_id),
        body: m.body.clone(),
        created_at_micros: wire::micros(m.created_at.as_ref()).unwrap_or(0),
        deleted: m.deleted_at.is_some(),
    }
}

fn event_summary(e: &Event) -> EventSummary {
    let (kind, message) = match &e.payload {
        Some(event::Payload::MessageCreated(c)) => {
            ("message_created", c.message.as_ref().map(message_summary))
        }
        Some(event::Payload::MessageEdited(_)) => ("message_edited", None),
        Some(event::Payload::MessageDeleted(_)) => ("message_deleted", None),
        Some(event::Payload::Encrypted(_)) => ("encrypted", None),
        Some(event::Payload::Redacted(_)) => ("redacted", None),
        Some(event::Payload::PayloadsPurged(_)) => ("payloads_purged", None),
        Some(_) => ("other", None),
        None => ("empty", None),
    };
    EventSummary {
        seq: e.seq,
        kind: kind.into(),
        message,
    }
}

fn app_event(ev: GatewayEvent) -> AppEvent {
    match ev {
        GatewayEvent::Applied(events) => AppEvent::Applied {
            channel_id: events
                .first()
                .map(|e| wire::display(&e.channel_id))
                .unwrap_or_default(),
            events: events.iter().map(event_summary).collect(),
        },
        GatewayEvent::AccountApplied(events) => AppEvent::AccountApplied {
            count: events.len(),
        },
        GatewayEvent::Notice(Notice::Welcomed(w)) => AppEvent::Welcomed {
            account_id: wire::display(&w.account_id),
        },
        GatewayEvent::Notice(Notice::Acked { command_id, seq }) => AppEvent::Acked {
            command_id: wire::display(&command_id),
            seq,
        },
        GatewayEvent::Notice(Notice::Error(e)) => AppEvent::Error {
            code: lumia_core::proto::ErrorCode::try_from(e.code)
                .map(|c| c.as_str_name().to_string())
                .unwrap_or_else(|_| "UNKNOWN".into()),
            message: e.message,
            command_id: if e.command_id.is_empty() {
                String::new()
            } else {
                wire::display(&e.command_id)
            },
        },
        GatewayEvent::Notice(Notice::SubscriptionFailed { channel_id, error }) => {
            AppEvent::SubscriptionFailed {
                channel_id: wire::display(&channel_id),
                message: error.map(|e| e.message).unwrap_or_default(),
            }
        }
        GatewayEvent::Notice(Notice::PresenceDiff(d)) => AppEvent::PresenceDiff {
            server_id: wire::display(&d.server_id),
            joined: d.joined.iter().map(presence_summary).collect(),
            updated: d.updated.iter().map(presence_summary).collect(),
            left_account_ids: d
                .left_account_ids
                .iter()
                .map(|id| wire::display(id))
                .collect(),
        },
        GatewayEvent::Notice(Notice::Disconnected(d)) => AppEvent::Disconnected {
            reason: lumia_core::proto::DisconnectReason::try_from(d.reason)
                .map(|r| r.as_str_name().to_string())
                .unwrap_or_else(|_| "UNKNOWN".into()),
            reconnect_after_seconds: d.reconnect_after_seconds,
        },
        GatewayEvent::SnapshotTaken { channel_id, seq } => AppEvent::SnapshotTaken {
            channel_id: wire::display(&channel_id),
            seq,
        },
        GatewayEvent::Reconnecting {
            attempt,
            delay,
            reason,
        } => AppEvent::Reconnecting {
            attempt,
            delay_millis: u64::try_from(delay.as_millis()).unwrap_or(u64::MAX),
            reason,
        },
        GatewayEvent::Closed(reason) => AppEvent::Closed { reason },
    }
}

/// Carries out the DM engine's outcomes: commands to the gateway loop,
/// everything else to the sink.
fn dispatch(
    outcomes: Vec<Outcome>,
    commands: Option<&mpsc::UnboundedSender<GatewayCommand>>,
    sink: Option<&EventSink>,
) {
    let emit = |ev: AppEvent| {
        if let Some(s) = sink {
            s(ev);
        }
    };
    for outcome in outcomes {
        match outcome {
            Outcome::Print(message) => emit(AppEvent::DmNote { message }),
            Outcome::Send(cmd) => {
                if let Some(tx) = commands {
                    let _ = tx.unbounded_send(GatewayCommand::Send(cmd));
                }
            }
            Outcome::Subscribe(channel_id) => {
                if let Some(tx) = commands {
                    let _ = tx.unbounded_send(GatewayCommand::Subscribe(channel_id.clone()));
                }
                emit(AppEvent::DmSubscribed {
                    channel_id: wire::display(&channel_id),
                });
            }
            Outcome::Operation {
                channel_id,
                seq,
                sender,
                ..
            } => emit(AppEvent::DmOperation {
                channel_id: wire::display(&channel_id),
                seq,
                sender_identity_id: hex::encode(&sender.identity_id),
                sender_device_id: wire::display(&sender.device_id),
            }),
            Outcome::GroupReady(channel_id) => emit(AppEvent::DmReady {
                channel_id: wire::display(&channel_id),
            }),
            Outcome::Delivered(id) => emit(AppEvent::DmDelivered {
                operation_id: wire::display(&id),
            }),
        }
    }
}

fn roster_summary(roster: &lumia_core::identity::roster::Verified, me: &[u8]) -> RosterSummary {
    let mut devices: Vec<DeviceSummary> = roster
        .devices
        .iter()
        .map(|(id, info)| DeviceSummary {
            device_id: wire::display(id),
            kind: format!("{:?}", info.kind).to_lowercase(),
            label: String::new(),
            web_origin: info.web_origin.clone(),
            added_at_micros: i64::try_from(info.added_at / 1000).unwrap_or(0),
            this_device: id.as_slice() == me,
            revoked: false,
        })
        .collect();
    devices.extend(roster.revoked.keys().map(|id| DeviceSummary {
        device_id: wire::display(id),
        kind: String::new(),
        label: String::new(),
        web_origin: String::new(),
        added_at_micros: 0,
        this_device: id.as_slice() == me,
        revoked: true,
    }));
    RosterSummary {
        identity_id: hex::encode(&roster.identity_id),
        version: roster.version,
        devices,
    }
}

fn device<'a>(
    stores: &'a mut IdentityStores,
    device_id: &[u8],
) -> Device<'a, SqliteStore, SqliteStore> {
    Device {
        store: &mut stores.store,
        keystore: &mut stores.keys,
        device_id: device_id.to_vec(),
    }
}

/// The host part of a base URL, as a file-system-safe name: `host` or
/// `host_port`.
#[must_use]
pub fn host_of(url: &str) -> String {
    url::Url::parse(url)
        .ok()
        .and_then(|u| {
            u.host_str().map(|h| match u.port() {
                Some(p) => format!("{h}_{p}"),
                None => h.to_string(),
            })
        })
        .unwrap_or_else(|| url.replace(['/', ':'], "_"))
}

// ---- The app -------------------------------------------------------------------

struct Inner {
    /// The store the gateway loop owns while running; back here when it stops.
    store: Option<SqliteStore>,
    /// A second connection for reads while the loop runs.
    reader: SqliteStore,
    /// This instance's secrets (tokens).
    keys: SqliteStore,
    commands: Option<mpsc::UnboundedSender<GatewayCommand>>,
    /// The sink while the loop runs; DM outcomes go there too.
    sink: Option<EventSink>,
    /// The single-writer lock, held shared while this process is in the
    /// foreground (`writer.rs`); none on the web.
    writer_guard: Option<WriterGuard>,
    /// The store's writer generation as last seen by this process; another
    /// process bumping it means its writes must be reloaded.
    writer_generation: u64,
}

/// The identity's stores: their own lock, so an identity flow that awaits
/// the network (and holds them meanwhile) never blocks a store read behind
/// `Inner`; the UI runs one identity flow at a time.
struct IdentityStores {
    store: SqliteStore,
    keys: SqliteStore,
}

/// Where this device runs, which decides the kind of device it is in a
/// roster (identity spec): a web origin, or a native shell.
#[derive(Debug, Clone)]
pub enum Place {
    /// A browser page served by `origin`: a WEB device scoped to it.
    Web { origin: String },
    /// A native shell (or the desktop shell around the web UI): a NATIVE device.
    Native,
}

/// One instance's client. Cheap to share (`Arc`); every method takes `&self`.
pub struct App {
    api: Api,
    host: String,
    device_id: Vec<u8>,
    place: Place,
    opener: Opener,
    inner: Mutex<Inner>,
    identity: AsyncMutex<IdentityStores>,
    /// The personal-DM engine, once this device holds an identity. Behind
    /// an async mutex: the gateway task and the shell's calls take turns,
    /// each holding it across its own awaits (roster and key-package
    /// fetches).
    dm: Mutex<Option<Arc<AsyncMutex<DmEngine>>>>,
    writer: Option<WriterLock>,
}

impl std::fmt::Debug for App {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("App")
            .field("host", &self.host)
            .field("device_id", &wire::display(&self.device_id))
            .finish_non_exhaustive()
    }
}

impl App {
    /// Opens the client for the instance at `base_url`: the four stores
    /// through `opener`, this device's id (created on first use, kept in
    /// the identity keystore), and the tokens held for this instance.
    ///
    /// # Errors
    /// When a store cannot be opened.
    pub fn open(base_url: &str, place: Place, opener: Opener) -> Result<Arc<Self>, ClientError> {
        Self::open_with(base_url, place, opener, None)
    }

    /// `open`, with the single-writer lock at `lock_path` shared with the
    /// notification extension (architecture 15.5): taken shared here, so
    /// this process counts as the writer until `background`.
    ///
    /// # Errors
    /// When a store cannot be opened, or the lock cannot be taken.
    pub fn open_locked(
        base_url: &str,
        place: Place,
        opener: Opener,
        lock_path: &std::path::Path,
    ) -> Result<Arc<Self>, ClientError> {
        Self::open_with(base_url, place, opener, Some(WriterLock::new(lock_path)))
    }

    fn open_with(
        base_url: &str,
        place: Place,
        opener: Opener,
        writer: Option<WriterLock>,
    ) -> Result<Arc<Self>, ClientError> {
        let host = host_of(base_url);
        let writer_guard = writer.as_ref().map(WriterLock::shared).transpose()?;
        let mut store = opener(StoreName::Instance)?;
        store.meta_set("base_url", base_url.trim_end_matches('/'))?;
        let reader = opener(StoreName::Instance)?;
        let writer_generation = writer_generation(&reader)?;
        let keys = opener(StoreName::Keys)?;
        let identity_store = opener(StoreName::IdentityStore)?;
        let mut identity_keys = opener(StoreName::IdentityKeys)?;
        let device_id = match identity_keys.get("device_id")? {
            Some(id) => id,
            None => {
                let id = wire::new_id();
                identity_keys.put("device_id", &id)?;
                id
            }
        };
        let rotate = Arc::clone(&opener);
        let api = Api::new(base_url)
            .with_tokens(tokens(&keys)?)
            .on_rotate(Arc::new(move |pair| {
                if let Ok(mut keys) = rotate(StoreName::Keys) {
                    let _ = keys.put("access_token", pair.access_token.as_bytes());
                    let _ = keys.put("refresh_token", pair.refresh_token.as_bytes());
                }
            }));
        // On the web the sink is not `Send` (it holds a JS function) and
        // the worker is single-threaded; natively everything inside is.
        #[allow(clippy::arc_with_non_send_sync)]
        Ok(Arc::new(Self {
            api,
            host,
            device_id,
            place,
            opener,
            inner: Mutex::new(Inner {
                store: Some(store),
                reader,
                keys,
                commands: None,
                sink: None,
                writer_guard,
                writer_generation,
            }),
            identity: AsyncMutex::new(IdentityStores {
                store: identity_store,
                keys: identity_keys,
            }),
            dm: Mutex::new(None),
            writer,
        }))
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// The instance's host name (with the port when not the default).
    #[must_use]
    pub fn host(&self) -> String {
        self.host.clone()
    }

    /// This device's id (UUID string).
    #[must_use]
    pub fn device_id(&self) -> String {
        wire::display(&self.device_id)
    }

    /// Whether an access token is held for this instance.
    #[must_use]
    pub fn signed_in(&self) -> bool {
        self.api.tokens().access.is_some()
    }

    /// `GET /instance`: the trust card.
    ///
    /// # Errors
    /// On transport or API failure.
    pub async fn instance_info(&self) -> Result<InstanceSummary, ClientError> {
        let info = self.api.instance_info().await?;
        Ok(InstanceSummary {
            host: info.host,
            operator_name: info.operator_name,
            operator_url: info.operator_url,
            terms_url: info.terms_url,
            privacy_url: info.privacy_url,
            software_version: info.software_version,
            identity_proof_sign_in_enabled: info.identity_proof_sign_in_enabled,
            identity_proof_create_enabled: info.identity_proof_create_enabled,
            open_registration: info.open_registration,
            web_push_public_key: info.web_push_public_key,
            magic_link_enabled: info.magic_link_enabled,
            oidc_name: info.oidc_name,
        })
    }

    /// Asks the instance to mail a sign-in link to `email`.
    ///
    /// # Errors
    /// `POLICY_DENIED` when the instance sends no mail, `RATE_LIMITED`; transport failure.
    pub async fn request_magic_link(&self, email: &str) -> Result<(), ClientError> {
        self.api.magic_link_request(email.trim()).await
    }

    /// `POST /auth/passkey/register/start` (architecture 6.3): the creation
    /// options for a new passkey of this account.
    ///
    /// # Errors
    /// On transport or API failure.
    pub async fn passkey_register_start(&self) -> Result<PasskeyChallengeSummary, ClientError> {
        let c = self.api.passkey_register_start().await?;
        Ok(PasskeyChallengeSummary {
            challenge_id: c.challenge_id,
            options_json: c.options_json,
        })
    }

    /// `POST /auth/passkey/register/finish`: the credential the platform
    /// made (`PublicKeyCredential.toJSON()`), kept under `label`.
    ///
    /// # Errors
    /// `UNAUTHENTICATED` for a spent challenge, `ALREADY_EXISTS`; transport failure.
    pub async fn passkey_register_finish(
        &self,
        challenge_id: &str,
        credential_json: &str,
        label: &str,
    ) -> Result<(), ClientError> {
        self.api
            .passkey_register_finish(challenge_id, credential_json, label.trim())
            .await
    }

    /// `POST /auth/passkey/assert/start`: the request options to sign in
    /// with a passkey, for a handle when given.
    ///
    /// # Errors
    /// On transport or API failure.
    pub async fn passkey_assert_start(
        &self,
        handle_or_email: &str,
    ) -> Result<PasskeyChallengeSummary, ClientError> {
        let c = self
            .api
            .passkey_assert_start(handle_or_email.trim())
            .await?;
        Ok(PasskeyChallengeSummary {
            challenge_id: c.challenge_id,
            options_json: c.options_json,
        })
    }

    /// `POST /auth/passkey/assert/finish`: the assertion signs this device in
    /// and the session is kept.
    ///
    /// # Errors
    /// `UNAUTHENTICATED`; transport failure.
    pub async fn sign_in_passkey(
        &self,
        challenge_id: &str,
        credential_json: &str,
    ) -> Result<(), ClientError> {
        let pair = self
            .api
            .passkey_assert_finish(challenge_id, credential_json, self.device_id.clone())
            .await?;
        self.save_tokens(&pair)
    }

    /// `GET /auth/passkeys`: this account's passkeys.
    ///
    /// # Errors
    /// On transport or API failure.
    pub async fn passkeys(&self) -> Result<Vec<PasskeySummary>, ClientError> {
        let list = self.api.passkeys().await?;
        Ok(list
            .passkeys
            .into_iter()
            .map(|p| PasskeySummary {
                passkey_id: p.passkey_id,
                label: p.label,
                created_at_micros: wire::micros(p.created_at.as_ref()),
                last_used_at_micros: wire::micros(p.last_used_at.as_ref()),
            })
            .collect())
    }

    /// `DELETE /auth/passkeys/{id}`.
    ///
    /// # Errors
    /// `NOT_FOUND`; transport failure.
    pub async fn remove_passkey(&self, passkey_id: &str) -> Result<(), ClientError> {
        self.api.delete_passkey(passkey_id).await
    }

    /// Signs in with the token from a magic-link mail (pasted as a code, or
    /// taken from the link's `magic` query) and keeps the session.
    ///
    /// # Errors
    /// `UNAUTHENTICATED` for an unknown, used or expired token; transport failure.
    pub async fn sign_in_magic_link(&self, token: &str) -> Result<(), ClientError> {
        let token = magic_token_of(token);
        let pair = self
            .api
            .magic_link_complete(&token, self.device_id.clone())
            .await?;
        self.save_tokens(&pair)
    }

    /// A one-time sign-in token for this account (`POST /auth/handoff`),
    /// completed as a magic-link token is: the native apps hand their
    /// session to the instance's web page with it (to make a passkey
    /// there), and the page hands a sign-in back to an app the same way.
    ///
    /// # Errors
    /// On transport or API failure.
    pub async fn handoff_token(&self) -> Result<String, ClientError> {
        Ok(self.api.handoff().await?.token)
    }

    /// Where a native app sends the system browser to sign in with the
    /// instance's OIDC provider. The sheet comes back to `return_to` (the
    /// app's own `lumia:` URL) with `magic={token}`, which
    /// [`Self::sign_in_magic_link`] takes whole.
    #[must_use]
    pub fn oidc_start_url(&self, return_to: &str) -> String {
        format!(
            "{}/api/v1/auth/oidc/start?return={}",
            self.api.base(),
            query_escape(return_to)
        )
    }

    /// Where a native app sends the system browser to sign in through the
    /// instance's web page, with a passkey or any method the page offers:
    /// the page signs in, mints a handoff token and returns to `return_to`
    /// with it as `magic`.
    #[must_use]
    pub fn web_sign_in_url(&self, return_to: &str) -> String {
        format!(
            "{}/sign-in?return={}",
            self.api.base(),
            query_escape(return_to)
        )
    }

    /// Where a native app sends the system browser signed in as this
    /// account, by a token from [`Self::handoff_token`], landing on `next`
    /// (`devices` for the passkeys there).
    #[must_use]
    pub fn web_handoff_url(&self, token: &str, next: &str) -> String {
        format!(
            "{}/sign-in?magic={}&next={}",
            self.api.base(),
            query_escape(token),
            query_escape(next)
        )
    }

    /// Creates an account with a password and signs in.
    ///
    /// # Errors
    /// On API failure (`ALREADY_EXISTS`, `INVALID_ARGUMENT`).
    pub async fn create_account(
        &self,
        handle: &str,
        password: &str,
        invite: Option<&str>,
    ) -> Result<(), ClientError> {
        let code = invite.map(invite_code_of).unwrap_or_default();
        let pair = self.api.create_account(handle, "", password, &code).await?;
        self.save_tokens(&pair)
    }

    /// `POST /servers`: a server this account owns, with its `@everyone`
    /// role and the owner's membership. The next snapshot lists it.
    ///
    /// # Errors
    /// `INVALID_ARGUMENT` for an empty name; transport failure.
    pub async fn create_server(&self, name: &str) -> Result<ServerSummary, ClientError> {
        let s = self.api.create_server(name.trim()).await?;
        Ok(ServerSummary {
            server_id: wire::display(&s.server_id),
            name: s.name,
        })
    }

    /// `POST /channels`: a text channel in a server this account may
    /// manage channels in. The next snapshot lists it.
    ///
    /// # Errors
    /// `PERMISSION_DENIED` without `manage_channels` there; transport failure.
    pub async fn create_channel(
        &self,
        server_id: &str,
        name: &str,
    ) -> Result<ChannelSummary, ClientError> {
        let id = uuid::Uuid::parse_str(server_id)
            .map_err(|_| ClientError::Transport(format!("not a uuid: {server_id}")))?;
        let c = self
            .api
            .create_channel(wire::uuid_bytes(id), name.trim())
            .await?;
        Ok(ChannelSummary {
            channel_id: wire::display(&c.channel_id),
            server_id: wire::display(&c.server_id),
            name: c.name,
            kind: lumia_core::proto::ChannelKind::try_from(c.kind)
                .map(|k| k.as_str_name().to_string())
                .unwrap_or_default(),
            last_seq: 0,
            participant_account_ids: Vec::new(),
            host: String::new(),
        })
    }

    /// `GET /invites/{code}`: what an invite link or code shows before
    /// joining. Public: no sign-in needed.
    ///
    /// # Errors
    /// `NOT_FOUND` for an unknown, expired or spent code; transport failure.
    pub async fn invite_preview(
        &self,
        link_or_code: &str,
    ) -> Result<InvitePreviewSummary, ClientError> {
        let code = invite_code_of(link_or_code);
        let p = self.api.invite_preview(&code).await?;
        let server = p.server.unwrap_or_default();
        let instance = p.instance.unwrap_or_default();
        Ok(InvitePreviewSummary {
            code,
            server_id: wire::display(&server.server_id),
            server_name: server.name,
            channel_name: p.channel.map(|c| c.name).unwrap_or_default(),
            member_count: p.member_count,
            as_guest: p.as_guest,
            host: instance.host,
            operator_name: instance.operator_name,
            open_registration: instance.open_registration,
            identity_proof_create_enabled: instance.identity_proof_create_enabled,
        })
    }

    /// `POST /invites`: an invite link for a server, unlimited unless
    /// `max_uses` is set, open-ended unless `expires_in_hours` is.
    ///
    /// # Errors
    /// `PERMISSION_DENIED` without `create_invites` there; transport failure.
    pub async fn create_invite(
        &self,
        server_id: &str,
        max_uses: u32,
        expires_in_hours: Option<u32>,
    ) -> Result<InviteSummary, ClientError> {
        let expires_at = expires_in_hours.map(|h| {
            let at =
                std::time::SystemTime::now() + std::time::Duration::from_secs(u64::from(h) * 3600);
            prost_types::Timestamp::from(at)
        });
        let req = lumia_core::proto::CreateInviteRequest {
            server_id: wire::uuid_bytes(
                uuid::Uuid::parse_str(server_id)
                    .map_err(|_| ClientError::Transport(format!("not a uuid: {server_id}")))?,
            ),
            max_uses,
            expires_at,
            ..Default::default()
        };
        let invite = self.api.create_invite(&req).await?;
        Ok(self.invite_summary(&invite))
    }

    /// `POST /invites/{code}/accept`: joins this account to the invite's
    /// server. Returns the server's id; the next snapshot lists it.
    ///
    /// # Errors
    /// `NOT_FOUND` for an unknown, expired or spent code; transport failure.
    pub async fn accept_invite(&self, link_or_code: &str) -> Result<String, ClientError> {
        let member = self
            .api
            .accept_invite(&invite_code_of(link_or_code))
            .await?;
        Ok(wire::display(&member.server_id))
    }

    /// `GET /servers/{id}/invites`: the server's invites, newest first,
    /// for a member who may invite.
    ///
    /// # Errors
    /// `PERMISSION_DENIED` without `create_invites` there; transport failure.
    pub async fn invites(&self, server_id: &str) -> Result<Vec<InviteSummary>, ClientError> {
        let list = self.api.invites(server_id).await?;
        Ok(list
            .invites
            .iter()
            .map(|i| self.invite_summary(i))
            .collect())
    }

    /// `DELETE /invites/{code}`: revokes an invite this account made, or
    /// any with `manage_server`.
    ///
    /// # Errors
    /// `PERMISSION_DENIED` otherwise; transport failure.
    pub async fn revoke_invite(&self, link_or_code: &str) -> Result<(), ClientError> {
        self.api.delete_invite(&invite_code_of(link_or_code)).await
    }

    fn invite_summary(&self, invite: &lumia_core::proto::Invite) -> InviteSummary {
        InviteSummary {
            code: invite.code.clone(),
            url: format!(
                "{}/invite/{}",
                self.api.base().trim_end_matches('/'),
                invite.code
            ),
            server_id: wire::display(&invite.server_id),
            expires_at_micros: wire::micros(invite.expires_at.as_ref()),
            max_uses: invite.max_uses,
            uses: invite.uses,
        }
    }

    /// Signs in with a password, and a TOTP code when the account requires one.
    ///
    /// # Errors
    /// `UNAUTHENTICATED` for a wrong password or code, `FAILED_PRECONDITION`
    /// with `mfa` when a code is required and not given.
    pub async fn sign_in(
        &self,
        handle: &str,
        password: &str,
        totp_code: Option<&str>,
    ) -> Result<(), ClientError> {
        let pair = match totp_code.filter(|c| !c.is_empty()) {
            Some(code) => {
                self.api
                    .sign_in_with_totp(handle, password, self.device_id.clone(), code)
                    .await?
            }
            None => {
                self.api
                    .sign_in(handle, password, self.device_id.clone())
                    .await?
            }
        };
        self.save_tokens(&pair)
    }

    /// Signs out: revokes the session and forgets the tokens.
    ///
    /// # Errors
    /// On a store failure; an API refusal is ignored, the tokens go anyway.
    pub async fn sign_out(&self) -> Result<(), ClientError> {
        let _ = self.api.sign_out().await;
        {
            let mut inner = self.lock();
            inner.keys.delete("access_token")?;
            inner.keys.delete("refresh_token")?;
        }
        self.api.clear_tokens();
        Ok(())
    }

    /// `GET /account/snapshot`: the account, its servers, the channels it
    /// can view (the servers' and its personal DMs), the requests waiting.
    ///
    /// # Errors
    /// On transport or API failure.
    pub async fn snapshot(&self) -> Result<SnapshotSummary, ClientError> {
        let snap = self.api.account_snapshot().await?;
        let account = snap.account.unwrap_or_default();
        Ok(SnapshotSummary {
            account_id: wire::display(&account.account_id),
            handle: account.handle,
            display_name: account.display_name,
            servers: snap
                .servers
                .iter()
                .map(|s| ServerSummary {
                    server_id: wire::display(&s.server_id),
                    name: s.name.clone(),
                })
                .collect(),
            channels: snap
                .channels
                .iter()
                .map(|c| ChannelSummary {
                    channel_id: wire::display(&c.channel_id),
                    server_id: wire::display(&c.server_id),
                    name: c.name.clone(),
                    kind: lumia_core::proto::ChannelKind::try_from(c.kind)
                        .map(|k| k.as_str_name().to_string())
                        .unwrap_or_default(),
                    last_seq: c.last_seq,
                    participant_account_ids: c
                        .participant_account_ids
                        .iter()
                        .map(|id| wire::display(id))
                        .collect(),
                    host: c.host.clone(),
                })
                .collect(),
            pending_requests: snap.pending_requests.iter().map(request_summary).collect(),
        })
    }

    /// `GET /accounts/{id}`: another account's public profile, when
    /// visible (a shared server or personal DM, or a request between).
    ///
    /// # Errors
    /// `NOT_FOUND` when not visible; transport failure.
    pub async fn account(&self, account_id: &str) -> Result<MemberSummary, ClientError> {
        let a = self.api.account(account_id).await?;
        Ok(MemberSummary {
            account_id: wire::display(&a.account_id),
            handle: a.handle,
            display_name: a.display_name,
            nickname: String::new(),
        })
    }

    /// `GET /servers/{id}/members`: a server's members with their
    /// accounts, optionally filtered by a handle or nickname prefix.
    ///
    /// # Errors
    /// On transport or API failure.
    pub async fn members(
        &self,
        server_id: &str,
        query: &str,
    ) -> Result<Vec<MemberSummary>, ClientError> {
        let page = self.api.members(server_id, query).await?;
        Ok(page
            .members
            .iter()
            .map(|m| {
                let account = page.accounts.iter().find(|a| a.account_id == m.account_id);
                MemberSummary {
                    account_id: wire::display(&m.account_id),
                    handle: account.map(|a| a.handle.clone()).unwrap_or_default(),
                    display_name: account.map(|a| a.display_name.clone()).unwrap_or_default(),
                    nickname: m.nickname.clone(),
                }
            })
            .collect())
    }

    /// The newest messages of a channel from the local store, oldest first.
    ///
    /// # Errors
    /// On a store failure or a malformed id.
    pub fn messages(
        &self,
        channel_id: &str,
        limit: u32,
    ) -> Result<Vec<MessageSummary>, ClientError> {
        let id = parse_uuid(channel_id)?;
        let inner = self.lock();
        let rows = inner.reader.messages(&id, limit as usize)?;
        Ok(rows
            .iter()
            .map(|m| MessageSummary {
                message_id: wire::display(&m.message_id),
                channel_id: wire::display(&m.channel_id),
                seq: m.seq,
                author_account_id: wire::display(&m.author_account_id),
                body: m.body.clone(),
                created_at_micros: m.created_at_micros,
                deleted: m.deleted,
            })
            .collect())
    }

    /// Starts the gateway loop, subscribing to `channel_ids` from the
    /// store's cursors, plus the personal DMs this device already knows.
    /// `sink` gets every event, in order, until `Closed` or `stop`. Call
    /// from inside the shell's runtime natively (the loop is spawned there).
    ///
    /// # Errors
    /// When not signed in, already started, or an id is malformed.
    pub fn start(
        self: &Arc<Self>,
        channel_ids: &[String],
        sink: EventSink,
    ) -> Result<(), ClientError> {
        if !self.signed_in() {
            return Err(ClientError::NotSignedIn);
        }
        let mut channels = channel_ids
            .iter()
            .map(|id| parse_uuid(id))
            .collect::<Result<Vec<_>, _>>()?;
        // Personal DMs whose participants this device knows are subscribed
        // from the start; the rest when the account log names them, so
        // their events are never processed before the rosters can be
        // fetched (the CLI does the same).
        if let Ok(engine) = self.dm_engine() {
            if let Some(e) = engine.try_lock() {
                for channel in e.channels().keys() {
                    if !channels.contains(channel) {
                        channels.push(channel.clone());
                    }
                }
            }
        }
        let mut inner = self.lock();
        let store = inner
            .store
            .take()
            .ok_or_else(|| ClientError::State("already started".into()))?;
        let (tx, rx) = mpsc::unbounded();
        let (etx, mut erx) = mpsc::unbounded();
        let api = self.api.clone();
        let device_id = self.device_id.clone();
        let back = Arc::clone(self);
        spawn(async move {
            let store =
                gateway::run_forever(api, store, "lumia-app", device_id, channels, rx, etx).await;
            let mut inner = back.lock();
            inner.store = Some(store);
            inner.commands = None;
            inner.sink = None;
        });
        let app = Arc::clone(self);
        let dtx = tx.clone();
        let task_sink = Arc::clone(&sink);
        spawn(async move {
            while let Some(ev) = erx.next().await {
                if let GatewayEvent::Notice(Notice::Welcomed(w)) = &ev {
                    let _ = app
                        .lock()
                        .reader
                        .meta_set("account_id", &hex::encode(&w.account_id));
                }
                let engine = app
                    .dm
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .clone();
                if let Some(engine) = engine {
                    let mut e = engine.lock().await;
                    let outcomes = match &ev {
                        GatewayEvent::Notice(Notice::Welcomed(w)) => {
                            e.set_account(w.account_id.clone());
                            e.on_welcome(w).await
                        }
                        GatewayEvent::Notice(n) => e.on_notice(n).await,
                        GatewayEvent::Applied(events) => e.on_events(events).await,
                        GatewayEvent::AccountApplied(events) => e.on_account_events(events).await,
                        _ => Ok(Vec::new()),
                    };
                    drop(e);
                    match outcomes {
                        Ok(out) => dispatch(out, Some(&dtx), Some(&task_sink)),
                        Err(err) => task_sink(AppEvent::DmError {
                            message: err.to_string(),
                        }),
                    }
                }
                task_sink(app_event(ev));
            }
        });
        inner.commands = Some(tx);
        inner.sink = Some(sink);
        Ok(())
    }

    /// Stops the gateway loop; the store comes back once it ends.
    pub fn stop(&self) {
        self.lock().commands = None;
    }

    /// Whether the gateway loop is running.
    #[must_use]
    pub fn started(&self) -> bool {
        self.lock().commands.is_some()
    }

    /// Subscribes to a channel from the store's cursor.
    ///
    /// # Errors
    /// When not started or the id is malformed.
    pub fn subscribe(&self, channel_id: &str) -> Result<(), ClientError> {
        self.send_command(GatewayCommand::Subscribe(parse_uuid(channel_id)?))
    }

    /// This client's status (`ONLINE`, `AWAY` or `DO_NOT_DISTURB`) and the
    /// servers it has on screen, whose presence it then receives as
    /// `PresenceDiff` events; the set replaces the last one.
    ///
    /// # Errors
    /// When not started or an id is malformed.
    pub fn set_presence(
        &self,
        status: &str,
        watch_server_ids: &[String],
    ) -> Result<(), ClientError> {
        let status = match status {
            "AWAY" => lumia_core::proto::PresenceStatus::Away,
            "DO_NOT_DISTURB" => lumia_core::proto::PresenceStatus::DoNotDisturb,
            _ => lumia_core::proto::PresenceStatus::Online,
        };
        let mut ids = Vec::with_capacity(watch_server_ids.len());
        for id in watch_server_ids {
            ids.push(parse_uuid(id)?);
        }
        self.send_command(GatewayCommand::Presence(
            lumia_core::proto::PresenceUpdate {
                status: status.into(),
                custom_status: String::new(),
                watch_server_ids: ids,
            },
        ))
    }

    /// Sends a message; returns the client-minted message id.
    ///
    /// # Errors
    /// When not started or the id is malformed.
    pub fn send_message(&self, channel_id: &str, body: &str) -> Result<String, ClientError> {
        let channel = parse_uuid(channel_id)?;
        let id = wire::new_id();
        self.send_command(GatewayCommand::Send(Command {
            command_id: id.clone(),
            channel_id: channel,
            op: Some(command::Op::SendMessage(SendMessage {
                body: body.to_string(),
                ..Default::default()
            })),
        }))?;
        Ok(wire::display(&id))
    }

    /// The app came back to the foreground: ask for every head.
    ///
    /// # Errors
    /// When not started.
    pub fn foreground(&self) -> Result<(), ClientError> {
        self.send_command(GatewayCommand::Foreground)
    }

    /// The app is going to the background: the single-writer lock is
    /// released so the notification extension can take it (the gateway
    /// loop is left to the platform, which suspends the process).
    pub fn background(&self) {
        self.lock().writer_guard = None;
    }

    /// The app is back in the foreground: the single-writer lock is taken
    /// shared again (waiting for an extension mid-run), and the answer is
    /// whether another process wrote meanwhile, in which case the gateway
    /// loop must be restarted so it resumes from the store's cursors.
    ///
    /// # Errors
    /// When the lock cannot be taken.
    pub fn resume(&self) -> Result<bool, ClientError> {
        let Some(writer) = &self.writer else {
            return Ok(false);
        };
        let guard = writer.shared()?;
        let mut inner = self.lock();
        inner.writer_guard = Some(guard);
        let now = writer_generation(&inner.reader)?;
        let changed = now != inner.writer_generation;
        inner.writer_generation = now;
        Ok(changed)
    }

    /// What a push should say, for the notification extension (architecture
    /// 15.5): under the exclusive writer lock, every channel of the account
    /// is brought up to the instance's head over HTTP and applied to the
    /// store, personal conversations are decrypted through the engine, and
    /// the newest message for this account (a personal DM, or a server
    /// message that mentions it) comes back. `None` when the app holds the
    /// lock (it is live and shows the message itself), when nothing new was
    /// readable, or when this device is not signed in. The store's writer
    /// generation is bumped so the app reloads.
    ///
    /// # Errors
    /// On a store failure; API failures on one channel leave it out.
    pub async fn notification_content(
        &self,
        timeout: std::time::Duration,
    ) -> Result<Option<NotificationContent>, ClientError> {
        let Some(writer) = &self.writer else {
            return Err(ClientError::State("no writer lock".into()));
        };
        // This client's own shared hold, taken at open, would block its
        // exclusive take (a lock is per open, and the in-process one counts
        // holders alike); it is let go for the duration and taken again.
        self.lock().writer_guard = None;
        let result = match writer.exclusive(timeout)? {
            Some(guard) => {
                let result = self.notification_content_exclusive().await;
                drop(guard);
                result
            }
            None => Ok(None),
        };
        self.lock().writer_guard = Some(writer.shared()?);
        result
    }

    /// The work of `notification_content`, under the exclusive lock.
    async fn notification_content_exclusive(
        &self,
    ) -> Result<Option<NotificationContent>, ClientError> {
        if !self.signed_in() {
            return Ok(None);
        }
        let snap = self.api.account_snapshot().await?;
        let me = snap.account.map(|a| a.account_id).unwrap_or_default();
        let mut store = (self.opener)(StoreName::Instance)?;
        let cursors: HashMap<Vec<u8>, u64> = store.cursors(LogKind::Channel)?.into_iter().collect();
        let mut best: Option<(i64, NotificationContent)> = None;
        let mut names: HashMap<Vec<u8>, String> = HashMap::new();
        for channel in &snap.channels {
            let cursor = cursors.get(&channel.channel_id).copied().unwrap_or(0);
            if channel.last_seq <= cursor {
                continue;
            }
            // Up to the head, in pages, bounded: a device far behind takes
            // the snapshot path when the app next runs.
            let id = wire::display(&channel.channel_id);
            let mut after = cursor;
            let mut applied: Vec<Event> = Vec::new();
            while after < channel.last_seq && applied.len() < 500 {
                let Ok(page) = self.api.channel_events(&id, after, 0).await else {
                    break;
                };
                if page.events.is_empty() {
                    break;
                }
                let stored: Vec<StoredEvent> = page
                    .events
                    .iter()
                    .map(|e| StoredEvent {
                        seq: e.seq,
                        bytes: e.encode_to_vec(),
                    })
                    .collect();
                after = stored.last().map_or(after, |e| e.seq);
                store.apply(LogKind::Channel, &channel.channel_id, &stored)?;
                applied.extend(page.events);
            }
            let personal = matches!(
                ChannelKind::try_from(channel.kind),
                Ok(ChannelKind::PersonalDm | ChannelKind::PersonalGroupDm)
            );
            if personal {
                let Ok(engine) = self.dm_engine() else {
                    continue;
                };
                let mut e = engine.lock().await;
                e.record_channel(channel)?;
                let outcomes = match e.catch_up(&channel.channel_id).await {
                    Ok(o) => o,
                    Err(_) => continue,
                };
                let my_identity = e.me().identity_id.clone();
                for outcome in outcomes {
                    let Outcome::Operation { sender, op, .. } = outcome else {
                        continue;
                    };
                    if sender.identity_id == my_identity {
                        continue;
                    }
                    let Some(dm_operation::Op::Send(send)) = op.op else {
                        continue;
                    };
                    let account = e
                        .account_of(&sender.identity_id)
                        .cloned()
                        .unwrap_or_default();
                    let title = self.name_of(&account, &mut names).await;
                    let at = op
                        .sent_at
                        .as_ref()
                        .map_or(0, |t| t.seconds * 1_000_000 + i64::from(t.nanos) / 1000);
                    consider(
                        &mut best,
                        at,
                        NotificationContent {
                            title,
                            body: send.body,
                            channel_id: id.clone(),
                            kind: "dm".into(),
                        },
                    );
                }
            } else {
                for ev in applied {
                    let Some(event::Payload::MessageCreated(created)) = ev.payload else {
                        continue;
                    };
                    let Some(message) = created.message else {
                        continue;
                    };
                    if ev.actor_account_id == me || me.is_empty() {
                        continue;
                    }
                    let mentioned = message.mentions.iter().any(
                        |m| matches!(&m.target, Some(mention::Target::AccountId(id)) if *id == me),
                    );
                    if !mentioned {
                        continue;
                    }
                    let author = self.name_of(&ev.actor_account_id, &mut names).await;
                    let title = if channel.name.is_empty() {
                        author
                    } else {
                        format!("{author} in #{}", channel.name)
                    };
                    let at = ev
                        .created_at
                        .as_ref()
                        .map_or(0, |t| t.seconds * 1_000_000 + i64::from(t.nanos) / 1000);
                    consider(
                        &mut best,
                        at,
                        NotificationContent {
                            title,
                            body: message.body,
                            channel_id: id.clone(),
                            kind: "mention".into(),
                        },
                    );
                }
            }
        }
        let generation = writer_generation(&store)? + 1;
        store.meta_set("writer_generation", &generation.to_string())?;
        self.lock().writer_generation = generation;
        Ok(best.map(|(_, content)| content))
    }

    /// An account's name for a notice: display name, else handle, else "Someone".
    async fn name_of(&self, account_id: &[u8], names: &mut HashMap<Vec<u8>, String>) -> String {
        if let Some(name) = names.get(account_id) {
            return name.clone();
        }
        let name = if account_id.is_empty() {
            "Someone".to_string()
        } else {
            match self.api.account(&wire::display(account_id)).await {
                Ok(a) if !a.display_name.is_empty() => a.display_name,
                Ok(a) if !a.handle.is_empty() => a.handle,
                _ => "Someone".to_string(),
            }
        };
        names.insert(account_id.to_vec(), name.clone());
        name
    }

    // ---- Identity ----

    /// Whether this device holds an identity roster.
    #[must_use]
    pub fn has_identity(&self) -> bool {
        match self.identity.try_lock() {
            Some(stores) => stores.store.meta_get("roster").ok().flatten().is_some(),
            None => true,
        }
    }

    /// Creates a new identity on this device; the recovery phrase is shown
    /// once and never stored.
    ///
    /// # Errors
    /// When this device already holds an identity, or on a store failure.
    pub async fn identity_create(&self, label: &str) -> Result<(String, String), ClientError> {
        let mut stores = self.identity.lock().await;
        let created = device(&mut stores, &self.device_id).create(label)?;
        Ok((created.identity_id.to_string(), created.phrase))
    }

    /// The identity's signed profile, if one was set on this device.
    ///
    /// # Errors
    /// On a store failure.
    pub async fn identity_profile(&self) -> Result<Option<ProfileSummary>, ClientError> {
        let local = {
            let mut stores = self.identity.lock().await;
            device(&mut stores, &self.device_id).profile()?
        };
        match local {
            Some(signed) => Ok(Some(profile_summary(&signed)?)),
            // Another device of the identity may have set one: the account
            // holds it, and this device shows it until it signs its own.
            None => match self.held_profile().await {
                Some(signed) => Ok(Some(profile_summary(&signed)?)),
                None => Ok(None),
            },
        }
    }

    /// The signed profile the instance holds for this account, when signed in.
    async fn held_profile(&self) -> Option<lumia_core::proto::SignedProfile> {
        if !self.signed_in() {
            return None;
        }
        let snap = self.api.account_snapshot().await.ok()?;
        snap.account?.profile
    }

    /// Signs a new profile with this device's key, keeps it, and when
    /// signed in to this instance sends it as the account's profile.
    ///
    /// # Errors
    /// When this device holds no identity, on a store failure, or when the
    /// instance refuses it.
    pub async fn identity_set_profile(
        &self,
        display_name: &str,
        pronouns: &str,
        bio: &str,
        avatar_url: &str,
    ) -> Result<ProfileSummary, ClientError> {
        // The version must pass the one the instance holds, which another
        // device of the identity may have set.
        let held = match self.held_profile().await {
            Some(signed) => Some(profile_summary(&signed)?.version),
            None => None,
        };
        let signed = {
            let mut stores = self.identity.lock().await;
            device(&mut stores, &self.device_id).set_profile(
                display_name,
                pronouns,
                bio,
                avatar_url,
                held,
            )?
        };
        if self.signed_in() {
            self.api.put_profile(signed.clone()).await?;
        }
        profile_summary(&signed)
    }

    /// Binds the signed-in password account to this device's identity.
    ///
    /// # Errors
    /// On API failure or a wrong password.
    pub async fn identity_link_account(&self, password: &str) -> Result<(), ClientError> {
        let api = self.api.clone();
        let mut stores = self.identity.lock().await;
        device(&mut stores, &self.device_id)
            .link_account(&api, password)
            .await
    }

    /// Step 1 of linking this device (identity spec 4.2): the request to
    /// show as a code or QR for a device that holds the identity to approve.
    ///
    /// # Errors
    /// On a store or keystore failure.
    pub async fn identity_link_request(&self, label: &str) -> Result<String, ClientError> {
        let (kind, origin) = match &self.place {
            Place::Web { origin } => (DeviceKind::Web, origin.clone()),
            Place::Native => (DeviceKind::Native, String::new()),
        };
        let mut stores = self.identity.lock().await;
        device(&mut stores, &self.device_id).link_request_as(label, kind, &origin)
    }

    /// On a device that holds the identity: approves a link request and
    /// returns the code the new device claims with.
    ///
    /// # Errors
    /// On a malformed request or API failure.
    pub async fn identity_approve(&self, request: &str) -> Result<String, ClientError> {
        let api = self.api.clone();
        let mut stores = self.identity.lock().await;
        device(&mut stores, &self.device_id)
            .approve(&api, request)
            .await
    }

    /// Step 7: claims the bundle an approving device uploaded, then signs
    /// in by identity proof.
    ///
    /// # Errors
    /// `NOT_FOUND` for a wrong or used code, or when the roster does not
    /// include this device.
    pub async fn identity_claim(&self, code: &str) -> Result<RosterSummary, ClientError> {
        let api = self.api.clone();
        let mut stores = self.identity.lock().await;
        let mut dev = device(&mut stores, &self.device_id);
        let roster = dev.claim(&api, code).await?;
        let pair = dev.sign_in(&api).await?;
        drop(stores);
        self.save_tokens(&pair)?;
        Ok(roster_summary(&roster, &self.device_id))
    }

    /// Signs in by identity proof with the roster this device holds.
    ///
    /// # Errors
    /// On API failure, or when the device is revoked in the held roster.
    pub async fn sign_in_identity(&self) -> Result<(), ClientError> {
        let api = self.api.clone();
        let mut stores = self.identity.lock().await;
        let pair = device(&mut stores, &self.device_id).sign_in(&api).await?;
        drop(stores);
        self.save_tokens(&pair)
    }

    /// The roster this device holds.
    ///
    /// # Errors
    /// When there is none.
    pub async fn identity_show(&self) -> Result<RosterSummary, ClientError> {
        let mut stores = self.identity.lock().await;
        let roster = device(&mut stores, &self.device_id).roster()?;
        Ok(roster_summary(&roster, &self.device_id))
    }

    /// Revokes another device of this identity, signed by this device with
    /// the account password as reauth (identity spec 4.4).
    ///
    /// # Errors
    /// On API failure, a wrong password, or when asked to revoke this device.
    pub async fn identity_revoke(
        &self,
        device_id: &str,
        password: &str,
    ) -> Result<RosterSummary, ClientError> {
        let target = parse_uuid(device_id)?;
        let api = self.api.clone();
        let mut stores = self.identity.lock().await;
        let mut dev = device(&mut stores, &self.device_id);
        dev.revoke_with_device(&api, password, &target).await?;
        let roster = dev.roster()?;
        Ok(roster_summary(&roster, &self.device_id))
    }

    // ---- Push ----

    /// Registers this device's Web Push subscription with the instance.
    ///
    /// # Errors
    /// `INVALID_ARGUMENT` for a malformed subscription; transport failure.
    pub async fn push_subscribe(
        &self,
        kind: PushKind,
        endpoint: &str,
        p256dh: &str,
        auth: &str,
    ) -> Result<(), ClientError> {
        self.api
            .put_push_subscription(&PushSubscription {
                kind: kind as i32,
                endpoint: endpoint.to_string(),
                p256dh: p256dh.to_string(),
                auth: auth.to_string(),
            })
            .await
    }

    /// Removes this device's push subscription of a kind from the instance.
    ///
    /// # Errors
    /// On transport or API failure.
    pub async fn push_unsubscribe(&self, kind: &str) -> Result<(), ClientError> {
        self.api.delete_push_subscription(kind).await
    }

    // ---- Personal DMs ----

    /// Whether personal DMs work on this device: it holds an identity.
    #[must_use]
    pub fn dm_available(&self) -> bool {
        self.dm_engine().is_ok()
    }

    /// The engine, built on first use from this device's identity and its
    /// own connection to the instance store.
    fn dm_engine(&self) -> Result<Arc<AsyncMutex<DmEngine>>, ClientError> {
        if let Some(engine) = self
            .dm
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .as_ref()
        {
            return Ok(Arc::clone(engine));
        }
        let (roster, seed, backup_key) = {
            let mut stores = self
                .identity
                .try_lock()
                .ok_or_else(|| ClientError::State("an identity flow is running".into()))?;
            let seed = stores
                .keys
                .get("device_key")?
                .ok_or_else(|| ClientError::State("no identity on this device".into()))?;
            let backup_key = stores.keys.get("backup_key")?;
            let roster = device(&mut stores, &self.device_id).roster()?;
            (roster, seed, backup_key)
        };
        let account = self
            .lock()
            .reader
            .meta_get("account_id")?
            .map(|h| hex::decode(h).unwrap_or_default())
            .unwrap_or_default();
        let store = (self.opener)(StoreName::Instance)?;
        let engine = DmEngine::new(
            self.api.clone(),
            store,
            account,
            self.device_id.clone(),
            &seed,
            backup_key,
            roster,
        )?;
        let engine = Arc::new(AsyncMutex::new(engine));
        *self.dm.lock().unwrap_or_else(PoisonError::into_inner) = Some(Arc::clone(&engine));
        Ok(engine)
    }

    /// Carries out outcomes from a shell-initiated engine call.
    fn dispatch(&self, outcomes: Vec<Outcome>) {
        let inner = self.lock();
        dispatch(outcomes, inner.commands.as_ref(), inner.sink.as_ref());
    }

    /// The personal conversations this device knows, with whether it
    /// holds their group.
    ///
    /// # Errors
    /// Without an identity, or on a store failure.
    pub async fn dm_channels(&self) -> Result<Vec<DmChannelSummary>, ClientError> {
        let engine = self.dm_engine()?;
        let e = engine.lock().await;
        let mut out = Vec::new();
        let channels: &HashMap<Vec<u8>, Vec<Vec<u8>>> = e.channels();
        for (channel, participants) in channels {
            out.push(DmChannelSummary {
                channel_id: wire::display(channel),
                participant_account_ids: participants.iter().map(|p| wire::display(p)).collect(),
                member: e.is_member(channel)?,
            });
        }
        Ok(out)
    }

    /// Opens (or finds) the conversation with `account_ids`; when this
    /// device opened a new one it creates the group and welcomes the
    /// others (`DmReady` follows on the sink).
    ///
    /// # Errors
    /// `FAILED_PRECONDITION` without an accepted request; or without an
    /// identity, or before the gateway loop started.
    pub async fn dm_open(&self, account_ids: &[String]) -> Result<DmOpened, ClientError> {
        let others = account_ids
            .iter()
            .map(|id| parse_uuid(id))
            .collect::<Result<Vec<_>, _>>()?;
        self.open_dm(others, false).await
    }

    async fn open_dm(&self, others: Vec<Vec<u8>>, accepter: bool) -> Result<DmOpened, ClientError> {
        let engine = self.dm_engine()?;
        let opened = self.api.open_personal_dm(others).await?;
        let channel = opened
            .channel
            .ok_or_else(|| ClientError::State("no channel in the response".into()))?;
        let mut e = engine.lock().await;
        e.record_channel(&channel)?;
        let member = e.is_member(&channel.channel_id)?;
        let creating = !member && (accepter || !opened.existed);
        if creating {
            if !self.started() {
                return Err(ClientError::State("not started".into()));
            }
            let outcomes = e.create_group(&channel.channel_id).await?;
            drop(e);
            self.dispatch(outcomes);
        }
        Ok(DmOpened {
            channel_id: wire::display(&channel.channel_id),
            existed: opened.existed,
            member,
            creating,
        })
    }

    /// Sends a DM request to an account.
    ///
    /// # Errors
    /// `ALREADY_EXISTS` when the pair already has a personal DM.
    pub async fn dm_request(&self, account_id: &str) -> Result<DmRequestSummary, ClientError> {
        let to = parse_uuid(account_id)?;
        let request = self.api.send_dm_request(to).await?;
        Ok(request_summary(&request))
    }

    /// Answers a DM request: `accept`, `decline` or `block`. Accepting
    /// opens the conversation and, as the accepter, creates its group;
    /// the result is then the opened conversation, otherwise the request.
    ///
    /// # Errors
    /// On API failure, an unknown response, or without an identity when accepting.
    pub async fn dm_respond(
        &self,
        request_id: &str,
        response: &str,
    ) -> Result<Result<DmOpened, DmRequestSummary>, ClientError> {
        let state = match response {
            "accept" => DmRequestState::Accepted,
            "decline" => DmRequestState::Declined,
            "block" => DmRequestState::Blocked,
            other => return Err(ClientError::State(format!("unknown response {other}"))),
        };
        let request = self.api.respond_dm_request(request_id, state).await?;
        if state == DmRequestState::Accepted {
            Ok(Ok(self
                .open_dm(vec![request.from_account_id.clone()], true)
                .await?))
        } else {
            Ok(Err(request_summary(&request)))
        }
    }

    /// Sends a message in a personal DM; `DmDelivered` follows on the sink
    /// once the ciphertext is acked.
    ///
    /// # Errors
    /// Without the group for the conversation, or before the loop started.
    pub async fn dm_send(&self, channel_id: &str, body: &str) -> Result<(), ClientError> {
        let channel = parse_uuid(channel_id)?;
        let engine = self.dm_engine()?;
        let mut e = engine.lock().await;
        e.load_participants(&channel)?;
        let outcomes = e.send_text(&channel, body)?;
        drop(e);
        self.dispatch(outcomes);
        Ok(())
    }

    /// Deletes one of this identity's messages in a personal DM.
    ///
    /// # Errors
    /// Without the group for the conversation, or before the loop started.
    pub async fn dm_delete(&self, channel_id: &str, message_id: &str) -> Result<(), ClientError> {
        let channel = parse_uuid(channel_id)?;
        let message = parse_uuid(message_id)?;
        let engine = self.dm_engine()?;
        let mut e = engine.lock().await;
        e.load_participants(&channel)?;
        let outcomes = e.send_delete(&channel, message)?;
        drop(e);
        self.dispatch(outcomes);
        Ok(())
    }

    /// Rejoins a conversation by external commit (e2ee spec section 10).
    ///
    /// # Errors
    /// Without an identity, or when the instance holds no group info.
    pub async fn dm_rejoin(&self, channel_id: &str) -> Result<(), ClientError> {
        let channel = parse_uuid(channel_id)?;
        let engine = self.dm_engine()?;
        let mut e = engine.lock().await;
        e.load_participants(&channel)?;
        let outcomes = e.rejoin(&channel).await?;
        drop(e);
        self.dispatch(outcomes);
        Ok(())
    }

    /// The conversation as this device can read it: the projection of its
    /// decrypted operations, oldest first, with counts of what it cannot.
    ///
    /// # Errors
    /// Without an identity, or on a store failure.
    pub async fn dm_messages(&self, channel_id: &str) -> Result<DmLogSummary, ClientError> {
        let channel = parse_uuid(channel_id)?;
        let engine = self.dm_engine()?;
        let e = engine.lock().await;
        let me = e.me().identity_id.clone();
        let mut projection = Projection::default();
        let mut before_join = 0u32;
        let mut undecryptable = 0u32;
        for row in e.rows(&channel)? {
            match row.status {
                DmStatus::Decrypted => {
                    if let Ok(op) = DmOperation::decode(row.bytes.as_slice()) {
                        projection.apply(
                            row.seq,
                            &row.sender_identity_id,
                            &row.sender_device_id,
                            &op,
                        );
                    }
                }
                DmStatus::BeforeJoin => before_join += 1,
                DmStatus::Undecryptable => undecryptable += 1,
            }
        }
        let messages = projection
            .messages()
            .map(|m| DmMessageSummary {
                message_id: wire::display(&m.message_id),
                seq: m.seq,
                sender_identity_id: hex::encode(&m.sender_identity_id),
                sender_device_id: wire::display(&m.sender_device_id),
                sender_account_id: e
                    .account_of(&m.sender_identity_id)
                    .map(|a| wire::display(a))
                    .unwrap_or_default(),
                body: m.body.clone(),
                sent_at_micros: i64::try_from(m.sent_at_nanos / 1000).unwrap_or(0),
                edited: m.edited,
                deleted: m.deleted,
                mine: m.sender_identity_id == me,
            })
            .collect();
        Ok(DmLogSummary {
            messages,
            before_join,
            undecryptable,
        })
    }

    // ---- Plumbing ----

    fn save_tokens(&self, pair: &TokenPair) -> Result<(), ClientError> {
        {
            let mut inner = self.lock();
            inner
                .keys
                .put("access_token", pair.access_token.as_bytes())?;
            inner
                .keys
                .put("refresh_token", pair.refresh_token.as_bytes())?;
        }
        self.api.set_tokens(pair);
        Ok(())
    }

    fn send_command(&self, command: GatewayCommand) -> Result<(), ClientError> {
        let inner = self.lock();
        let tx = inner
            .commands
            .as_ref()
            .ok_or_else(|| ClientError::State("not started".into()))?;
        tx.unbounded_send(command)
            .map_err(|_| ClientError::State("the gateway loop has stopped".into()))
    }
}

/// What a push should say, decided on the device (architecture 15.5).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NotificationContent {
    /// Who, and for a mention, where.
    pub title: String,
    /// The message.
    pub body: String,
    /// The conversation or channel to open (UUID string).
    pub channel_id: String,
    /// `dm` or `mention`.
    pub kind: String,
}

/// Keeps the newest candidate.
fn consider(best: &mut Option<(i64, NotificationContent)>, at: i64, content: NotificationContent) {
    if best.as_ref().is_none_or(|(when, _)| at >= *when) {
        *best = Some((at, content));
    }
}

/// The store's writer generation: bumped by a process that wrote while
/// another may have been suspended.
fn writer_generation(store: &SqliteStore) -> Result<u64, ClientError> {
    Ok(store
        .meta_get("writer_generation")?
        .and_then(|s| s.parse().ok())
        .unwrap_or(0))
}

#[cfg(test)]
mod invite_tests {
    use super::invite_code_of;

    #[test]
    fn a_link_or_a_code_gives_the_code() {
        assert_eq!(invite_code_of("abc23xyz"), "abc23xyz");
        assert_eq!(invite_code_of("  abc23xyz\n"), "abc23xyz");
        assert_eq!(
            invite_code_of("https://chat.example.com/invite/abc23xyz"),
            "abc23xyz"
        );
        assert_eq!(
            invite_code_of("https://chat.example.com/invite/abc23xyz/"),
            "abc23xyz"
        );
        assert_eq!(
            invite_code_of("http://10.0.2.2:4000/invite/abc23xyz?x=1"),
            "abc23xyz"
        );
        assert_eq!(invite_code_of("/invite/abc23xyz#top"), "abc23xyz");
    }
}
