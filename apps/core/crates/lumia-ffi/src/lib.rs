//! UniFFI surface for the native shells (architecture section 13): one
//! [`Client`] per instance over `lumia_client::app::App`, and a
//! [`Listener`] the shell implements to receive what the gateway loop and
//! the DM engine produce.
//!
//! Marshalling only. Every method is synchronous and blocking, meant to be
//! called from a background thread on the shell's side; the gateway loop
//! runs on the client's own tokio runtime and reports through the listener
//! from a runtime thread. Ids cross the boundary as UUID strings and times
//! as microseconds since the epoch. The records here mirror the app's
//! summaries one for one.
//!
//! The stores are SQLite files under the shell's data directory, in the
//! clear: the platform keystore for the at-rest key is the shells' next
//! step (plan step 7).

use std::path::PathBuf;
use std::sync::Arc;

use lumia_client::app::{self, App, EventSink, Opener, Place, StoreName};
use lumia_client::sqlite::SqliteStore;
use lumia_client::ClientError;
use lumia_core::proto::PushKind;

uniffi::setup_scaffolding!();

/// Why a call failed. The shell gets the message; the variant names the
/// cause a caller can act on.
#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum LumiaError {
    /// The instance answered with the protocol's error envelope.
    #[error("{code}: {detail}")]
    Api {
        /// `lumia.v1.ErrorCode` as its name.
        code: String,
        /// The server's message.
        detail: String,
    },
    /// This device is revoked in the roster it now holds.
    #[error("this device was revoked (roster v{roster_version})")]
    DeviceRevoked {
        /// The adopted roster's version.
        roster_version: u64,
    },
    /// Not signed in to this instance.
    #[error("not signed in")]
    NotSignedIn,
    /// A malformed argument, a lifecycle mistake, or any other failure.
    #[error("{detail}")]
    Other { detail: String },
}

impl From<ClientError> for LumiaError {
    fn from(e: ClientError) -> Self {
        match e {
            ClientError::Api { code, message, .. } => Self::Api {
                code,
                detail: message,
            },
            ClientError::DeviceRevoked { roster_version } => Self::DeviceRevoked { roster_version },
            ClientError::NotSignedIn => Self::NotSignedIn,
            other => Self::Other {
                detail: other.to_string(),
            },
        }
    }
}

// ---- Records, mirroring lumia_client::app -------------------------------------

/// The trust card.
#[derive(Debug, Clone, uniffi::Record)]
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
    pub web_push_public_key: String,
    pub magic_link_enabled: bool,
    pub oidc_name: String,
}

impl From<app::InstanceSummary> for InstanceSummary {
    fn from(s: app::InstanceSummary) -> Self {
        Self {
            host: s.host,
            operator_name: s.operator_name,
            operator_url: s.operator_url,
            terms_url: s.terms_url,
            privacy_url: s.privacy_url,
            software_version: s.software_version,
            identity_proof_sign_in_enabled: s.identity_proof_sign_in_enabled,
            identity_proof_create_enabled: s.identity_proof_create_enabled,
            open_registration: s.open_registration,
            web_push_public_key: s.web_push_public_key,
            magic_link_enabled: s.magic_link_enabled,
            oidc_name: s.oidc_name,
        }
    }
}

/// A server the account belongs to.
#[derive(Debug, Clone, uniffi::Record)]
pub struct ServerSummary {
    pub server_id: String,
    pub name: String,
}

impl From<app::ServerSummary> for ServerSummary {
    fn from(s: app::ServerSummary) -> Self {
        Self {
            server_id: s.server_id,
            name: s.name,
        }
    }
}

/// What an invite shows before joining.
#[derive(Debug, Clone, uniffi::Record)]
pub struct InvitePreviewSummary {
    pub code: String,
    pub server_id: String,
    pub server_name: String,
    pub channel_name: String,
    pub member_count: u32,
    pub as_guest: bool,
    pub host: String,
    pub operator_name: String,
    pub open_registration: bool,
    pub identity_proof_create_enabled: bool,
}

impl From<app::InvitePreviewSummary> for InvitePreviewSummary {
    fn from(p: app::InvitePreviewSummary) -> Self {
        Self {
            code: p.code,
            server_id: p.server_id,
            server_name: p.server_name,
            channel_name: p.channel_name,
            member_count: p.member_count,
            as_guest: p.as_guest,
            host: p.host,
            operator_name: p.operator_name,
            open_registration: p.open_registration,
            identity_proof_create_enabled: p.identity_proof_create_enabled,
        }
    }
}

/// An invite this account minted: the code and the link that carries it.
#[derive(Debug, Clone, uniffi::Record)]
pub struct InviteSummary {
    pub code: String,
    pub url: String,
    pub server_id: String,
    pub expires_at_micros: Option<i64>,
    pub max_uses: u32,
    pub uses: u32,
    /// The address an email invite went to; empty for a link.
    pub email: String,
}

