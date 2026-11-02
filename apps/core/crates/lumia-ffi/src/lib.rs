//! UniFFI surface for the native shells (architecture section 13, plan
//! step 3 milestone 3): one [`Client`] per instance and a [`Listener`] the
//! shell implements to receive what the gateway loop produces.
//!
//! Marshalling only. Every method is synchronous and blocking, meant to be
//! called from a background thread on the shell's side; the gateway loop
//! runs on the client's own tokio runtime and reports through the listener
//! from a runtime thread. Ids cross the boundary as UUID strings and times
//! as microseconds since the epoch.
//!
//! The store and keystore here are the development ones from
//! `lumia-client` (SQLite on disk, secrets in plain files at mode 0600).
//! The shells supply platform secret stores when they arrive (plan steps 6
//! and 7); until then this surface is for wiring and testing a shell, not
//! for shipping one.

use std::path::PathBuf;
use std::sync::{Arc, Mutex, PoisonError};

use lumia_client::api::{Api, Tokens};
use lumia_client::futures_channel::mpsc;
use lumia_client::futures_util::StreamExt as _;
use lumia_client::gateway::{self, GatewayCommand, GatewayEvent};
use lumia_client::identity::Device;
use lumia_client::keystore::FileKeystore;
use lumia_client::sqlite::SqliteStore;
use lumia_client::ClientError;
use lumia_core::keystore::Keystore;
use lumia_core::proto::{command, event, Command, Event, SendMessage};
use lumia_core::store::{Store, StoredMessage};
use lumia_core::sync::Notice;
use lumia_core::wire;

uniffi::setup_scaffolding!();

