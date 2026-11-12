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
use serde::{Deserialize, Serialize};

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
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerSummary {
    pub server_id: String,
    pub name: String,
    /// The server's picture, a path of this instance as
    /// [`MemberSummary::avatar_url`] is; empty for none.
    #[serde(default)]
    pub icon_url: String,
    /// How long the server keeps its messages, in days; 0 for ever.
    #[serde(default)]
    pub retention_days: u32,
    /// Whether this account may change the server's settings: its owner,
    /// or a member whose roles grant `MANAGE_SERVER` or `ADMINISTRATOR`
    /// (architecture 6.7). What a shell offers; the instance decides.
    #[serde(default)]
    pub can_manage: bool,
}

impl ServerSummary {
    /// The server as the snapshot says it, with what `memberships` (the
    /// account's own) says this account may do there.
    fn of(server: &lumia_core::proto::Server, memberships: &[lumia_core::proto::Member]) -> Self {
        use lumia_core::proto::Permission;
        let can_manage = memberships
            .iter()
            .filter(|m| m.server_id == server.server_id)
            .flat_map(|m| m.permissions.iter())
            .any(|p| {
                *p == Permission::Administrator as i32 || *p == Permission::ManageServer as i32
            });
        Self {
            server_id: wire::display(&server.server_id),
            name: server.name.clone(),
            icon_url: server.icon_url.clone(),
            retention_days: server.retention_days,
            can_manage,
        }
    }
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
    /// The address an email invite went to; empty for a link.
    pub email: String,
}

/// The status this person chose and their custom status text (protocol 6.7).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StatusSummary {
    /// `ONLINE`, `AWAY` or `DO_NOT_DISTURB`.
    pub status: String,
    pub custom_status: String,
}

const STATUS_META: &str = "presence:status";
const CUSTOM_STATUS_META: &str = "presence:custom_status";

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
#[derive(Debug, Clone, Serialize, Deserialize)]
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
#[derive(Debug, Clone, Serialize, Deserialize)]
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
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotSummary {
    pub account_id: String,
    pub handle: String,
    pub display_name: String,
    /// The account's picture, as [`MemberSummary::avatar_url`]; empty for none.
    #[serde(default)]
    pub avatar_url: String,
    pub servers: Vec<ServerSummary>,
    pub channels: Vec<ChannelSummary>,
    pub pending_requests: Vec<DmRequestSummary>,
}

/// A person: a member of a server, or a public profile.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemberSummary {
    pub account_id: String,
    pub handle: String,
    pub display_name: String,
    pub nickname: String,
    /// From the account's verified profile, as the display name is.
    pub pronouns: String,
    pub bio: String,
    /// The account's picture: a path on this instance (`/api/v1/avatars/…`,
    /// which [`App::avatar`] fetches and a browser resolves against the
    /// instance), an absolute URL when a signed profile issued elsewhere
    /// names one (not fetched: it would tell that host who is looking), or
    /// empty for none.
    #[serde(default)]
    pub avatar_url: String,
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
    /// Someone else is typing in a server's channel (protocol 6.7), until
    /// `until_micros` on this device's clock; they are no longer shown
    /// then, nor once a message of theirs arrives. Sent again while they
    /// go on.
    Typing {
        channel_id: String,
        account_id: String,
        until_micros: i64,
    },
}

/// The most bytes [`App::avatar`] takes: several times what a 512-pixel
/// square encodes to.
const AVATAR_MAX_BYTES: usize = 2 * 1024 * 1024;

/// Whether `avatar_url` is a picture this instance serves: its avatar path
/// and a name of 64 lowercase hexadecimal digits, and nothing else.
#[must_use]
pub fn is_own_avatar(avatar_url: &str) -> bool {
    avatar_url
        .strip_prefix("/api/v1/avatars/")
        .is_some_and(|name| {
            name.len() == 64 && name.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
        })
}

/// How often typing is sent while someone types in a channel (protocol 6.7).
pub const TYPING_EVERY: std::time::Duration = std::time::Duration::from_secs(5);