impl From<app::InviteSummary> for InviteSummary {
    fn from(i: app::InviteSummary) -> Self {
        Self {
            code: i.code,
            url: i.url,
            server_id: i.server_id,
            expires_at_micros: i.expires_at_micros,
            max_uses: i.max_uses,
            uses: i.uses,
            email: i.email,
        }
    }
}

/// A channel the account can view.
#[derive(Debug, Clone, uniffi::Record)]
pub struct ChannelSummary {
    pub channel_id: String,
    pub server_id: String,
    pub name: String,
    pub kind: String,
    pub last_seq: u64,
    pub participant_account_ids: Vec<String>,
    pub host: String,
}

impl From<app::ChannelSummary> for ChannelSummary {
    fn from(c: app::ChannelSummary) -> Self {
        Self {
            channel_id: c.channel_id,
            server_id: c.server_id,
            name: c.name,
            kind: c.kind,
            last_seq: c.last_seq,
            participant_account_ids: c.participant_account_ids,
            host: c.host,
        }
    }
}

/// A DM request waiting for an answer.
#[derive(Debug, Clone, uniffi::Record)]
pub struct DmRequestSummary {
    pub request_id: String,
    pub from_account_id: String,
    pub to_account_id: String,
    pub state: String,
    pub shared_server_ids: Vec<String>,
    pub mutual_friend_count: u32,
}

impl From<app::DmRequestSummary> for DmRequestSummary {
    fn from(r: app::DmRequestSummary) -> Self {
        Self {
            request_id: r.request_id,
            from_account_id: r.from_account_id,
            to_account_id: r.to_account_id,
            state: r.state,
            shared_server_ids: r.shared_server_ids,
            mutual_friend_count: r.mutual_friend_count,
        }
    }
}

/// The account snapshot.
#[derive(Debug, Clone, uniffi::Record)]
pub struct SnapshotSummary {
    pub account_id: String,
    pub handle: String,
    pub display_name: String,
    pub servers: Vec<ServerSummary>,
    pub channels: Vec<ChannelSummary>,
    pub pending_requests: Vec<DmRequestSummary>,
}

impl From<app::SnapshotSummary> for SnapshotSummary {
    fn from(s: app::SnapshotSummary) -> Self {
        Self {
            account_id: s.account_id,
            handle: s.handle,
            display_name: s.display_name,
            servers: s.servers.into_iter().map(Into::into).collect(),
            channels: s.channels.into_iter().map(Into::into).collect(),
            pending_requests: s.pending_requests.into_iter().map(Into::into).collect(),
        }
    }
}

/// A person: a member of a server, or a public profile.
#[derive(Debug, Clone, uniffi::Record)]
pub struct MemberSummary {
    pub account_id: String,
    pub handle: String,
    pub display_name: String,
    pub nickname: String,
    /// From the account's verified profile, as the display name is.
    pub pronouns: String,
    pub bio: String,
}

impl From<app::MemberSummary> for MemberSummary {
    fn from(m: app::MemberSummary) -> Self {
        Self {
            account_id: m.account_id,
            handle: m.handle,
            display_name: m.display_name,
            nickname: m.nickname,
            pronouns: m.pronouns,
            bio: m.bio,
        }
    }
}

/// A message as the local store holds it.
#[derive(Debug, Clone, uniffi::Record)]
pub struct MessageSummary {
    pub message_id: String,
    pub channel_id: String,
    pub seq: u64,
    pub author_account_id: String,
    pub body: String,
    pub created_at_micros: i64,
    pub deleted: bool,
}

impl From<app::MessageSummary> for MessageSummary {
    fn from(m: app::MessageSummary) -> Self {
        Self {
            message_id: m.message_id,
            channel_id: m.channel_id,
            seq: m.seq,
            author_account_id: m.author_account_id,
            body: m.body,
            created_at_micros: m.created_at_micros,
            deleted: m.deleted,
        }
    }
}

/// One applied channel event.
#[derive(Debug, Clone, uniffi::Record)]
pub struct EventSummary {
    pub seq: u64,
    pub kind: String,
    pub message: Option<MessageSummary>,
}

impl From<app::EventSummary> for EventSummary {
    fn from(e: app::EventSummary) -> Self {
        Self {
            seq: e.seq,
            kind: e.kind,
            message: e.message.map(Into::into),
        }
    }
}

/// A device of the identity's roster.
#[derive(Debug, Clone, uniffi::Record)]
pub struct DeviceSummary {
    pub device_id: String,
    pub kind: String,
    pub label: String,
    pub web_origin: String,
    pub added_at_micros: i64,
    pub this_device: bool,
    pub revoked: bool,
}