/// Why a call failed. The shell gets the message; the variant names the
/// cause a caller can act on.
#[derive(Debug, thiserror::Error, uniffi::Error)]
#[uniffi(flat_error)]
pub enum LumiaError {
    /// The instance answered with the protocol's error envelope.
    #[error("{code}: {message}")]
    Api {
        /// `lumia.v1.ErrorCode` as its name.
        code: String,
        /// The server's message.
        message: String,
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
    /// The gateway loop is not running, or is already running.
    #[error("{0}")]
    Lifecycle(String),
    /// A malformed argument, such as an id that is not a UUID.
    #[error("{0}")]
    Argument(String),
    /// Transport, store, keystore, or identity failure.
    #[error("{0}")]
    Other(String),
}

impl From<ClientError> for LumiaError {
    fn from(e: ClientError) -> Self {
        match e {
            ClientError::Api { code, message, .. } => Self::Api { code, message },
            ClientError::DeviceRevoked { roster_version } => Self::DeviceRevoked { roster_version },
            ClientError::NotSignedIn => Self::NotSignedIn,
            other => Self::Other(other.to_string()),
        }
    }
}

/// A server the account belongs to.
#[derive(Debug, Clone, uniffi::Record)]
pub struct ServerSummary {
    /// UUID string.
    pub server_id: String,
    /// Display name.
    pub name: String,
}

/// A channel the account can view.
#[derive(Debug, Clone, uniffi::Record)]
pub struct ChannelSummary {
    /// UUID string.
    pub channel_id: String,
    /// Display name, without the `#`.
    pub name: String,
    /// The log's head when listed.
    pub last_seq: u64,
}

/// A message as the local store holds it.
#[derive(Debug, Clone, uniffi::Record)]
pub struct MessageSummary {
    /// UUID string (also the create command's id).
    pub message_id: String,
    /// UUID string.
    pub channel_id: String,
    /// Sequence number of its create event.
    pub seq: u64,
    /// UUID string.
    pub author_account_id: String,
    /// Current body; empty when deleted.
    pub body: String,
    /// Microseconds since the epoch.
    pub created_at_micros: i64,
    /// Whether it was deleted.
    pub deleted: bool,
}

/// One applied channel event.
#[derive(Debug, Clone, uniffi::Record)]
pub struct EventSummary {
    /// Position in the channel log.
    pub seq: u64,
    /// The payload case: `message_created`, `message_edited`, `redacted`, ...
    pub kind: String,
    /// For `message_created`, the message.
    pub message: Option<MessageSummary>,
}

/// What `identity_create` returns: the phrase to show once.
#[derive(Debug, Clone, uniffi::Record)]
pub struct IdentityCreated {
    /// The 24 words. Shown once; never stored by the core.
    pub phrase: String,
    /// The identity id, hex.
    pub identity_id: String,
}

/// A device in the roster.
#[derive(Debug, Clone, uniffi::Record)]
pub struct DeviceSummary {
    /// UUID string.
    pub device_id: String,
    /// Human label from the certificate; not signed.
    pub label: String,
    /// Whether this is the calling device.
    pub this_device: bool,
    /// Whether the entry is a revocation.
    pub revoked: bool,
}

/// The roster this device holds.
#[derive(Debug, Clone, uniffi::Record)]
pub struct RosterSummary {
    /// The identity id, hex.
    pub identity_id: String,
    /// Roster version.
    pub version: u64,
    /// Whether the roster carries a valid root signature.
    pub root_signed: bool,
    /// Current and revoked devices.
    pub devices: Vec<DeviceSummary>,
}

/// What the gateway loop reports, in order.
#[derive(Debug, Clone, uniffi::Enum)]
pub enum ClientEvent {
    /// The handshake completed.
    Welcomed {
        /// UUID string.
        account_id: String,
    },
    /// Channel events were applied to the store.
    Applied {
        /// UUID string.
        channel_id: String,
        /// In `seq` order.
        events: Vec<EventSummary>,
    },
    /// Account events were applied to the store.
    AccountApplied {
        /// How many.
        count: u32,
    },
    /// A command was acknowledged.
    Acked {
        /// UUID string.
        command_id: String,
        /// Its event's sequence number.
        seq: u64,
    },
    /// The server reported an error.
    Error {
        /// `lumia.v1.ErrorCode` as its name.
        code: String,
        /// The server's message.
        message: String,
    },
    /// A subscription failed and was dropped.
    SubscriptionFailed {
        /// UUID string.
        channel_id: String,
        /// The server's reason, when given.
        message: String,
    },
    /// A channel took the snapshot path up to `seq`.
    SnapshotTaken {
        /// UUID string.
        channel_id: String,
        /// The snapshot's sequence number.
        seq: u64,
    },
    /// The server is closing the connection.
    Disconnected {
        /// `lumia.v1.DisconnectReason` as its name.
        reason: String,
        /// Seconds before reconnecting is welcome.
        reconnect_after_seconds: u32,
    },
    /// The connection dropped; the loop retries after `delay_millis`.
    Reconnecting {
        /// Consecutive failed attempts so far.
        attempt: u32,
        /// The wait before this attempt.
        delay_millis: u64,
        /// Why the last connection ended.
        reason: String,
    },
    /// The loop ended for good.
    Closed {
        /// Why.
        reason: String,
    },
}

/// The shell's side of the gateway loop.
#[uniffi::export(callback_interface)]
pub trait Listener: Send + Sync {
    /// Called from a runtime thread for every event, in order.
    fn on_event(&self, event: ClientEvent);
}

struct Inner {
    /// Owned here between runs; moved into the gateway loop while it runs.
    store: Option<SqliteStore>,
    /// A second connection for reads while the loop owns the first.
    reader: SqliteStore,
    keystore: FileKeystore,
    identity_store: SqliteStore,
    identity_keys: FileKeystore,
    commands: Option<mpsc::UnboundedSender<GatewayCommand>>,
    task: Option<tokio::task::JoinHandle<SqliteStore>>,
}

/// One instance's client: HTTP API, gateway loop, local store, identity.
#[derive(uniffi::Object)]
pub struct Client {
    runtime: tokio::runtime::Runtime,
    api: Api,
    host: String,
    device_id: Vec<u8>,
    inner: Mutex<Inner>,
}

impl std::fmt::Debug for Client {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Client")
            .field("host", &self.host)
            .field("device_id", &wire::display(&self.device_id))
            .finish_non_exhaustive()
    }
}

