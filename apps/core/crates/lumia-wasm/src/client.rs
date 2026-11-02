//! The client for the web shell (plan step 6): the same `lumia-client`
//! the CLI and the FFI shells use, with its SQLite store on the OPFS pool
//! VFS, running inside a dedicated worker. One [`Client`] per instance.
//!
//! Mirrors `lumia-ffi`'s `Client`: constructors and calls are async and
//! return promises, summaries are plain JS objects, and the gateway loop
//! reports through a listener function called with `{kind: "..."}`
//! objects in order. Marshalling only; nothing here is a decision.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use js_sys::Function;
use lumia_client::api::{Api, Tokens};
use lumia_client::futures_channel::mpsc;
use lumia_client::futures_util::StreamExt as _;
use lumia_client::gateway::{self, GatewayCommand, GatewayEvent};
use lumia_client::identity::Device;
use lumia_client::sqlite::SqliteStore;
use lumia_client::ClientError;
use lumia_core::keystore::Keystore;
use lumia_core::proto::{command, event, Command, Event, SendMessage};
use lumia_core::store::Store;
use lumia_core::sync::Notice;
use lumia_core::wire;
use serde::Serialize;
use wasm_bindgen::prelude::*;

const VFS: &str = "opfs-sahpool";

/// Installs the OPFS pool VFS once per worker and makes it the default.
async fn ensure_vfs() -> Result<(), JsError> {
    thread_local! {
        static INSTALLED: RefCell<bool> = const { RefCell::new(false) };
    }
    if INSTALLED.with(|i| *i.borrow()) {
        return Ok(());
    }
    sqlite_wasm_vfs::sahpool::install::<sqlite_wasm_rs::WasmOsCallback>(
        &sqlite_wasm_vfs::sahpool::OpfsSAHPoolCfgBuilder::new()
            .directory(".lumia")
            .initial_capacity(12)
            .build(),
        true,
    )
    .await
    .map_err(|e| JsError::new(&format!("opfs: {e:?}")))?;
    INSTALLED.with(|i| *i.borrow_mut() = true);
    Ok(())
}

fn js<E: std::fmt::Display>(e: E) -> JsError {
    JsError::new(&e.to_string())
}