impl From<app::DeviceSummary> for DeviceSummary {
    fn from(d: app::DeviceSummary) -> Self {
        Self {
            device_id: d.device_id,
            kind: d.kind,
            label: d.label,
            web_origin: d.web_origin,
            added_at_micros: d.added_at_micros,
            this_device: d.this_device,
            revoked: d.revoked,
        }
    }
}

/// The identity's roster.
#[derive(Debug, Clone, uniffi::Record)]
pub struct RosterSummary {
    pub identity_id: String,
    pub version: u64,
    pub devices: Vec<DeviceSummary>,
}

impl From<app::RosterSummary> for RosterSummary {
    fn from(r: app::RosterSummary) -> Self {
        Self {
            identity_id: r.identity_id,
            version: r.version,
            devices: r.devices.into_iter().map(Into::into).collect(),
        }
    }
}

/// What creating an identity gives back: the id, and the phrase shown once.
#[derive(Debug, Clone, uniffi::Record)]
pub struct IdentityCreated {
    pub identity_id: String,
    pub phrase: String,
}

/// A personal conversation this device knows.
#[derive(Debug, Clone, uniffi::Record)]
pub struct DmChannelSummary {
    pub channel_id: String,
    pub participant_account_ids: Vec<String>,
    pub member: bool,
}

impl From<app::DmChannelSummary> for DmChannelSummary {
    fn from(c: app::DmChannelSummary) -> Self {
        Self {
            channel_id: c.channel_id,
            participant_account_ids: c.participant_account_ids,
            member: c.member,
        }
    }
}

/// A message of a personal conversation, as projected.
#[derive(Debug, Clone, uniffi::Record)]
pub struct DmMessageSummary {
    pub message_id: String,
    pub seq: u64,
    pub sender_identity_id: String,
    pub sender_device_id: String,
    pub sender_account_id: String,
    pub body: String,
    pub sent_at_micros: i64,
    pub edited: bool,
    pub deleted: bool,
    pub mine: bool,
}

impl From<app::DmMessageSummary> for DmMessageSummary {
    fn from(m: app::DmMessageSummary) -> Self {
        Self {
            message_id: m.message_id,
            seq: m.seq,
            sender_identity_id: m.sender_identity_id,
            sender_device_id: m.sender_device_id,
            sender_account_id: m.sender_account_id,
            body: m.body,
            sent_at_micros: m.sent_at_micros,
            edited: m.edited,
            deleted: m.deleted,
            mine: m.mine,
        }
    }
}

/// What a push should say, decided on the device.
#[derive(Debug, Clone, uniffi::Record)]
pub struct NotificationContent {
    pub title: String,
    pub body: String,
    pub channel_id: String,
    pub kind: String,
}

impl From<app::NotificationContent> for NotificationContent {
    fn from(c: app::NotificationContent) -> Self {
        Self {
            title: c.title,
            body: c.body,
            channel_id: c.channel_id,
            kind: c.kind,
        }
    }
}

/// A personal conversation as this device can read it.
#[derive(Debug, Clone, uniffi::Record)]
pub struct DmLogSummary {
    pub messages: Vec<DmMessageSummary>,
    pub before_join: u32,
    pub undecryptable: u32,
}

impl From<app::DmLogSummary> for DmLogSummary {
    fn from(l: app::DmLogSummary) -> Self {
        Self {
            messages: l.messages.into_iter().map(Into::into).collect(),
            before_join: l.before_join,
            undecryptable: l.undecryptable,
        }
    }
}

/// What opening a conversation found or did.
#[derive(Debug, Clone, uniffi::Record)]
pub struct DmOpened {
    pub channel_id: String,
    pub existed: bool,
    pub member: bool,
    pub creating: bool,
}

impl From<app::DmOpened> for DmOpened {
    fn from(o: app::DmOpened) -> Self {
        Self {
            channel_id: o.channel_id,
            existed: o.existed,
            member: o.member,
            creating: o.creating,
        }
    }
}

/// The answer to a DM request: the opened conversation when accepted,
/// otherwise the request as it now stands.
#[derive(Debug, Clone, uniffi::Enum)]
pub enum DmResponse {
    Opened { opened: DmOpened },
    Answered { request: DmRequestSummary },
}

