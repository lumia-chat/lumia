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

use std::collections::{HashMap, HashSet};
use std::future::Future;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

use futures_util::lock::Mutex as AsyncMutex;
use futures_util::StreamExt as _;
use lumia_core::body;
use lumia_core::keystore::Keystore;
use lumia_core::mls::projection::Projection;
use lumia_core::proto::{
    account_event, command, dm_operation, event, mention, ChannelKind, Command, DeviceKind,
    DmOperation, DmRequest, DmRequestState, Event, PushKind, PushSubscription, SendMessage,
    TokenPair,
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
    /// Whether a deleted message keeps its place as "(deleted)" there; off,
    /// the core leaves deleted messages out of what it reads (architecture
    /// 6.7).
    #[serde(default)]
    pub show_deleted: bool,
    /// Whether this account may change the server's settings: its owner,
    /// or a member whose roles grant `MANAGE_SERVER` or `ADMINISTRATOR`
    /// (architecture 6.7). What a shell offers; the instance decides.
    #[serde(default)]
    pub can_manage: bool,
    /// Whether it may create channels there (`MANAGE_CHANNELS`), and so is
    /// offered "Create a channel" (architecture 6.8).
    #[serde(default)]
    pub can_manage_channels: bool,
    /// Whether it may start server DMs there (`START_SERVER_DMS`).
    #[serde(default)]
    pub can_start_dms: bool,
    /// Whether it may mention everyone there (`MENTION_EVERYONE`), and so
    /// is offered `@everyone` in the composer (architecture 6.6).
    #[serde(default)]
    pub can_mention_everyone: bool,
    /// Whether it may delete others' messages there (`MANAGE_MESSAGES`), and
    /// so is offered "Delete message" on them (architecture 6.6).
    #[serde(default)]
    pub can_manage_messages: bool,
    /// This account's own switches there (architecture 6.8): adds to
    /// channels wait as invitations; nobody may start a server DM with it.
    #[serde(default)]
    pub channel_adds_by_invitation: bool,
    #[serde(default)]
    pub server_dms_off: bool,
}

impl ServerSummary {
    /// The server as the snapshot says it, with what `memberships` (the
    /// account's own) says this account may do there.
    fn of(server: &lumia_core::proto::Server, memberships: &[lumia_core::proto::Member]) -> Self {
        use lumia_core::proto::Permission;
        let own = memberships.iter().find(|m| m.server_id == server.server_id);
        let holds = |wanted: Permission| {
            own.is_some_and(|m| {
                m.permissions
                    .iter()
                    .any(|p| *p == wanted as i32 || *p == Permission::Administrator as i32)
            })
        };
        Self {
            server_id: wire::display(&server.server_id),
            name: server.name.clone(),
            icon_url: server.icon_url.clone(),
            retention_days: server.retention_days,
            show_deleted: server.show_deleted,
            can_manage: holds(Permission::ManageServer),
            can_manage_channels: holds(Permission::ManageChannels),
            can_start_dms: holds(Permission::StartServerDms),
            can_mention_everyone: holds(Permission::MentionEveryone),
            can_manage_messages: holds(Permission::ManageMessages),
            channel_adds_by_invitation: own.is_some_and(|m| m.channel_adds_by_invitation),
            server_dms_off: own.is_some_and(|m| m.server_dms_off),
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

/// The profile a Profile page shows: the newer of this device's signed
/// profile (`local`) and the one the instance holds for the account
/// (`held`). When the instance's is at least as new, the account's name and
/// picture as they are now: another device, or a `PATCH /account` since,
/// may have changed them. A newer one of this device's, not yet sent,
/// shows as it is.
fn shown_profile(
    local: Option<&lumia_core::proto::SignedProfile>,
    held: Option<&lumia_core::proto::Account>,
) -> Result<Option<ProfileSummary>, ClientError> {
    let mine = local.map(profile_summary).transpose()?;
    let theirs = held
        .and_then(|a| a.profile.as_ref())
        .map(profile_summary)
        .transpose()?;
    let newer_here = match (&mine, &theirs) {
        (Some(m), Some(t)) => m.version > t.version,
        (Some(_), None) => true,
        (None, _) => false,
    };
    if newer_here {
        return Ok(mine);
    }
    let Some(account) = held else {
        return Ok(theirs.or(mine));
    };
    let base = theirs.or(mine);
    Ok(base.map(|p| ProfileSummary {
        display_name: account.display_name.clone(),
        avatar_url: account.avatar_url.clone(),
        pronouns: account.pronouns.clone(),
        bio: account.bio.clone(),
        ..p
    }))
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

/// The mentions a message's body makes (architecture 6.6): each `@handle`
/// of one of `members`, matched without regard to case, and `@everyone`
/// when `everyone` (this account may mention everyone), with its byte range.
/// An `@` counts at the start or after anything but a letter, digit or
/// underscore (so an address like `ada@example.com` is none), and a handle
/// ends where its characters do, less a trailing `.` or `-` that matches
/// nobody (`@ada.` at a sentence's end is Ada).
#[must_use]
pub fn find_mentions(body: &str, members: &[MemberSummary], everyone: bool) -> Vec<MentionSummary> {
    let handle_char = |c: char| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '-');
    let mut found = Vec::new();
    let mut before: Option<char> = None;
    for (at, c) in body.char_indices() {
        let starts = c == '@' && !before.is_some_and(|b| b.is_alphanumeric() || b == '_');
        before = Some(c);
        if !starts {
            continue;
        }
        let rest = &body[at + 1..];
        let mut word = &rest[..rest.find(|c: char| !handle_char(c)).unwrap_or(rest.len())];
        loop {
            if word.is_empty() {
                break;
            }
            let end = u32::try_from(at + 1 + word.len()).unwrap_or(u32::MAX);
            let start = u32::try_from(at).unwrap_or(u32::MAX);
            if everyone && word.eq_ignore_ascii_case("everyone") {
                found.push(MentionSummary {
                    account_id: String::new(),
                    everyone: true,
                    start,
                    end,
                });
                break;
            }
            if let Some(m) = members
                .iter()
                .find(|m| !m.handle.is_empty() && m.handle.eq_ignore_ascii_case(word))
            {
                found.push(MentionSummary {
                    account_id: m.account_id.clone(),
                    everyone: false,
                    start,
                    end,
                });
                break;
            }
            match word.strip_suffix(['.', '-']) {
                Some(shorter) => word = shorter,
                None => break,
            }
        }
    }
    found
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
    /// `ChannelKind` as its name (`TEXT`, `SERVER_DM`, `PERSONAL_DM`, ...).
    pub kind: String,
    pub last_seq: u64,
    /// DM kinds: who is in it.
    pub participant_account_ids: Vec<String>,
    /// Personal DMs: the hosting instance.
    pub host: String,
    /// `TEXT`: made private, so entered only by being added (architecture 6.8).
    #[serde(default)]
    pub private: bool,
    /// `TEXT`: joined by every newcomer to the server.
    #[serde(default)]
    pub is_default: bool,
    /// How many are in it, where a listing says (Browse); 0 elsewhere.
    #[serde(default)]
    pub member_count: u32,
    /// The channel's topic; empty for none.
    #[serde(default)]
    pub topic: String,
}

impl ChannelSummary {
    fn of(c: &lumia_core::proto::Channel) -> Self {
        Self {
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
            private: c.private,
            is_default: c.is_default,
            member_count: c.member_count,
            topic: c.topic.clone(),
        }
    }
}

/// An invitation into a channel, waiting for this account's yes
/// (architecture 6.8): join the channel to take it, dismiss it to say no.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChannelInvitationSummary {
    pub channel: ChannelSummary,
    pub invited_by_account_id: String,
    pub created_at_micros: i64,
}

/// Who an add brought in at once, and who it invited instead (architecture 6.8).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChannelAddSummary {
    pub added_account_ids: Vec<String>,
    pub invited_account_ids: Vec<String>,
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
    /// Channels this account was asked into and has not answered.
    #[serde(default)]
    pub channel_invitations: Vec<ChannelInvitationSummary>,
    /// Where this account has read each conversation, as the instance
    /// counted it; [`App::read_states`] keeps it current.
    #[serde(default)]
    pub read_states: Vec<ReadStateSummary>,
    /// The rail as this account arranged it, ready to draw: every server of
    /// `servers` placed once, which are in its order (architecture 6.9).
    #[serde(default)]
    pub rail: Vec<crate::rail::RailItem>,
}

impl SnapshotSummary {
    /// The rail tidied against the servers held, and the servers put in its order.
    fn arranged(mut self) -> Self {
        let ids: Vec<String> = self.servers.iter().map(|s| s.server_id.clone()).collect();
        self.rail = crate::rail::tidy(&self.rail, &ids);
        let order: HashMap<String, usize> = crate::rail::server_order(&self.rail)
            .into_iter()
            .enumerate()
            .map(|(at, id)| (id, at))
            .collect();
        self.servers
            .sort_by_key(|s| order.get(&s.server_id).copied().unwrap_or(usize::MAX));
        self
    }
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
    /// Their switches in the server listed (architecture 6.8): an add
    /// becomes an invitation; nobody may start a server DM with them.
    /// False for a public profile.
    #[serde(default)]
    pub channel_adds_by_invitation: bool,
    #[serde(default)]
    pub server_dms_off: bool,
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
    /// A notice the instance posted (architecture 6.8): `joined`, `added`,
    /// `left` or `removed`, with an empty body; empty for a message. The
    /// author is who did it, `notice_account_id` who it is about.
    pub notice: String,
    pub notice_account_id: String,
    /// Whether its author has edited it ("(edited)").
    #[serde(default)]
    pub edited: bool,
    /// What it quotes (architecture 6.6).
    pub reply: Option<QuoteSummary>,
    /// Whom it mentions, and where in its body.
    pub mentions: Vec<MentionSummary>,
    /// Its reactions, in the order each emoji was first used.
    pub reactions: Vec<ReactionSummary>,
    /// Whether it notifies this account: it mentions it or everyone, or
    /// quotes one of its messages.
    pub mentions_me: bool,
    /// What it carries (specs/media.md); empty once deleted.
    #[serde(default)]
    pub attachments: Vec<AttachmentSummary>,
    /// Its link cards, in the order their links appear.
    #[serde(default)]
    pub cards: Vec<CardSummary>,
}

/// Bytes as a JSON string: base64, padded. Ids are strings everywhere a shell
/// sees them; keys and placeholders are bytes the web carries the same way.
mod b64 {
    use base64::Engine as _;
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(bytes: &[u8], s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&base64::engine::general_purpose::STANDARD.encode(bytes))
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<u8>, D::Error> {
        let text = String::deserialize(d)?;
        base64::engine::general_purpose::STANDARD
            .decode(text)
            .map_err(serde::de::Error::custom)
    }
}

/// One blob of an attachment or a card, with what opens it. A shell hands it
/// back to [`App::attachment_bytes`] as it was given; it never decrypts.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct BlobSummary {
    pub blob_id: String,
    /// The stored ciphertext's length: the plaintext's and 16.
    pub size_bytes: u64,
    #[serde(with = "b64")]
    pub key: Vec<u8>,
    #[serde(with = "b64")]
    pub nonce: Vec<u8>,
    /// BLAKE3-256 of the plaintext.
    #[serde(with = "b64")]
    pub hash: Vec<u8>,
}

/// An attachment as a message carries it (specs/media.md 5).
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AttachmentSummary {
    pub attachment_id: String,
    /// `image`, `gif`, `video`, `audio` or `file`.
    pub kind: String,
    /// The name it is shown and saved under.
    pub name: String,
    pub media_type: String,
    /// Images, GIFs and video, after orientation; 0 when unknown.
    pub width: u32,
    pub height: u32,
    /// Video and audio, when known; 0 otherwise.
    pub duration_ms: u32,
    /// ThumbHash of the picture, poster or first frame, drawn until the
    /// thumbnail arrives ([`App::placeholder_image`]); empty when none.
    #[serde(with = "b64")]
    pub placeholder: Vec<u8>,
    /// An image's accessibility text.
    pub alt: String,
    /// The size of the file itself, for a file row.
    pub size_bytes: u64,
    /// The display picture, the GIF, the video, the audio or the file.
    pub main: BlobSummary,
    /// At most 400 px: an image's, a GIF's first frame, a video's poster.
    pub thumbnail: Option<BlobSummary>,
}

/// A link's card (specs/media.md 6). Drawn only under a link the message's
/// text holds, named by `host`, opening `url`.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CardSummary {
    /// The link as the text has it: what the card opens.
    pub url: String,
    pub title: String,
    pub description: String,
    /// The page's own name for its site. Not what a shell shows as the
    /// site: in a personal DM the sender could have written anything.
    pub site_name: String,
    /// The site to show: the link's host, lowercase, without `www.`.
    pub host: String,
    /// `#rrggbb`, or empty.
    pub color: String,
    /// Server content: the picture, a path of this instance
    /// ([`App::card_image`]); empty otherwise.
    pub image_url: String,
    /// Personal DMs: the picture, a blob ([`App::attachment_bytes`]).
    pub image: Option<BlobSummary>,
    pub image_width: u32,
    pub image_height: u32,
    /// The page is a video: drawn with a play symbol, opening the link.
    pub video: bool,
}

/// An attachment uploaded and waiting in the composer: what its strip shows.
/// It goes with the next send that names `draft_id`, or with
/// [`App::discard_attachment`].
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AttachmentDraft {
    pub draft_id: String,
    pub channel_id: String,
    /// As [`AttachmentSummary::kind`]: a picture sent as a file, or one that
    /// could not be read, is a `file`.
    pub kind: String,
    /// The name it will be sent under (an image's extension is what the
    /// core made of it).
    pub name: String,
    pub media_type: String,
    pub width: u32,
    pub height: u32,
    pub duration_ms: u32,
    #[serde(with = "b64")]
    pub placeholder: Vec<u8>,
    pub alt: String,
    /// The size it will be sent at, after processing.
    pub size_bytes: u64,
}

/// Decoded pixels: RGBA8, row by row.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PixelsInput {
    pub rgba: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

/// A file a person chose to attach, and what the platform knows of it.
#[derive(Debug, Clone, Default)]
pub struct AttachmentInput {
    /// The file's bytes, as chosen.
    pub bytes: Vec<u8>,
    /// The file's name.
    pub name: String,
    /// Sent as a file: a picture keeps its pixels and loses its metadata.
    pub as_file: bool,
    /// The picture decoded by the platform, upright, for a format the core
    /// does not decode (HEIC, HEIF).
    pub pixels: Option<PixelsInput>,
    /// A video's poster frame, from the platform.
    pub poster: Option<PixelsInput>,
    /// A video's or audio's length, when the platform read it.
    pub duration_ms: Option<u32>,
    /// A video's size after rotation, when the platform read it (for WebM);
    /// 0 when it did not.
    pub video_width: u32,
    pub video_height: u32,
    /// An image's accessibility text.
    pub alt: String,
}

/// The largest blob of each role the instance takes, in bytes of ciphertext.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct UploadLimitsSummary {
    pub image_max_bytes: u64,
    pub file_max_bytes: u64,
    pub thumbnail_max_bytes: u64,
}

/// A placeholder opened as a picture: a few dozen pixels a side, RGBA8.
#[derive(Debug, Clone, Default, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PlaceholderImage {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

fn kind_name(kind: i32) -> String {
    match lumia_core::proto::AttachmentKind::try_from(kind) {
        Ok(k) if k != lumia_core::proto::AttachmentKind::Unspecified => {
            k.as_str_name().to_lowercase()
        }
        _ => "file".into(),
    }
}

fn blob_summary(b: &lumia_core::proto::BlobRef) -> BlobSummary {
    BlobSummary {
        blob_id: hex_id(&b.blob_id),
        size_bytes: b.size_bytes,
        key: b.key.clone(),
        nonce: b.nonce.clone(),
        hash: b.hash.clone(),
    }
}

fn blob_ref(b: &BlobSummary) -> Result<lumia_core::proto::BlobRef, ClientError> {
    Ok(lumia_core::proto::BlobRef {
        blob_id: parse_uuid(&b.blob_id)?,
        size_bytes: b.size_bytes,
        key: b.key.clone(),
        nonce: b.nonce.clone(),
        hash: b.hash.clone(),
    })
}

fn attachment_summary(a: &lumia_core::proto::Attachment) -> AttachmentSummary {
    let main = a.main.as_ref().map(blob_summary).unwrap_or_default();
    AttachmentSummary {
        attachment_id: hex_id(&a.attachment_id),
        kind: kind_name(a.kind),
        name: a.name.clone(),
        media_type: a.media_type.clone(),
        width: a.width,
        height: a.height,
        duration_ms: a.duration_ms,
        placeholder: a.placeholder.clone(),
        alt: a.alt.clone(),
        size_bytes: main
            .size_bytes
            .saturating_sub(lumia_core::media::TAG_BYTES as u64),
        main,
        thumbnail: a.thumbnail.as_ref().map(blob_summary),
    }
}

fn card_summary(c: &lumia_core::proto::Card) -> CardSummary {
    CardSummary {
        url: c.url.clone(),
        title: c.title.clone(),
        description: c.description.clone(),
        site_name: c.site_name.clone(),
        host: lumia_core::cards::host_for_display(&c.url).unwrap_or_default(),
        color: c.color.clone(),
        image_url: c.image_url.clone(),
        image: c.image.as_ref().map(blob_summary),
        image_width: c.image_width,
        image_height: c.image_height,
        video: c.video,
    }
}

fn draft_summary(channel: &[u8], a: &lumia_core::proto::Attachment) -> AttachmentDraft {
    let shown = attachment_summary(a);
    AttachmentDraft {
        draft_id: shown.attachment_id,
        channel_id: hex_id(channel),
        kind: shown.kind,
        name: shown.name,
        media_type: shown.media_type,
        width: shown.width,
        height: shown.height,
        duration_ms: shown.duration_ms,
        placeholder: shown.placeholder,
        alt: shown.alt,
        size_bytes: shown.size_bytes,
    }
}

/// A quote of another message of the conversation (architecture 6.6).
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct QuoteSummary {
    pub message_id: String,
    /// Its author; in a personal conversation, empty where unknown.
    pub author_account_id: String,
    /// What of it was quoted; empty once it is deleted.
    pub text: String,
    /// The message quoted was deleted, and the text with it.
    pub original_deleted: bool,
    /// Sent without notifying the author quoted; on a send, the reply bar's
    /// "@ On" switch turned off.
    #[serde(default)]
    pub silent: bool,
}

/// A mention: an account, or everyone, and its byte range in the body.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct MentionSummary {
    /// The account mentioned; empty when it is everyone.
    pub account_id: String,
    pub everyone: bool,
    /// Byte offsets into the body, half-open.
    pub start: u32,
    pub end: u32,
}

/// One emoji's reactions to a message.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ReactionSummary {
    pub reaction: String,
    pub count: u32,
    /// Whether this account is among them.
    pub me: bool,
    /// Who reacted, as many as are known (the first fifty a snapshot gives,
    /// and those since).
    pub account_ids: Vec<String>,
}