/// Whether typing in `channel` is due to be sent at `now_micros`, given
/// when it last was and where: once in [`TYPING_EVERY`] for a channel, and
/// at once for another.
fn typing_due(last: Option<&(Vec<u8>, i64)>, channel: &[u8], now_micros: i64) -> bool {
    let every = i64::try_from(TYPING_EVERY.as_micros()).unwrap_or(i64::MAX);
    last.is_none_or(|(last_channel, at)| last_channel != channel || now_micros - at >= every)
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

/// How many pages of 200 a member list is read to: a server of ten thousand.
/// Past that a list is cut, and a shell wants a search, not a list.
const MEMBER_PAGES: usize = 50;

/// Where the last snapshot and the last member lists are kept in the
/// instance store's meta area (`App::kept_snapshot`, `App::kept_members`).
const KEPT_SNAPSHOT: &str = "kept_snapshot";
const KEPT_MEMBERS: &str = "kept_members";

fn stored_summary(m: &lumia_core::store::StoredMessage) -> MessageSummary {
    MessageSummary {
        message_id: wire::display(&m.message_id),
        channel_id: wire::display(&m.channel_id),
        seq: m.seq,
        author_account_id: wire::display(&m.author_account_id),
        body: m.body.clone(),
        created_at_micros: m.created_at_micros,
        deleted: m.deleted,
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
        GatewayEvent::Notice(Notice::Typing(t)) => AppEvent::Typing {
            channel_id: wire::display(&t.channel_id),
            account_id: wire::display(&t.account_id),
            until_micros: wire::micros(t.until.as_ref()).unwrap_or(0),
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
            // Taken by the gateway task before dispatch, where the
            // identity's stores are at hand.
            Outcome::BackupKeyHandoff(_) => {}
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
            label: info.label.clone(),
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
    /// The servers whose presence is watched, re-sent with a status change.
    watching: Vec<Vec<u8>>,
    /// The channel typing was last sent for and when, in microseconds.
    typed: Option<(Vec<u8>, i64)>,
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
        let read = Arc::clone(&opener);
        let held = tokens(&keys)?;
        // The refresh token this process last saw in the store or put
        // there. When the store holds another, someone else on these
        // stores has rotated the pair (the notification extension, a
        // second copy of a command), and theirs is the live one. A pair
        // this process failed to write is not mistaken for that: the
        // store still holds what was last seen.
        let seen = Arc::new(Mutex::new(held.refresh.clone()));
        let (wrote, looked) = (Arc::clone(&seen), Arc::clone(&seen));
        let api = Api::new(base_url)
            .with_tokens(held)
            .on_rotate(Arc::new(move |pair| {
                if let Ok(mut keys) = rotate(StoreName::Keys) {
                    let _ = keys.put("access_token", pair.access_token.as_bytes());
                    if keys
                        .put("refresh_token", pair.refresh_token.as_bytes())
                        .is_ok()
                    {
                        *wrote.lock().unwrap_or_else(PoisonError::into_inner) =
                            Some(pair.refresh_token.clone());
                    }
                }
            }))
            .stored(Arc::new(move || {
                let stored = tokens(&read(StoreName::Keys).ok()?).ok()?;
                let mut seen = looked.lock().unwrap_or_else(PoisonError::into_inner);
                if stored.refresh.is_none() || stored.refresh == *seen {
                    return None;
                }
                seen.clone_from(&stored.refresh);
                Some(stored)
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
                watching: Vec::new(),
                typed: None,
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
        self.take_session(&pair).await
    }

    /// Signs in with a password and the passkey factor (protocol 6.1's
    /// `details.mfa` of `passkey`): the assertion the browser made for an
    /// assert challenge, by one of the account's passkeys.
    ///
    /// # Errors
    /// `UNAUTHENTICATED` for a wrong password or assertion; transport failure.
    pub async fn sign_in_with_passkey(
        &self,
        handle: &str,
        password: &str,
        challenge_id: &str,
        credential_json: &str,
    ) -> Result<(), ClientError> {
        let pair = self
            .api
            .sign_in_with_passkey(
                handle,
                password,
                self.device_id.clone(),
                challenge_id,
                credential_json,
            )
            .await?;
        self.take_session(&pair).await
    }

    /// Whether a password sign-in must also present one of this account's
    /// passkeys.
    ///
    /// # Errors
    /// On transport or API failure.
    pub async fn passkey_required(&self) -> Result<bool, ClientError> {
        Ok(self.api.passkeys().await?.required_for_password)
    }

    /// `PUT /auth/passkeys/required`: requires (or stops requiring) one of
    /// this account's passkeys at a password sign-in.
    ///
    /// # Errors
    /// `FAILED_PRECONDITION` with `state` `no_passkey` when there is none to
    /// require; transport failure.
    pub async fn set_passkey_required(&self, required: bool) -> Result<(), ClientError> {
        self.api.set_passkey_required(required).await
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
        self.take_session(&pair).await
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
        let pair = self
            .api
            .create_account(handle, "", password, &code, self.device_id.clone())
            .await?;
        self.take_session(&pair).await
    }

    /// `POST /servers`: a server this account owns, with its `@everyone`
    /// role and the owner's membership. The next snapshot lists it.
    ///
    /// # Errors
    /// `INVALID_ARGUMENT` for an empty name; transport failure.
    pub async fn create_server(&self, name: &str) -> Result<ServerSummary, ClientError> {
        let s = self.api.create_server(name.trim()).await?;
        Ok(ServerSummary {
            can_manage: true,
            ..ServerSummary::of(&s, &[])
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
            email: invite.email.clone(),
        }
    }

    /// `POST /invites/email`: a one-use invite good for a week, mailed to
    /// `email` as a link (architecture 6.2).
    ///
    /// # Errors
    /// `PERMISSION_DENIED` without `create_invites` there, `FAILED_PRECONDITION`
    /// when the instance sends no mail, `INVALID_ARGUMENT` for a malformed
    /// address; transport failure.
    pub async fn email_invite(
        &self,
        server_id: &str,
        email: &str,
    ) -> Result<InviteSummary, ClientError> {
        let req = lumia_core::proto::EmailInviteRequest {
            server_id: wire::uuid_bytes(
                uuid::Uuid::parse_str(server_id)
                    .map_err(|_| ClientError::Transport(format!("not a uuid: {server_id}")))?,
            ),
            email: email.trim().to_string(),
            ..Default::default()
        };
        let invite = self.api.email_invite(&req).await?;
        Ok(self.invite_summary(&invite))
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
        self.take_session(&pair).await
    }

    /// Signs out: revokes the session and forgets the tokens.
    ///
    /// # Errors
    /// On a store failure; an API refusal is ignored, the tokens go anyway.
    pub async fn sign_out(&self) -> Result<(), ClientError> {
        let _ = self.api.sign_out().await;
        // Each is tried whatever became of the one before, and the client
        // stops presenting the pair either way: a store that refuses one
        // delete must not leave the other token to open the app signed in.
        let (tokens, kept) = {
            let mut inner = self.lock();
            let access = inner.keys.delete("access_token");
            let refresh = inner.keys.delete("refresh_token");
            // What was kept of the account for an offline launch goes with
            // it, and its account log and unsent commands: the next account
            // to sign in here starts its own log from the beginning, and
            // sends nothing of this one's. On the store the loop holds when
            // it is back, or through the reader on the same file.
            let snapshot = inner.reader.meta_set(KEPT_SNAPSHOT, "");
            let members = inner.reader.meta_set(KEPT_MEMBERS, "");
            let log = inner
                .store
                .as_ref()
                .unwrap_or(&inner.reader)
                .forget_account();
            (access.and(refresh), snapshot.and(members).and(log))
        };
        self.api.clear_tokens();
        tokens?;
        kept?;
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
        let summary = SnapshotSummary {
            account_id: wire::display(&account.account_id),
            handle: account.handle,
            display_name: account.display_name,
            avatar_url: account.avatar_url,
            servers: snap
                .servers
                .iter()
                .map(|s| ServerSummary::of(s, &snap.memberships))
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
        };
        self.keep(KEPT_SNAPSHOT, &summary);
        Ok(summary)
    }

    /// The last snapshot this device got, kept in the instance store so a
    /// launch has the servers and channels before the instance answers, or
    /// without it answering at all (architecture 13, "Local-first"). `None`
    /// before the first one, and after signing out.
    ///
    /// # Errors
    /// On a store failure.
    pub fn kept_snapshot(&self) -> Result<Option<SnapshotSummary>, ClientError> {
        self.kept(KEPT_SNAPSHOT)
    }

    /// Writes `value` under `key` in the instance store. Best effort: what
    /// is kept is a copy of what the instance holds, so a failure to keep
    /// it costs an offline launch and nothing else.
    fn keep<T: Serialize>(&self, key: &str, value: &T) {
        if let Ok(json) = serde_json::to_string(value) {
            let _ = self.lock().reader.meta_set(key, &json);
        }
    }

    fn kept<T: for<'de> Deserialize<'de>>(&self, key: &str) -> Result<Option<T>, ClientError> {
        let json = self.lock().reader.meta_get(key)?;
        // An empty or unreadable value is the same as none: the next answer replaces it.
        Ok(json
            .filter(|j| !j.is_empty())
            .and_then(|j| serde_json::from_str(&j).ok()))
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
            pronouns: a.pronouns,
            bio: a.bio,
            avatar_url: a.avatar_url,
        })
    }

    /// `GET /servers/{id}/members`: a server's members with their
    /// accounts, optionally filtered by a handle or nickname prefix. Every
    /// page is read, up to [`MEMBER_PAGES`] of them.
    ///
    /// # Errors
    /// On transport or API failure.
    pub async fn members(
        &self,
        server_id: &str,
        query: &str,
    ) -> Result<Vec<MemberSummary>, ClientError> {
        let mut members: Vec<MemberSummary> = Vec::new();
        let mut cursor = String::new();
        for _ in 0..MEMBER_PAGES {
            let page = self.api.members(server_id, query, &cursor).await?;
            members.extend(page.members.iter().map(|m| {
                let account = page.accounts.iter().find(|a| a.account_id == m.account_id);
                MemberSummary {
                    account_id: wire::display(&m.account_id),
                    handle: account.map(|a| a.handle.clone()).unwrap_or_default(),
                    display_name: account.map(|a| a.display_name.clone()).unwrap_or_default(),
                    nickname: m.nickname.clone(),
                    pronouns: account.map(|a| a.pronouns.clone()).unwrap_or_default(),
                    bio: account.map(|a| a.bio.clone()).unwrap_or_default(),
                    avatar_url: account.map(|a| a.avatar_url.clone()).unwrap_or_default(),
                }
            }));
            cursor = page.page.map(|p| p.next_cursor).unwrap_or_default();
            if cursor.is_empty() {
                break;
            }
        }
        // The unfiltered list is what an offline launch names people from.
        if query.is_empty() {
            let mut all: HashMap<String, Vec<MemberSummary>> =
                self.kept(KEPT_MEMBERS).ok().flatten().unwrap_or_default();
            all.insert(server_id.to_string(), members.clone());
            self.keep(KEPT_MEMBERS, &all);
        }
        Ok(members)
    }

    /// The members last listed for a server, kept as [`App::kept_snapshot`]
    /// is; empty when it was never listed.
    ///
    /// # Errors
    /// On a store failure.
    pub fn kept_members(&self, server_id: &str) -> Result<Vec<MemberSummary>, ClientError> {
        let all: Option<HashMap<String, Vec<MemberSummary>>> = self.kept(KEPT_MEMBERS)?;
        Ok(all
            .and_then(|mut all| all.remove(server_id))
            .unwrap_or_default())
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
        Ok(rows.iter().map(stored_summary).collect())
    }

    /// The page of a channel's local messages above one already read: the
    /// newest `limit` with `seq` below `before_seq`, oldest first.
    ///
    /// # Errors
    /// On a store failure or a malformed id.
    pub fn messages_before(
        &self,
        channel_id: &str,
        before_seq: u64,
        limit: u32,
    ) -> Result<Vec<MessageSummary>, ClientError> {
        let id = parse_uuid(channel_id)?;
        let inner = self.lock();
        let rows = inner
            .reader
            .messages_before(&id, before_seq, limit as usize)?;
        Ok(rows.iter().map(stored_summary).collect())
    }

    /// `GET /channels/{id}/messages`: scrollback from the instance, for
    /// history older than the local store holds (a store starts at the
    /// channel's snapshot or at the events since this device subscribed).
    /// The newest `limit` messages below `before_seq`, oldest first, and
    /// whether older ones remain. They are not written to the store: the
    /// store's messages are the projection of its contiguous events, and
    /// these lie below them.
    ///
    /// # Errors
    /// On transport or API failure.
    pub async fn older_messages(
        &self,
        channel_id: &str,
        before_seq: u64,
        limit: u32,
    ) -> Result<(Vec<MessageSummary>, bool), ClientError> {
        let page = self
            .api
            .list_messages(channel_id, before_seq, limit)
            .await?;
        Ok((
            page.messages.iter().map(message_summary).collect(),
            page.has_more,
        ))
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
        let task_sink = Arc::clone(&sink);
        spawn(async move {
            while let Some(ev) = erx.next().await {
                if let GatewayEvent::Notice(Notice::Welcomed(w)) = &ev {
                    let _ = app
                        .lock()
                        .reader
                        .meta_set(gateway::ACCOUNT_META, &hex::encode(&w.account_id));
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
                        Ok(mut out) => {
                            let handoffs: Vec<_> = out
                                .iter()
                                .filter_map(|o| match o {
                                    Outcome::BackupKeyHandoff(h) => Some(h.clone()),
                                    _ => None,
                                })
                                .collect();
                            out.retain(|o| !matches!(o, Outcome::BackupKeyHandoff(_)));
                            for handoff in handoffs {
                                let opened =
                                    engine.lock().await.accept_backup_key_handoff(&handoff);
                                match opened {
                                    Ok(Some(key)) => {
                                        let stored =
                                            app.identity.lock().await.keys.put("backup_key", &key);
                                        task_sink(match stored {
                                            Ok(()) => AppEvent::DmNote {
                                                message:
                                                    "the backup key of a root rotation arrived"
                                                        .into(),
                                            },
                                            Err(err) => AppEvent::DmError {
                                                message: err.to_string(),
                                            },
                                        });
                                    }
                                    Ok(None) => {}
                                    Err(err) => task_sink(AppEvent::DmError {
                                        message: err.to_string(),
                                    }),
                                }
                            }
                            // The loop's sender is the application's alone:
                            // a copy held here kept the loop running after
                            // `stop`, on a session being signed out.
                            let commands = app.lock().commands.clone();
                            dispatch(out, commands.as_ref(), Some(&task_sink));
                        }
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

    /// The status this person chose (`ONLINE`, `AWAY` or `DO_NOT_DISTURB`)
    /// and their custom status text (protocol 6.7), kept in the store
    /// across launches.
    ///
    /// # Errors
    /// On a store failure.
    pub fn status(&self) -> Result<StatusSummary, ClientError> {
        let inner = self.lock();
        Ok(StatusSummary {
            status: inner
                .reader
                .meta_get(STATUS_META)?
                .unwrap_or_else(|| "ONLINE".into()),
            custom_status: inner
                .reader
                .meta_get(CUSTOM_STATUS_META)?
                .unwrap_or_default(),
        })
    }

    /// Chooses the status and custom text: kept, and sent at once with the
    /// servers watched when the gateway is up (every welcome re-sends it).
    ///
    /// # Errors
    /// On a store failure.
    pub fn set_status(&self, status: &str, custom_status: &str) -> Result<(), ClientError> {
        let status = match status {
            "AWAY" => "AWAY",
            "DO_NOT_DISTURB" => "DO_NOT_DISTURB",
            _ => "ONLINE",
        };
        let custom_status: String = custom_status.trim().chars().take(100).collect();
        let watching = {
            let inner = self.lock();
            inner.reader.set_meta(STATUS_META, status)?;
            inner.reader.set_meta(CUSTOM_STATUS_META, &custom_status)?;
            inner.watching.clone()
        };
        if self.started() {
            self.send_presence(status, &custom_status, watching)?;
        }
        Ok(())
    }

    /// The servers this client has on screen, whose presence it then
    /// receives as `PresenceDiff` events; the set replaces the last one
    /// and goes with the chosen status.
    ///
    /// # Errors
    /// When not started or an id is malformed.
    pub fn watch_presence(&self, watch_server_ids: &[String]) -> Result<(), ClientError> {
        let mut ids = Vec::with_capacity(watch_server_ids.len());
        for id in watch_server_ids {
            ids.push(parse_uuid(id)?);
        }
        self.lock().watching.clone_from(&ids);
        let choice = self.status()?;
        self.send_presence(&choice.status, &choice.custom_status, ids)
    }

    fn send_presence(
        &self,
        status: &str,
        custom_status: &str,
        watch_server_ids: Vec<Vec<u8>>,
    ) -> Result<(), ClientError> {
        let status = match status {
            "AWAY" => lumia_core::proto::PresenceStatus::Away,
            "DO_NOT_DISTURB" => lumia_core::proto::PresenceStatus::DoNotDisturb,
            _ => lumia_core::proto::PresenceStatus::Online,
        };
        self.send_command(GatewayCommand::Presence(
            lumia_core::proto::PresenceUpdate {
                status: status.into(),
                custom_status: custom_status.to_string(),
                watch_server_ids,
            },
        ))
    }

    /// Changes this account's handle, its display name, or both, on this
    /// instance (`PATCH /account`); `None` leaves one as it is and an empty
    /// display name clears it. Any signed-in device may, with or without
    /// the identity's key: this is the account's own name here, which a
    /// signed profile saved later replaces and the other way round
    /// (architecture 6.2). Returns the account's snapshot as it then is,
    /// kept as `snapshot` keeps it.
    ///
    /// # Errors
    /// `ALREADY_EXISTS` for a handle another account has or holds from a
    /// rename, `INVALID_ARGUMENT` for one that is not a handle (2 to 32 of
    /// a-z, 0-9, `_`, `.`, `-`) or a name over 64 characters.
    pub async fn update_account(
        &self,
        handle: Option<&str>,
        display_name: Option<&str>,
    ) -> Result<SnapshotSummary, ClientError> {
        let trimmed = |v: Option<&str>| v.map(|v| v.trim().to_string());
        self.api
            .update_account(trimmed(handle), trimmed(display_name))
            .await?;
        self.snapshot().await
    }

    /// Makes `picture`, a file's bytes as the person chose it (PNG, JPEG,
    /// WebP or GIF, at most 8 MB), this account's picture on the instance
    /// (`PUT /account/avatar`). The instance keeps its own square of it,
    /// so nothing of the file is passed on but what it shows. `media_type`
    /// is what the shell believes the file is and is not relied on.
    /// Returns the account's snapshot as it then is.
    ///
    /// # Errors
    /// `INVALID_ARGUMENT` with `details.reason` `unsupported`, `too_large`
    /// or `invalid`; `FAILED_PRECONDITION` with `details.state` `blobs` on
    /// an instance that keeps no pictures; `RATE_LIMITED` when it is busy.
    pub async fn set_avatar(
        &self,
        picture: Vec<u8>,
        media_type: &str,
    ) -> Result<SnapshotSummary, ClientError> {
        self.api.put_avatar(picture, media_type).await?;
        self.snapshot().await
    }

    /// Changes a server's settings (`PATCH /servers/{id}`, architecture
    /// 6.7): its name, how long it keeps messages in days (0 for ever), or
    /// both; `None` leaves one as it is. Every member hears of it through
    /// the account log. Returns the account's snapshot as it then is.
    ///
    /// # Errors
    /// `PERMISSION_DENIED` without `MANAGE_SERVER` there; `INVALID_ARGUMENT`
    /// with `details.field` `name` for an empty name or one over 100
    /// characters; transport failure.
    pub async fn update_server(
        &self,
        server_id: &str,
        name: Option<&str>,
        retention_days: Option<u32>,
    ) -> Result<SnapshotSummary, ClientError> {
        let server_id = parse_uuid(server_id)?;
        self.api
            .update_server(
                &wire::display(&server_id),
                name.map(|n| n.trim().to_string()),
                retention_days,
            )
            .await?;
        self.snapshot().await
    }

    /// Makes `picture`, a file's bytes as chosen, a server's picture
    /// (`PUT /servers/{id}/icon`), as [`App::set_avatar`] does the
    /// account's. Returns the account's snapshot as it then is.
    ///
    /// # Errors
    /// As [`App::set_avatar`], and `PERMISSION_DENIED` without
    /// `MANAGE_SERVER` there.
    pub async fn set_server_icon(
        &self,
        server_id: &str,
        picture: Vec<u8>,
        media_type: &str,
    ) -> Result<SnapshotSummary, ClientError> {
        let server_id = parse_uuid(server_id)?;
        self.api
            .put_server_icon(&wire::display(&server_id), picture, media_type)
            .await?;
        self.snapshot().await
    }

    /// Takes a server's picture away (`DELETE /servers/{id}/icon`).
    ///
    /// # Errors
    /// `PERMISSION_DENIED` without `MANAGE_SERVER` there; transport failure.
    pub async fn remove_server_icon(
        &self,
        server_id: &str,
    ) -> Result<SnapshotSummary, ClientError> {
        let server_id = parse_uuid(server_id)?;
        self.api
            .delete_server_icon(&wire::display(&server_id))
            .await?;
        self.snapshot().await
    }

    /// Takes this account's picture away (`DELETE /account/avatar`).
    ///
    /// # Errors
    /// On transport or API failure.
    pub async fn remove_avatar(&self) -> Result<SnapshotSummary, ClientError> {
        self.api.delete_avatar().await?;
        self.snapshot().await
    }

    /// The bytes of the picture an `avatar_url` names, when it is one of
    /// this instance's (protocol: `/api/v1/avatars/` and 64 hexadecimal
    /// digits): JPEG or PNG the instance encoded itself, at most 512 pixels
    /// a side. They never change under a URL, so a shell may keep them for
    /// good. For a shell that draws its own pictures; a browser or a
    /// platform image view can ask the instance directly.
    ///
    /// # Errors
    /// `State` for a URL that is not this instance's picture (another
    /// host's is not fetched: that would tell it who is looking);
    /// `NOT_FOUND` when the picture is gone; transport failure.
    pub async fn avatar(&self, avatar_url: &str) -> Result<Vec<u8>, ClientError> {
        if !is_own_avatar(avatar_url) {
            return Err(ClientError::State("not a picture of this instance".into()));
        }
        self.api.avatar(avatar_url, AVATAR_MAX_BYTES).await
    }

    /// Says this person is typing in a server's channel (protocol 6.7).
    /// Call it on every edit of the composer: it is sent at most once in
    /// [`TYPING_EVERY`] for a channel, and not at all for a personal
    /// conversation (where the instance is not to learn who is writing)
    /// or a channel the kept snapshot does not list as a server's. Nothing
    /// is said when typing stops; the last one runs out.
    ///
    /// # Errors
    /// When not started or the id is malformed.
    pub fn typing(&self, channel_id: &str) -> Result<(), ClientError> {
        let channel = parse_uuid(channel_id)?;
        let now = crate::time::now_micros();
        {
            let mut inner = self.lock();
            if !typing_due(inner.typed.as_ref(), &channel, now) {
                return Ok(());
            }
            inner.typed = Some((channel.clone(), now));
        }
        let in_a_server = self.kept_snapshot()?.is_some_and(|snapshot| {
            snapshot.channels.iter().any(|c| {
                parse_uuid(&c.channel_id).is_ok_and(|id| id == channel)
                    && !c.server_id.is_empty()
                    && !c.kind.starts_with("PERSONAL")
            })
        });
        if !in_a_server {
            return Ok(());
        }
        self.send_command(GatewayCommand::Typing(channel))
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
            None => match self.held_profile().await? {
                Some(signed) => Ok(Some(profile_summary(&signed)?)),
                None => Ok(None),
            },
        }
    }

    /// The signed profile the instance holds for this account, when signed
    /// in: none when it holds none, an error when it could not be asked
    /// (a new version must pass the held one, so a guess will not do).
    async fn held_profile(&self) -> Result<Option<lumia_core::proto::SignedProfile>, ClientError> {
        if !self.signed_in() {
            return Ok(None);
        }
        let snap = self.api.account_snapshot().await?;
        Ok(snap.account.and_then(|a| a.profile))
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
        let held = match self.held_profile().await? {
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
            .await?;
        drop(stores);
        self.announce_device().await;
        Ok(())
    }

    /// Publishes this device's key packages when it gained its identity
    /// while the gateway loop was already running. The engine publishes
    /// them when the gateway welcomes it, and a device linked after that
    /// welcome had no engine to hear it: nobody could add it to a group
    /// until its next connection. Best effort, since the next welcome
    /// publishes them anyway.
    async fn announce_device(&self) {
        if !self.started() {
            return;
        }
        let Ok(engine) = self.dm_engine_ready().await else {
            return;
        };
        let outcomes = engine
            .lock()
            .await
            .on_welcome(&lumia_core::proto::Welcome::default())
            .await;
        if let Ok(outcomes) = outcomes {
            self.dispatch(outcomes);
        }
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
        self.take_session(&pair).await?;
        self.announce_device().await;
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
        self.take_session(&pair).await
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
        if let Some(engine) = self.built_engine() {
            return Ok(engine);
        }
        let mut stores = self
            .identity
            .try_lock()
            .ok_or_else(|| ClientError::State("an identity flow is running".into()))?;
        self.build_engine(&mut stores)
    }

    /// [`App::dm_engine`] for a caller that can wait: when the engine is not
    /// built yet and an identity call holds the identity's stores (the roster
    /// being read at launch, a link being claimed), this waits for it instead
    /// of failing with "an identity flow is running".
    async fn dm_engine_ready(&self) -> Result<Arc<AsyncMutex<DmEngine>>, ClientError> {
        if let Some(engine) = self.built_engine() {
            return Ok(engine);
        }
        let mut stores = self.identity.lock().await;
        // Another caller may have built it while this one waited.
        if let Some(engine) = self.built_engine() {
            return Ok(engine);
        }
        self.build_engine(&mut stores)
    }

    fn built_engine(&self) -> Option<Arc<AsyncMutex<DmEngine>>> {
        self.dm
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .as_ref()
            .map(Arc::clone)
    }

    fn build_engine(
        &self,
        stores: &mut IdentityStores,
    ) -> Result<Arc<AsyncMutex<DmEngine>>, ClientError> {
        let (roster, seed, backup_key) = {
            let seed = stores
                .keys
                .get("device_key")?
                .ok_or_else(|| ClientError::State("no identity on this device".into()))?;
            let backup_key = stores.keys.get("backup_key")?;
            let roster = device(stores, &self.device_id).roster()?;
            (roster, seed, backup_key)
        };
        let account = self
            .lock()
            .reader
            .meta_get(gateway::ACCOUNT_META)?
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
        let engine = self.dm_engine_ready().await?;
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
        let engine = self.dm_engine_ready().await?;
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

    /// Sends a message in a personal DM and returns its id, which is also
    /// the id of the command that carries it: `DmDelivered` names it on the
    /// sink once the ciphertext is acked, an `Error` names it when the
    /// instance refuses it, and the message is in the log under it.
    ///
    /// # Errors
    /// Without the group for the conversation, or before the loop started.
    pub async fn dm_send(&self, channel_id: &str, body: &str) -> Result<String, ClientError> {
        let channel = parse_uuid(channel_id)?;
        let engine = self.dm_engine_ready().await?;
        let mut e = engine.lock().await;
        e.load_participants(&channel)?;
        let outcomes = e.send_text(&channel, body)?;
        drop(e);
        // The engine queues one command for a send; its id is the operation's.
        let id = outcomes
            .iter()
            .find_map(|o| match o {
                Outcome::Send(command) => Some(wire::display(&command.command_id)),
                _ => None,
            })
            .unwrap_or_default();
        self.dispatch(outcomes);
        Ok(id)
    }

    /// Deletes one of this identity's messages in a personal DM.
    ///
    /// # Errors
    /// Without the group for the conversation, or before the loop started.
    pub async fn dm_delete(&self, channel_id: &str, message_id: &str) -> Result<(), ClientError> {
        let channel = parse_uuid(channel_id)?;
        let message = parse_uuid(message_id)?;
        let engine = self.dm_engine_ready().await?;
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
        let engine = self.dm_engine_ready().await?;
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
        let engine = self.dm_engine_ready().await?;
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

    /// A sign-in's pair becomes this client's session, and one it already
    /// held is revoked: linking a phone that had signed in with a password,
    /// or signing in again without signing out, used to leave the earlier
    /// session alive on the instance with nothing holding its tokens, where
    /// signing out later could not reach it. The new pair is kept first, so
    /// a gateway loop running on the old session, told it was revoked,
    /// finds the new one and comes back on it. A refusal is ignored, as
    /// when signing out.
    async fn take_session(&self, pair: &TokenPair) -> Result<(), ClientError> {
        let held = self.api.tokens();
        self.save_tokens(pair)?;
        if held.access.is_some() {
            let _ = self.api.revoke(held).await;
        }
        Ok(())
    }

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
        self.api.take_pair(pair);
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

#[cfg(all(test, not(target_arch = "wasm32")))]
#[allow(clippy::unwrap_used)]
mod kept_tests {
    use std::collections::HashMap;
    use std::sync::Arc;

    use super::*;

    fn app() -> Arc<App> {
        let opener: Opener = Arc::new(|_| SqliteStore::in_memory());
        App::open("http://127.0.0.1:9", Place::Native, opener).unwrap()
    }

    fn snapshot() -> SnapshotSummary {
        SnapshotSummary {
            account_id: "a".into(),
            handle: "ada".into(),
            display_name: "Ada".into(),
            avatar_url: String::new(),
            servers: vec![ServerSummary {
                server_id: "s".into(),
                name: "Acme".into(),
                icon_url: String::new(),
                retention_days: 0,
                can_manage: false,
            }],
            channels: vec![ChannelSummary {
                channel_id: "c".into(),
                server_id: "s".into(),
                name: "general".into(),
                kind: "TEXT".into(),
                last_seq: 7,
                participant_account_ids: vec!["a".into()],
                host: String::new(),
            }],
            pending_requests: Vec::new(),
        }
    }

    #[test]
    fn a_kept_snapshot_reads_back_and_nothing_kept_is_none() {
        let app = app();
        assert!(app.kept_snapshot().unwrap().is_none());
        app.keep(KEPT_SNAPSHOT, &snapshot());
        let kept = app.kept_snapshot().unwrap().unwrap();
        assert_eq!(kept.handle, "ada");
        assert_eq!(kept.servers[0].name, "Acme");
        assert_eq!(kept.channels[0].last_seq, 7);
        // What signing out leaves, and anything unreadable, is none again.
        app.lock().reader.meta_set(KEPT_SNAPSHOT, "").unwrap();
        assert!(app.kept_snapshot().unwrap().is_none());
        app.lock()
            .reader
            .meta_set(KEPT_SNAPSHOT, "{not json")
            .unwrap();
        assert!(app.kept_snapshot().unwrap().is_none());
    }

    #[test]
    fn typing_is_spaced_out_and_never_sent_for_a_personal_conversation() {
        const SECOND: i64 = 1_000_000;
        let (here, there) = (vec![1u8; 16], vec![2u8; 16]);
        assert!(typing_due(None, &here, 0));
        let last = (here.clone(), 100 * SECOND);
        assert!(!typing_due(Some(&last), &here, 104 * SECOND));
        assert!(typing_due(Some(&last), &here, 105 * SECOND));
        assert!(typing_due(Some(&last), &there, 101 * SECOND));

        let app = app();
        let channel = |id: &str, server: &str, kind: &str| ChannelSummary {
            channel_id: id.into(),
            server_id: server.into(),
            kind: kind.into(),
            ..snapshot().channels[0].clone()
        };
        let general = "018f0000-0000-7000-8000-000000000001";
        let personal = "018f0000-0000-7000-8000-000000000002";
        let unlisted = "018f0000-0000-7000-8000-000000000003";
        let server = "018f0000-0000-7000-8000-0000000000aa";
        app.keep(
            KEPT_SNAPSHOT,
            &SnapshotSummary {
                channels: vec![
                    channel(general, server, "TEXT"),
                    channel(personal, "", "PERSONAL_DM"),
                ],
                ..snapshot()
            },
        );
        // No gateway loop runs behind this client, so what would be sent is refused
        // and what is held back is not.
        assert!(app.typing(personal).is_ok());
        assert!(app.typing(unlisted).is_ok());
        assert!(matches!(app.typing(general), Err(ClientError::State(_))));
        assert!(
            app.typing(general).is_ok(),
            "a second within five seconds is not sent at all"
        );
        assert!(app.typing("not an id").is_err());
    }

    /// An instance that answers every request with an empty success and
    /// keeps what was asked of it: each request's first line and the token
    /// it came with.
    fn listening() -> (String, Arc<std::sync::Mutex<Vec<String>>>) {
        use std::io::{Read, Write};

        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let asked = Arc::new(std::sync::Mutex::new(Vec::new()));
        let seen = Arc::clone(&asked);
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let mut stream = stream.unwrap();
                let mut request = Vec::new();
                let mut chunk = [0u8; 4096];
                while !request.windows(4).any(|w| w == b"\r\n\r\n") {
                    let n = stream.read(&mut chunk).unwrap();
                    if n == 0 {
                        break;
                    }
                    request.extend_from_slice(&chunk[..n]);
                }
                let head = String::from_utf8_lossy(&request).to_lowercase();
                let line = head.lines().next().unwrap_or_default().to_string();
                let token = head
                    .lines()
                    .find_map(|l| l.strip_prefix("authorization: bearer "))
                    .unwrap_or("none");
                seen.lock().unwrap().push(format!("{line} as {token}"));
                stream
                    .write_all(b"HTTP/1.1 200 OK\r\ncontent-length: 0\r\nconnection: close\r\n\r\n")
                    .unwrap();
            }
        });
        (base, asked)
    }

    fn pair(access: &str, refresh: &str) -> TokenPair {
        TokenPair {
            access_token: access.into(),
            refresh_token: refresh.into(),
            ..Default::default()
        }
    }

    /// A phone signs in with a password and is then linked, which signs it
    /// in again: the first session used to stay alive on the instance with
    /// no client holding it, and signing out revoked only the second.
    #[tokio::test(flavor = "multi_thread")]
    async fn a_sign_in_over_a_held_session_revokes_the_held_one() {
        let (base, asked) = listening();
        let opener: Opener = Arc::new(|_| SqliteStore::in_memory());
        let app = App::open(&base, Place::Native, opener).unwrap();
        // The first sign-in holds nothing, so there is nothing to revoke.
        app.take_session(&pair("a1", "r1")).await.unwrap();
        assert!(asked.lock().unwrap().is_empty());
        app.take_session(&pair("a2", "r2")).await.unwrap();
        assert_eq!(
            *asked.lock().unwrap(),
            ["post /api/v1/auth/sign-out http/1.1 as a1"]
        );
        assert_eq!(app.api.tokens().access.as_deref(), Some("a2"));
        assert_eq!(
            tokens(&app.lock().keys).unwrap().refresh.as_deref(),
            Some("r2")
        );
        // Signing out revokes the one that is left and holds nothing after.
        app.sign_out().await.unwrap();
        assert_eq!(
            asked.lock().unwrap().last().map(String::as_str),
            Some("post /api/v1/auth/sign-out http/1.1 as a2")
        );
        assert!(app.api.tokens().access.is_none());
        let kept = tokens(&app.lock().keys).unwrap();
        assert!(kept.access.is_none() && kept.refresh.is_none());
    }

    #[test]
    fn only_this_instances_pictures_are_fetched() {
        let name = "0123456789abcdef".repeat(4);
        assert!(is_own_avatar(&format!("/api/v1/avatars/{name}")));
        for other in [
            String::new(),
            format!("https://elsewhere.example/api/v1/avatars/{name}"),
            format!("//elsewhere.example/api/v1/avatars/{name}"),
            format!("/api/v1/avatars/{name}/../../account/snapshot"),
            format!("/api/v1/avatars/{}", name.to_uppercase()),
            "/api/v1/avatars/short".to_string(),
            format!("/api/v1/avatars/{name}?x=1"),
        ] {
            assert!(!is_own_avatar(&other), "{other}");
        }
    }

    #[test]
    fn kept_members_are_per_server() {
        let app = app();
        assert!(app.kept_members("s").unwrap().is_empty());
        let member = MemberSummary {
            account_id: "a".into(),
            handle: "ada".into(),
            display_name: "Ada".into(),
            nickname: String::new(),
            pronouns: "she/her".into(),
            bio: String::new(),
            avatar_url: String::new(),
        };
        let all: HashMap<String, Vec<MemberSummary>> =
            HashMap::from([("s".to_string(), vec![member])]);
        app.keep(KEPT_MEMBERS, &all);
        assert_eq!(app.kept_members("s").unwrap()[0].pronouns, "she/her");
        assert!(app.kept_members("other").unwrap().is_empty());
    }
}