/// What the gateway loop and the DM engine report, in order.
#[derive(Debug, Clone, uniffi::Enum)]
pub enum ClientEvent {
    Welcomed {
        account_id: String,
    },
    Applied {
        channel_id: String,
        events: Vec<EventSummary>,
    },
    AccountApplied {
        count: u64,
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
    DmNote {
        message: String,
    },
    DmError {
        message: String,
    },
    DmSubscribed {
        channel_id: String,
    },
    DmOperation {
        channel_id: String,
        seq: u64,
        sender_identity_id: String,
        sender_device_id: String,
    },
    DmReady {
        channel_id: String,
    },
    DmDelivered {
        operation_id: String,
    },
    PresenceDiff {
        server_id: String,
        joined: Vec<PresenceSummary>,
        updated: Vec<PresenceSummary>,
        left_account_ids: Vec<String>,
    },
}

/// The identity's signed profile, as this device keeps it.
#[derive(Debug, Clone, uniffi::Record)]
pub struct StatusSummary {
    pub status: String,
    pub custom_status: String,
}

impl From<app::StatusSummary> for StatusSummary {
    fn from(s: app::StatusSummary) -> Self {
        Self {
            status: s.status,
            custom_status: s.custom_status,
        }
    }
}

/// The identity's signed profile, as this device keeps it.
#[derive(Debug, Clone, uniffi::Record)]
pub struct ProfileSummary {
    pub version: u64,
    pub display_name: String,
    pub pronouns: String,
    pub bio: String,
    pub avatar_url: String,
}

impl From<app::ProfileSummary> for ProfileSummary {
    fn from(p: app::ProfileSummary) -> Self {
        Self {
            version: p.version,
            display_name: p.display_name,
            pronouns: p.pronouns,
            bio: p.bio,
            avatar_url: p.avatar_url,
        }
    }
}

/// A WebAuthn ceremony's start: the challenge's id and the options JSON.
#[derive(Debug, Clone, uniffi::Record)]
pub struct PasskeyChallengeSummary {
    pub challenge_id: String,
    pub options_json: String,
}

impl From<app::PasskeyChallengeSummary> for PasskeyChallengeSummary {
    fn from(c: app::PasskeyChallengeSummary) -> Self {
        Self {
            challenge_id: c.challenge_id,
            options_json: c.options_json,
        }
    }
}

/// One of the account's passkeys.
#[derive(Debug, Clone, uniffi::Record)]
pub struct PasskeySummary {
    pub passkey_id: String,
    pub label: String,
    pub created_at_micros: Option<i64>,
    pub last_used_at_micros: Option<i64>,
}

impl From<app::PasskeySummary> for PasskeySummary {
    fn from(p: app::PasskeySummary) -> Self {
        Self {
            passkey_id: p.passkey_id,
            label: p.label,
            created_at_micros: p.created_at_micros,
            last_used_at_micros: p.last_used_at_micros,
        }
    }
}

/// One account's presence in a server.
#[derive(Debug, Clone, uniffi::Record)]
pub struct PresenceSummary {
    pub account_id: String,
    pub status: String,
    pub custom_status: String,
}

impl From<app::PresenceSummary> for PresenceSummary {
    fn from(p: app::PresenceSummary) -> Self {
        Self {
            account_id: p.account_id,
            status: p.status,
            custom_status: p.custom_status,
        }
    }
}

impl From<app::AppEvent> for ClientEvent {
    fn from(e: app::AppEvent) -> Self {
        use app::AppEvent as E;
        match e {
            E::Welcomed { account_id } => Self::Welcomed { account_id },
            E::Applied { channel_id, events } => Self::Applied {
                channel_id,
                events: events.into_iter().map(Into::into).collect(),
            },
            E::AccountApplied { count } => Self::AccountApplied {
                count: count as u64,
            },
            E::Acked { command_id, seq } => Self::Acked { command_id, seq },
            E::Error {
                code,
                message,
                command_id,
            } => Self::Error {
                code,
                message,
                command_id,
            },
            E::SubscriptionFailed {
                channel_id,
                message,
            } => Self::SubscriptionFailed {
                channel_id,
                message,
            },
            E::SnapshotTaken { channel_id, seq } => Self::SnapshotTaken { channel_id, seq },
            E::Disconnected {
                reason,
                reconnect_after_seconds,
            } => Self::Disconnected {
                reason,
                reconnect_after_seconds,
            },
            E::Reconnecting {
                attempt,
                delay_millis,
                reason,
            } => Self::Reconnecting {
                attempt,
                delay_millis,
                reason,
            },
            E::Closed { reason } => Self::Closed { reason },
            E::DmNote { message } => Self::DmNote { message },
            E::DmError { message } => Self::DmError { message },
            E::DmSubscribed { channel_id } => Self::DmSubscribed { channel_id },
            E::DmOperation {
                channel_id,
                seq,
                sender_identity_id,
                sender_device_id,
            } => Self::DmOperation {
                channel_id,
                seq,
                sender_identity_id,
                sender_device_id,
            },
            E::DmReady { channel_id } => Self::DmReady { channel_id },
            E::DmDelivered { operation_id } => Self::DmDelivered { operation_id },
            E::PresenceDiff {
                server_id,
                joined,
                updated,
                left_account_ids,
            } => Self::PresenceDiff {
                server_id,
                joined: joined.into_iter().map(Into::into).collect(),
                updated: updated.into_iter().map(Into::into).collect(),
                left_account_ids,
            },
        }
    }
}

/// The shell's side of the gateway loop.
#[uniffi::export(callback_interface)]
pub trait Listener: Send + Sync {
    /// Called from a runtime thread for every event, in order.
    fn on_event(&self, event: ClientEvent);
}

// ---- The client ------------------------------------------------------------------

/// One instance's client: HTTP API, gateway loop, local store, identity,
/// personal DMs.
#[derive(uniffi::Object)]
pub struct Client {
    runtime: tokio::runtime::Runtime,
    app: Arc<App>,
}

impl std::fmt::Debug for Client {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.app.fmt(f)
    }
}