#[uniffi::export]
impl Client {
    /// Opens the client for the instance at `base_url` with state under
    /// `data_dir`: `<data_dir>/<host>/` for this instance and
    /// `<data_dir>/identity/` for the device's identity, shared by every
    /// instance opened from the same directory.
    ///
    /// # Errors
    /// When the directories, store, or keystore cannot be opened.
    #[uniffi::constructor]
    pub fn new(base_url: String, data_dir: String) -> Result<Arc<Self>, LumiaError> {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .map_err(|e| LumiaError::Other(e.to_string()))?;
        let host = host_of(&base_url);
        let data_dir = PathBuf::from(data_dir);
        let dir = data_dir.join(&host);
        let identity_dir = data_dir.join("identity");
        for d in [&dir, &identity_dir] {
            std::fs::create_dir_all(d).map_err(|e| LumiaError::Other(e.to_string()))?;
        }
        let mut store = SqliteStore::open(&dir.join("store.sqlite")).map_err(other)?;
        store
            .meta_set("base_url", base_url.trim_end_matches('/'))
            .map_err(other)?;
        let reader = SqliteStore::open(&dir.join("store.sqlite")).map_err(other)?;
        let keys_dir = dir.join("keys");
        let keystore = FileKeystore::open(&keys_dir).map_err(other)?;
        let identity_store = SqliteStore::open(&identity_dir.join("meta.sqlite")).map_err(other)?;
        let mut identity_keys = FileKeystore::open(&identity_dir.join("keys")).map_err(other)?;
        let device_id = match identity_keys.get("device_id").map_err(other)? {
            Some(id) => id,
            None => {
                let id = wire::new_id();
                identity_keys.put("device_id", &id).map_err(other)?;
                id
            }
        };
        let api = Api::new(&base_url)
            .with_tokens(tokens(&keystore)?)
            .on_rotate(Arc::new(move |pair| {
                if let Ok(mut keys) = FileKeystore::open(&keys_dir) {
                    let _ = keys.put("access_token", pair.access_token.as_bytes());
                    let _ = keys.put("refresh_token", pair.refresh_token.as_bytes());
                }
            }));
        Ok(Arc::new(Self {
            runtime,
            api,
            host,
            device_id,
            inner: Mutex::new(Inner {
                store: Some(store),
                reader,
                keystore,
                identity_store,
                identity_keys,
                commands: None,
                task: None,
            }),
        }))
    }

    /// The instance host this client talks to.
    pub fn host(&self) -> String {
        self.host.clone()
    }

    /// This device's id as a UUID string.
    pub fn device_id(&self) -> String {
        wire::display(&self.device_id)
    }

    /// Whether an access token is on hand.
    pub fn signed_in(&self) -> bool {
        self.api.tokens().access.is_some()
    }

    // ---- Accounts and sessions ----------------------------------------------------

    /// Creates a password account and signs in.
    ///
    /// # Errors
    /// On API failure, such as `ALREADY_EXISTS`.
    pub fn create_account(&self, handle: String, password: String) -> Result<(), LumiaError> {
        let pair = self
            .runtime
            .block_on(self.api.create_account(&handle, &password))?;
        self.save_tokens(&pair)
    }

    /// Signs in with a password.
    ///
    /// # Errors
    /// On API failure, such as `UNAUTHENTICATED`.
    pub fn sign_in(&self, handle: String, password: String) -> Result<(), LumiaError> {
        let pair =
            self.runtime
                .block_on(self.api.sign_in(&handle, &password, self.device_id.clone()))?;
        self.save_tokens(&pair)
    }

    /// Creates an account bound to this device's identity and signs in.
    ///
    /// # Errors
    /// On API failure, or [`LumiaError::DeviceRevoked`].
    pub fn create_account_by_identity(&self, handle: String) -> Result<(), LumiaError> {
        let mut inner = self.lock();
        let pair = self
            .runtime
            .block_on(device(&mut inner, &self.device_id).create_account(&self.api, &handle))?;
        drop(inner);
        self.save_tokens(&pair)
    }

    /// Signs in by identity proof.
    ///
    /// # Errors
    /// On API failure, or [`LumiaError::DeviceRevoked`].
    pub fn sign_in_identity(&self) -> Result<(), LumiaError> {
        let mut inner = self.lock();
        let pair = self
            .runtime
            .block_on(device(&mut inner, &self.device_id).sign_in(&self.api))?;
        drop(inner);
        self.save_tokens(&pair)
    }

    // ---- Servers and channels -----------------------------------------------------

    /// Servers the account belongs to.
    ///
    /// # Errors
    /// On API failure.
    pub fn servers(&self) -> Result<Vec<ServerSummary>, LumiaError> {
        let list = self.runtime.block_on(self.api.servers())?;
        Ok(list
            .servers
            .into_iter()
            .map(|s| ServerSummary {
                server_id: wire::display(&s.server_id),
                name: s.name,
            })
            .collect())
    }