fn host_of(url: &str) -> String {
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

fn parse_uuid(s: &str) -> Result<Vec<u8>, JsError> {
    uuid::Uuid::parse_str(s)
        .map(wire::uuid_bytes)
        .map_err(|_| JsError::new(&format!("not a uuid: {s}")))
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

fn to_js<T: Serialize>(value: &T) -> Result<JsValue, JsError> {
    serde_wasm_bindgen::to_value(value).map_err(|e| JsError::new(&e.to_string()))
}

// ---- Summaries, as the JS side sees them ---------------------------------------

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct InstanceSummary {
    host: String,
    operator_name: String,
    operator_url: String,
    terms_url: String,
    privacy_url: String,
    software_version: String,
    identity_proof_sign_in_enabled: bool,
    identity_proof_create_enabled: bool,
    open_registration: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ServerSummary {
    server_id: String,
    name: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ChannelSummary {
    channel_id: String,
    server_id: String,
    name: String,
    kind: String,
    last_seq: u64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SnapshotSummary {
    account_id: String,
    handle: String,
    display_name: String,
    servers: Vec<ServerSummary>,
    channels: Vec<ChannelSummary>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct MessageSummary {
    message_id: String,
    channel_id: String,
    seq: u64,
    author_account_id: String,
    body: String,
    created_at_micros: i64,
    deleted: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct EventSummary {
    seq: u64,
    kind: String,
    message: Option<MessageSummary>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DeviceSummary {
    device_id: String,
    kind: String,
    label: String,
    web_origin: String,
    this_device: bool,
    revoked: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct RosterSummary {
    identity_id: String,
    version: u64,
    devices: Vec<DeviceSummary>,
}

#[derive(Serialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
enum ClientEvent {
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
        Some(_) => ("other", None),
        None => ("empty", None),
    };
    EventSummary {
        seq: e.seq,
        kind: kind.into(),
        message,
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
            count: events.len(),
        },
        GatewayEvent::Notice(Notice::Welcomed(w)) => ClientEvent::Welcomed {
            account_id: wire::display(&w.account_id),
        },
        GatewayEvent::Notice(Notice::Acked { command_id, seq }) => ClientEvent::Acked {
            command_id: wire::display(&command_id),
            seq,
        },
        GatewayEvent::Notice(Notice::Error(e)) => ClientEvent::Error {
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

// ---- The client -----------------------------------------------------------------

struct Inner {
    /// The store the gateway loop owns while running; back here when it stops.
    store: Option<SqliteStore>,
    /// A second connection for reads while the loop runs.
    reader: SqliteStore,
    /// This instance's secrets (tokens).
    keys: SqliteStore,
    commands: Option<mpsc::UnboundedSender<GatewayCommand>>,
}

/// The identity's stores, shared by every instance on this origin: the
/// roster and meta, and the secrets (device key, backup key, link secret).
/// Their own cell, so an identity flow that awaits the network (and holds
/// them meanwhile) never blocks a store read behind `Inner`; the worker is
/// single-threaded and the UI runs one identity flow at a time.
struct IdentityStores {
    store: SqliteStore,
    keys: SqliteStore,
}

/// One instance's client, over the store on OPFS.
#[wasm_bindgen]
pub struct Client {
    api: Api,
    host: String,
    device_id: Vec<u8>,
    inner: Rc<RefCell<Inner>>,
    identity: Rc<RefCell<IdentityStores>>,
}

impl std::fmt::Debug for Client {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Client")
            .field("host", &self.host)
            .field("device_id", &wire::display(&self.device_id))
            .finish_non_exhaustive()
    }
}

#[wasm_bindgen]
impl Client {
    /// Opens the client for the instance at `base_url`: installs the OPFS
    /// VFS, opens the stores (one per instance plus the identity's), and
    /// loads the tokens it holds.
    ///
    /// # Errors
    /// When OPFS is unavailable (not in a worker, or storage refused) or a
    /// store fails to open.
    pub async fn open(base_url: String) -> Result<Client, JsError> {
        ensure_vfs().await?;
        let host = host_of(&base_url);
        let store_path = format!("lumia-{host}-store.sqlite");
        let keys_path = format!("lumia-{host}-keys.sqlite");
        let mut store = SqliteStore::open_on_vfs(&store_path, VFS).map_err(js)?;
        store
            .meta_set("base_url", base_url.trim_end_matches('/'))
            .map_err(js)?;
        let reader = SqliteStore::open_on_vfs(&store_path, VFS).map_err(js)?;
        let keys = SqliteStore::open_on_vfs(&keys_path, VFS).map_err(js)?;
        let identity_store =
            SqliteStore::open_on_vfs("lumia-identity-store.sqlite", VFS).map_err(js)?;
        let mut identity_keys =
            SqliteStore::open_on_vfs("lumia-identity-keys.sqlite", VFS).map_err(js)?;
        let device_id = match identity_keys.get("device_id").map_err(js)? {
            Some(id) => id,
            None => {
                let id = wire::new_id();
                identity_keys.put("device_id", &id).map_err(js)?;
                id
            }
        };
        let rotate_path = keys_path.clone();
        let api = Api::new(&base_url)
            .with_tokens(tokens(&keys).map_err(js)?)
            .on_rotate(Arc::new(move |pair| {
                if let Ok(mut keys) = SqliteStore::open_on_vfs(&rotate_path, VFS) {
                    let _ = keys.put("access_token", pair.access_token.as_bytes());
                    let _ = keys.put("refresh_token", pair.refresh_token.as_bytes());
                }
            }));
        Ok(Self {
            api,
            host,
            device_id,
            inner: Rc::new(RefCell::new(Inner {
                store: Some(store),
                reader,
                keys,
                commands: None,
            })),
            identity: Rc::new(RefCell::new(IdentityStores {
                store: identity_store,
                keys: identity_keys,
            })),
        })
    }

    /// The instance host this client talks to.
    #[must_use]
    pub fn host(&self) -> String {
        self.host.clone()
    }

    /// This device's id (UUID string).
    #[must_use]
    #[wasm_bindgen(js_name = deviceId)]
    pub fn device_id(&self) -> String {
        wire::display(&self.device_id)
    }

    /// Whether an access token is held for this instance.
    #[must_use]
    #[wasm_bindgen(js_name = signedIn)]
    pub fn signed_in(&self) -> bool {
        self.api.tokens().access.is_some()
    }

    /// `GET /instance`: the trust card.
    ///
    /// # Errors
    /// On transport or API failure.
    #[wasm_bindgen(js_name = instanceInfo)]
    pub async fn instance_info(&self) -> Result<JsValue, JsError> {
        let info = self.api.instance_info().await.map_err(js)?;
        to_js(&InstanceSummary {
            host: info.host,
            operator_name: info.operator_name,
            operator_url: info.operator_url,
            terms_url: info.terms_url,
            privacy_url: info.privacy_url,
            software_version: info.software_version,
            identity_proof_sign_in_enabled: info.identity_proof_sign_in_enabled,
            identity_proof_create_enabled: info.identity_proof_create_enabled,
            open_registration: info.open_registration,
        })
    }

    /// Creates an account with a password and signs in.
    ///
    /// # Errors
    /// On API failure (`ALREADY_EXISTS`, `INVALID_ARGUMENT`).
    #[wasm_bindgen(js_name = createAccount)]
    pub async fn create_account(&self, handle: String, password: String) -> Result<(), JsError> {
        let pair = self
            .api
            .create_account(&handle, &password)
            .await
            .map_err(js)?;
        self.save_tokens(&pair)
    }

    /// Signs in with a password, and a TOTP code when the account requires one.
    ///
    /// # Errors
    /// `UNAUTHENTICATED` for a wrong password or code, `FAILED_PRECONDITION`
    /// with `mfa` when a code is required and not given.
    #[wasm_bindgen(js_name = signIn)]
    pub async fn sign_in(
        &self,
        handle: String,
        password: String,
        totp_code: Option<String>,
    ) -> Result<(), JsError> {
        let pair = match totp_code.filter(|c| !c.is_empty()) {
            Some(code) => self
                .api
                .sign_in_with_totp(&handle, &password, self.device_id.clone(), &code)
                .await
                .map_err(js)?,
            None => self
                .api
                .sign_in(&handle, &password, self.device_id.clone())
                .await
                .map_err(js)?,
        };
        self.save_tokens(&pair)
    }

    /// Signs out: revokes the session and forgets the tokens.
    ///
    /// # Errors
    /// On a store failure; an API refusal is ignored, the tokens go anyway.
    #[wasm_bindgen(js_name = signOut)]
    pub async fn sign_out(&self) -> Result<(), JsError> {
        let _ = self.api.sign_out().await;
        let mut inner = self.inner.borrow_mut();
        inner.keys.delete("access_token").map_err(js)?;
        inner.keys.delete("refresh_token").map_err(js)?;
        drop(inner);
        self.api.clear_tokens();
        Ok(())
    }

    /// `GET /account/snapshot`: the account, its servers and the channels it can view.
    ///
    /// # Errors
    /// On transport or API failure.
    pub async fn snapshot(&self) -> Result<JsValue, JsError> {
        let snap = self.api.account_snapshot().await.map_err(js)?;
        let account = snap.account.unwrap_or_default();
        to_js(&SnapshotSummary {
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
                })
                .collect(),
        })
    }

    /// The newest messages of a channel from the local store, oldest first.
    ///
    /// # Errors
    /// On a store failure or a malformed id.
    pub fn messages(&self, channel_id: String, limit: u32) -> Result<JsValue, JsError> {
        let id = parse_uuid(&channel_id)?;
        let inner = self.inner.borrow();
        let rows = inner.reader.messages(&id, limit as usize).map_err(js)?;
        let out: Vec<MessageSummary> = rows
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
            .collect();
        to_js(&out)
    }

    /// Starts the gateway loop on this worker, subscribing to `channel_ids`
    /// from the store's cursors. `listener` is called with each event
    /// object, in order, until `{kind: "closed"}` or `stop`.
    ///
    /// # Errors
    /// When not signed in, already started, or an id is malformed.
    pub fn start(&self, channel_ids: Vec<String>, listener: Function) -> Result<(), JsError> {
        if !self.signed_in() {
            return Err(JsError::new("not signed in"));
        }
        let channels = channel_ids
            .iter()
            .map(|id| parse_uuid(id))
            .collect::<Result<Vec<_>, _>>()?;
        let mut inner = self.inner.borrow_mut();
        let store = inner
            .store
            .take()
            .ok_or_else(|| JsError::new("already started"))?;
        let (tx, rx) = mpsc::unbounded();
        let (etx, mut erx) = mpsc::unbounded();
        let api = self.api.clone();
        let device_id = self.device_id.clone();
        let back = Rc::clone(&self.inner);
        wasm_bindgen_futures::spawn_local(async move {
            let store =
                gateway::run_forever(api, store, "lumia-web", device_id, channels, rx, etx).await;
            let mut inner = back.borrow_mut();
            inner.store = Some(store);
            inner.commands = None;
        });
        wasm_bindgen_futures::spawn_local(async move {
            while let Some(ev) = erx.next().await {
                if let Ok(value) = to_js(&client_event(ev)) {
                    let _ = listener.call1(&JsValue::NULL, &value);
                }
            }
        });
        inner.commands = Some(tx);
        Ok(())
    }

    /// Stops the gateway loop; the store comes back once it ends.
    pub fn stop(&self) {
        self.inner.borrow_mut().commands = None;
    }

    /// Subscribes to a channel from the store's cursor.
    ///
    /// # Errors
    /// When not started or the id is malformed.
    pub fn subscribe(&self, channel_id: String) -> Result<(), JsError> {
        self.send_command(GatewayCommand::Subscribe(parse_uuid(&channel_id)?))
    }

    /// Sends a message; returns the client-minted message id.
    ///
    /// # Errors
    /// When not started or the id is malformed.
    #[wasm_bindgen(js_name = sendMessage)]
    pub fn send_message(&self, channel_id: String, body: String) -> Result<String, JsError> {
        let channel = parse_uuid(&channel_id)?;
        let id = wire::new_id();
        self.send_command(GatewayCommand::Send(Command {
            command_id: id.clone(),
            channel_id: channel,
            op: Some(command::Op::SendMessage(SendMessage {
                body,
                ..Default::default()
            })),
        }))?;
        Ok(wire::display(&id))
    }

    /// The page came back to the foreground: ask for every head.
    ///
    /// # Errors
    /// When not started.
    pub fn foreground(&self) -> Result<(), JsError> {
        self.send_command(GatewayCommand::Foreground)
    }

    // ---- Identity (a web device is linked from a native one) ----

    /// Whether this device holds an identity roster.
    #[must_use]
    #[wasm_bindgen(js_name = hasIdentity)]
    pub fn has_identity(&self) -> bool {
        self.identity
            .borrow()
            .store
            .meta_get("roster")
            .ok()
            .flatten()
            .is_some()
    }

    /// Step 1 of linking this web device (identity spec 4.2): the request
    /// to show as a code or QR for a native device to approve.
    ///
    /// # Errors
    /// On a store or keystore failure.
    #[wasm_bindgen(js_name = identityLinkRequest)]
    pub fn identity_link_request(&self, label: String) -> Result<String, JsError> {
        let mut stores = self.identity.borrow_mut();
        let mut device = device(&mut stores, &self.device_id);
        device
            .link_request_as(&label, lumia_core::proto::DeviceKind::Web, &origin())
            .map_err(js)
    }

    /// Step 7: claims the bundle a native device uploaded, then signs in
    /// by identity proof.
    ///
    /// # Errors
    /// `NOT_FOUND` for a wrong or used code, or when the roster does not
    /// include this device.
    #[wasm_bindgen(js_name = identityClaim)]
    #[allow(clippy::await_holding_refcell_ref)] // see IdentityStores
    pub async fn identity_claim(&self, code: String) -> Result<JsValue, JsError> {
        let api = self.api.clone();
        let mut stores = self.identity.borrow_mut();
        let mut device = device(&mut stores, &self.device_id);
        let roster = device.claim(&api, &code).await.map_err(js)?;
        let pair = device.sign_in(&api).await.map_err(js)?;
        drop(stores);
        self.save_tokens(&pair)?;
        to_js(&roster_summary(&roster, &self.device_id))
    }

    /// Signs in by identity proof with the roster this device holds.
    ///
    /// # Errors
    /// On API failure, or when the device is revoked in the held roster.
    #[wasm_bindgen(js_name = signInIdentity)]
    #[allow(clippy::await_holding_refcell_ref)] // see IdentityStores
    pub async fn sign_in_identity(&self) -> Result<(), JsError> {
        let api = self.api.clone();
        let mut stores = self.identity.borrow_mut();
        let mut device = device(&mut stores, &self.device_id);
        let pair = device.sign_in(&api).await.map_err(js)?;
        drop(stores);
        self.save_tokens(&pair)
    }

    /// The roster this device holds.
    ///
    /// # Errors
    /// When there is none.
    #[wasm_bindgen(js_name = identityShow)]
    pub fn identity_show(&self) -> Result<JsValue, JsError> {
        let mut stores = self.identity.borrow_mut();
        let device = device(&mut stores, &self.device_id);
        let roster = device.roster().map_err(js)?;
        to_js(&roster_summary(&roster, &self.device_id))
    }

    fn save_tokens(&self, pair: &lumia_core::proto::TokenPair) -> Result<(), JsError> {
        let mut inner = self.inner.borrow_mut();
        inner
            .keys
            .put("access_token", pair.access_token.as_bytes())
            .map_err(js)?;
        inner
            .keys
            .put("refresh_token", pair.refresh_token.as_bytes())
            .map_err(js)?;
        self.api.set_tokens(pair);
        Ok(())
    }

    fn send_command(&self, command: GatewayCommand) -> Result<(), JsError> {
        let inner = self.inner.borrow();
        let tx = inner
            .commands
            .as_ref()
            .ok_or_else(|| JsError::new("not started"))?;
        tx.unbounded_send(command)
            .map_err(|_| JsError::new("the gateway loop has stopped"))
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

fn roster_summary(roster: &lumia_core::identity::roster::Verified, me: &[u8]) -> RosterSummary {
    let mut devices: Vec<DeviceSummary> = roster
        .devices
        .iter()
        .map(|(id, info)| DeviceSummary {
            device_id: wire::display(id),
            kind: format!("{:?}", info.kind).to_lowercase(),
            label: String::new(),
            web_origin: info.web_origin.clone(),
            this_device: id.as_slice() == me,
            revoked: false,
        })
        .collect();
    devices.extend(roster.revoked.keys().map(|id| DeviceSummary {
        device_id: wire::display(id),
        kind: String::new(),
        label: String::new(),
        web_origin: String::new(),
        this_device: id.as_slice() == me,
        revoked: true,
    }));
    RosterSummary {
        identity_id: hex::encode(&roster.identity_id),
        version: roster.version,
        devices,
    }
}

/// The worker's origin, which scopes a web device's key (architecture 11.4).
fn origin() -> String {
    js_sys::global()
        .dyn_into::<web_sys_lite::WorkerGlobalScope>()
        .ok()
        .map(|g| g.origin())
        .unwrap_or_default()
}

/// Just enough of `WorkerGlobalScope` to read the origin, without web-sys.
mod web_sys_lite {
    use wasm_bindgen::prelude::*;

    #[wasm_bindgen]
    extern "C" {
        #[wasm_bindgen(js_name = WorkerGlobalScope)]
        pub type WorkerGlobalScope;

        #[wasm_bindgen(method, getter)]
        pub fn origin(this: &WorkerGlobalScope) -> String;
    }
}