type Res<T> = Result<T, LumiaError>;

#[uniffi::export]
impl Client {
    /// Opens the client for the instance at `base_url` with state under
    /// `data_dir`: `<data_dir>/<host>/` for this instance and
    /// `<data_dir>/identity/` for the device's identity, shared by every
    /// instance opened from the same directory.
    ///
    /// # Errors
    /// When the directories or a store cannot be opened.
    #[uniffi::constructor]
    pub fn new(base_url: String, data_dir: String) -> Res<Arc<Self>> {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .map_err(|e| LumiaError::Other {
                detail: e.to_string(),
            })?;
        let host = app::host_of(&base_url);
        let data_dir = PathBuf::from(data_dir);
        let dir = data_dir.join(&host);
        let identity_dir = data_dir.join("identity");
        for d in [&dir, &identity_dir] {
            std::fs::create_dir_all(d).map_err(|e| LumiaError::Other {
                detail: e.to_string(),
            })?;
        }
        let lock_dir = dir.clone();
        let opener: Opener = Arc::new(move |name| {
            let path = match name {
                StoreName::Instance => dir.join("store.sqlite"),
                StoreName::Keys => dir.join("keys.sqlite"),
                StoreName::IdentityStore => identity_dir.join("store.sqlite"),
                StoreName::IdentityKeys => identity_dir.join("keys.sqlite"),
            };
            SqliteStore::open(&path)
        });
        // The single-writer lock with the notification extension sits
        // beside this instance's stores (architecture 15.5).
        let app = App::open_locked(
            &base_url,
            Place::Native,
            opener,
            &lock_dir.join("writer.lock"),
        )?;
        Ok(Arc::new(Self { runtime, app }))
    }

    /// The instance's host name (with the port when not the default).
    pub fn host(&self) -> String {
        self.app.host()
    }

    /// This device's id (UUID string).
    pub fn device_id(&self) -> String {
        self.app.device_id()
    }

    /// Whether an access token is held for this instance.
    pub fn signed_in(&self) -> bool {
        self.app.signed_in()
    }

    /// `GET /instance`: the trust card.
    pub fn instance_info(&self) -> Res<InstanceSummary> {
        Ok(self.runtime.block_on(self.app.instance_info())?.into())
    }

    /// Creates an account with a password and signs in; with an invite link
    /// or code, joins its server too (and is admitted when registration is closed).
    pub fn create_account(
        &self,
        handle: String,
        password: String,
        invite: Option<String>,
    ) -> Res<()> {
        Ok(self.runtime.block_on(
            self.app
                .create_account(&handle, &password, invite.as_deref()),
        )?)
    }

    /// `GET /invites/{code}`: what an invite link or code shows before joining. Public.
    pub fn invite_preview(&self, link_or_code: String) -> Res<InvitePreviewSummary> {
        Ok(self
            .runtime
            .block_on(self.app.invite_preview(&link_or_code))?
            .into())
    }

    /// `POST /invites`: an invite link for a server (0 uses is unlimited).
    pub fn create_invite(
        &self,
        server_id: String,
        max_uses: u32,
        expires_in_hours: Option<u32>,
    ) -> Res<InviteSummary> {
        Ok(self
            .runtime
            .block_on(
                self.app
                    .create_invite(&server_id, max_uses, expires_in_hours),
            )?
            .into())
    }

    /// `POST /invites/email`: a one-use invite good for a week, mailed to
    /// the address as a link (architecture 6.2).
    pub fn email_invite(&self, server_id: String, email: String) -> Res<InviteSummary> {
        Ok(self
            .runtime
            .block_on(self.app.email_invite(&server_id, &email))?
            .into())
    }

    /// `POST /invites/{code}/accept`: joins the invite's server; returns its id.
    pub fn accept_invite(&self, link_or_code: String) -> Res<String> {
        Ok(self
            .runtime
            .block_on(self.app.accept_invite(&link_or_code))?)
    }

    /// `GET /servers/{id}/invites`: the server's invites, newest first.
    pub fn invites(&self, server_id: String) -> Res<Vec<InviteSummary>> {
        Ok(self
            .runtime
            .block_on(self.app.invites(&server_id))?
            .into_iter()
            .map(Into::into)
            .collect())
    }

    /// `DELETE /invites/{code}`: revokes an invite.
    pub fn revoke_invite(&self, link_or_code: String) -> Res<()> {
        Ok(self
            .runtime
            .block_on(self.app.revoke_invite(&link_or_code))?)
    }