    /// Creates a server the account owns.
    ///
    /// # Errors
    /// On API failure.
    pub fn create_server(&self, name: String) -> Result<ServerSummary, LumiaError> {
        let s = self.runtime.block_on(self.api.create_server(&name))?;
        Ok(ServerSummary {
            server_id: wire::display(&s.server_id),
            name: s.name,
        })
    }

    /// Channels in a server the account can view.
    ///
    /// # Errors
    /// On API failure or a malformed id.
    pub fn channels(&self, server_id: String) -> Result<Vec<ChannelSummary>, LumiaError> {
        parse_uuid(&server_id)?;
        let list = self.runtime.block_on(self.api.channels(&server_id))?;
        Ok(list
            .channels
            .into_iter()
            .map(|c| ChannelSummary {
                channel_id: wire::display(&c.channel_id),
                name: c.name,
                last_seq: c.last_seq,
            })
            .collect())
    }

    /// Creates a channel in a server.
    ///
    /// # Errors
    /// On API failure or a malformed id.
    pub fn create_channel(
        &self,
        server_id: String,
        name: String,
    ) -> Result<ChannelSummary, LumiaError> {
        let server = parse_uuid(&server_id)?;
        let c = self
            .runtime
            .block_on(self.api.create_channel(server, &name))?;
        Ok(ChannelSummary {
            channel_id: wire::display(&c.channel_id),
            name: c.name,
            last_seq: c.last_seq,
        })
    }

    /// The newest messages the local store holds for a channel, oldest
    /// first, up to `limit`.
    ///
    /// # Errors
    /// On a store failure or a malformed id.
    pub fn messages(
        &self,
        channel_id: String,
        limit: u32,
    ) -> Result<Vec<MessageSummary>, LumiaError> {
        let channel = parse_uuid(&channel_id)?;
        let inner = self.lock();
        let rows = inner
            .reader
            .messages(&channel, limit as usize)
            .map_err(other)?;
        Ok(rows.iter().map(message_summary).collect())
    }

    // ---- Gateway loop ---------------------------------------------------------------

    /// Starts the gateway loop subscribed to `channel_ids`, reporting to
    /// `listener` until [`Client::stop`]. Reconnects on its own.
    ///
    /// # Errors
    /// When already started, not signed in, or an id is malformed.
    pub fn start(
        &self,
        channel_ids: Vec<String>,
        listener: Box<dyn Listener>,
    ) -> Result<(), LumiaError> {
        if !self.signed_in() {
            return Err(LumiaError::NotSignedIn);
        }
        let channels = channel_ids
            .iter()
            .map(|id| parse_uuid(id))
            .collect::<Result<Vec<_>, _>>()?;
        let mut inner = self.lock();
        let store = inner
            .store
            .take()
            .ok_or_else(|| LumiaError::Lifecycle("already started".into()))?;
        let (tx, rx) = mpsc::unbounded();
        let (etx, mut erx) = mpsc::unbounded();
        let api = self.api.clone();
        let device_id = self.device_id.clone();
        let task = self.runtime.spawn(async move {
            gateway::run_forever(api, store, "lumia-ffi", device_id, channels, rx, etx).await
        });
        self.runtime.spawn(async move {
            while let Some(ev) = erx.next().await {
                listener.on_event(client_event(ev));
            }
        });
        inner.commands = Some(tx);
        inner.task = Some(task);
        Ok(())
    }

    /// Stops the gateway loop and takes the store back.
    ///
    /// # Errors
    /// When not started.
    pub fn stop(&self) -> Result<(), LumiaError> {
        let mut inner = self.lock();
        let task = inner
            .task
            .take()
            .ok_or_else(|| LumiaError::Lifecycle("not started".into()))?;
        inner.commands = None;
        let store = self
            .runtime
            .block_on(task)
            .map_err(|e| LumiaError::Other(e.to_string()))?;
        inner.store = Some(store);
        Ok(())
    }

    /// Subscribes the running loop to a channel from the store's cursor.
    ///
    /// # Errors
    /// When not started or the id is malformed.
    pub fn subscribe(&self, channel_id: String) -> Result<(), LumiaError> {
        let channel = parse_uuid(&channel_id)?;
        self.send_command(GatewayCommand::Subscribe(channel))
    }