/// A conversation's read state for this account (architecture 6.6): the
/// last event read, how many messages others sent after it, and how many
/// of those notify it (in a DM, every one).
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ReadStateSummary {
    pub channel_id: String,
    pub read_seq: u64,
    pub unread: u32,
    pub mentions: u32,
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

/// Where linking this device by prompt stands (identity spec 4.2.1).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LinkProgress {
    /// `waiting` (for another device to answer), `compare` (show `digits`),
    /// `linked`, `declined` or `expired`.
    pub stage: String,
    /// The six digits to compare, as `482 913`, at `compare`.
    pub digits: String,
}

/// A prompt on a device that holds the identity: another device asks to be
/// linked (identity spec 4.2.1).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LinkPrompt {
    pub session_id: String,
    /// What it would add.
    pub request: LinkRequestSummary,
    /// `new` (nobody answered yet), `waiting` (answered here, waiting for
    /// the new device), `compare` (show `digits`), or `refused` (the new
    /// device's secret did not match its commitment: never approve).
    pub stage: String,
    pub digits: String,
}

/// What a link request asks to add, shown on the approving device before
/// it approves (identity spec 4.2 step 2).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LinkRequestSummary {
    pub device_id: String,
    /// `native` or `web`.
    pub kind: String,
    pub label: String,
    /// The origin a WEB device is scoped to; empty for a native one.
    pub web_origin: String,
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
    /// Whether this device holds the group past its creation (can read
    /// and send).
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
    /// What it quotes (architecture 6.6); the author is known when the
    /// quoted message is in the conversation.
    pub reply: Option<QuoteSummary>,
    /// Its reactions, each emoji with who reacted (their accounts on this
    /// instance, where known).
    pub reactions: Vec<ReactionSummary>,
    /// What it carries (specs/media.md).
    #[serde(default)]
    pub attachments: Vec<AttachmentSummary>,
    /// Its link cards: only those under a link its text holds.
    #[serde(default)]
    pub cards: Vec<CardSummary>,
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
    /// Someone's name or picture changed (`ProfileUpdated` on this
    /// account's log, its own included): the account as others see it.
    /// The kept member lists have it already; what shows the person draws
    /// it again, and a new picture is a new address to fetch.
    ProfileUpdated {
        account_id: String,
        handle: String,
        display_name: String,
        avatar_url: String,
        pronouns: String,
        bio: String,
    },
    /// A link session moved (`LinkSessionUpdated` on this account's log,
    /// identity spec 4.2.1), after the `AccountApplied` it came in: this
    /// device's own as the new device ([`App::link_progress`]) or one that
    /// prompts here ([`App::link_prompts`]).
    LinkSessionUpdated {
        session_id: String,
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
    /// The loop ended for good. `removed`: because the identity revoked this
    /// device, which has erased what it kept (identity spec 4.4) by the time
    /// this comes; the shell says so instead of only signing out.
    Closed {
        reason: String,
        removed: bool,
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

/// What a serialized link request asks to add.
fn summary_of(bytes: &[u8]) -> Result<LinkRequestSummary, ClientError> {
    let request = lumia_core::proto::LinkRequest::decode(bytes)
        .map_err(|_| ClientError::State("that is not a link request".into()))?;
    if request.device_id.len() != 16 || request.ephemeral_public_key.len() != 32 {
        return Err(ClientError::State("that is not a link request".into()));
    }
    let kind = DeviceKind::try_from(request.kind).unwrap_or(DeviceKind::Native);
    Ok(LinkRequestSummary {
        device_id: wire::display(&request.device_id),
        kind: format!("{kind:?}").to_lowercase(),
        label: request.label,
        web_origin: request.web_origin,
    })
}

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

/// How many pages a read of messages goes on through while deleted ones
/// left out keep it short (architecture 6.7).
const DELETED_PAGES: usize = 10;

/// Where the last snapshot and the last member lists are kept in the
/// instance store's meta area (`App::kept_snapshot`, `App::kept_members`).
const KEPT_SNAPSHOT: &str = "kept_snapshot";
const KEPT_MEMBERS: &str = "kept_members";

/// Kept in the keys store from the moment a call hears that the identity
/// revoked this device until it has erased: one that closed first erases
/// at its next launch.
const REVOKED_HEARD: &str = "device_revoked";

fn hex_id(id: &[u8]) -> String {
    if id.is_empty() {
        String::new()
    } else {
        wire::display(id)
    }
}

fn stored_summary(m: &lumia_core::store::StoredMessage, me: &[u8]) -> MessageSummary {
    let mentions_me = m.mentions.iter().any(|x| x.everyone || x.account_id == me)
        || m.reply
            .as_ref()
            .is_some_and(|q| q.author_account_id == me && !q.silent);
    MessageSummary {
        message_id: wire::display(&m.message_id),
        channel_id: wire::display(&m.channel_id),
        seq: m.seq,
        author_account_id: wire::display(&m.author_account_id),
        body: m.body.clone(),
        created_at_micros: m.created_at_micros,
        deleted: m.deleted,
        notice: m.notice.clone(),
        notice_account_id: hex_id(&m.notice_account_id),
        edited: m.edited,
        reply: m.reply.as_ref().map(|q| QuoteSummary {
            message_id: wire::display(&q.message_id),
            author_account_id: hex_id(&q.author_account_id),
            text: q.text.clone(),
            original_deleted: q.original_deleted,
            silent: q.silent,
        }),
        mentions: m
            .mentions
            .iter()
            .map(|x| MentionSummary {
                account_id: hex_id(&x.account_id),
                everyone: x.everyone,
                start: x.start,
                end: x.end,
            })
            .collect(),
        reactions: m
            .reactions
            .iter()
            .map(|r| ReactionSummary {
                reaction: r.reaction.clone(),
                count: u32::try_from(r.account_ids.len()).unwrap_or(u32::MAX),
                me: r.account_ids.iter().any(|a| a == me),
                account_ids: r.account_ids.iter().map(|a| wire::display(a)).collect(),
            })
            .collect(),
        mentions_me: !me.is_empty() && mentions_me,
        attachments: m.attachments.iter().map(attachment_summary).collect(),
        cards: m.cards.iter().map(card_summary).collect(),
    }
}

fn message_summary(m: &lumia_core::proto::Message, me: &[u8]) -> MessageSummary {
    let stored = lumia_core::store::StoredMessage::of(m);
    let mut summary = stored_summary(&stored, me);
    // The instance counted these, beyond the fifty it names.
    for (r, given) in summary.reactions.iter_mut().zip(&m.reactions) {
        r.count = given.count;
        r.me = given.me;
    }
    summary
}

fn event_summary(e: &Event) -> EventSummary {
    let (kind, message) = match &e.payload {
        Some(event::Payload::MessageCreated(c)) => (
            "message_created",
            c.message.as_ref().map(|m| message_summary(m, &[])),
        ),
        Some(event::Payload::MessageEdited(_)) => ("message_edited", None),
        Some(event::Payload::MessageCardsSet(_)) => ("message_cards_set", None),
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

/// The people an account log's events say changed their name or picture.
fn profiles_updated(events: &[lumia_core::proto::AccountEvent]) -> Vec<AppEvent> {
    events
        .iter()
        .filter_map(|e| match &e.payload {
            Some(account_event::Payload::ProfileUpdated(p)) => p.account.as_ref(),
            _ => None,
        })
        .map(|a| AppEvent::ProfileUpdated {
            account_id: wire::display(&a.account_id),
            handle: a.handle.clone(),
            display_name: a.display_name.clone(),
            avatar_url: a.avatar_url.clone(),
            pronouns: a.pronouns.clone(),
            bio: a.bio.clone(),
        })
        .collect()
}

/// The link sessions an account batch moved, each once.
fn link_sessions_updated(events: &[lumia_core::proto::AccountEvent]) -> Vec<AppEvent> {
    let mut ids: Vec<String> = Vec::new();
    for e in events {
        if let Some(account_event::Payload::LinkSessionUpdated(l)) = &e.payload {
            let id = wire::display(&l.session_id);
            if !ids.contains(&id) {
                ids.push(id);
            }
        }
    }
    ids.into_iter()
        .map(|session_id| AppEvent::LinkSessionUpdated { session_id })
        .collect()
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
        GatewayEvent::Closed(reason) => AppEvent::Closed {
            reason,
            removed: false,
        },
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
    /// Read state, as the last snapshot counted it and the events since
    /// have added to it (architecture 6.6).
    reads: Reads,
    /// The servers that show deleted messages (architecture 6.7), as the last
    /// snapshot said, and the server of each channel this client has heard
    /// of: the snapshot's, and those Browse and a server DM begun named,
    /// which a channel read before joining is. Every other conversation
    /// leaves deleted messages out.
    shows_deleted: HashSet<String>,
    channel_servers: HashMap<Vec<u8>, String>,
    /// The rail arrangements this client sent ([`App::arrange_rail`]).
    rails: RailSends,
    /// The reauth token the last sign-in carried or a reauth gave, until an
    /// identity operation spends it (identity spec 10). Memory only.
    reauth: Option<String>,
    /// A phrase made before its identity, held in memory only: the web's
    /// sign-up shows and confirms it first (identity spec 4.1 step 7).
    held_phrase: Option<HeldPhrase>,
    /// This device's link session, as the new device (identity spec 4.2.1).
    linking: Option<NewLink>,
    /// Moves on with every `link_start` and `link_cancel`: a session opened
    /// by a start given up or overtaken meanwhile is declined as it opens.
    link_runs: u64,
    /// The link sessions this device answered, by session id.
    answers: HashMap<Vec<u8>, Answered>,
    /// The positions last asked for, of the held or kept phrase.
    quiz: Option<Vec<usize>>,
}

/// The link session this device opened as the new device, and what it
/// needs to finish it: its request, its secret, and the answer it revealed
/// that secret to (identity spec 4.2.1). The digits are of that answer,
/// whatever the instance says later: one that could change it after the
/// reveal could try answers until the digits matched the other device's.
#[derive(Clone)]
struct NewLink {
    session_id: Vec<u8>,
    request: Vec<u8>,
    reveal: Vec<u8>,
    answer: Option<Vec<u8>>,
}

/// A link session this device answered: its secret, and the request and
/// commitment it answered, held from before the secret was drawn. The
/// digits and the approval are of these, whatever the instance says later:
/// one that could change the commitment after seeing the answer could try
/// secrets until the digits matched the new device's. `code` is the
/// approval's, kept until the session has it.
#[derive(Clone)]
struct Answered {
    answer: Vec<u8>,
    request: Vec<u8>,
    commitment: Vec<u8>,
    code: Option<String>,
}

/// What [`App::link_start`] answers when it was given up, or started again,
/// while its session opened: a shell drops it.
pub const LINK_GIVEN_UP: &str = "linking was given up";

/// A phrase made and not yet an identity's, and whether the person has
/// shown they hold it.
struct HeldPhrase {
    entropy: Vec<u8>,
    confirmed: bool,
}

/// The arrangements of the rail this client has sent: how many, how many
/// were answered, and the last one sent while it stands.
///
/// They go one at a time, in the order they were made, so the instance keeps
/// the last (latest write wins). A snapshot read while one is on its way, or
/// begun before one was answered, may hold the rail as it was before; it
/// shows the last one sent instead, so a shell drawing it does not put an
/// edit back or make the next edit from a rail the instance no longer keeps.
#[derive(Debug, Default)]
struct RailSends {
    sent: u64,
    answered: u64,
    /// `None` once the last one sent was refused.
    latest: Option<Vec<crate::rail::RailItem>>,
}

/// Read state as this client keeps it between snapshots.
#[derive(Debug, Default)]
struct Reads {
    /// This account.
    me: Vec<u8>,
    by_channel: HashMap<Vec<u8>, ReadCount>,
}

#[derive(Debug, Default, Clone)]
struct ReadCount {
    /// `TEXT`, `SERVER_DM`, `PERSONAL_DM`, …
    kind: String,
    read_seq: u64,
    unread: u32,
    mentions: u32,
    /// The newest event counted, so a replayed one is not counted again.
    counted_through: u64,
}

impl Reads {
    fn dm(kind: &str) -> bool {
        matches!(
            kind,
            "SERVER_DM" | "SERVER_GROUP_DM" | "PERSONAL_DM" | "PERSONAL_GROUP_DM"
        )
    }

    /// Adds what the events bring: a message from someone else is unread,
    /// and notifies this account when it mentions it or everyone, quotes
    /// one of its messages, or is in a DM.
    fn count(&mut self, events: &[Event]) {
        for e in events {
            let Some(count) = self.by_channel.get_mut(&e.channel_id) else {
                continue;
            };
            if e.seq <= count.counted_through.max(count.read_seq) {
                continue;
            }
            count.counted_through = e.seq;
            let (others, notifies) = match &e.payload {
                Some(event::Payload::MessageCreated(c)) => match &c.message {
                    Some(m) if m.notice.is_none() && m.parent_id.is_empty() => {
                        let stored = lumia_core::store::StoredMessage::of(m);
                        let mine = stored.author_account_id == self.me;
                        let named = stored
                            .mentions
                            .iter()
                            .any(|x| x.everyone || x.account_id == self.me)
                            || stored
                                .reply
                                .as_ref()
                                .is_some_and(|q| q.author_account_id == self.me && !q.silent);
                        (!mine, named)
                    }
                    _ => (false, false),
                },
                Some(event::Payload::Encrypted(p)) => (
                    p.kind == lumia_core::proto::EncryptedKind::Application as i32
                        && e.actor_account_id != self.me,
                    false,
                ),
                _ => (false, false),
            };
            if others {
                count.unread += 1;
                if notifies || Self::dm(&count.kind) {
                    count.mentions += 1;
                }
            }
        }
    }

    /// Another device read a conversation through `seq`.
    fn read_elsewhere(&mut self, channel_id: &[u8], seq: u64) {
        if let Some(count) = self.by_channel.get_mut(channel_id) {
            count.read_seq = count.read_seq.max(seq);
            if seq >= count.counted_through {
                count.unread = 0;
                count.mentions = 0;
            }
        }
    }

    fn summaries(&self) -> Vec<ReadStateSummary> {
        self.by_channel
            .iter()
            .map(|(id, c)| ReadStateSummary {
                channel_id: wire::display(id),
                read_seq: c.read_seq,
                unread: c.unread,
                mentions: c.mentions,
            })
            .collect()
    }
}

/// How far a personal conversation is read when the person has read it
/// through `seq`, its newest message on screen: on through what follows that
/// shows no message of its own (a reaction, an edit, a delete, what this
/// device could not decrypt), up to the next message that shows. The
/// instance counts every application message of the others as unread, not
/// knowing what is in it; read only to the newest message, those after it
/// stayed unread for good.
fn dm_read_through(rows: &[crate::sqlite::DmRow], seq: u64) -> u64 {
    let deleted: HashSet<Vec<u8>> = rows
        .iter()
        .filter_map(
            |row| match DmOperation::decode(row.bytes.as_slice()).ok()?.op? {
                dm_operation::Op::Delete(d) => Some(d.message_id),
                _ => None,
            },
        )
        .collect();
    let mut through = seq;
    for row in rows.iter().filter(|row| row.seq > seq) {
        let shows = row.status == DmStatus::Decrypted
            && matches!(
                DmOperation::decode(row.bytes.as_slice()).ok().and_then(|op| op.op),
                Some(dm_operation::Op::Send(send)) if !deleted.contains(&send.message_id)
            );
        if shows {
            break;
        }
        through = row.seq;
    }
    through
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
    /// New when the device is erased: a revoked id is never this device's
    /// again.
    device_id: Mutex<Vec<u8>>,
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
    /// Held while an arrangement of the rail is sent: they go one at a time.
    rail_turn: AsyncMutex<()>,
    /// The largest message body the instance takes, in bytes: its
    /// `Welcome.limits`, or the protocol's until one arrives.
    max_message_bytes: AtomicU32,
    /// Attachments waiting to be sent, cards made for personal DMs, and the
    /// blobs opened (specs/media.md).
    media: Mutex<Media>,
}

/// What this client holds of attachments and cards between calls. Memory
/// only, but the opened blobs natively once a directory is given.
#[derive(Default)]
struct Media {
    /// Uploaded and not yet sent, by attachment id.
    drafts: HashMap<String, Draft>,
    /// Cards this device built for links in personal conversations, by
    /// conversation and link, for the next send there (specs/media.md 6.5).
    dm_cards: HashMap<(Vec<u8>, String), lumia_core::proto::Card>,
    /// What `GET /instance` said of uploads: `Some(None)` when it keeps no
    /// blobs.
    limits: Option<Option<lumia_core::proto::UploadLimits>>,
    /// What `GET /instance` said of server link cards.
    link_cards: Option<bool>,
    cache: crate::attachments::BlobCache,
}

/// An attachment uploaded into a conversation and not yet sent, keys and all.
struct Draft {
    channel_id: Vec<u8>,
    attachment: lumia_core::proto::Attachment,
}

/// The largest picture of a server card fetched.
const CARD_IMAGE_MAX_BYTES: usize = 1 << 20;

impl std::fmt::Debug for App {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("App")
            .field("host", &self.host)
            .field("device_id", &wire::display(&self.this_device()))
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
        let mut keys = opener(StoreName::Keys)?;
        let identity_store = opener(StoreName::IdentityStore)?;
        let mut identity_keys = opener(StoreName::IdentityKeys)?;
        // A call heard that the identity revoked this device, and it was not
        // erased before the app closed: it is now, before anything is read.
        // The marker goes last, and only once everything else went: what did
        // not is tried again at the next sign-in, sign-out or launch.
        let unerased = keys.get(REVOKED_HEARD)?.is_some() && {
            let rest = [store.erase(), identity_store.erase(), identity_keys.erase()];
            rest.iter().any(Result::is_err) || keys.erase().is_err()
        };
        store.meta_set("base_url", base_url.trim_end_matches('/'))?;
        let reader = opener(StoreName::Instance)?;
        let writer_generation = writer_generation(&reader)?;
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
        let heard = Arc::clone(&opener);
        let held = if unerased {
            // Still the revoked device's: nothing it held is presented.
            let _ = keys.delete("access_token");
            let _ = keys.delete("refresh_token");
            Tokens::default()
        } else {
            tokens(&keys)?
        };
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
            .on_revoked(Arc::new(move || {
                if let Ok(mut keys) = heard(StoreName::Keys) {
                    let _ = keys.put(REVOKED_HEARD, b"1");
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
        if unerased {
            api.hear_revoked();
        }
        // On the web the sink is not `Send` (it holds a JS function) and
        // the worker is single-threaded; natively everything inside is.
        #[allow(clippy::arc_with_non_send_sync)]
        Ok(Arc::new(Self {
            api,
            host,
            device_id: Mutex::new(device_id),
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
                reads: Reads::default(),
                shows_deleted: HashSet::new(),
                channel_servers: HashMap::new(),
                rails: RailSends::default(),
                reauth: None,
                held_phrase: None,
                linking: None,
                link_runs: 0,
                answers: HashMap::new(),
                quiz: None,
            }),
            rail_turn: AsyncMutex::new(()),
            max_message_bytes: AtomicU32::new(body::MAX_MESSAGE_BYTES),
            identity: AsyncMutex::new(IdentityStores {
                store: identity_store,
                keys: identity_keys,
            }),
            dm: Mutex::new(None),
            writer,
            media: Mutex::new(Media::default()),
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
        wire::display(&self.this_device())
    }

    fn this_device(&self) -> Vec<u8> {
        self.device_id
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
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
        self.erase_if_revoked().await;
        let pair = self
            .api
            .passkey_assert_finish(challenge_id, credential_json, self.this_device())
            .await;
        let pair = self.erased_if_revoked(pair).await?;
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
        self.erase_if_revoked().await;
        let pair = self
            .api
            .sign_in_with_passkey(
                handle,
                password,
                self.this_device(),
                challenge_id,
                credential_json,
            )
            .await;
        let pair = self.erased_if_revoked(pair).await?;
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
        self.erase_if_revoked().await;
        let token = magic_token_of(token);
        let pair = self
            .api
            .magic_link_complete(&token, self.this_device())
            .await;
        let pair = self.erased_if_revoked(pair).await?;
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
        self.erase_if_revoked().await;
        let code = invite.map(invite_code_of).unwrap_or_default();
        let pair = self
            .api
            .create_account(handle, "", password, &code, self.this_device())
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
        // Its owner may do everything there.
        Ok(ServerSummary {
            can_manage: true,
            can_manage_channels: true,
            can_start_dms: true,
            can_mention_everyone: true,
            can_manage_messages: true,
            ..ServerSummary::of(&s, &[])
        })
    }

    /// `POST /channels`: a text channel in a server this account may
    /// manage channels in, private when asked (architecture 6.8). This
    /// account is in it; the next snapshot lists it.
    ///
    /// # Errors
    /// `PERMISSION_DENIED` without `manage_channels` there; transport failure.
    pub async fn create_channel(
        &self,
        server_id: &str,
        name: &str,
        private: bool,
    ) -> Result<ChannelSummary, ClientError> {
        let id = uuid::Uuid::parse_str(server_id)
            .map_err(|_| ClientError::Transport(format!("not a uuid: {server_id}")))?;
        let c = self
            .api
            .create_channel(wire::uuid_bytes(id), name.trim(), private)
            .await?;
        Ok(ChannelSummary {
            last_seq: 0,
            ..ChannelSummary::of(&c)
        })
    }

    /// `GET /servers/{id}/channels/browse`: the public channels of a server
    /// this account can see and is not in, each with how many are in it, in
    /// the order the sidebar shows them (architecture 6.8).
    ///
    /// # Errors
    /// On transport or API failure.
    pub async fn browse_channels(
        &self,
        server_id: &str,
    ) -> Result<Vec<ChannelSummary>, ClientError> {
        let server_id = parse_uuid(server_id)?;
        let list = self.api.browse_channels(&wire::display(&server_id)).await?;
        let channels: Vec<ChannelSummary> = list.channels.iter().map(ChannelSummary::of).collect();
        // A channel read before joining is one of these.
        self.note_channels(&channels);
        Ok(channels)
    }

    /// `POST /channels/{id}/join`: a public channel this account can see,
    /// or one it was invited to; the "joined" notice is posted there.
    /// Returns the snapshot as it then is, the channel in it.
    ///
    /// # Errors
    /// `NOT_FOUND` for a channel it may not join; transport failure.
    pub async fn join_channel(&self, channel_id: &str) -> Result<SnapshotSummary, ClientError> {
        let channel_id = parse_uuid(channel_id)?;
        self.api.join_channel(&wire::display(&channel_id)).await?;
        self.snapshot().await
    }

    /// `POST /channels/{id}/leave`: any channel, a default one included.
    /// Returns the snapshot as it then is.
    ///
    /// # Errors
    /// `NOT_FOUND` when not in it; transport failure.
    pub async fn leave_channel(&self, channel_id: &str) -> Result<SnapshotSummary, ClientError> {
        let channel_id = parse_uuid(channel_id)?;
        self.api.leave_channel(&wire::display(&channel_id)).await?;
        self.snapshot().await
    }

    /// `GET /channels/{id}/members`: who is in a channel this account can
    /// see, as account ids in the order they came in.
    ///
    /// # Errors
    /// `NOT_FOUND` for one it cannot see; transport failure.
    pub async fn channel_members(&self, channel_id: &str) -> Result<Vec<String>, ClientError> {
        let channel_id = parse_uuid(channel_id)?;
        let list = self
            .api
            .channel_members(&wire::display(&channel_id))
            .await?;
        Ok(list
            .account_ids
            .iter()
            .map(|id| wire::display(id))
            .collect())
    }

    /// `POST /channels/{id}/members`: members of the server added to a
    /// channel this account is in, or invited when they take adds that way.
    ///
    /// # Errors
    /// `PERMISSION_DENIED` (or `NOT_FOUND`) when it may not add there;
    /// `INVALID_ARGUMENT` with `details.field` `account_ids` for someone
    /// not in the server or unable to see the channel; transport failure.
    pub async fn add_channel_members(
        &self,
        channel_id: &str,
        account_ids: &[String],
    ) -> Result<ChannelAddSummary, ClientError> {
        let channel_id = parse_uuid(channel_id)?;
        let ids = account_ids
            .iter()
            .map(|id| parse_uuid(id))
            .collect::<Result<Vec<_>, _>>()?;
        let added = self
            .api
            .add_channel_members(&wire::display(&channel_id), ids)
            .await?;
        Ok(ChannelAddSummary {
            added_account_ids: added
                .added_account_ids
                .iter()
                .map(|id| wire::display(id))
                .collect(),
            invited_account_ids: added
                .invited_account_ids
                .iter()
                .map(|id| wire::display(id))
                .collect(),
        })
    }

    /// `DELETE /channels/{id}/members/{account_id}`: someone taken out of
    /// a channel, by a member with `MANAGE_CHANNELS`; this account's own id
    /// is leaving.
    ///
    /// # Errors
    /// `PERMISSION_DENIED` without `MANAGE_CHANNELS`; transport failure.
    pub async fn remove_channel_member(
        &self,
        channel_id: &str,
        account_id: &str,
    ) -> Result<(), ClientError> {
        let channel_id = parse_uuid(channel_id)?;
        let account_id = parse_uuid(account_id)?;
        self.api
            .remove_channel_member(&wire::display(&channel_id), &wire::display(&account_id))
            .await
    }

    /// `DELETE /channel-invitations/{channel_id}`: no to an invitation.
    /// Returns the snapshot as it then is. To say yes, join the channel.
    ///
    /// # Errors
    /// `NOT_FOUND` without one; transport failure.
    pub async fn dismiss_channel_invitation(
        &self,
        channel_id: &str,
    ) -> Result<SnapshotSummary, ClientError> {
        let channel_id = parse_uuid(channel_id)?;
        self.api
            .dismiss_channel_invitation(&wire::display(&channel_id))
            .await?;
        self.snapshot().await
    }

    /// `PATCH /servers/{id}/membership`: this account's own switches in a
    /// server (architecture 6.8); `None` leaves one as it is. Returns the
    /// snapshot as it then is.
    ///
    /// # Errors
    /// `NOT_FOUND` for a server it is not in; transport failure.
    pub async fn update_membership(
        &self,
        server_id: &str,
        channel_adds_by_invitation: Option<bool>,
        server_dms_off: Option<bool>,
    ) -> Result<SnapshotSummary, ClientError> {
        let server_id = parse_uuid(server_id)?;
        let req = lumia_core::proto::UpdateMembershipRequest {
            channel_adds_by_invitation,
            server_dms_off,
        };
        self.api
            .update_membership(&wire::display(&server_id), &req)
            .await?;
        self.snapshot().await
    }

    /// `POST /servers/{id}/dms`: the 1:1 server DM with a member of the
    /// server, started if there is none (architecture 6.4). The account
    /// log brings it to the snapshot; the channel is returned to open.
    ///
    /// # Errors
    /// `PERMISSION_DENIED` without `START_SERVER_DMS`;
    /// `FAILED_PRECONDITION` with `details.state` `server_dms_off` when they
    /// turned server DMs off; `NOT_FOUND` for someone not in the server or
    /// blocked either way; transport failure.
    pub async fn open_server_dm(
        &self,
        server_id: &str,
        account_id: &str,
    ) -> Result<ChannelSummary, ClientError> {
        let server_id = parse_uuid(server_id)?;
        let account_id = parse_uuid(account_id)?;
        let c = self
            .api
            .open_server_dm(&wire::display(&server_id), account_id)
            .await?;
        let channel = ChannelSummary::of(&c);
        self.note_channels(std::slice::from_ref(&channel));
        Ok(channel)
    }

    /// `PATCH /servers/{id}` with exactly `channel_ids` as the server's
    /// default channels, the ones every newcomer joins: public text
    /// channels of the server, or none. Returns the snapshot as it then is.
    ///
    /// # Errors
    /// `PERMISSION_DENIED` without `MANAGE_SERVER`; `INVALID_ARGUMENT` with
    /// `details.field` `default_channels`; transport failure.
    pub async fn set_default_channels(
        &self,
        server_id: &str,
        channel_ids: &[String],
    ) -> Result<SnapshotSummary, ClientError> {
        let server_id = parse_uuid(server_id)?;
        let ids = channel_ids
            .iter()
            .map(|id| parse_uuid(id))
            .collect::<Result<Vec<_>, _>>()?;
        self.api
            .set_default_channels(&wire::display(&server_id), ids)
            .await?;
        self.snapshot().await
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
        self.erase_if_revoked().await;
        let pair = match totp_code.filter(|c| !c.is_empty()) {
            Some(code) => {
                self.api
                    .sign_in_with_totp(handle, password, self.this_device(), code)
                    .await
            }
            None => self.api.sign_in(handle, password, self.this_device()).await,
        };
        let pair = self.erased_if_revoked(pair).await?;
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
        // Signed out because a call heard this device was revoked: it
        // erases now, not at the next sign-in.
        self.erase_if_revoked().await;
        tokens?;
        kept?;
        Ok(())
    }

    /// `GET /account/snapshot`: the account, its servers, the channels it is
    /// shown (the servers' it is in, its server DMs and its personal DMs),
    /// the requests and invitations waiting.
    ///
    /// # Errors
    /// On transport or API failure.
    pub async fn snapshot(&self) -> Result<SnapshotSummary, ClientError> {
        let answered = self.lock().rails.answered;
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
            channels: snap.channels.iter().map(ChannelSummary::of).collect(),
            pending_requests: snap.pending_requests.iter().map(request_summary).collect(),
            channel_invitations: snap
                .channel_invitations
                .iter()
                .map(|i| ChannelInvitationSummary {
                    channel: ChannelSummary::of(&i.channel.clone().unwrap_or_default()),
                    invited_by_account_id: wire::display(&i.invited_by_account_id),
                    created_at_micros: wire::micros(i.created_at.as_ref()).unwrap_or(0),
                })
                .collect(),
            read_states: Vec::new(),
            rail: snap
                .rail
                .as_ref()
                .map(crate::rail::from_proto)
                .unwrap_or_default(),
        }
        .arranged();
        let mut summary = summary;
        {
            // An arrangement on its way, or answered since this was asked
            // for, is newer than the instance's rail here ([`RailSends`]).
            let inner = self.lock();
            let rails = &inner.rails;
            if let Some(latest) = &rails.latest {
                if rails.sent > rails.answered || rails.answered > answered {
                    summary.rail = latest.clone();
                    summary = summary.arranged();
                }
            }
        }
        {
            let mut inner = self.lock();
            let reads = &mut inner.reads;
            reads.me = account.account_id.clone();
            reads.by_channel = snap
                .read_states
                .iter()
                .map(|r| {
                    let kind = snap
                        .channels
                        .iter()
                        .find(|c| c.channel_id == r.channel_id)
                        .and_then(|c| lumia_core::proto::ChannelKind::try_from(c.kind).ok())
                        .map(|k| k.as_str_name().to_string())
                        .unwrap_or_default();
                    let head = snap
                        .channels
                        .iter()
                        .find(|c| c.channel_id == r.channel_id)
                        .map_or(r.read_seq, |c| c.last_seq);
                    let count = ReadCount {
                        kind,
                        read_seq: r.read_seq,
                        unread: r.unread,
                        mentions: r.mentions,
                        counted_through: head,
                    };
                    (r.channel_id.clone(), count)
                })
                .collect();
            summary.read_states = reads.summaries();
        }
        self.note_shows_deleted(&summary);
        self.keep(KEPT_SNAPSHOT, &summary);
        Ok(summary)
    }

    /// Which servers draw their deleted messages, and whose the snapshot's channels are.
    fn note_shows_deleted(&self, snapshot: &SnapshotSummary) {
        let mut inner = self.lock();
        inner.shows_deleted = snapshot
            .servers
            .iter()
            .filter(|s| s.show_deleted)
            .map(|s| s.server_id.clone())
            .collect();
        drop(inner);
        self.note_channels(&snapshot.channels);
    }

    /// Whose these channels are, for whether their deleted messages are read.
    fn note_channels(&self, channels: &[ChannelSummary]) {
        let mut inner = self.lock();
        for c in channels.iter().filter(|c| !c.server_id.is_empty()) {
            if let Ok(id) = parse_uuid(&c.channel_id) {
                inner.channel_servers.insert(id, c.server_id.clone());
            }
        }
    }

    /// Whether a channel draws its deleted messages: its server's say.
    fn shows_deleted(inner: &Inner, channel: &[u8]) -> bool {
        inner
            .channel_servers
            .get(channel)
            .is_some_and(|server| inner.shows_deleted.contains(server))
    }

    /// The last snapshot this device got, kept in the instance store so a
    /// launch has the servers and channels before the instance answers, or
    /// without it answering at all (architecture 13, "Local-first"). `None`
    /// before the first one, and after signing out.
    ///
    /// # Errors
    /// On a store failure.
    pub fn kept_snapshot(&self) -> Result<Option<SnapshotSummary>, ClientError> {
        // One kept before arrangements reads as its servers in their order.
        let kept: Option<SnapshotSummary> = self
            .kept::<SnapshotSummary>(KEPT_SNAPSHOT)?
            .map(SnapshotSummary::arranged);
        if let Some(snapshot) = &kept {
            self.note_shows_deleted(snapshot);
        }
        Ok(kept)
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
            channel_adds_by_invitation: false,
            server_dms_off: false,
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
                    channel_adds_by_invitation: m.channel_adds_by_invitation,
                    server_dms_off: m.server_dms_off,
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

    /// The people of `profiles` ([`AppEvent::ProfileUpdated`]) as they now
    /// are, in every member list kept: an offline launch names them so.
    /// Each server's nickname and switches are its own and stay.
    fn keep_profiles(&self, profiles: &[AppEvent]) {
        if profiles.is_empty() {
            return;
        }
        let Some(mut all) = self
            .kept::<HashMap<String, Vec<MemberSummary>>>(KEPT_MEMBERS)
            .ok()
            .flatten()
        else {
            return;
        };
        for profile in profiles {
            let AppEvent::ProfileUpdated {
                account_id,
                handle,
                display_name,
                avatar_url,
                pronouns,
                bio,
            } = profile
            else {
                continue;
            };
            for member in all.values_mut().flatten() {
                if member.account_id == *account_id {
                    member.handle.clone_from(handle);
                    member.display_name.clone_from(display_name);
                    member.avatar_url.clone_from(avatar_url);
                    member.pronouns.clone_from(pronouns);
                    member.bio.clone_from(bio);
                }
            }
        }
        self.keep(KEPT_MEMBERS, &all);
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

    /// The newest messages of a channel from the local store, oldest first,
    /// without the deleted ones unless its server shows them (architecture
    /// 6.7).
    ///
    /// # Errors
    /// On a store failure or a malformed id.
    pub fn messages(
        &self,
        channel_id: &str,
        limit: u32,
    ) -> Result<Vec<MessageSummary>, ClientError> {
        self.messages_before(channel_id, u64::MAX, limit)
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
        let shown = Self::shows_deleted(&inner, &id);
        // Read back until `limit` messages that are there, or the store's
        // first: a run of deleted ones left out must not end the page early,
        // or a shell would take it for the start of the channel.
        let mut kept: Vec<lumia_core::store::StoredMessage> = Vec::new();
        let mut below = before_seq;
        for _ in 0..DELETED_PAGES {
            let rows = inner.reader.messages_before(&id, below, limit as usize)?;
            let full = rows.len() == limit as usize;
            below = rows.first().map_or(0, |m| m.seq);
            let mut page: Vec<lumia_core::store::StoredMessage> =
                rows.into_iter().filter(|m| shown || !m.deleted).collect();
            page.append(&mut kept);
            kept = page;
            if kept.len() >= limit as usize || !full || below == 0 {
                break;
            }
        }
        let skip = kept.len().saturating_sub(limit as usize);
        Ok(kept[skip..]
            .iter()
            .map(|m| stored_summary(m, &inner.reads.me))
            .collect())
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
        let (me, shown) = {
            let inner = self.lock();
            let shown = parse_uuid(channel_id).is_ok_and(|id| Self::shows_deleted(&inner, &id));
            (inner.reads.me.clone(), shown)
        };
        // As `messages_before` does: pages read on while deleted messages left
        // out leave this one short, so scrollback does not stop at them.
        let mut found: Vec<MessageSummary> = Vec::new();
        let mut below = before_seq;
        let mut more = true;
        for _ in 0..DELETED_PAGES {
            let page = self.api.list_messages(channel_id, below, limit).await?;
            more = page.has_more;
            below = page.messages.first().map_or(0, |m| m.seq);
            let mut these: Vec<MessageSummary> = page
                .messages
                .iter()
                .filter(|m| shown || m.deleted_at.is_none())
                .map(|m| message_summary(m, &me))
                .collect();
            these.append(&mut found);
            found = these;
            if found.len() >= limit as usize || !more || below == 0 {
                break;
            }
        }
        Ok((found, more))
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
        let device_id = self.this_device();
        let back = Arc::clone(self);
        let end_sink = Arc::clone(&sink);
        // Dropped when the task passing the loop's events on has passed the
        // last of them.
        let (passed_on, all_passed) = crate::futures_channel::oneshot::channel::<()>();
        spawn(async move {
            let (store, revoked) =
                gateway::run_until_end(api, store, "lumia-app", device_id, channels, rx, etx).await;
            {
                let mut inner = back.lock();
                inner.store = Some(store);
                inner.commands = None;
                inner.sink = None;
            }
            // The identity no longer holds this device: what it kept goes,
            // and only then is the shell told the loop has ended, after every
            // event before it.
            if let Some(reason) = revoked {
                let _ = all_passed.await;
                let _ = back.erase_device().await;
                end_sink(AppEvent::Closed {
                    reason,
                    removed: true,
                });
            }
        });
        let app = Arc::clone(self);
        let task_sink = Arc::clone(&sink);
        spawn(async move {
            let _passed_on = passed_on;
            while let Some(ev) = erx.next().await {
                match &ev {
                    GatewayEvent::Notice(Notice::Welcomed(w)) => {
                        let mut inner = app.lock();
                        let _ = inner
                            .reader
                            .meta_set(gateway::ACCOUNT_META, &hex::encode(&w.account_id));
                        inner.reads.me = w.account_id.clone();
                        drop(inner);
                        if let Some(max) = w
                            .limits
                            .as_ref()
                            .map(|l| l.max_message_bytes)
                            .filter(|max| *max > 0)
                        {
                            app.max_message_bytes.store(max, Ordering::Relaxed);
                        }
                    }
                    GatewayEvent::Applied(events) => app.lock().reads.count(events),
                    GatewayEvent::AccountApplied(events) => {
                        let mut inner = app.lock();
                        for e in events {
                            if let Some(account_event::Payload::ReadPositionUpdated(r)) = &e.payload
                            {
                                inner.reads.read_elsewhere(&r.channel_id, r.seq);
                            }
                        }
                        drop(inner);
                        app.keep_profiles(&profiles_updated(events));
                    }
                    _ => {}
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
                let profiles = match &ev {
                    GatewayEvent::AccountApplied(events) => {
                        let mut after = profiles_updated(events);
                        after.extend(link_sessions_updated(events));
                        after
                    }
                    _ => Vec::new(),
                };
                task_sink(app_event(ev));
                for profile in profiles {
                    task_sink(profile);
                }
            }
        });
        inner.commands = Some(tx);
        inner.sink = Some(sink);
        Ok(())
    }

    /// Erases what this device kept, for an identity that no longer holds it
    /// (identity spec 4.4, architecture 12): this instance's store (server
    /// channels, personal conversations as decrypted, MLS groups, cursors),
    /// its tokens, and the identity's stores (roster, device key, backup key,
    /// a kept phrase), so nothing on it reads a private message again. The
    /// gateway loop does it when the instance says this device is revoked;
    /// a shell calls it for a revocation it learns of another way. The app
    /// is then signed out, with no identity; it can be set up or linked anew.
    /// Only a device that hears of its revocation erases: one kept offline
    /// keeps what it had, under the platform keystore's key.
    ///
    /// # Errors
    /// On a store failure, after erasing what it could.
    pub async fn erase_device(&self) -> Result<(), ClientError> {
        self.stop();
        // Held first, so no DM engine is built again from what is erased.
        let mut stores = self.identity.lock().await;
        // The DM engine writes to the instance's store through its own
        // connection: what it is doing is let finish, and nothing more is
        // done with it while the store is erased.
        let engine = self
            .dm
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take();
        let _engine = match &engine {
            Some(engine) => Some(engine.lock().await),
            None => None,
        };
        self.api.clear_tokens();
        self.api.take_revoked();
        let mut first_error = None;
        let mut inner = self.lock();
        inner.reauth = None;
        inner.held_phrase = None;
        inner.quiz = None;
        inner.linking = None;
        inner.answers.clear();
        // What says this device is to erase, kept until everything went: an
        // erase that did not finish is tried again (`erase_if_revoked`, and
        // at the next launch).
        let _ = inner.keys.put(REVOKED_HEARD, b"1");
        // Signed in again, the device is a new one: the instance refuses a
        // session to the revoked id (`DEVICE_REVOKED`), and the roster keeps
        // it revoked.
        let id = wire::new_id();
        for result in [
            // The store and the reader are the one file: erased once.
            inner
                .store
                .as_ref()
                .unwrap_or(&inner.reader)
                .erase()
                .map_err(ClientError::from),
            stores.store.erase().map_err(ClientError::from),
            stores.keys.erase().map_err(ClientError::from),
            stores.keys.put("device_id", &id).map_err(ClientError::from),
        ] {
            if let Err(e) = result {
                first_error.get_or_insert(e);
            }
        }
        if first_error.is_none() {
            if let Err(e) = inner.keys.erase() {
                first_error = Some(e.into());
            }
        }
        if first_error.is_some() {
            // Still the revoked device's: nothing it held is presented.
            let _ = inner.keys.delete("access_token");
            let _ = inner.keys.delete("refresh_token");
            self.api.hear_revoked();
        }
        drop(inner);
        *self
            .device_id
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = id;
        first_error.map_or(Ok(()), Err)
    }

    /// Erases this device before a sign-in when one of its calls heard that
    /// the identity revoked it and no loop is running to erase it: the app
    /// went to sign-in on that answer before the loop heard it, or never
    /// started one. The sign-in that follows is a new device's.
    /// A loop still giving the store back erases it when it ends instead.
    async fn erase_if_revoked(&self) {
        let idle = {
            let inner = self.lock();
            inner.commands.is_none() && inner.store.is_some()
        };
        if idle && self.api.take_revoked() {
            let _ = self.erase_device().await;
        }
    }

    /// A sign-in's answer, this device erased first when the instance
    /// refused it because the identity revoked this device's id: the person
    /// tries again as a new device.
    async fn erased_if_revoked<T>(&self, answer: Result<T, ClientError>) -> Result<T, ClientError> {
        if answer.is_err() {
            self.erase_if_revoked().await;
        }
        answer
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

    /// Keeps `items` as this account's rail (`PUT /account/rail`,
    /// architecture 6.9): the whole of it, the latest write winning; its
    /// other devices hear of it through the account log. Make `items` with
    /// the edits in [`crate::rail`]. Returns the account's snapshot with its
    /// rail as the instance kept it: the last snapshot read, the instance's
    /// answer standing in for a read of its own, since the `RailArranged`
    /// this write raises has every client read the snapshot again anyway.
    ///
    /// # Errors
    /// `INVALID_ARGUMENT` with `details.field` (`name`, `tone`, `folder_id`,
    /// `folders`); transport failure.
    pub async fn arrange_rail(
        &self,
        items: &[crate::rail::RailItem],
    ) -> Result<SnapshotSummary, ClientError> {
        let turn = {
            let rails = &mut self.lock().rails;
            rails.sent += 1;
            rails.latest = Some(items.to_vec());
            rails.sent
        };
        let kept = {
            let _turn = self.rail_turn.lock().await;
            let kept = self.api.put_rail(crate::rail::to_proto(items)).await;
            let rails = &mut self.lock().rails;
            rails.answered = rails.answered.max(turn);
            // Refused, the last one sent is not what the instance keeps.
            if kept.is_err() && turn == rails.sent {
                rails.latest = None;
            }
            kept
        };
        let kept = kept?;
        let Some(mut summary) = self.kept::<SnapshotSummary>(KEPT_SNAPSHOT).ok().flatten() else {
            return self.snapshot().await;
        };
        summary.rail = crate::rail::from_proto(&kept);
        {
            let inner = self.lock();
            let rails = &inner.rails;
            // A later one on its way is newer than this answer ([`RailSends`]).
            if let Some(latest) = &rails.latest {
                if rails.sent > rails.answered {
                    summary.rail = latest.clone();
                }
            }
            summary.read_states = inner.reads.summaries();
        }
        let summary = summary.arranged();
        self.keep(KEPT_SNAPSHOT, &summary);
        Ok(summary)
    }

    /// Whether a server's deleted messages keep their place as "(deleted)"
    /// (architecture 6.7; off by default). Every member hears of it through
    /// the account log. Returns the account's snapshot as it then is.
    ///
    /// # Errors
    /// `PERMISSION_DENIED` without `MANAGE_SERVER` there; transport failure.
    pub async fn set_show_deleted(
        &self,
        server_id: &str,
        show: bool,
    ) -> Result<SnapshotSummary, ClientError> {
        let server_id = parse_uuid(server_id)?;
        self.api
            .set_show_deleted(&wire::display(&server_id), show)
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

    /// The largest message body the instance takes, in bytes of UTF-8
    /// (protocol 14): what its `Welcome` said, or the protocol's own
    /// until one has. A composer counts against it with
    /// [`lumia_core::body::room`].
    pub fn max_message_bytes(&self) -> u32 {
        self.max_message_bytes.load(Ordering::Relaxed)
    }

    /// Refuses a body over the limit before anything is sent or written.
    fn fits(&self, text: &str) -> Result<(), ClientError> {
        let max = self.max_message_bytes();
        if body::fits(text, max) {
            return Ok(());
        }
        Err(ClientError::TooLong {
            over: body::room(text, max).left.unsigned_abs(),
            max,
        })
    }

    /// Sends a message; returns the client-minted message id.
    ///
    /// # Errors
    /// When not started or the id is malformed.
    pub fn send_message(&self, channel_id: &str, body: &str) -> Result<String, ClientError> {
        self.send_message_with(channel_id, body, &[], None)
    }

    /// Sends a message with its mentions (byte ranges into `body`) and what
    /// it quotes, if anything (architecture 6.6); returns the client-minted
    /// message id. The instance refuses a mention of everyone without
    /// `MENTION_EVERYONE`, and a quote of a message of another channel.
    ///
    /// # Errors
    /// When not started or an id is malformed.
    pub fn send_message_with(
        &self,
        channel_id: &str,
        body: &str,
        mentions: &[MentionSummary],
        reply_to: Option<&QuoteSummary>,
    ) -> Result<String, ClientError> {
        self.send_message_with_media(channel_id, body, mentions, reply_to, &[], &[])
    }

    /// [`App::send_message_with`], carrying the attachments `attachments`
    /// names (draft ids from [`App::attach`], uploaded into this channel)
    /// and asking for the cards of `card_urls`: the links of
    /// [`App::card_links`] whose cards the person kept. The instance builds
    /// those it has not yet and adds them to the message when they arrive.
    /// The drafts go with the send; until it is queued they stay.
    ///
    /// # Errors
    /// When not started, an id is malformed, the body is too long, or an
    /// attachment is not one waiting for this channel; more than 10.
    pub fn send_message_with_media(
        &self,
        channel_id: &str,
        body: &str,
        mentions: &[MentionSummary],
        reply_to: Option<&QuoteSummary>,
        attachments: &[String],
        card_urls: &[String],
    ) -> Result<String, ClientError> {
        self.fits(body)?;
        let channel = parse_uuid(channel_id)?;
        let attachments = distinct(attachments);
        let carried = self.drafts(&channel, &attachments)?;
        let card_urls = cards_kept(body, card_urls);
        let id = wire::new_id();
        let mentions = mentions
            .iter()
            .map(|m| {
                Ok(lumia_core::proto::Mention {
                    target: Some(if m.everyone {
                        mention::Target::Everyone(true)
                    } else {
                        mention::Target::AccountId(parse_uuid(&m.account_id)?)
                    }),
                    start: m.start,
                    end: m.end,
                })
            })
            .collect::<Result<Vec<_>, ClientError>>()?;
        let reply_to = reply_to
            .map(|q| {
                Ok::<_, ClientError>(lumia_core::proto::QuoteReply {
                    message_id: parse_uuid(&q.message_id)?,
                    text: q.text.clone(),
                    silent: q.silent,
                    ..Default::default()
                })
            })
            .transpose()?;
        self.send_command(GatewayCommand::Send(Command {
            command_id: id.clone(),
            channel_id: channel,
            op: Some(command::Op::SendMessage(SendMessage {
                body: body.to_string(),
                mentions,
                reply_to,
                attachments: carried,
                card_urls,
                ..Default::default()
            })),
        }))?;
        self.sent_drafts(&attachments);
        Ok(wire::display(&id))
    }

    /// Edits one of this account's messages in a server's channel: its new
    /// body and mentions (architecture 6.6). The instance refuses another's
    /// message, a notice, and `@everyone` without `MENTION_EVERYONE`.
    /// Returns the command's id, which a refusal's `AppEvent::Error` names.
    ///
    /// # Errors
    /// When not started or an id is malformed.
    pub fn edit_message(
        &self,
        channel_id: &str,
        message_id: &str,
        body: &str,
        mentions: &[MentionSummary],
    ) -> Result<String, ClientError> {
        self.edit_message_with_media(channel_id, message_id, body, mentions, &[], None)
    }

    /// [`App::edit_message`], dropping the attachments `remove_attachment_ids`
    /// names (an edit cannot add one) and, when `card_urls` is given, keeping
    /// the cards of those links only (a new one is fetched by the instance).
    /// Without it, the cards of the links still in the body stay.
    ///
    /// # Errors
    /// When not started or an id is malformed.
    pub fn edit_message_with_media(
        &self,
        channel_id: &str,
        message_id: &str,
        body: &str,
        mentions: &[MentionSummary],
        remove_attachment_ids: &[String],
        card_urls: Option<&[String]>,
    ) -> Result<String, ClientError> {
        self.fits(body)?;
        let removed = remove_attachment_ids
            .iter()
            .map(|id| parse_uuid(id))
            .collect::<Result<Vec<_>, _>>()?;
        let cards = card_urls.map(|urls| lumia_core::proto::CardUrls {
            urls: cards_kept(body, urls),
        });
        let channel = parse_uuid(channel_id)?;
        let mentions = mentions
            .iter()
            .map(|m| {
                Ok(lumia_core::proto::Mention {
                    target: Some(if m.everyone {
                        mention::Target::Everyone(true)
                    } else {
                        mention::Target::AccountId(parse_uuid(&m.account_id)?)
                    }),
                    start: m.start,
                    end: m.end,
                })
            })
            .collect::<Result<Vec<_>, ClientError>>()?;
        let id = wire::new_id();
        self.send_command(GatewayCommand::Send(Command {
            command_id: id.clone(),
            channel_id: channel,
            op: Some(command::Op::EditMessage(lumia_core::proto::EditMessage {
                message_id: parse_uuid(message_id)?,
                body: body.to_string(),
                mentions,
                remove_attachment_ids: removed,
                cards,
            })),
        }))?;
        Ok(wire::display(&id))
    }

    /// Deletes a message of a server's channel: this account's own, or any
    /// for one holding `MANAGE_MESSAGES` there (architecture 6.6). Returns
    /// the command's id, which a refusal's `AppEvent::Error` names.
    ///
    /// # Errors
    /// When not started or an id is malformed.
    pub fn delete_message(
        &self,
        channel_id: &str,
        message_id: &str,
    ) -> Result<String, ClientError> {
        let channel = parse_uuid(channel_id)?;
        let id = wire::new_id();
        self.send_command(GatewayCommand::Send(Command {
            command_id: id.clone(),
            channel_id: channel,
            op: Some(command::Op::DeleteMessage(
                lumia_core::proto::DeleteMessage {
                    message_id: parse_uuid(message_id)?,
                },
            )),
        }))?;
        Ok(wire::display(&id))
    }

    /// Adds this account's reaction to a message of a server channel, or
    /// takes it away (`remove`). The instance refuses anything but one
    /// emoji.
    ///
    /// # Errors
    /// When not started or an id is malformed.
    pub fn react(
        &self,
        channel_id: &str,
        message_id: &str,
        emoji: &str,
        remove: bool,
    ) -> Result<(), ClientError> {
        let channel = parse_uuid(channel_id)?;
        let message_id = parse_uuid(message_id)?;
        let reaction = emoji.to_string();
        let op = if remove {
            command::Op::RemoveReaction(lumia_core::proto::RemoveReaction {
                message_id,
                reaction,
            })
        } else {
            command::Op::AddReaction(lumia_core::proto::AddReaction {
                message_id,
                reaction,
            })
        };
        self.send_command(GatewayCommand::Send(Command {
            command_id: wire::new_id(),
            channel_id: channel,
            op: Some(op),
        }))
    }

    /// Every conversation's read state as this client counts it: the
    /// snapshot's, with the messages since added and the reads of this
    /// account's other devices taken away.
    #[must_use]
    pub fn read_states(&self) -> Vec<ReadStateSummary> {
        self.lock().reads.summaries()
    }

    /// The badge: what notifies this account and is unread, everywhere.
    #[must_use]
    pub fn badge(&self) -> u32 {
        self.lock()
            .reads
            .by_channel
            .values()
            .map(|c| c.mentions)
            .sum()
    }

    /// `POST /channels/{id}/read`: this account has read a conversation
    /// through `seq` (call it when the newest message is on screen and the
    /// window has focus). The position only moves forward; the answer is
    /// the state as the instance then counts it, which this client takes.
    ///
    /// # Errors
    /// On transport or API failure, or a malformed id.
    pub async fn mark_read(
        &self,
        channel_id: &str,
        seq: u64,
    ) -> Result<ReadStateSummary, ClientError> {
        let id = parse_uuid(channel_id)?;
        let personal = self
            .lock()
            .reads
            .by_channel
            .get(&id)
            .is_some_and(|c| c.kind.starts_with("PERSONAL"));
        let seq = match self.built_engine() {
            Some(engine) if personal => {
                let rows = engine.lock().await.rows(&id).unwrap_or_default();
                dm_read_through(&rows, seq)
            }
            _ => seq,
        };
        if let Some(count) = self.lock().reads.by_channel.get(&id) {
            if seq <= count.read_seq && count.unread == 0 {
                return Ok(ReadStateSummary {
                    channel_id: channel_id.to_string(),
                    read_seq: count.read_seq,
                    unread: 0,
                    mentions: 0,
                });
            }
        }
        let state = self.api.mark_read(channel_id, seq).await?;
        let mut inner = self.lock();
        let count = inner.reads.by_channel.entry(id).or_default();
        count.read_seq = count.read_seq.max(state.read_seq);
        count.counted_through = count.counted_through.max(state.read_seq);
        count.unread = state.unread;
        count.mentions = state.mentions;
        Ok(ReadStateSummary {
            channel_id: channel_id.to_string(),
            read_seq: count.read_seq,
            unread: count.unread,
            mentions: count.mentions,
        })
    }

    /// `GET /account/mentions`: the messages that notify this account,
    /// newest first, in one server or (`None`) all; `before` is the last
    /// message id of the page before. Returns the page and whether there
    /// is more.
    ///
    /// # Errors
    /// On transport or API failure.
    pub async fn mentions(
        &self,
        server_id: Option<&str>,
        before: Option<&str>,
    ) -> Result<(Vec<MessageSummary>, bool), ClientError> {
        let page = self.api.mentions(server_id, before).await?;
        let me = self.lock().reads.me.clone();
        Ok((
            page.messages
                .iter()
                .map(|m| message_summary(m, &me))
                .collect(),
            page.has_more,
        ))
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
                            body: notification_text(&send.body, &send.attachments),
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
                            body: notification_text(&message.body, &message.attachments),
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

    /// [`App::has_identity`] for a flow that refuses or branches on it:
    /// waits for an identity call in flight rather than answering yes.
    async fn holds_identity(&self) -> bool {
        let stores = self.identity.lock().await;
        stores.store.meta_get("roster").ok().flatten().is_some()
    }

    /// Creates a new identity on this device; the recovery phrase is shown
    /// once and never stored.
    ///
    /// # Errors
    /// When this device already holds an identity, or on a store failure.
    pub async fn identity_create(&self, label: &str) -> Result<(String, String), ClientError> {
        let mut stores = self.identity.lock().await;
        let created = device(&mut stores, &self.this_device()).create(label)?;
        Ok((created.identity_id.to_string(), created.phrase))
    }

    /// The identity's signed profile, if one was set on this device.
    ///
    /// # Errors
    /// On a store failure.
    pub async fn identity_profile(&self) -> Result<Option<ProfileSummary>, ClientError> {
        let local = {
            let mut stores = self.identity.lock().await;
            device(&mut stores, &self.this_device()).profile()?
        };
        // The account as the instance holds it: its signed profile, set by
        // any device of the identity, and its name and picture now (a
        // `PATCH /account` after the profile stands over it). Not asked
        // when signed out; when it cannot be asked, what this device has.
        let held = if self.signed_in() {
            match self.api.account_snapshot().await {
                Ok(snap) => snap.account,
                Err(e) if local.is_none() => return Err(e),
                Err(_) => None,
            }
        } else {
            None
        };
        shown_profile(local.as_ref(), held.as_ref())
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
            device(&mut stores, &self.this_device()).set_profile(
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
        device(&mut stores, &self.this_device())
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
        self.erase_if_revoked().await;
        let (kind, origin) = match &self.place {
            Place::Web { origin } => (DeviceKind::Web, origin.clone()),
            Place::Native => (DeviceKind::Native, String::new()),
        };
        let mut stores = self.identity.lock().await;
        device(&mut stores, &self.this_device()).link_request_as(label, kind, &origin)
    }

    // ---- Linking by prompt (identity spec 4.2.1) ----

    /// As the new device: opens a link session, so that every device of the
    /// account that holds the identity prompts. [`App::link_progress`]
    /// follows it. A session this device opened before is declined; one
    /// given up ([`App::link_cancel`]) or started again while this one
    /// opened is declined as it opens, and this answers [`LINK_GIVEN_UP`].
    ///
    /// # Errors
    /// When this device already holds an identity; given up meanwhile; API
    /// failure.
    pub async fn link_start(&self, label: &str) -> Result<LinkProgress, ClientError> {
        use base64::Engine as _;
        use lumia_core::identity::link::{commitment, session_secret};
        self.erase_if_revoked().await;
        let (run, earlier) = {
            let mut inner = self.lock();
            inner.link_runs += 1;
            (inner.link_runs, inner.linking.take())
        };
        if let Some(earlier) = earlier {
            let _ = self.api.decline_link_session(&earlier.session_id).await;
        }
        if self.holds_identity().await {
            return Err(ClientError::State(
                "this device already has private messages".into(),
            ));
        }
        let request = base64::engine::general_purpose::STANDARD
            .decode(self.identity_link_request(label).await?)
            .map_err(|e| ClientError::State(e.to_string()))?;
        let reveal = session_secret()?;
        let opened = self
            .api
            .open_link_session(request.clone(), commitment(&reveal))
            .await?;
        {
            let mut inner = self.lock();
            if inner.link_runs == run {
                inner.linking = Some(NewLink {
                    session_id: opened.session_id,
                    request,
                    reveal,
                    answer: None,
                });
                return Ok(LinkProgress {
                    stage: "waiting".into(),
                    digits: String::new(),
                });
            }
        }
        let _ = self.api.decline_link_session(&opened.session_id).await;
        Err(ClientError::State(LINK_GIVEN_UP.into()))
    }

    /// As the new device: where its link session stands, taking the next
    /// step when it is this device's: revealing its secret once another
    /// device answered, claiming the code once it came. Call it again on
    /// every `LinkSessionUpdated`, or every second or two while the screen
    /// is open. The digits are of the answer this device revealed to; a
    /// session whose answer changed after is declined.
    ///
    /// # Errors
    /// Without a session in progress; an answer changed after the reveal;
    /// API failure.
    pub async fn link_progress(&self) -> Result<LinkProgress, ClientError> {
        use lumia_core::identity::link::{digits, digits_words};
        let link = self
            .lock()
            .linking
            .clone()
            .ok_or_else(|| ClientError::State("no link in progress".into()))?;
        let progress = |stage: &str, digits: String| LinkProgress {
            stage: stage.into(),
            digits,
        };
        let session = match self.api.link_session(&link.session_id).await {
            Ok(session) => session,
            Err(ClientError::Api { code, .. }) if code == "NOT_FOUND" => {
                self.end_link(&link.session_id);
                return Ok(progress("expired", String::new()));
            }
            Err(e) => return Err(e),
        };
        if session.declined {
            self.end_link(&link.session_id);
            return Ok(progress("declined", String::new()));
        }
        if session.answer.is_empty() {
            return Ok(progress("waiting", String::new()));
        }
        let answer = match link.answer {
            Some(answer) if answer != session.answer => {
                self.end_link(&link.session_id);
                let _ = self.api.decline_link_session(&link.session_id).await;
                return Err(ClientError::State(
                    "the answer to this link changed after this device answered it: start again"
                        .into(),
                ));
            }
            Some(answer) => answer,
            // Held before the reveal goes, so a reveal whose answer was lost
            // is not made against another.
            None => {
                if let Some(held) = self.lock().linking.as_mut() {
                    if held.session_id == link.session_id {
                        held.answer = Some(session.answer.clone());
                    }
                }
                session.answer.clone()
            }
        };
        if !session.code.is_empty() {
            // A claim that came through before a sign-in that did not leaves
            // the identity here and the code spent: only the sign-in is left.
            if self.holds_identity().await {
                self.sign_in_identity().await?;
                self.announce_device().await;
            } else {
                self.identity_claim(&session.code).await?;
            }
            self.end_link(&link.session_id);
            return Ok(progress("linked", String::new()));
        }
        // Revealed already when the session holds it, though the answer to
        // the reveal was lost.
        if session.reveal.is_empty() {
            self.api
                .reveal_link_session(&link.session_id, link.reveal.clone())
                .await?;
        }
        let shown = digits(&link.request, &answer, &link.reveal);
        Ok(progress("compare", digits_words(shown)))
    }

    /// Forgets the link session `session_id` when it is still the one held.
    fn end_link(&self, session_id: &[u8]) {
        let mut inner = self.lock();
        if inner
            .linking
            .as_ref()
            .is_some_and(|l| l.session_id == session_id)
        {
            inner.linking = None;
        }
    }

    /// As the new device: gives up the link session in progress, or one
    /// still opening.
    pub async fn link_cancel(&self) {
        let link = {
            let mut inner = self.lock();
            inner.link_runs += 1;
            inner.linking.take()
        };
        if let Some(link) = link {
            let _ = self.api.decline_link_session(&link.session_id).await;
        }
    }

    /// On a device that holds the identity: the account's link sessions
    /// that prompt here, each with where it stands. None on a browser,
    /// which never approves (identity spec 7). Ask again on every
    /// `LinkSessionUpdated`. A session this device answered is `refused`
    /// when its request or commitment is not the one it answered, or its
    /// reveal does not match that commitment.
    ///
    /// # Errors
    /// API failure.
    pub async fn link_prompts(&self) -> Result<Vec<LinkPrompt>, ClientError> {
        use lumia_core::identity::link::{digits, digits_words, reveals};
        if matches!(self.place, Place::Web { .. }) || !self.holds_identity().await {
            return Ok(Vec::new());
        }
        let sessions = self.api.link_sessions().await?;
        let mut out = Vec::new();
        let mut inner = self.lock();
        inner
            .answers
            .retain(|id, _| sessions.iter().any(|s| s.session_id == *id));
        for session in sessions {
            let Ok(request) = summary_of(&session.request) else {
                continue;
            };
            let mine = inner.answers.get(&session.session_id);
            let (stage, shown) = match mine {
                _ if session.answer.is_empty() => ("new", String::new()),
                // Another device answered it: it is that device's to finish.
                Some(mine) if mine.answer != session.answer => continue,
                None => continue,
                Some(mine)
                    if mine.request != session.request || mine.commitment != session.commitment =>
                {
                    ("refused", String::new())
                }
                Some(_) if session.reveal.is_empty() => ("waiting", String::new()),
                Some(mine) if reveals(&mine.commitment, &session.reveal) => (
                    "compare",
                    digits_words(digits(&mine.request, &mine.answer, &session.reveal)),
                ),
                Some(_) => ("refused", String::new()),
            };
            out.push(LinkPrompt {
                session_id: wire::display(&session.session_id),
                request,
                stage: stage.into(),
                digits: shown,
            });
        }
        Ok(out)
    }

    /// On a device that holds the identity: answers a prompt, so the new
    /// device reveals and both show their digits. The session's request and
    /// commitment are held here first, and the secret before it is sent: an
    /// answer the instance took though its reply was lost is still this
    /// device's.
    ///
    /// # Errors
    /// `FAILED_PRECONDITION` (`answered`) when another device answered first.
    pub async fn link_prompt_answer(&self, session_id: &str) -> Result<(), ClientError> {
        let id = parse_uuid(session_id)?;
        let session = self.api.link_session(&id).await?;
        let answer = lumia_core::identity::link::session_secret()?;
        self.lock().answers.insert(
            id.clone(),
            Answered {
                answer: answer.clone(),
                request: session.request,
                commitment: session.commitment,
                code: None,
            },
        );
        let sent = self.api.answer_link_session(&id, answer).await;
        // Refused: no answer of this device's is on it.
        if matches!(sent, Err(ClientError::Api { .. })) {
            self.lock().answers.remove(&id);
        }
        sent.map(|_| ())
    }

    /// On a device that holds the identity: the person saw the same digits
    /// on both screens. Approves the request it answered (identity spec 4.2
    /// steps 3 to 6) and gives the new device its code through the session;
    /// a code that did not reach the session is posted again the next time,
    /// not approved twice. A shell asks for the device's own unlock first.
    ///
    /// # Errors
    /// When the digits cannot have matched (no answer here, a request or
    /// commitment other than the one answered, no reveal, or a reveal that
    /// does not match the commitment); a declined session; API failure.
    pub async fn link_prompt_confirm(&self, session_id: &str) -> Result<(), ClientError> {
        use base64::Engine as _;
        use lumia_core::identity::link::reveals;
        let id = parse_uuid(session_id)?;
        let session = self.api.link_session(&id).await?;
        if session.declined {
            self.lock().answers.remove(&id);
            return Err(ClientError::State(
                "this request was declined on another device".into(),
            ));
        }
        let mine = self.lock().answers.get(&id).cloned();
        let Some(mine) = mine.filter(|mine| {
            mine.answer == session.answer
                && mine.request == session.request
                && mine.commitment == session.commitment
                && reveals(&mine.commitment, &session.reveal)
        }) else {
            return Err(ClientError::State(
                "this request cannot be approved: start again on the new device".into(),
            ));
        };
        // Its code reached the session, though the reply was lost.
        if !session.code.is_empty() {
            self.lock().answers.remove(&id);
            return Ok(());
        }
        let code = match mine.code {
            Some(code) => code,
            None => {
                let request = base64::engine::general_purpose::STANDARD.encode(&mine.request);
                let code = self.identity_approve(&request).await?;
                if let Some(held) = self.lock().answers.get_mut(&id) {
                    held.code = Some(code.clone());
                }
                code
            }
        };
        self.api.link_session_code(&id, &code).await?;
        self.lock().answers.remove(&id);
        Ok(())
    }

    /// On any device of the account: "Not me". The session takes nothing more.
    ///
    /// # Errors
    /// API failure.
    pub async fn link_prompt_decline(&self, session_id: &str) -> Result<(), ClientError> {
        let id = parse_uuid(session_id)?;
        self.api.decline_link_session(&id).await?;
        self.lock().answers.remove(&id);
        Ok(())
    }

    /// What a link request, as the new device shows it, asks to add: read
    /// before approving, so the person sees which device they are letting
    /// in (identity spec 4.2 step 2). Nothing is sent.
    ///
    /// # Errors
    /// When the text is not a link request.
    pub fn identity_link_request_summary(request: &str) -> Result<LinkRequestSummary, ClientError> {
        use base64::Engine as _;
        // However it wrapped when it was copied across.
        let request: String = request.split_whitespace().collect();
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(request)
            .map_err(|_| ClientError::State("that is not a link request".into()))?;
        summary_of(&bytes)
    }

    /// On a device that holds the identity: approves a link request and
    /// returns the code the new device claims with.
    ///
    /// # Errors
    /// On a malformed request or API failure.
    pub async fn identity_approve(&self, request: &str) -> Result<String, ClientError> {
        let api = self.api.clone();
        let mut stores = self.identity.lock().await;
        device(&mut stores, &self.this_device())
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
        let mut dev = device(&mut stores, &self.this_device());
        let roster = dev.claim(&api, code).await?;
        let pair = dev.sign_in(&api).await?;
        drop(stores);
        self.take_session(&pair).await?;
        self.announce_device().await;
        Ok(roster_summary(&roster, &self.this_device()))
    }

    /// Signs in by identity proof with the roster this device holds.
    ///
    /// # Errors
    /// On API failure, or when the device is revoked in the held roster.
    pub async fn sign_in_identity(&self) -> Result<(), ClientError> {
        let api = self.api.clone();
        let mut stores = self.identity.lock().await;
        let pair = device(&mut stores, &self.this_device())
            .sign_in(&api)
            .await?;
        drop(stores);
        self.take_session(&pair).await
    }

    /// The roster this device holds.
    ///
    /// # Errors
    /// When there is none.
    pub async fn identity_show(&self) -> Result<RosterSummary, ClientError> {
        let mut stores = self.identity.lock().await;
        let roster = device(&mut stores, &self.this_device()).roster()?;
        Ok(roster_summary(&roster, &self.this_device()))
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
        let mut dev = device(&mut stores, &self.this_device());
        dev.revoke_with_device(&api, password, &target).await?;
        let roster = dev.roster()?;
        Ok(roster_summary(&roster, &self.this_device()))
    }

    // ---- Setting up (identity spec 4.1, 4.3, 4.7; architecture 5.4) ----

    /// The identity the signed-in account is bound to on this instance,
    /// hex, or none: what tells "set up here" from "link this device"
    /// (`Account.identity_id`).
    ///
    /// # Errors
    /// When not signed in, or on transport failure.
    pub async fn account_identity(&self) -> Result<Option<String>, ClientError> {
        let snap = self.api.account_snapshot().await?;
        Ok(snap
            .account
            .map(|a| a.identity_id)
            .filter(|id| !id.is_empty())
            .map(hex::encode))
    }

    /// Whether a reauth token is in hand for identity operations.
    #[must_use]
    pub fn has_reauth(&self) -> bool {
        self.lock().reauth.is_some()
    }

    /// Reauth by the account's password.
    ///
    /// # Errors
    /// `UNAUTHENTICATED` for a wrong one; transport failure.
    pub async fn reauth_password(&self, password: &str) -> Result<(), ClientError> {
        let resp = self.api.reauth_password(password).await?;
        self.lock().reauth = Some(resp.reauth_token);
        Ok(())
    }

    /// Reauth by a TOTP code, or by the code [`App::reauth_mail_code`]
    /// mailed (`mailed`).
    ///
    /// # Errors
    /// `UNAUTHENTICATED` for a wrong one; transport failure.
    pub async fn reauth_code(&self, code: &str, mailed: bool) -> Result<(), ClientError> {
        use lumia_core::proto::reauth_request::Credential;
        let credential = if mailed {
            Credential::MagicCode(code.trim().into())
        } else {
            Credential::TotpCode(code.trim().into())
        };
        let resp = self
            .api
            .reauth(lumia_core::proto::ReauthRequest {
                credential: Some(credential),
                ..Default::default()
            })
            .await?;
        self.lock().reauth = Some(resp.reauth_token);
        Ok(())
    }

    /// Mails six digits to the account's address for [`App::reauth_code`].
    ///
    /// # Errors
    /// `FAILED_PRECONDITION` (`no_email`), `POLICY_DENIED`, `RATE_LIMITED`.
    pub async fn reauth_mail_code(&self) -> Result<(), ClientError> {
        self.api.reauth_mail_code().await
    }

    /// The URL of a fresh login at the account's OIDC provider, which lands
    /// at `return_to` with `?reauth={token}` for [`App::reauth_take`].
    ///
    /// # Errors
    /// `FAILED_PRECONDITION` (`no_oidc`); transport failure.
    pub async fn reauth_oidc_url(&self, return_to: &str) -> Result<String, ClientError> {
        self.api.reauth_oidc_start(return_to).await
    }

    /// Holds a reauth token an OIDC login landed with.
    pub fn reauth_take(&self, token: &str) {
        self.lock().reauth = Some(token.trim().to_string()).filter(|t| !t.is_empty());
    }

    fn take_reauth(&self) -> Result<String, ClientError> {
        self.lock().reauth.take().ok_or(ClientError::ReauthNeeded)
    }

    /// The words of the phrase's wordlist that begin with `prefix`, at most
    /// four: what a phrase field offers as the person types.
    #[must_use]
    pub fn phrase_completions(prefix: &str) -> Vec<String> {
        lumia_core::identity::phrase::completions(prefix, 4)
            .into_iter()
            .map(str::to_string)
            .collect()
    }

    /// A new phrase held in memory, for a flow that shows and confirms it
    /// before the identity is made (the web's sign-up): its words. Making
    /// another forgets the last.
    ///
    /// # Errors
    /// When randomness is unavailable.
    pub fn identity_new_phrase(&self) -> Result<String, ClientError> {
        let phrase = lumia_core::identity::phrase::Phrase::generate()?;
        let mut inner = self.lock();
        inner.held_phrase = Some(HeldPhrase {
            entropy: phrase.entropy(),
            confirmed: false,
        });
        inner.quiz = None;
        Ok(phrase.words())
    }

    /// The phrase this device keeps until it is confirmed (a native device
    /// that set up silently), or none once it is. A shell shows it only
    /// after the device's own unlock.
    ///
    /// # Errors
    /// On a keystore failure.
    pub async fn identity_phrase(&self) -> Result<Option<String>, ClientError> {
        let mut stores = self.identity.lock().await;
        Ok(device(&mut stores, &self.this_device())
            .kept_phrase()?
            .map(|p| p.words()))
    }

    /// Whether the phrase still waits to be confirmed: what the reminders
    /// on Home and Devices follow.
    ///
    /// # Errors
    /// On a keystore failure.
    pub async fn identity_phrase_unconfirmed(&self) -> Result<bool, ClientError> {
        Ok(self.identity_phrase().await?.is_some())
    }

    /// Three positions (1 to 24) to ask for, of the held phrase, else the
    /// kept one. The same until answered.
    ///
    /// # Errors
    /// When there is no phrase to ask about.
    pub async fn identity_phrase_quiz(&self) -> Result<Vec<u32>, ClientError> {
        if self.quiz_phrase().await?.is_none() {
            return Err(ClientError::State("there is no phrase to confirm".into()));
        }
        let mut inner = self.lock();
        let quiz = match &inner.quiz {
            Some(quiz) => quiz.clone(),
            None => {
                let quiz = lumia_core::identity::phrase::Phrase::quiz()?;
                inner.quiz = Some(quiz.clone());
                quiz
            }
        };
        Ok(quiz
            .into_iter()
            .map(|p| u32::try_from(p).unwrap_or(0))
            .collect())
    }

    /// Checks the words given for [`App::identity_phrase_quiz`]'s positions.
    /// Right: a kept phrase is deleted and the reminders end, or a held one
    /// is marked confirmed. Wrong: the next quiz asks other positions, and
    /// the shell shows the phrase again (identity spec 4.1 step 7).
    ///
    /// # Errors
    /// When there is no phrase or no quiz to answer; on a keystore failure.
    pub async fn identity_phrase_confirm(&self, words: Vec<String>) -> Result<bool, ClientError> {
        let phrase = self
            .quiz_phrase()
            .await?
            .ok_or_else(|| ClientError::State("there is no phrase to confirm".into()))?;
        let quiz = self
            .lock()
            .quiz
            .take()
            .ok_or_else(|| ClientError::State("ask for the quiz first".into()))?;
        if !phrase.answers(&quiz, &words) {
            return Ok(false);
        }
        let held = {
            let mut inner = self.lock();
            match inner.held_phrase.as_mut() {
                Some(held) => {
                    held.confirmed = true;
                    true
                }
                None => false,
            }
        };
        if !held {
            let mut stores = self.identity.lock().await;
            device(&mut stores, &self.this_device()).forget_phrase()?;
        }
        Ok(true)
    }

    async fn quiz_phrase(
        &self,
    ) -> Result<Option<lumia_core::identity::phrase::Phrase>, ClientError> {
        let held = self.lock().held_phrase.as_ref().map(|h| h.entropy.clone());
        if let Some(entropy) = held {
            return Ok(Some(lumia_core::identity::phrase::Phrase::from_entropy(
                &entropy,
            )?));
        }
        let mut stores = self.identity.lock().await;
        device(&mut stores, &self.this_device()).kept_phrase()
    }

    /// Sets up private messages on this device for the signed-in account
    /// (identity spec 4.1): an identity made here unless this device holds
    /// one, the account bound to it, and the identity backup written.
    ///
    /// The phrase: a held one ([`App::identity_new_phrase`]) is used once
    /// confirmed; without one, a native device makes one and keeps it until
    /// confirmed, and a web device refuses (it shows and confirms the phrase
    /// first, keeping none). Binding an account takes a reauth token in
    /// hand, the sign-in's or a reauth's, else fails with
    /// [`ClientError::ReauthNeeded`] before anything is made.
    ///
    /// # Errors
    /// [`ClientError::AccountHasIdentity`] when the account is bound to another
    /// identity (the shell offers link and restore); [`ClientError::ReauthNeeded`];
    /// a web device without a confirmed phrase; API or store failure.
    pub async fn identity_setup(&self, label: &str) -> Result<String, ClientError> {
        let bound = self
            .api
            .account_snapshot()
            .await?
            .account
            .map(|a| a.identity_id);
        let bound = bound.filter(|id| !id.is_empty());
        let mine = self.held_identity_id().await?;
        if let Some(bound) = &bound {
            if mine.as_ref() == Some(bound) {
                return Ok(hex::encode(bound));
            }
            return Err(ClientError::AccountHasIdentity);
        }
        // Asked before anything is made, so a missing token leaves this
        // device as it was.
        let reauth = self.take_reauth()?;
        let identity_id = match mine {
            Some(id) => hex::encode(id),
            None => self.make_identity_or_keep(label, &reauth).await?,
        };
        let api = self.api.clone();
        {
            let mut stores = self.identity.lock().await;
            device(&mut stores, &self.this_device())
                .link_account_with(&api, &reauth)
                .await?;
        }
        self.after_identity().await?;
        Ok(identity_id)
    }

    /// Starting over (identity spec 4.7): the signed-in account, bound to
    /// an identity this person can no longer reach, moved to this device's,
    /// made here as [`App::identity_setup`] makes one when there is none.
    /// Needs a reauth token in hand. A shell says first whether contacts
    /// will see a key change.
    ///
    /// # Errors
    /// [`ClientError::ReauthNeeded`]; `ALREADY_EXISTS` when this device's identity is
    /// another account's there; a web device without a confirmed phrase;
    /// API or store failure.
    pub async fn identity_start_over(&self, label: &str) -> Result<String, ClientError> {
        let reauth = self.take_reauth()?;
        let identity_id = match self.held_identity_id().await? {
            Some(id) => hex::encode(id),
            None => self.make_identity_or_keep(label, &reauth).await?,
        };
        let api = self.api.clone();
        {
            let mut stores = self.identity.lock().await;
            device(&mut stores, &self.this_device())
                .rebind_account(&api, &reauth)
                .await?;
        }
        self.after_identity().await?;
        Ok(identity_id)
    }

    /// Restores the identity from its phrase on this device, every other
    /// device of it revoked (identity spec 4.3), and signs in by it. A web
    /// device forgets the backup key after; a shell offers this on the web
    /// only on the flagship's origin (spec 7).
    ///
    /// # Errors
    /// A phrase that is not the identity's, or was rotated away; API failure.
    pub async fn identity_recover(
        &self,
        phrase: &str,
        label: &str,
    ) -> Result<RosterSummary, ClientError> {
        self.identity_recover_keeping(phrase, label, Vec::new())
            .await
    }

    /// The devices restoring from `phrase` would stop: the roster the
    /// instance holds for the identity, read before anything changes, so
    /// the person can keep a device they still have
    /// ([`App::identity_recover_keeping`]). Empty when the instance knows no
    /// roster for it.
    ///
    /// # Errors
    /// A phrase that is not one; API failure.
    pub async fn identity_recover_devices(
        &self,
        phrase: &str,
    ) -> Result<Vec<DeviceSummary>, ClientError> {
        self.erase_if_revoked().await;
        let api = self.api.clone();
        let mut stores = self.identity.lock().await;
        let held = device(&mut stores, &self.this_device())
            .recover_preview(&api, phrase)
            .await?;
        Ok(held.map_or_else(Vec::new, |held| {
            roster_summary(&held, &self.this_device())
                .devices
                .into_iter()
                .filter(|d| !d.revoked)
                .collect()
        }))
    }

    /// [`App::identity_recover`], keeping the devices in `keep` (their ids,
    /// as [`App::identity_recover_devices`] names them) instead of revoking
    /// them (identity spec 4.3 step 3).
    ///
    /// # Errors
    /// As [`App::identity_recover`].
    pub async fn identity_recover_keeping(
        &self,
        phrase: &str,
        label: &str,
        keep: Vec<String>,
    ) -> Result<RosterSummary, ClientError> {
        let keep = keep
            .iter()
            .map(|id| parse_uuid(id))
            .collect::<Result<Vec<_>, _>>()?;
        self.erase_if_revoked().await;
        let (kind, origin) = self.device_kind();
        let api = self.api.clone();
        let mut stores = self.identity.lock().await;
        let mut dev = device(&mut stores, &self.this_device());
        let pair = dev
            .recover_keeping(&api, phrase, label, kind, &origin, &keep)
            .await?;
        if kind == DeviceKind::Web {
            dev.forget_backup_key()?;
        }
        let roster = dev.roster()?;
        drop(stores);
        self.take_session(&pair).await?;
        self.announce_device().await;
        Ok(roster_summary(&roster, &self.this_device()))
    }

    fn device_kind(&self) -> (DeviceKind, String) {
        match &self.place {
            Place::Web { origin } => (DeviceKind::Web, origin.clone()),
            Place::Native => (DeviceKind::Native, String::new()),
        }
    }

    async fn held_identity_id(&self) -> Result<Option<Vec<u8>>, ClientError> {
        let mut stores = self.identity.lock().await;
        let dev = device(&mut stores, &self.this_device());
        if dev.store.meta_get("roster")?.is_none() {
            return Ok(None);
        }
        Ok(Some(dev.roster()?.identity_id))
    }

    /// [`App::make_identity`], putting `reauth` back when it fails, so the
    /// token is not lost to a refusal that had nothing to do with it (a
    /// browser's unconfirmed phrase).
    async fn make_identity_or_keep(
        &self,
        label: &str,
        reauth: &str,
    ) -> Result<String, ClientError> {
        let made = self.make_identity(label).await;
        if made.is_err() {
            self.lock().reauth = Some(reauth.to_string());
        }
        made
    }

    /// Makes this device's identity from the held, confirmed phrase, or on
    /// a native device from a new one it keeps until confirmed.
    async fn make_identity(&self, label: &str) -> Result<String, ClientError> {
        use lumia_core::identity::phrase::Phrase;
        let (kind, origin) = self.device_kind();
        let held = self.lock().held_phrase.take();
        let (phrase, keep) = match held {
            Some(HeldPhrase {
                entropy,
                confirmed: true,
            }) => (Phrase::from_entropy(&entropy)?, false),
            Some(unconfirmed) => {
                self.lock().held_phrase = Some(unconfirmed);
                return Err(ClientError::State("confirm the phrase first".into()));
            }
            None if kind == DeviceKind::Web => {
                return Err(ClientError::State(
                    "a browser shows and confirms the phrase before it sets up".into(),
                ))
            }
            None => (Phrase::generate()?, true),
        };
        let mut stores = self.identity.lock().await;
        device(&mut stores, &self.this_device()).create_from(&phrase, label, kind, &origin, keep)
    }

    /// After this device's identity is bound: the identity backup written
    /// (a web device then forgets the backup key, spec 7), and its key
    /// packages published when the gateway already runs.
    async fn after_identity(&self) -> Result<(), ClientError> {
        let account = self.api.account_snapshot().await?.account;
        let accounts = account
            .map(|a| {
                vec![lumia_core::proto::HostAccount {
                    host: self.host.clone(),
                    account_id: a.account_id,
                }]
            })
            .unwrap_or_default();
        let api = self.api.clone();
        {
            let mut stores = self.identity.lock().await;
            let mut dev = device(&mut stores, &self.this_device());
            let written = dev.write_backup(&api, accounts).await;
            if self.device_kind().0 == DeviceKind::Web {
                dev.forget_backup_key()?;
            }
            // The identity stands without its backup; the next native write
            // brings it up to date.
            if let Err(e) = written {
                tracing::warn!("identity backup not written: {e}");
            }
        }
        self.announce_device().await;
        Ok(())
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
            let roster = device(stores, &self.this_device()).roster()?;
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
            self.this_device(),
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
                member: e.is_ready(channel)?,
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
        self.dm_send_with(channel_id, body, None).await
    }

    /// Sends a message in a personal DM quoting another of the
    /// conversation, or nothing; otherwise as [`App::dm_send`].
    ///
    /// # Errors
    /// Without the group for the conversation, or before the loop started.
    pub async fn dm_send_with(
        &self,
        channel_id: &str,
        body: &str,
        quote: Option<&QuoteSummary>,
    ) -> Result<String, ClientError> {
        self.dm_send_with_media(channel_id, body, quote, &[], &[])
            .await
    }

    /// [`App::dm_send_with`], carrying the attachments `attachments` names
    /// (draft ids from [`App::attach`], uploaded into this conversation)
    /// and the cards of `card_urls` this device built with
    /// [`App::card_preview`] (a link without one goes without a card). The
    /// blobs they use are named to the instance so it keeps them; the keys
    /// travel only inside the encrypted message.
    ///
    /// # Errors
    /// Without the group for the conversation, before the loop started, or
    /// when an attachment is not one waiting for this conversation.
    pub async fn dm_send_with_media(
        &self,
        channel_id: &str,
        body: &str,
        quote: Option<&QuoteSummary>,
        attachments: &[String],
        card_urls: &[String],
    ) -> Result<String, ClientError> {
        self.fits(body)?;
        let channel = parse_uuid(channel_id)?;
        let quote = quote
            .map(|q| {
                Ok::<_, ClientError>(lumia_core::proto::DmQuote {
                    message_id: parse_uuid(&q.message_id)?,
                    text: q.text.clone(),
                })
            })
            .transpose()?;
        let attachments = distinct(attachments);
        let carried = self.drafts(&channel, &attachments)?;
        let cards = self.dm_cards_for(&channel, body, card_urls, &[]);
        let blobs = crate::attachments::blob_ids(&carried, &cards);
        let engine = self.dm_engine_ready().await?;
        let mut e = engine.lock().await;
        e.load_participants(&channel)?;
        let used: Vec<String> = cards.iter().map(|c| c.url.clone()).collect();
        let outcomes = e.send_message(
            &channel,
            lumia_core::proto::DmSend {
                body: body.to_string(),
                quote,
                attachments: carried,
                cards,
                ..Default::default()
            },
            blobs,
        )?;
        drop(e);
        self.sent_drafts(&attachments);
        {
            let mut media = self.media();
            for url in used {
                media.dm_cards.remove(&(channel.clone(), url));
            }
        }
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

    /// Adds this identity's reaction to a message of a personal DM, or
    /// takes it away (`remove`).
    ///
    /// # Errors
    /// Without the group for the conversation, or before the loop started.
    pub async fn dm_react(
        &self,
        channel_id: &str,
        message_id: &str,
        emoji: &str,
        remove: bool,
    ) -> Result<(), ClientError> {
        let channel = parse_uuid(channel_id)?;
        let message = parse_uuid(message_id)?;
        let engine = self.dm_engine_ready().await?;
        let mut e = engine.lock().await;
        e.load_participants(&channel)?;
        let outcomes = e.send_react(&channel, message, emoji, remove)?;
        drop(e);
        self.dispatch(outcomes);
        Ok(())
    }

    /// Edits one of this identity's messages in a personal DM.
    ///
    /// # Errors
    /// Without the group for the conversation, or before the loop started.
    pub async fn dm_edit(
        &self,
        channel_id: &str,
        message_id: &str,
        body: &str,
    ) -> Result<(), ClientError> {
        self.dm_edit_with_media(channel_id, message_id, body, &[], None)
            .await
    }

    /// [`App::dm_edit`], dropping the attachments `remove_attachment_ids`
    /// names and, when `card_urls` is given, keeping the cards of those
    /// links only: a card the message had, or one this device built with
    /// [`App::card_preview`]. Without it the cards of the links still in the
    /// body stay. The blobs of what the edit drops are deleted once it is
    /// sent (specs/media.md 4.4); that is best effort.
    ///
    /// # Errors
    /// Without the group for the conversation, or before the loop started.
    pub async fn dm_edit_with_media(
        &self,
        channel_id: &str,
        message_id: &str,
        body: &str,
        remove_attachment_ids: &[String],
        card_urls: Option<&[String]>,
    ) -> Result<(), ClientError> {
        self.fits(body)?;
        let channel = parse_uuid(channel_id)?;
        let message = parse_uuid(message_id)?;
        let removed = remove_attachment_ids
            .iter()
            .map(|id| parse_uuid(id))
            .collect::<Result<Vec<_>, _>>()?;
        let engine = self.dm_engine_ready().await?;
        let mut e = engine.lock().await;
        e.load_participants(&channel)?;
        let (projection, _, _) = project(&e, &channel)?;
        let mine = projection
            .message(&message)
            .filter(|m| m.sender_identity_id == e.me().identity_id && !m.deleted)
            .cloned();
        let (had_attachments, had_cards) =
            mine.map(|m| (m.attachments, m.cards)).unwrap_or_default();
        let dropped: Vec<_> = had_attachments
            .into_iter()
            .filter(|a| removed.contains(&a.attachment_id))
            .collect();
        let cards = card_urls.map(|urls| self.dm_cards_for(&channel, body, urls, &had_cards));
        let after = cards
            .clone()
            .unwrap_or_else(|| lumia_core::mls::projection::cards_in(body, &had_cards));
        let before_blobs = crate::attachments::blob_ids(&[], &had_cards);
        let after_blobs = crate::attachments::blob_ids(&[], &after);
        let new_blobs: Vec<Vec<u8>> = after_blobs
            .iter()
            .filter(|id| !before_blobs.contains(id))
            .cloned()
            .collect();
        let mut gone = crate::attachments::blob_ids(&dropped, &[]);
        gone.extend(
            before_blobs
                .into_iter()
                .filter(|id| !after_blobs.contains(id)),
        );
        let used: Vec<String> = after.iter().map(|c| c.url.clone()).collect();
        let outcomes = e.send_edit_using(
            &channel,
            lumia_core::proto::DmEdit {
                message_id: message,
                body: body.to_string(),
                remove_attachment_ids: removed,
                cards: cards.map(|cards| lumia_core::proto::DmCards { cards }),
            },
            new_blobs,
        )?;
        drop(e);
        self.dispatch(outcomes);
        {
            let mut media = self.media();
            for url in used {
                media.dm_cards.remove(&(channel.clone(), url));
            }
        }
        self.delete_blobs(&gone).await;
        Ok(())
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
        // Its blobs go once the delete is sent (specs/media.md 4.4).
        let (projection, _, _) = project(&e, &channel)?;
        let gone = projection
            .message(&message)
            .filter(|m| m.sender_identity_id == e.me().identity_id)
            .map(|m| crate::attachments::blob_ids(&m.attachments, &m.cards))
            .unwrap_or_default();
        let outcomes = e.send_delete(&channel, message)?;
        drop(e);
        self.dispatch(outcomes);
        self.delete_blobs(&gone).await;
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
        let (projection, before_join, undecryptable) = project(&e, &channel)?;
        // No server governs a personal conversation: its deleted messages are
        // left out, as a server's are by default (architecture 6.7).
        let messages = projection
            .messages()
            .filter(|m| !m.deleted)
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
                reply: m.quote.as_ref().map(|q| QuoteSummary {
                    message_id: wire::display(&q.message_id),
                    author_account_id: projection
                        .message(&q.message_id)
                        .and_then(|o| e.account_of(&o.sender_identity_id))
                        .map(|a| wire::display(a))
                        .unwrap_or_default(),
                    text: q.text.clone(),
                    original_deleted: q.original_deleted,
                    silent: false,
                }),
                reactions: m
                    .reactions
                    .iter()
                    .map(|(emoji, who)| ReactionSummary {
                        reaction: emoji.clone(),
                        count: u32::try_from(who.len()).unwrap_or(u32::MAX),
                        me: who.contains(&me),
                        account_ids: who
                            .iter()
                            .filter_map(|identity| e.account_of(identity))
                            .map(|a| wire::display(a))
                            .collect(),
                    })
                    .collect(),
                attachments: m.attachments.iter().map(attachment_summary).collect(),
                cards: m.cards.iter().map(card_summary).collect(),
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
        // A reauth token is the session's that earned it: a new session
        // brings its own or none (identity spec 10).
        self.lock().reauth = Some(pair.reauth_token.clone()).filter(|t| !t.is_empty());
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

// ---- Attachments and link cards (specs/media.md) -------------------------------

impl App {
    fn media(&self) -> std::sync::MutexGuard<'_, Media> {
        self.media.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Keeps the attachments this device opens in `dir` as well as in
    /// memory, up to 512 MiB, the least recently used going first. Natively
    /// only; without it they are kept in memory alone. A shell gives a
    /// directory the system may clear, or one of its own beside the stores.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn set_media_cache_dir(&self, dir: std::path::PathBuf) {
        self.media().cache.set_dir(dir);
    }

    /// What the instance takes of attachments (`GET /instance`, asked once
    /// per launch): `None` when it keeps none, and a shell then offers no
    /// way to attach.
    ///
    /// # Errors
    /// On transport or API failure.
    pub async fn upload_limits(&self) -> Result<Option<UploadLimitsSummary>, ClientError> {
        Ok(self.limits().await?.map(|l| UploadLimitsSummary {
            image_max_bytes: l.image_max_bytes,
            file_max_bytes: l.file_max_bytes,
            thumbnail_max_bytes: l.thumbnail_max_bytes,
        }))
    }

    async fn limits(&self) -> Result<Option<lumia_core::proto::UploadLimits>, ClientError> {
        let known = self.media().limits;
        if let Some(known) = known {
            return Ok(known);
        }
        Ok(self.learn_instance().await?.0)
    }

    /// `GET /instance`'s uploads and link cards, kept for the launch.
    async fn learn_instance(
        &self,
    ) -> Result<(Option<lumia_core::proto::UploadLimits>, bool), ClientError> {
        let info = self.api.instance_info().await?;
        let mut media = self.media();
        media.limits = Some(info.upload_limits);
        media.link_cards = Some(info.link_cards_enabled);
        Ok((info.upload_limits, info.link_cards_enabled))
    }

    /// Makes an attachment of a file and uploads it into the conversation
    /// `channel_id` (specs/media.md 3 and 4.1): processed on this device
    /// (metadata stripped, a picture resized and re-encoded, a thumbnail and
    /// a placeholder made), each blob sealed under a key of its own and
    /// stored. Answers the draft a composer shows; [`App::send_message_with_media`]
    /// or [`App::dm_send_with_media`] sends it by its id.
    ///
    /// # Errors
    /// `NoBlobs` when the instance keeps no attachments; `Media` when the
    /// file cannot be sent (too large even after processing, in words);
    /// the instance's refusal (`PAYLOAD_TOO_LARGE`, `PERMISSION_DENIED`
    /// without `ATTACH_FILES`); transport failure.
    pub async fn attach(
        &self,
        channel_id: &str,
        input: AttachmentInput,
    ) -> Result<AttachmentDraft, ClientError> {
        let channel = parse_uuid(channel_id)?;
        let limits = self.limits().await?.ok_or(ClientError::NoBlobs)?;
        let pixels = |p: PixelsInput| lumia_core::media::Pixels {
            rgba: p.rgba,
            width: p.width,
            height: p.height,
        };
        let video_size = (input.video_width > 0 && input.video_height > 0)
            .then_some((input.video_width, input.video_height));
        let processed = lumia_core::media::process(lumia_core::media::Input {
            bytes: input.bytes,
            name: input.name,
            as_file: input.as_file,
            pixels: input.pixels.map(pixels),
            poster: input.poster.map(pixels),
            duration_ms: input.duration_ms,
            video_size,
            caps: crate::attachments::caps(&limits),
        })?;
        let attachment =
            crate::attachments::upload(&self.api, &channel, processed, &input.alt).await?;
        let draft = draft_summary(&channel, &attachment);
        self.media().drafts.insert(
            draft.draft_id.clone(),
            Draft {
                channel_id: channel,
                attachment,
            },
        );
        Ok(draft)
    }

    /// Sets a waiting attachment's alt text (at most 1,000 characters).
    ///
    /// # Errors
    /// `State` when no attachment waits under `draft_id`.
    pub fn set_attachment_alt(
        &self,
        draft_id: &str,
        alt: &str,
    ) -> Result<AttachmentDraft, ClientError> {
        let mut media = self.media();
        let draft = media.drafts.get_mut(draft_id).ok_or_else(not_waiting)?;
        draft.attachment.alt = crate::attachments::clip_alt(alt);
        Ok(draft_summary(&draft.channel_id, &draft.attachment))
    }

    /// Takes a waiting attachment out of the composer and its blobs off the
    /// instance (best effort: one left is removed within a day anyway).
    pub async fn discard_attachment(&self, draft_id: &str) {
        let draft = self.media().drafts.remove(draft_id);
        if let Some(draft) = draft {
            self.delete_blobs(&crate::attachments::blob_ids(&[draft.attachment], &[]))
                .await;
        }
    }

    /// The waiting attachments `ids` names, all uploaded into `channel`.
    fn drafts(
        &self,
        channel: &[u8],
        ids: &[String],
    ) -> Result<Vec<lumia_core::proto::Attachment>, ClientError> {
        if ids.len() > lumia_core::media::MAX_ATTACHMENTS {
            return Err(ClientError::State(format!(
                "a message carries at most {} attachments",
                lumia_core::media::MAX_ATTACHMENTS
            )));
        }
        let media = self.media();
        ids.iter()
            .map(|id| {
                media
                    .drafts
                    .get(id)
                    .filter(|d| d.channel_id == channel)
                    .map(|d| d.attachment.clone())
                    .ok_or_else(not_waiting)
            })
            .collect()
    }

    fn sent_drafts(&self, ids: &[String]) {
        let mut media = self.media();
        for id in ids {
            media.drafts.remove(id);
        }
    }

    async fn delete_blobs(&self, ids: &[Vec<u8>]) {
        for id in ids {
            if let Err(e) = self.api.delete_blob(id).await {
                tracing::debug!("a blob was not deleted: {e}");
            }
        }
    }

    /// The links of a message body that may have cards, at most three, in
    /// order (specs/media.md 6.1): what a composer previews.
    #[must_use]
    pub fn card_links(body: &str) -> Vec<String> {
        lumia_core::cards::card_links(body)
    }

    /// Whether this instance makes link cards in server channels
    /// (`InstanceInfo.link_cards_enabled`), as learnt by
    /// [`App::upload_limits`]; false until then.
    #[must_use]
    pub fn server_link_cards(&self) -> bool {
        self.media().link_cards == Some(true)
    }

    /// The links a composer previews in `channel_id` (at most three, in
    /// order): [`App::card_links`] when the conversation can have cards, and
    /// none when it cannot, so a composer shows nothing waiting for one. A
    /// server channel has cards when its instance makes them
    /// (`InstanceInfo.link_cards_enabled`, learnt by [`App::upload_limits`],
    /// which every shell asks when it opens; none until then); a personal
    /// conversation has them natively, where this device fetches the page,
    /// and never on the web (specs/media.md 6.5). Answers at once from what
    /// this client holds, for a composer that follows every keystroke.
    #[must_use]
    pub fn preview_links(&self, channel_id: &str, body: &str) -> Vec<String> {
        let Ok(channel) = parse_uuid(channel_id) else {
            return Vec::new();
        };
        let personal = self
            .lock()
            .reads
            .by_channel
            .get(&channel)
            .is_some_and(|c| c.kind.starts_with("PERSONAL"));
        let cards = if personal {
            cfg!(not(target_arch = "wasm32"))
        } else {
            self.server_link_cards()
        };
        if cards {
            Self::card_links(body)
        } else {
            Vec::new()
        }
    }

    /// The card a composer shows under a link while the person writes
    /// (specs/media.md 6.4, 6.5). In a server channel the instance builds it
    /// (`POST /unfurl`, cached there). In a personal conversation this
    /// device fetches the page itself, natively, and keeps the card, its
    /// picture uploaded into the conversation, for the next send; the web
    /// cannot (a browser does not read other sites) and answers none. `None`
    /// when the page gives no card or could not be read.
    ///
    /// # Errors
    /// In a server channel, the instance's refusal (`RATE_LIMITED`) or the
    /// transport; a malformed id.
    pub async fn card_preview(
        &self,
        channel_id: &str,
        url: &str,
    ) -> Result<Option<CardSummary>, ClientError> {
        let channel = parse_uuid(channel_id)?;
        let card = if self.is_personal(&channel).await {
            self.dm_card(&channel, url).await
        } else {
            self.api.unfurl(channel, url).await?
        };
        Ok(card.as_ref().map(card_summary))
    }

    /// Whether `channel` is a personal conversation, as far as this client
    /// has heard of it.
    async fn is_personal(&self, channel: &[u8]) -> bool {
        let said = self
            .lock()
            .reads
            .by_channel
            .get(channel)
            .map(|c| c.kind.starts_with("PERSONAL"));
        if let Some(said) = said {
            return said;
        }
        match self.built_engine() {
            Some(engine) => engine.lock().await.channels().contains_key(channel),
            None => false,
        }
    }

    /// A card this device builds for a link in a personal conversation.
    #[cfg(not(target_arch = "wasm32"))]
    async fn dm_card(&self, channel: &[u8], url: &str) -> Option<lumia_core::proto::Card> {
        let key = (channel.to_vec(), url.to_owned());
        let made = self.media().dm_cards.get(&key).cloned();
        if made.is_some() {
            return made;
        }
        let fetched = match crate::cards::fetch_card(url, "").await {
            Ok(Some(fetched)) => fetched,
            Ok(None) => return None,
            Err(e) => {
                tracing::debug!("no card for a link: {e}");
                return None;
            }
        };
        let page = fetched.card;
        let mut card = lumia_core::proto::Card {
            url: url.to_owned(),
            title: page.title,
            description: page.description,
            site_name: page.site_name,
            color: page.color,
            video: page.video,
            ..Default::default()
        };
        if let Some(thumbnail) = fetched.image.and_then(card_thumbnail) {
            match crate::attachments::upload_blob(
                &self.api,
                channel,
                lumia_core::proto::BlobRole::Thumbnail,
                &thumbnail,
            )
            .await
            {
                Ok(blob) => {
                    let size = lumia_core::media::probe(&thumbnail);
                    card.image_width = size.map_or(0, |p| p.width);
                    card.image_height = size.map_or(0, |p| p.height);
                    card.image = Some(blob);
                }
                Err(e) => tracing::debug!("a card's picture was not uploaded: {e}"),
            }
        }
        self.media().dm_cards.insert(key, card.clone());
        Some(card)
    }

    /// The web makes no cards in personal conversations (specs/media.md 6.5).
    #[cfg(target_arch = "wasm32")]
    #[allow(clippy::unused_async)]
    async fn dm_card(&self, _channel: &[u8], _url: &str) -> Option<lumia_core::proto::Card> {
        None
    }

    /// The cards of `urls` (the links of `body` the person kept) for a
    /// personal message: one it already had, or one this device built.
    fn dm_cards_for(
        &self,
        channel: &[u8],
        body: &str,
        urls: &[String],
        had: &[lumia_core::proto::Card],
    ) -> Vec<lumia_core::proto::Card> {
        let media = self.media();
        cards_kept(body, urls)
            .into_iter()
            .filter_map(|url| {
                had.iter()
                    .find(|c| c.url == url)
                    .or_else(|| media.dm_cards.get(&(channel.to_vec(), url)))
                    .cloned()
            })
            .collect()
    }

    /// An attachment's or a card's blob, opened: fetched with this session
    /// (`GET /blobs/{id}`, following a redirect to the bucket without it),
    /// decrypted with the key it names and checked against its hash. Kept
    /// after the first fetch (see [`App::set_media_cache_dir`]). A picture's
    /// bytes are someone else's choice: check [`App::attachment_drawable`]
    /// before handing them to a decoder.
    ///
    /// # Errors
    /// `NOT_FOUND` when this account cannot read a message naming it;
    /// `Media` when it does not open or is not the blob named; transport.
    pub async fn attachment_bytes(&self, blob: &BlobSummary) -> Result<Vec<u8>, ClientError> {
        let blob = blob_ref(blob)?;
        let kept = self.media().cache.get(&blob);
        if let Some(bytes) = kept {
            return Ok(bytes);
        }
        let bytes = crate::attachments::fetch(&self.api, &blob).await?;
        self.media().cache.put(&blob.blob_id, &bytes);
        Ok(bytes)
    }

    /// A server card's picture (`GET /cards/{address}`, public, never
    /// changing), for a shell that draws its own pictures; one that loads
    /// URLs can ask the instance for `image_url` itself.
    ///
    /// # Errors
    /// `State` for a path that is not a card picture of this instance;
    /// `NOT_FOUND`; transport failure.
    pub async fn card_image(&self, image_url: &str) -> Result<Vec<u8>, ClientError> {
        let named = image_url
            .strip_prefix("/api/v1/cards/")
            .is_some_and(|name| {
                !name.is_empty() && name.bytes().all(|b| b.is_ascii_alphanumeric())
            });
        if !named {
            return Err(ClientError::State(
                "not a card picture of this instance".into(),
            ));
        }
        self.api.avatar(image_url, CARD_IMAGE_MAX_BYTES).await
    }

    /// Whether a picture someone sent may be decoded (specs/media.md 5): its
    /// header names a size within 8,192 px a side and 40 million pixels.
    /// A picture that is not is drawn as a file row.
    #[must_use]
    pub fn attachment_drawable(bytes: &[u8]) -> bool {
        lumia_core::media::probe(bytes).is_some_and(|p| p.within_bounds())
    }

    /// An attachment's placeholder as a tiny picture, to scale up and draw
    /// until its thumbnail arrives; `None` for an empty or malformed one.
    #[must_use]
    pub fn placeholder_image(placeholder: &[u8]) -> Option<PlaceholderImage> {
        lumia_core::media::placeholder_image(placeholder).map(|p| PlaceholderImage {
            width: p.width,
            height: p.height,
            rgba: p.rgba,
        })
    }
}

/// What a notification says of a message: its text, or for one that is only
/// attachments, what they are (specs/media.md 5: notifications stay text).
fn notification_text(body: &str, attachments: &[lumia_core::proto::Attachment]) -> String {
    use lumia_core::proto::AttachmentKind;
    let kinds = attachments
        .iter()
        .map(|a| match AttachmentKind::try_from(a.kind) {
            Ok(AttachmentKind::Image | AttachmentKind::Gif) => "photo",
            Ok(AttachmentKind::Video) => "video",
            Ok(AttachmentKind::Audio) => "recording",
            _ => "file",
        });
    words_for(body, &kinds.collect::<Vec<_>>())
}

/// [`notification_text`] from a message summary's attachments, for a shell that words a
/// notice itself (the desktop's, of a personal conversation it reads back from the store).
#[must_use]
pub fn notice_text(body: &str, attachments: &[AttachmentSummary]) -> String {
    let kinds = attachments.iter().map(|a| match a.kind.as_str() {
        "image" | "gif" => "photo",
        "video" => "video",
        "audio" => "recording",
        _ => "file",
    });
    words_for(body, &kinds.collect::<Vec<_>>())
}

/// The body when it says anything, else what `kinds` are: "Sent a photo", "Sent 2 files".
fn words_for(body: &str, kinds: &[&str]) -> String {
    let Some(first) = kinds.first().copied() else {
        return body.to_owned();
    };
    if !body.trim().is_empty() {
        return body.to_owned();
    }
    match kinds.len() {
        1 => format!("Sent a {first}"),
        n if kinds.iter().all(|k| *k == first) => format!("Sent {n} {first}s"),
        n => format!("Sent {n} attachments"),
    }
}

fn not_waiting() -> ClientError {
    ClientError::State("that attachment is no longer waiting to be sent".into())
}

/// `ids` once each, in order.
fn distinct(ids: &[String]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for id in ids {
        if !out.contains(id) {
            out.push(id.clone());
        }
    }
    out
}

/// The links of `urls` that are links of `body` with cards (specs/media.md
/// 6.1), once each, at most three.
fn cards_kept(body: &str, urls: &[String]) -> Vec<String> {
    let links = lumia_core::cards::card_links(body);
    distinct(urls)
        .into_iter()
        .filter(|u| links.contains(u))
        .take(lumia_core::cards::MAX_CARDS)
        .collect()
}

/// A card's picture made a thumbnail (specs/media.md 6.3): decoded within
/// the bounds and at most 400 px, or none.
#[cfg(not(target_arch = "wasm32"))]
fn card_thumbnail(bytes: Vec<u8>) -> Option<Vec<u8>> {
    use lumia_core::media::AttachmentKind;
    let processed = lumia_core::media::process(lumia_core::media::Input {
        bytes,
        name: "card".into(),
        ..lumia_core::media::Input::default()
    })
    .ok()?;
    matches!(processed.kind, AttachmentKind::Image | AttachmentKind::Gif)
        .then_some(processed.thumbnail)
        .flatten()
}

/// A personal conversation's projection from this device's log, with the
/// counts of what it cannot read: sent before it joined, undecryptable.
fn project(e: &DmEngine, channel: &[u8]) -> Result<(Projection, u32, u32), ClientError> {
    let mut projection = Projection::default();
    let mut before_join = 0u32;
    let mut undecryptable = 0u32;
    for row in e.rows(channel)? {
        match row.status {
            DmStatus::Decrypted => {
                if let Ok(op) = DmOperation::decode(row.bytes.as_slice()) {
                    projection.apply(row.seq, &row.sender_identity_id, &row.sender_device_id, &op);
                }
            }
            DmStatus::BeforeJoin => before_join += 1,
            DmStatus::Undecryptable => undecryptable += 1,
        }
    }
    Ok((projection, before_join, undecryptable))
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
mod mention_tests {
    use super::{find_mentions, MemberSummary};

    fn member(id: &str, handle: &str) -> MemberSummary {
        MemberSummary {
            account_id: id.into(),
            handle: handle.into(),
            display_name: String::new(),
            nickname: String::new(),
            pronouns: String::new(),
            bio: String::new(),
            avatar_url: String::new(),
            channel_adds_by_invitation: false,
            server_dms_off: false,
        }
    }

    #[test]
    fn handles_and_everyone_are_found_with_their_byte_ranges() {
        let people = [member("a", "ada"), member("b", "bo.k")];
        let found = |body: &str, everyone| {
            find_mentions(body, &people, everyone)
                .into_iter()
                .map(|m| {
                    (
                        m.account_id,
                        m.everyone,
                        &body[m.start as usize..m.end as usize],
                    )
                })
                .map(|(id, e, text)| (id, e, text.to_string()))
                .collect::<Vec<_>>()
        };
        assert_eq!(
            found("hi @Ada and @bo.k.", false),
            [
                ("a".into(), false, "@Ada".into()),
                ("b".into(), false, "@bo.k".into())
            ]
        );
        assert_eq!(found("é @ada", false)[0].2, "@ada");
        assert!(found("mail ada@ada.com or @nobody", false).is_empty());
        assert!(found("@everyone look", false).is_empty());
        assert_eq!(
            found("(@everyone) look", true),
            [(String::new(), true, "@everyone".into())]
        );
    }
}

#[cfg(test)]
mod media_tests {
    use super::{cards_kept, notice_text, notification_text, AttachmentSummary};
    use lumia_core::proto::{Attachment, AttachmentKind};

    fn of(kind: AttachmentKind) -> Attachment {
        Attachment {
            kind: kind as i32,
            ..Attachment::default()
        }
    }

    #[test]
    fn a_message_of_attachments_alone_is_told_in_words() {
        let photo = of(AttachmentKind::Image);
        assert_eq!(
            notification_text("look", std::slice::from_ref(&photo)),
            "look"
        );
        assert_eq!(
            notification_text("  ", std::slice::from_ref(&photo)),
            "Sent a photo"
        );
        assert_eq!(
            notification_text("", &[photo.clone(), of(AttachmentKind::Gif)]),
            "Sent 2 photos"
        );
        assert_eq!(
            notification_text("", &[photo, of(AttachmentKind::File)]),
            "Sent 2 attachments"
        );
        assert_eq!(notification_text("", &[]), "");
    }

    #[test]
    fn a_summary_of_attachments_alone_is_told_in_the_same_words() {
        let of = |kind: &str| AttachmentSummary {
            kind: kind.into(),
            ..AttachmentSummary::default()
        };
        assert_eq!(notice_text("look", &[of("image")]), "look");
        assert_eq!(notice_text("", &[of("gif")]), "Sent a photo");
        assert_eq!(notice_text("", &[of("audio")]), "Sent a recording");
        assert_eq!(notice_text("", &[of("file"), of("file")]), "Sent 2 files");
        assert_eq!(
            notice_text("", &[of("video"), of("file")]),
            "Sent 2 attachments"
        );
        assert_eq!(notice_text("", &[]), "");
    }

    #[test]
    fn only_links_of_the_body_keep_cards_once_each_and_three_at_most() {
        let body = "https://a.example https://b.example `https://c.example` https://d.example https://e.example";
        let asked: Vec<String> = [
            "https://b.example",
            "https://b.example",
            "https://c.example",
            "https://z.example",
            "https://a.example",
            "https://d.example",
            "https://e.example",
        ]
        .map(String::from)
        .to_vec();
        assert_eq!(
            cards_kept(body, &asked),
            [
                "https://b.example",
                "https://a.example",
                "https://d.example"
            ]
        );
    }
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

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod link_request_tests {
    use super::App;
    use base64::Engine as _;
    use lumia_core::proto::{DeviceKind, LinkRequest};
    use prost::Message as _;

    #[test]
    fn a_request_says_what_it_would_add_and_anything_else_is_refused() {
        let request = LinkRequest {
            device_id: vec![7; 16],
            ephemeral_public_key: vec![9; 32],
            kind: DeviceKind::Web as i32,
            label: "Safari".into(),
            web_origin: "https://my.lumia.chat".into(),
            ..Default::default()
        };
        let text = base64::engine::general_purpose::STANDARD.encode(request.encode_to_vec());
        let summary = App::identity_link_request_summary(&format!("  {text}\n")).unwrap();
        assert_eq!(summary.kind, "web");
        assert_eq!(summary.label, "Safari");
        assert_eq!(summary.web_origin, "https://my.lumia.chat");
        assert_eq!(summary.device_id, "07070707-0707-0707-0707-070707070707");
        // Wrapped across lines when it was copied.
        let (head, tail) = text.split_at(20);
        let wrapped = App::identity_link_request_summary(&format!("{head}\r\n  {tail}")).unwrap();
        assert_eq!(wrapped.label, "Safari");
        assert!(App::identity_link_request_summary("hello").is_err());
        assert!(App::identity_link_request_summary("").is_err());
        let short = base64::engine::general_purpose::STANDARD.encode(
            LinkRequest {
                device_id: vec![1; 3],
                ..Default::default()
            }
            .encode_to_vec(),
        );
        assert!(App::identity_link_request_summary(&short).is_err());
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod dm_read_tests {
    use super::dm_read_through;
    use crate::sqlite::{DmRow, DmStatus};
    use lumia_core::proto::{dm_operation, DmDelete, DmOperation, DmReact, DmSend};
    use prost::Message as _;

    fn row(seq: u64, op: Option<dm_operation::Op>) -> DmRow {
        DmRow {
            seq,
            sender_identity_id: vec![2; 32],
            sender_device_id: vec![2; 16],
            epoch: 1,
            status: if op.is_some() {
                DmStatus::Decrypted
            } else {
                DmStatus::Undecryptable
            },
            bytes: op.map_or_else(Vec::new, |op| {
                DmOperation {
                    op: Some(op),
                    ..Default::default()
                }
                .encode_to_vec()
            }),
        }
    }

    fn send(id: u8) -> Option<dm_operation::Op> {
        Some(dm_operation::Op::Send(DmSend {
            message_id: vec![id; 16],
            body: "hi".into(),
            ..Default::default()
        }))
    }

    #[test]
    fn what_shows_no_message_after_the_newest_is_read_with_it() {
        let react = Some(dm_operation::Op::React(DmReact {
            message_id: vec![1; 16],
            ..Default::default()
        }));
        let rows = [row(3, send(1)), row(5, react), row(6, None)];
        assert_eq!(
            dm_read_through(&rows, 3),
            6,
            "a reaction and an unreadable one"
        );

        // The next message that shows is not read before it is seen.
        let rows = [row(3, send(1)), row(5, None), row(7, send(2)), row(8, None)];
        assert_eq!(dm_read_through(&rows, 3), 5);

        // One sent and deleted shows nothing.
        let delete = Some(dm_operation::Op::Delete(DmDelete {
            message_id: vec![2; 16],
        }));
        let rows = [row(3, send(1)), row(4, send(2)), row(5, delete)];
        assert_eq!(dm_read_through(&rows, 3), 5);
        assert_eq!(dm_read_through(&[], 3), 3);
    }
}

#[cfg(test)]
mod read_tests {
    use super::{ReadCount, Reads};
    use lumia_core::proto::{event, mention, Event, Mention, Message, MessageCreated, QuoteReply};

    const ME: [u8; 16] = [1; 16];
    const THEM: [u8; 16] = [2; 16];
    const ROOM: [u8; 16] = [7; 16];

    fn sent(seq: u64, author: [u8; 16], mentions: Vec<Mention>, quoted: Option<[u8; 16]>) -> Event {
        Event {
            channel_id: ROOM.to_vec(),
            seq,
            payload: Some(event::Payload::MessageCreated(MessageCreated {
                message: Some(Message {
                    message_id: vec![seq as u8; 16],
                    channel_id: ROOM.to_vec(),
                    author_account_id: author.to_vec(),
                    body: "hi".into(),
                    mentions,
                    reply_to: quoted.map(|a| QuoteReply {
                        message_id: vec![9; 16],
                        author_account_id: a.to_vec(),
                        text: "x".into(),
                        original_deleted: false,
                        silent: false,
                    }),
                    ..Default::default()
                }),
            })),
            ..Default::default()
        }
    }

    fn reads(kind: &str) -> Reads {
        let mut reads = Reads {
            me: ME.to_vec(),
            ..Reads::default()
        };
        reads.by_channel.insert(
            ROOM.to_vec(),
            ReadCount {
                kind: kind.into(),
                read_seq: 3,
                counted_through: 3,
                ..ReadCount::default()
            },
        );
        reads
    }

    fn me(seq: u64) -> Mention {
        Mention {
            target: Some(mention::Target::AccountId(ME.to_vec())),
            start: 0,
            end: seq as u32,
        }
    }

    #[test]
    fn what_others_send_is_unread_and_what_names_me_notifies() {
        let mut r = reads("TEXT");
        r.count(&[
            sent(4, THEM, Vec::new(), None),
            sent(5, ME, Vec::new(), None),
            sent(6, THEM, vec![me(1)], None),
            sent(7, THEM, Vec::new(), Some(ME)),
        ]);
        // A replayed event is not counted twice.
        r.count(&[sent(7, THEM, Vec::new(), Some(ME))]);
        let c = &r.by_channel[ROOM.as_slice()];
        assert_eq!((c.unread, c.mentions), (3, 2));

        r.read_elsewhere(&ROOM, 7);
        let c = &r.by_channel[ROOM.as_slice()];
        assert_eq!((c.read_seq, c.unread, c.mentions), (7, 0, 0));
    }

    #[test]
    fn a_silent_reply_does_not_notify_the_author_it_quotes() {
        let mut r = reads("TEXT");
        let mut quiet = sent(4, THEM, Vec::new(), Some(ME));
        if let Some(event::Payload::MessageCreated(MessageCreated { message: Some(m) })) =
            quiet.payload.as_mut()
        {
            if let Some(q) = m.reply_to.as_mut() {
                q.silent = true;
            }
        }
        r.count(&[quiet]);
        let c = &r.by_channel[ROOM.as_slice()];
        assert_eq!((c.unread, c.mentions), (1, 0));
    }

    #[test]
    fn every_message_of_a_dm_notifies() {
        let mut r = reads("SERVER_DM");
        r.count(&[
            sent(4, THEM, Vec::new(), None),
            sent(5, THEM, Vec::new(), None),
        ]);
        let c = &r.by_channel[ROOM.as_slice()];
        assert_eq!((c.unread, c.mentions), (2, 2));
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

    /// An app on files in a directory of its own, as a device keeps them.
    fn app_at(dir: &std::path::Path) -> Arc<App> {
        let at = dir.to_path_buf();
        let opener: Opener = Arc::new(move |name| {
            SqliteStore::open(&at.join(match name {
                StoreName::Instance => "store.sqlite",
                StoreName::Keys => "keys.sqlite",
                StoreName::IdentityStore => "identity.sqlite",
                StoreName::IdentityKeys => "identity-keys.sqlite",
            }))
        });
        App::open("http://127.0.0.1:9", Place::Native, opener).unwrap()
    }

    /// An erase that could not finish (another writer held the store) keeps
    /// what says it is to erase, and the next launch finishes it.
    #[tokio::test]
    async fn an_erase_that_did_not_finish_is_finished_at_the_next_launch() {
        let dir =
            std::env::temp_dir().join(format!("lumia-erase-{}", wire::display(&wire::new_id())));
        std::fs::create_dir_all(&dir).unwrap();
        let app = app_at(&dir);
        app.lock().reader.meta_set(KEPT_SNAPSHOT, "kept").unwrap();
        app.identity
            .lock()
            .await
            .keys
            .put("device_key", b"secret")
            .unwrap();
        let id = app.device_id();

        let other = rusqlite::Connection::open(dir.join("store.sqlite")).unwrap();
        other.execute_batch("BEGIN IMMEDIATE;").unwrap();
        assert!(app.erase_device().await.is_err());
        assert!(app.lock().keys.get(REVOKED_HEARD).unwrap().is_some());
        drop(app);
        drop(other);

        let app = app_at(&dir);
        assert_eq!(app.lock().reader.meta_get(KEPT_SNAPSHOT).unwrap(), None);
        assert!(app
            .identity
            .lock()
            .await
            .keys
            .get("device_key")
            .unwrap()
            .is_none());
        assert!(app.lock().keys.get(REVOKED_HEARD).unwrap().is_none());
        assert_ne!(app.device_id(), id);
        drop(app);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_body_over_the_limit_is_refused_before_it_is_sent() {
        let app = app();
        assert_eq!(app.max_message_bytes(), body::MAX_MESSAGE_BYTES);
        let channel = "01a1da65-e156-7245-b673-417136cc841d";
        let over = "a".repeat(body::MAX_MESSAGE_BYTES as usize + 3);
        let refused = |e: Result<String, ClientError>| matches!(e, Err(ClientError::TooLong { over: 3, max }) if max == body::MAX_MESSAGE_BYTES);
        assert!(refused(app.send_message(channel, &over)));
        assert!(refused(app.edit_message(channel, channel, &over, &[])));
        // At the limit it goes on to the gateway, which is not started here.
        let at = "a".repeat(body::MAX_MESSAGE_BYTES as usize);
        assert!(!matches!(
            app.send_message(channel, &at),
            Err(ClientError::TooLong { .. })
        ));
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
                show_deleted: false,
                can_manage: false,
                can_manage_channels: false,
                can_start_dms: false,
                can_mention_everyone: false,
                can_manage_messages: false,
                channel_adds_by_invitation: false,
                server_dms_off: false,
            }],
            channels: vec![ChannelSummary {
                channel_id: "c".into(),
                server_id: "s".into(),
                name: "general".into(),
                kind: "TEXT".into(),
                last_seq: 7,
                participant_account_ids: vec!["a".into()],
                host: String::new(),
                private: false,
                is_default: false,
                member_count: 0,
                topic: String::new(),
            }],
            pending_requests: Vec::new(),
            channel_invitations: Vec::new(),
            read_states: Vec::new(),
            rail: Vec::new(),
        }
    }

    #[test]
    fn deleted_messages_are_read_only_where_the_server_shows_them() {
        use lumia_core::proto::{event, Event, Message, MessageCreated, MessageDeleted};
        use lumia_core::store::{LogKind, Store, StoredEvent};
        use prost::Message as _;

        let app = app();
        let id = "018f0000-0000-7000-8000-000000000001";
        let ch = parse_uuid(id).unwrap();
        let at = |seq: u64, payload: event::Payload| StoredEvent {
            seq,
            bytes: Event {
                channel_id: ch.clone(),
                seq,
                payload: Some(payload),
                ..Default::default()
            }
            .encode_to_vec(),
        };
        let mut events: Vec<StoredEvent> = (1..=30)
            .map(|seq| {
                at(
                    seq,
                    event::Payload::MessageCreated(MessageCreated {
                        message: Some(Message {
                            message_id: vec![seq as u8; 16],
                            channel_id: ch.clone(),
                            seq,
                            body: format!("m{seq}"),
                            ..Default::default()
                        }),
                    }),
                )
            })
            .collect();
        // The newest 20 are deleted: a page of 5 must read past them all.
        for (i, seq) in (11..=30u64).enumerate() {
            events.push(at(
                31 + i as u64,
                event::Payload::MessageDeleted(MessageDeleted {
                    message_id: vec![seq as u8; 16],
                    purge_cache: false,
                }),
            ));
        }
        app.lock()
            .reader
            .apply(LogKind::Channel, &ch, &events)
            .unwrap();

        // No snapshot yet, or the server's setting off: left out.
        let page = app.messages(id, 5).unwrap();
        let bodies: Vec<&str> = page.iter().map(|m| m.body.as_str()).collect();
        assert_eq!(bodies, ["m6", "m7", "m8", "m9", "m10"]);
        let older = app.messages_before(id, page[0].seq, 5).unwrap();
        assert_eq!(older.first().map(|m| m.body.as_str()), Some("m1"));

        // The server says to show them, but the snapshot does not list this channel (one
        // read before joining): still left out, since nothing names its server.
        let mut showing = snapshot();
        showing.servers[0].show_deleted = true;
        app.note_shows_deleted(&showing);
        assert!(app.messages(id, 5).unwrap().iter().all(|m| !m.deleted));

        // Browse names its server, whose say it follows: they keep their places.
        app.note_channels(&[ChannelSummary {
            channel_id: id.into(),
            ..snapshot().channels[0].clone()
        }]);
        let page = app.messages(id, 5).unwrap();
        assert!(page.iter().all(|m| m.deleted));
        assert_eq!(page.last().map(|m| m.seq), Some(30));
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

    fn signed(version: u64, name: &str) -> lumia_core::proto::SignedProfile {
        lumia_core::proto::SignedProfile {
            body: lumia_core::proto::ProfileBody {
                version,
                display_name: name.into(),
                ..Default::default()
            }
            .encode_to_vec(),
            ..Default::default()
        }
    }

    #[test]
    fn a_profile_page_shows_the_account_as_it_is_unless_this_device_has_newer() {
        let account = |profile: Option<lumia_core::proto::SignedProfile>, name: &str| {
            lumia_core::proto::Account {
                display_name: name.into(),
                avatar_url: "/api/v1/avatars/now".into(),
                profile,
                ..Default::default()
            }
        };
        let name = |p: Option<ProfileSummary>| p.map(|p| (p.version, p.display_name, p.avatar_url));
        // Renamed with `PATCH /account` after this device signed version 3: the account's name.
        let held = account(Some(signed(3, "Ada")), "Ada L.");
        assert_eq!(
            name(shown_profile(Some(&signed(3, "Ada")), Some(&held)).unwrap()),
            Some((3, "Ada L.".into(), "/api/v1/avatars/now".into()))
        );
        // Another device signed version 5: its version, the account's name.
        let held = account(Some(signed(5, "Countess")), "Countess");
        assert_eq!(
            name(shown_profile(Some(&signed(3, "Ada")), Some(&held)).unwrap()),
            Some((5, "Countess".into(), "/api/v1/avatars/now".into()))
        );
        // This device signed a newer one, not yet sent: it shows as it is.
        let held = account(Some(signed(3, "Ada")), "Ada");
        assert_eq!(
            name(shown_profile(Some(&signed(4, "Ada K.")), Some(&held)).unwrap()),
            Some((4, "Ada K.".into(), String::new()))
        );
        // Signed out, or not asked: this device's.
        assert_eq!(
            name(shown_profile(Some(&signed(2, "Ada")), None).unwrap()),
            Some((2, "Ada".into(), String::new()))
        );
        // Nothing signed anywhere: nothing to show.
        assert!(shown_profile(None, Some(&account(None, "Ada")))
            .unwrap()
            .is_none());
    }

    #[test]
    fn a_profile_update_is_kept_in_every_member_list_with_each_servers_nickname() {
        let app = app();
        let member = |id: &str, name: &str, nickname: &str| MemberSummary {
            account_id: id.into(),
            handle: name.to_lowercase(),
            display_name: name.into(),
            nickname: nickname.into(),
            pronouns: String::new(),
            bio: String::new(),
            avatar_url: String::new(),
            channel_adds_by_invitation: false,
            server_dms_off: false,
        };
        let event = lumia_core::proto::AccountEvent {
            payload: Some(account_event::Payload::ProfileUpdated(
                lumia_core::proto::ProfileUpdated {
                    account: Some(lumia_core::proto::Account {
                        account_id: vec![7; 16],
                        handle: "ada".into(),
                        display_name: "Ada Lovelace".into(),
                        avatar_url: "/api/v1/avatars/new".into(),
                        ..Default::default()
                    }),
                },
            )),
            ..Default::default()
        };
        let profiles = profiles_updated(&[event]);
        let [AppEvent::ProfileUpdated { account_id, .. }] = profiles.as_slice() else {
            panic!("{profiles:?}");
        };
        let ada = account_id.clone();
        let mut all = HashMap::new();
        all.insert(
            "s".to_string(),
            vec![member(&ada, "Ada", ""), member("grace", "Grace", "")],
        );
        all.insert("t".to_string(), vec![member(&ada, "Ada", "Countess")]);
        app.keep(KEPT_MEMBERS, &all);
        app.keep_profiles(&profiles);
        let s = app.kept_members("s").unwrap();
        assert_eq!(s[0].display_name, "Ada Lovelace");
        assert_eq!(s[0].avatar_url, "/api/v1/avatars/new");
        assert_eq!(s[1].display_name, "Grace", "nobody else changes");
        let t = app.kept_members("t").unwrap();
        assert_eq!(
            (t[0].display_name.as_str(), t[0].nickname.as_str()),
            ("Ada Lovelace", "Countess")
        );
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
            channel_adds_by_invitation: false,
            server_dms_off: false,
        };
        let all: HashMap<String, Vec<MemberSummary>> =
            HashMap::from([("s".to_string(), vec![member])]);
        app.keep(KEPT_MEMBERS, &all);
        assert_eq!(app.kept_members("s").unwrap()[0].pronouns, "she/her");
        assert!(app.kept_members("other").unwrap().is_empty());
    }
}