    /// The servers this client has on screen, whose presence it wants
    /// (protocol 6.7); sent with the chosen status.
    pub fn watch_presence(&self, watch_server_ids: Vec<String>) -> Res<()> {
        Ok(self.app.watch_presence(&watch_server_ids)?)
    }

    /// The status this person chose and their custom status text, kept
    /// across launches.
    pub fn status(&self) -> Res<StatusSummary> {
        Ok(self.app.status()?.into())
    }

    /// Chooses the status (`ONLINE`, `AWAY`, `DO_NOT_DISTURB`) and custom
    /// text; sent at once when the gateway is up, and on every welcome.
    pub fn set_status(&self, status: String, custom_status: String) -> Res<()> {
        Ok(self.app.set_status(&status, &custom_status)?)
    }

    /// `POST /servers`: a server this account owns.
    pub fn create_server(&self, name: String) -> Res<ServerSummary> {
        Ok(self.runtime.block_on(self.app.create_server(&name))?.into())
    }

    /// `POST /channels`: a text channel in a server this account may manage channels in.
    pub fn create_channel(&self, server_id: String, name: String) -> Res<ChannelSummary> {
        Ok(self
            .runtime
            .block_on(self.app.create_channel(&server_id, &name))?
            .into())
    }

    /// The creation options for a new passkey of this account (architecture 6.3).
    pub fn passkey_register_start(&self) -> Res<PasskeyChallengeSummary> {
        Ok(self
            .runtime
            .block_on(self.app.passkey_register_start())?
            .into())
    }

    /// The credential the platform made, kept under `label`.
    pub fn passkey_register_finish(
        &self,
        challenge_id: String,
        credential_json: String,
        label: String,
    ) -> Res<()> {
        Ok(self.runtime.block_on(self.app.passkey_register_finish(
            &challenge_id,
            &credential_json,
            &label,
        ))?)
    }

    /// The request options to sign in with a passkey, for a handle when given.
    pub fn passkey_assert_start(&self, handle_or_email: String) -> Res<PasskeyChallengeSummary> {
        Ok(self
            .runtime
            .block_on(self.app.passkey_assert_start(&handle_or_email))?
            .into())
    }

    /// The assertion signs this device in.
    pub fn sign_in_passkey(&self, challenge_id: String, credential_json: String) -> Res<()> {
        Ok(self
            .runtime
            .block_on(self.app.sign_in_passkey(&challenge_id, &credential_json))?)
    }

    /// This account's passkeys.
    pub fn passkeys(&self) -> Res<Vec<PasskeySummary>> {
        Ok(self
            .runtime
            .block_on(self.app.passkeys())?
            .into_iter()
            .map(Into::into)
            .collect())
    }

    /// Removes one of this account's passkeys.
    pub fn remove_passkey(&self, passkey_id: String) -> Res<()> {
        Ok(self
            .runtime
            .block_on(self.app.remove_passkey(&passkey_id))?)
    }

    /// Asks the instance to mail a sign-in link to `email`.
    pub fn request_magic_link(&self, email: String) -> Res<()> {
        Ok(self.runtime.block_on(self.app.request_magic_link(&email))?)
    }

    /// Signs in with the token from a magic-link mail (the code, or the whole link).
    pub fn sign_in_magic_link(&self, token: String) -> Res<()> {
        Ok(self.runtime.block_on(self.app.sign_in_magic_link(&token))?)
    }

    /// A one-time sign-in token for this account, for the instance's web
    /// page to sign in as it (a passkey is made there) or for another device.
    pub fn handoff_token(&self) -> Res<String> {
        Ok(self.runtime.block_on(self.app.handoff_token())?)
    }

    /// Where the system browser signs in with the instance's OIDC provider;
    /// it comes back to `return_to` with `magic={token}`.
    pub fn oidc_start_url(&self, return_to: String) -> String {
        self.app.oidc_start_url(&return_to)
    }

    /// Where the system browser signs in through the instance's web page
    /// (a passkey, say); it comes back to `return_to` with `magic={token}`.
    pub fn web_sign_in_url(&self, return_to: String) -> String {
        self.app.web_sign_in_url(&return_to)
    }

    /// Where the system browser lands signed in as this account by `token`
    /// (from `handoff_token`), on `next` (`devices` for the passkeys there).
    pub fn web_handoff_url(&self, token: String, next: String) -> String {
        self.app.web_handoff_url(&token, &next)
    }

    /// Signs in with a password, and a TOTP code when the account requires one.
    pub fn sign_in(&self, handle: String, password: String, totp_code: Option<String>) -> Res<()> {
        Ok(self
            .runtime
            .block_on(self.app.sign_in(&handle, &password, totp_code.as_deref()))?)
    }