    /// Sends a message through the running loop; returns the command id,
    /// which `Acked` and the message's id will carry.
    ///
    /// # Errors
    /// When not started or the id is malformed.
    pub fn send_message(&self, channel_id: String, body: String) -> Result<String, LumiaError> {
        let channel = parse_uuid(&channel_id)?;
        let command_id = wire::new_id();
        self.send_command(GatewayCommand::Send(Command {
            command_id: command_id.clone(),
            channel_id: channel,
            op: Some(command::Op::SendMessage(SendMessage {
                body,
                ..Default::default()
            })),
        }))?;
        Ok(wire::display(&command_id))
    }

    /// Tells the running loop the app is in the foreground: it asks for
    /// every head.
    ///
    /// # Errors
    /// When not started.
    pub fn foreground(&self) -> Result<(), LumiaError> {
        self.send_command(GatewayCommand::Foreground)
    }

    // ---- Identity -------------------------------------------------------------------

    /// Creates a new identity on this device. The phrase is shown once.
    ///
    /// # Errors
    /// On a keystore or store failure.
    pub fn identity_create(&self, label: String) -> Result<IdentityCreated, LumiaError> {
        let mut inner = self.lock();
        let created = device(&mut inner, &self.device_id).create(&label)?;
        Ok(IdentityCreated {
            phrase: created.phrase,
            identity_id: created.identity_id,
        })
    }

    /// The roster this device holds.
    ///
    /// # Errors
    /// When there is no identity on this device.
    pub fn identity_show(&self) -> Result<RosterSummary, LumiaError> {
        let mut inner = self.lock();
        let verified = device(&mut inner, &self.device_id).roster()?;
        Ok(roster_summary(&verified, &self.device_id))
    }

    /// Binds the signed-in password account to this device's identity.
    ///
    /// # Errors
    /// On a wrong password or `CONFLICT`.
    pub fn identity_link_account(&self, password: String) -> Result<(), LumiaError> {
        let mut inner = self.lock();
        self.runtime
            .block_on(device(&mut inner, &self.device_id).link_account(&self.api, &password))?;
        Ok(())
    }

    /// On a new device: the link request to show as a QR, base64.
    ///
    /// # Errors
    /// On a keystore failure.
    pub fn identity_link_request(&self, label: String) -> Result<String, LumiaError> {
        let mut inner = self.lock();
        Ok(device(&mut inner, &self.device_id).link_request(&label)?)
    }

    /// On an existing device: approve a link request; returns the code.
    ///
    /// # Errors
    /// On a malformed request or API failure.
    pub fn identity_approve(&self, request: String) -> Result<String, LumiaError> {
        let mut inner = self.lock();
        Ok(self
            .runtime
            .block_on(device(&mut inner, &self.device_id).approve(&self.api, &request))?)
    }

    /// On the new device: claim the bundle by code and adopt the roster.
    ///
    /// # Errors
    /// On API failure, a bundle that does not open, or a roster that does
    /// not verify.
    pub fn identity_claim(&self, code: String) -> Result<RosterSummary, LumiaError> {
        let mut inner = self.lock();
        let verified = self
            .runtime
            .block_on(device(&mut inner, &self.device_id).claim(&self.api, &code))?;
        Ok(roster_summary(&verified, &self.device_id))
    }

    /// Revokes a device: root-signed with the phrase, or signed by this
    /// device with the account password as reauth. Exactly one of the two.
    ///
    /// # Errors
    /// On a wrong phrase or password, an unknown device, or `POLICY_DENIED`.
    pub fn identity_revoke(
        &self,
        device_id: String,
        phrase: Option<String>,
        password: Option<String>,
    ) -> Result<(), LumiaError> {
        let target = parse_uuid(&device_id)?;
        let mut inner = self.lock();
        let mut dev = device(&mut inner, &self.device_id);
        match (phrase, password) {
            (Some(phrase), _) => self
                .runtime
                .block_on(dev.revoke_with_phrase(&self.api, &phrase, &target))?,
            (None, Some(password)) => self
                .runtime
                .block_on(dev.revoke_with_device(&self.api, &password, &target))?,
            (None, None) => return Err(LumiaError::Argument("phrase or password".into())),
        }
        Ok(())
    }

    /// Publishes the roster this device holds to this instance.
    ///
    /// # Errors
    /// On API failure, including `POLICY_DENIED` for a roster that adds
    /// revocations without being root-signed.
    pub fn identity_publish(&self) -> Result<(), LumiaError> {
        let mut inner = self.lock();
        let verified = device(&mut inner, &self.device_id).roster()?;
        self.runtime
            .block_on(self.api.publish_roster(verified.signed))?;
        Ok(())
    }
}

