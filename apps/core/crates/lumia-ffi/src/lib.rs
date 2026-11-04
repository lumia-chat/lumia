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
}

impl From<app::MemberSummary> for MemberSummary {
    fn from(m: app::MemberSummary) -> Self {
        Self {
            account_id: m.account_id,
            handle: m.handle,
            display_name: m.display_name,
            nickname: m.nickname,
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

    /// Creates an account with a password and signs in.
    pub fn create_account(&self, handle: String, password: String) -> Res<()> {
        Ok(self
            .runtime
            .block_on(self.app.create_account(&handle, &password))?)
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

    /// Registers this device's push token with the instance, to be
    /// reached through the relay: `platform` is `apns` or `fcm`.
    pub fn push_subscribe(&self, platform: String, token: String) -> Res<()> {
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
            .block_on(self.app.push_subscribe(kind, &token, "", ""))?)
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