    /// Signs out: revokes the session and forgets the tokens.
    pub fn sign_out(&self) -> Res<()> {
        Ok(self.runtime.block_on(self.app.sign_out())?)
    }

    /// `GET /account/snapshot`.
    pub fn snapshot(&self) -> Res<SnapshotSummary> {
        Ok(self.runtime.block_on(self.app.snapshot())?.into())
    }

    /// `GET /accounts/{id}`: a public profile, when visible.
    pub fn account(&self, account_id: String) -> Res<MemberSummary> {
        Ok(self.runtime.block_on(self.app.account(&account_id))?.into())
    }

    /// `GET /servers/{id}/members`.
    pub fn members(&self, server_id: String, query: String) -> Res<Vec<MemberSummary>> {
        Ok(self
            .runtime
            .block_on(self.app.members(&server_id, &query))?
            .into_iter()
            .map(Into::into)
            .collect())
    }

    /// The newest messages of a channel from the local store, oldest first.
    pub fn messages(&self, channel_id: String, limit: u32) -> Res<Vec<MessageSummary>> {
        Ok(self
            .app
            .messages(&channel_id, limit)?
            .into_iter()
            .map(Into::into)
            .collect())
    }

    /// Starts the gateway loop; `listener` gets every event, in order,
    /// from a runtime thread, until `Closed` or `stop`.
    pub fn start(&self, channel_ids: Vec<String>, listener: Box<dyn Listener>) -> Res<()> {
        let listener: Arc<dyn Listener> = Arc::from(listener);
        let sink: EventSink = Arc::new(move |event| listener.on_event(event.into()));
        let app = Arc::clone(&self.app);
        // Spawning happens on the client's runtime.
        let _guard = self.runtime.enter();
        Ok(app.start(&channel_ids, sink)?)
    }

    /// Stops the gateway loop.
    pub fn stop(&self) {
        self.app.stop();
    }

    /// Subscribes to a channel from the store's cursor.
    pub fn subscribe(&self, channel_id: String) -> Res<()> {
        Ok(self.app.subscribe(&channel_id)?)
    }

    /// Sends a message; returns the client-minted message id.
    pub fn send_message(&self, channel_id: String, body: String) -> Res<String> {
        Ok(self.app.send_message(&channel_id, &body)?)
    }

    /// The app came back to the foreground: ask for every head.
    pub fn foreground(&self) -> Res<()> {
        Ok(self.app.foreground()?)
    }

    /// The app is going to the background: releases the single-writer lock
    /// so the notification extension can take it.
    pub fn background(&self) {
        self.app.background();
    }

    /// The app is back in the foreground: takes the single-writer lock
    /// again; true when the extension wrote meanwhile, so the gateway loop
    /// must be stopped and started to resume from the store.
    pub fn resume(&self) -> Res<bool> {
        Ok(self.app.resume()?)
    }

    /// For the notification extension: what the push should say, after
    /// syncing and decrypting under the exclusive writer lock (tried for
    /// `timeout_ms`); nil when the app holds the lock or nothing new was
    /// readable, so the generic notice stands.
    pub fn notification_content(&self, timeout_ms: u64) -> Res<Option<NotificationContent>> {
        Ok(self
            .runtime
            .block_on(
                self.app
                    .notification_content(std::time::Duration::from_millis(timeout_ms)),
            )?
            .map(Into::into))
    }

    // ---- Identity ----

    /// Whether this device holds an identity roster.
    pub fn has_identity(&self) -> bool {
        self.app.has_identity()
    }

    /// Creates a new identity on this device; the phrase is shown once.
    pub fn identity_create(&self, label: String) -> Res<IdentityCreated> {
        let (identity_id, phrase) = self.runtime.block_on(self.app.identity_create(&label))?;
        Ok(IdentityCreated {
            identity_id,
            phrase,
        })
    }

    /// Binds the signed-in password account to this device's identity.
    /// The identity's signed profile, if one was set on this device.
    pub fn identity_profile(&self) -> Res<Option<ProfileSummary>> {
        Ok(self
            .runtime
            .block_on(self.app.identity_profile())?
            .map(Into::into))
    }

    /// Signs a new profile with this device's key and, when signed in, sends it.
    pub fn identity_set_profile(
        &self,
        display_name: String,
        pronouns: String,
        bio: String,
        avatar_url: String,
    ) -> Res<ProfileSummary> {
        Ok(self
            .runtime
            .block_on(
                self.app
                    .identity_set_profile(&display_name, &pronouns, &bio, &avatar_url),
            )?
            .into())
    }

    pub fn identity_link_account(&self, password: String) -> Res<()> {
        Ok(self
            .runtime
            .block_on(self.app.identity_link_account(&password))?)
    }

    /// Step 1 of linking this device: the request for another device to approve.
    pub fn identity_link_request(&self, label: String) -> Res<String> {
        Ok(self
            .runtime
            .block_on(self.app.identity_link_request(&label))?)
    }