impl Client {
    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn save_tokens(&self, pair: &lumia_core::proto::TokenPair) -> Result<(), LumiaError> {
        self.api.set_tokens(pair);
        let mut inner = self.lock();
        inner
            .keystore
            .put("access_token", pair.access_token.as_bytes())
            .map_err(other)?;
        inner
            .keystore
            .put("refresh_token", pair.refresh_token.as_bytes())
            .map_err(other)?;
        Ok(())
    }

    fn send_command(&self, command: GatewayCommand) -> Result<(), LumiaError> {
        let tx = self
            .lock()
            .commands
            .clone()
            .ok_or_else(|| LumiaError::Lifecycle("not started".into()))?;
        tx.unbounded_send(command)
            .map_err(|_| LumiaError::Lifecycle("the gateway loop has stopped".into()))
    }
}

fn device<'a>(inner: &'a mut Inner, device_id: &[u8]) -> Device<'a, SqliteStore, FileKeystore> {
    Device {
        store: &mut inner.identity_store,
        keystore: &mut inner.identity_keys,
        device_id: device_id.to_vec(),
    }
}

fn client_event(ev: GatewayEvent) -> ClientEvent {
    match ev {
        GatewayEvent::Applied(events) => ClientEvent::Applied {
            channel_id: events
                .first()
                .map(|e| wire::display(&e.channel_id))
                .unwrap_or_default(),
            events: events.iter().map(event_summary).collect(),
        },
        GatewayEvent::AccountApplied(events) => ClientEvent::AccountApplied {
            count: u32::try_from(events.len()).unwrap_or(u32::MAX),
        },
        GatewayEvent::Notice(Notice::Welcomed(w)) => ClientEvent::Welcomed {
            account_id: wire::display(&w.account_id),
        },
        GatewayEvent::Notice(Notice::Acked { command_id, seq }) => ClientEvent::Acked {
            command_id: wire::display(&command_id),
            seq,
        },
        GatewayEvent::Notice(Notice::Error(e)) => ClientEvent::Error {
            code: error_code(e.code),
            message: e.message,
        },
        GatewayEvent::Notice(Notice::SubscriptionFailed { channel_id, error }) => {
            ClientEvent::SubscriptionFailed {
                channel_id: wire::display(&channel_id),
                message: error.map(|e| e.message).unwrap_or_default(),
            }
        }
        GatewayEvent::Notice(Notice::Disconnected(d)) => ClientEvent::Disconnected {
            reason: lumia_core::proto::DisconnectReason::try_from(d.reason)
                .map(|r| r.as_str_name().to_string())
                .unwrap_or_else(|_| "UNKNOWN".into()),
            reconnect_after_seconds: d.reconnect_after_seconds,
        },
        GatewayEvent::SnapshotTaken { channel_id, seq } => ClientEvent::SnapshotTaken {
            channel_id: wire::display(&channel_id),
            seq,
        },
        GatewayEvent::Reconnecting {
            attempt,
            delay,
            reason,
        } => ClientEvent::Reconnecting {
            attempt,
            delay_millis: u64::try_from(delay.as_millis()).unwrap_or(u64::MAX),
            reason,
        },
        GatewayEvent::Closed(reason) => ClientEvent::Closed { reason },
    }
}

fn event_summary(e: &Event) -> EventSummary {
    let (kind, message) = match &e.payload {
        Some(event::Payload::MessageCreated(c)) => (
            "message_created",
            c.message.as_ref().map(|m| MessageSummary {
                message_id: wire::display(&m.message_id),
                channel_id: wire::display(&m.channel_id),
                seq: m.seq,
                author_account_id: wire::display(&m.author_account_id),
                body: m.body.clone(),
                created_at_micros: wire::micros(m.created_at.as_ref()).unwrap_or(0),
                deleted: m.deleted_at.is_some(),
            }),
        ),
        Some(event::Payload::MessageEdited(_)) => ("message_edited", None),
        Some(event::Payload::MessageDeleted(_)) => ("message_deleted", None),
        Some(event::Payload::Redacted(_)) => ("redacted", None),
        Some(_) => ("other", None),
        None => ("empty", None),
    };
    EventSummary {
        seq: e.seq,
        kind: kind.to_string(),
        message,
    }
}