    /// On a device that holds the identity: approves a link request.
    pub fn identity_approve(&self, request: String) -> Res<String> {
        Ok(self.runtime.block_on(self.app.identity_approve(&request))?)
    }

    /// Claims the bundle an approving device uploaded, then signs in.
    pub fn identity_claim(&self, code: String) -> Res<RosterSummary> {
        Ok(self
            .runtime
            .block_on(self.app.identity_claim(&code))?
            .into())
    }

    /// Signs in by identity proof with the roster this device holds.
    pub fn sign_in_identity(&self) -> Res<()> {
        Ok(self.runtime.block_on(self.app.sign_in_identity())?)
    }

    /// The roster this device holds.
    pub fn identity_show(&self) -> Res<RosterSummary> {
        Ok(self.runtime.block_on(self.app.identity_show())?.into())
    }

    /// Revokes another device, signed by this one with the password as reauth.
    pub fn identity_revoke(&self, device_id: String, password: String) -> Res<RosterSummary> {
        Ok(self
            .runtime
            .block_on(self.app.identity_revoke(&device_id, &password))?
            .into())
    }

    // ---- Push ----

    /// Registers this device's push token with the instance: `platform`
    /// is `apns` or `fcm` (through the relay) or `unified_push` (the
    /// distributor's endpoint, with its Web Push keys when it gave any,
    /// base64url; empty otherwise).
    pub fn push_subscribe(
        &self,
        platform: String,
        token: String,
        p256dh: String,
        auth: String,
    ) -> Res<()> {
        let kind = match platform.as_str() {
            "apns" => PushKind::RelayApns,
            "fcm" => PushKind::RelayFcm,
            "unified_push" => PushKind::UnifiedPush,
            other => {
                return Err(LumiaError::Other {
                    detail: format!("unknown platform {other}"),
                })
            }
        };
        Ok(self
            .runtime
            .block_on(self.app.push_subscribe(kind, &token, &p256dh, &auth))?)
    }

    /// Removes this device's push subscription of a platform (`apns`,
    /// `fcm` through the relay; `unified_push` served by the instance).
    pub fn push_unsubscribe(&self, platform: String) -> Res<()> {
        let kind = match platform.as_str() {
            "unified_push" => platform,
            _ => format!("relay_{platform}"),
        };
        Ok(self.runtime.block_on(self.app.push_unsubscribe(&kind))?)
    }

    // ---- Personal DMs ----

    /// Whether personal DMs work on this device: it holds an identity.
    pub fn dm_available(&self) -> bool {
        self.app.dm_available()
    }

    /// The personal conversations this device knows.
    pub fn dm_channels(&self) -> Res<Vec<DmChannelSummary>> {
        Ok(self
            .runtime
            .block_on(self.app.dm_channels())?
            .into_iter()
            .map(Into::into)
            .collect())
    }

    /// Opens (or finds) the conversation with `account_ids`.
    pub fn dm_open(&self, account_ids: Vec<String>) -> Res<DmOpened> {
        Ok(self
            .runtime
            .block_on(self.app.dm_open(&account_ids))?
            .into())
    }

    /// Sends a DM request to an account.
    pub fn dm_request(&self, account_id: String) -> Res<DmRequestSummary> {
        Ok(self
            .runtime
            .block_on(self.app.dm_request(&account_id))?
            .into())
    }

    /// Answers a DM request: `accept`, `decline` or `block`.
    pub fn dm_respond(&self, request_id: String, response: String) -> Res<DmResponse> {
        Ok(
            match self
                .runtime
                .block_on(self.app.dm_respond(&request_id, &response))?
            {
                Ok(opened) => DmResponse::Opened {
                    opened: opened.into(),
                },
                Err(request) => DmResponse::Answered {
                    request: request.into(),
                },
            },
        )
    }

    /// Sends a message in a personal DM.
    pub fn dm_send(&self, channel_id: String, body: String) -> Res<()> {
        Ok(self
            .runtime
            .block_on(self.app.dm_send(&channel_id, &body))?)
    }

    /// Deletes one of this identity's messages in a personal DM.
    pub fn dm_delete(&self, channel_id: String, message_id: String) -> Res<()> {
        Ok(self
            .runtime
            .block_on(self.app.dm_delete(&channel_id, &message_id))?)
    }

    /// Rejoins a conversation by external commit.
    pub fn dm_rejoin(&self, channel_id: String) -> Res<()> {
        Ok(self.runtime.block_on(self.app.dm_rejoin(&channel_id))?)
    }

    /// The conversation as this device can read it.
    pub fn dm_messages(&self, channel_id: String) -> Res<DmLogSummary> {
        Ok(self
            .runtime
            .block_on(self.app.dm_messages(&channel_id))?
            .into())
    }
}