fn message_summary(m: &StoredMessage) -> MessageSummary {
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

fn roster_summary(
    verified: &lumia_core::identity::roster::Verified,
    this_device: &[u8],
) -> RosterSummary {
    let mut devices: Vec<DeviceSummary> = verified
        .devices
        .keys()
        .map(|id| DeviceSummary {
            device_id: wire::display(id),
            label: verified
                .body
                .devices
                .iter()
                .find(|d| d.device_id == *id)
                .map(|d| d.label.clone())
                .unwrap_or_default(),
            this_device: id == this_device,
            revoked: false,
        })
        .collect();
    devices.extend(verified.revoked.keys().map(|id| DeviceSummary {
        device_id: wire::display(id),
        label: String::new(),
        this_device: id == this_device,
        revoked: true,
    }));
    RosterSummary {
        identity_id: hex_encode(&verified.identity_id),
        version: verified.version,
        root_signed: verified.root_signed,
        devices,
    }
}

fn error_code(code: i32) -> String {
    lumia_core::proto::ErrorCode::try_from(code)
        .map(|c| c.as_str_name().to_string())
        .unwrap_or_else(|_| "UNKNOWN".into())
}

fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn tokens(keys: &FileKeystore) -> Result<Tokens, LumiaError> {
    let read = |name: &str| -> Result<Option<String>, LumiaError> {
        Ok(keys
            .get(name)
            .map_err(other)?
            .and_then(|b| String::from_utf8(b).ok()))
    };
    Ok(Tokens {
        access: read("access_token")?,
        refresh: read("refresh_token")?,
    })
}

fn parse_uuid(s: &str) -> Result<Vec<u8>, LumiaError> {
    uuid::Uuid::parse_str(s)
        .map(wire::uuid_bytes)
        .map_err(|_| LumiaError::Argument(format!("not a uuid: {s}")))
}

fn other<E: std::fmt::Display>(e: E) -> LumiaError {
    LumiaError::Other(e.to_string())
}

/// `http://127.0.0.1:4000` becomes `127.0.0.1_4000`, the same directory
/// name `lumia-cli` uses, so one data directory serves both.
fn host_of(url: &str) -> String {
    let rest = url
        .split_once("://")
        .map_or(url, |(_, rest)| rest)
        .trim_end_matches('/');
    rest.replace([':', '/'], "_")
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> String {
        let dir = std::env::temp_dir().join(format!(
            "lumia-ffi-{name}-{}",
            wire::display(&wire::new_id())
        ));
        dir.to_string_lossy().to_string()
    }

    #[test]
    fn host_directory_matches_the_cli() {
        assert_eq!(host_of("http://127.0.0.1:4000"), "127.0.0.1_4000");
        assert_eq!(host_of("https://chat.example/"), "chat.example");
    }

    #[test]
    fn opens_offline_and_runs_the_identity_flows_that_need_no_instance() {
        let client = Client::new("http://127.0.0.1:1".into(), scratch("open")).unwrap();
        assert!(!client.signed_in());
        assert_eq!(client.host(), "127.0.0.1_1");
        assert!(client.identity_show().is_err());

        let created = client.identity_create("test".into()).unwrap();
        assert_eq!(created.phrase.split_whitespace().count(), 24);
        let roster = client.identity_show().unwrap();
        assert_eq!(roster.version, 1);
        assert!(roster.root_signed);
        assert_eq!(roster.identity_id, created.identity_id);
        assert_eq!(roster.devices.len(), 1);
        assert!(roster.devices[0].this_device);
        assert_eq!(roster.devices[0].device_id, client.device_id());

        // Another data directory is another device; the same one is the same device.
        let other = Client::new("http://127.0.0.1:2".into(), scratch("open-2")).unwrap();
        assert_ne!(other.device_id(), client.device_id());
    }

    #[test]
    fn gateway_lifecycle_is_guarded() {
        let client = Client::new("http://127.0.0.1:1".into(), scratch("life")).unwrap();
        assert!(matches!(client.stop(), Err(LumiaError::Lifecycle(_))));
        assert!(matches!(
            client.send_message(wire::display(&wire::new_id()), "x".into()),
            Err(LumiaError::Lifecycle(_))
        ));
        assert!(matches!(
            client.subscribe("not-a-uuid".into()),
            Err(LumiaError::Argument(_))
        ));
        struct Noop;
        impl Listener for Noop {
            fn on_event(&self, _: ClientEvent) {}
        }
        assert!(matches!(
            client.start(vec![], Box::new(Noop)),
            Err(LumiaError::NotSignedIn)
        ));
    }
}
