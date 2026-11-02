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

use futures_util::lock::Mutex;
use js_sys::Function;
use lumia_client::api::{Api, Tokens};
use lumia_client::dm::{DmEngine, Outcome};
use lumia_client::futures_channel::mpsc;
use lumia_client::futures_util::StreamExt as _;
use lumia_client::gateway::{self, GatewayCommand, GatewayEvent};
use lumia_client::identity::Device;
use lumia_client::sqlite::{DmStatus, SqliteStore};
use lumia_client::ClientError;
use lumia_core::keystore::Keystore;
use lumia_core::mls::projection::Projection;
use lumia_core::proto::{
    command, event, Command, DmOperation, DmRequest, DmRequestState, Event, SendMessage,
};
use lumia_core::store::Store;
use lumia_core::sync::Notice;
use lumia_core::wire;
use prost::Message as _;
use serde::Serialize;
use wasm_bindgen::prelude::*;

/// The OPFS pool VFS behind SQLite3 Multiple Ciphers' shim, which encrypts
/// pages on their way to it (e2ee spec section 8). Opening on the
/// prefixed name creates the shim over the real VFS on first use.
const VFS: &str = "multipleciphers-opfs-sahpool";

/// Installs the OPFS pool VFS once per worker.
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
    /// DM kinds: who is in it.
    participant_account_ids: Vec<String>,
    /// Personal DMs: the hosting instance.
    host: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DmRequestSummary {
    request_id: String,
    from_account_id: String,
    to_account_id: String,
    state: String,
    shared_server_ids: Vec<String>,
    mutual_friend_count: u32,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SnapshotSummary {
    account_id: String,
    handle: String,
    display_name: String,
    servers: Vec<ServerSummary>,
    channels: Vec<ChannelSummary>,
    pending_requests: Vec<DmRequestSummary>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct MemberSummary {
    account_id: String,
    handle: String,
    display_name: String,
    nickname: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DmChannelSummary {
    channel_id: String,
    participant_account_ids: Vec<String>,
    /// Whether this device holds the group (can read and send).
    member: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DmMessageSummary {
    message_id: String,
    seq: u64,
    sender_identity_id: String,
    sender_device_id: String,
    /// The sender's account on this instance, when known.
    sender_account_id: String,
    body: String,
    sent_at_micros: i64,
    edited: bool,
    deleted: bool,
    mine: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DmLogSummary {
    messages: Vec<DmMessageSummary>,
    /// Rows sent before this device joined (unreadable by design).
    before_join: u32,
    /// Rows that could not be decrypted.
    undecryptable: u32,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DmOpened {
    channel_id: String,
    existed: bool,
    member: bool,
    /// This device is creating the group; `dmReady` follows.
    creating: bool,
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
    added_at_micros: i64,
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
}

fn emit(listener: &Function, ev: ClientEvent) {
    if let Ok(value) = to_js(&ev) {
        let _ = listener.call1(&JsValue::NULL, &value);
    }
}

/// Carries out the DM engine's outcomes: commands to the gateway loop,
/// everything else to the listener.
fn dispatch(
    outcomes: Vec<Outcome>,
    commands: Option<&mpsc::UnboundedSender<GatewayCommand>>,
    listener: Option<&Function>,
) {
    let emit = |ev: ClientEvent| {
        if let Some(l) = listener {
            emit(l, ev);
        }
    };
    for outcome in outcomes {
        match outcome {
            Outcome::Print(message) => emit(ClientEvent::DmNote { message }),
            Outcome::Send(cmd) => {
                if let Some(tx) = commands {
                    let _ = tx.unbounded_send(GatewayCommand::Send(cmd));
                }
            }
            Outcome::Subscribe(channel_id) => {
                if let Some(tx) = commands {
                    let _ = tx.unbounded_send(GatewayCommand::Subscribe(channel_id.clone()));
                }
                emit(ClientEvent::DmSubscribed {
                    channel_id: wire::display(&channel_id),
                });
            }
            Outcome::Operation {
                channel_id,
                seq,
                sender,
                ..
            } => emit(ClientEvent::DmOperation {
                channel_id: wire::display(&channel_id),
                seq,
                sender_identity_id: hex::encode(&sender.identity_id),
                sender_device_id: wire::display(&sender.device_id),
            }),
            Outcome::GroupReady(channel_id) => emit(ClientEvent::DmReady {
                channel_id: wire::display(&channel_id),
            }),
            Outcome::Delivered(id) => emit(ClientEvent::DmDelivered {
                operation_id: wire::display(&id),
            }),
        }
    }
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
    /// The page's listener while the loop runs; DM outcomes go there too.
    listener: Option<Function>,
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
    /// The at-rest key of every store this client opens (e2ee spec 8).
    store_key: Option<Vec<u8>>,
    inner: Rc<RefCell<Inner>>,
    identity: Rc<RefCell<IdentityStores>>,
    /// The personal-DM engine, once this device holds an identity. Behind
    /// an async mutex: the gateway task and the page's calls take turns,
    /// each holding it across its own awaits (roster and key-package
    /// fetches) on this single-threaded worker.
    dm: Rc<RefCell<Option<Rc<Mutex<DmEngine>>>>>,
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
    pub async fn open(base_url: String, store_key: Option<Vec<u8>>) -> Result<Client, JsError> {
        ensure_vfs().await?;
        if let Some(k) = &store_key {
            if k.len() != 32 {
                return Err(JsError::new("the store key must be 32 bytes"));
            }
        }
        let host = host_of(&base_url);
        let store_path = format!("lumia-{host}-store.sqlite");
        let keys_path = format!("lumia-{host}-keys.sqlite");
        let open = |path: &str| SqliteStore::open_on_vfs_keyed(path, VFS, store_key.as_deref());
        let mut store = open(&store_path).map_err(js)?;
        store
            .meta_set("base_url", base_url.trim_end_matches('/'))
            .map_err(js)?;
        let reader = open(&store_path).map_err(js)?;
        let keys = open(&keys_path).map_err(js)?;
        let identity_store = open("lumia-identity-store.sqlite").map_err(js)?;
        let mut identity_keys = open("lumia-identity-keys.sqlite").map_err(js)?;
        let device_id = match identity_keys.get("device_id").map_err(js)? {
            Some(id) => id,
            None => {
                let id = wire::new_id();
                identity_keys.put("device_id", &id).map_err(js)?;
                id
            }
        };
        let rotate_path = keys_path.clone();
        let rotate_key = store_key.clone();
        let api = Api::new(&base_url)
            .with_tokens(tokens(&keys).map_err(js)?)
            .on_rotate(Arc::new(move |pair| {
                if let Ok(mut keys) =
                    SqliteStore::open_on_vfs_keyed(&rotate_path, VFS, rotate_key.as_deref())
                {
                    let _ = keys.put("access_token", pair.access_token.as_bytes());
                    let _ = keys.put("refresh_token", pair.refresh_token.as_bytes());
                }
            }));
        Ok(Self {
            api,
            host,
            device_id,
            store_key,
            inner: Rc::new(RefCell::new(Inner {
                store: Some(store),
                reader,
                keys,
                commands: None,
                listener: None,
            })),
            identity: Rc::new(RefCell::new(IdentityStores {
                store: identity_store,
                keys: identity_keys,
            })),
            dm: Rc::new(RefCell::new(None)),
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

    /// `GET /servers/{id}/members`: a server's members with their
    /// accounts, optionally filtered by a handle or nickname prefix.
    ///
    /// # Errors
    /// On transport or API failure.
    pub async fn members(&self, server_id: String, query: String) -> Result<JsValue, JsError> {
        let page = self.api.members(&server_id, &query).await.map_err(js)?;
        let out: Vec<MemberSummary> = page
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
            .collect();
        to_js(&out)
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
        let mut channels = channel_ids
            .iter()
            .map(|id| parse_uuid(id))
            .collect::<Result<Vec<_>, _>>()?;
        // Personal DMs whose participants this device knows are subscribed
        // from the start; the rest when the account log names them, so
        // their events are never processed before the rosters can be
        // fetched (the CLI does the same).
        let engine = self.dm_engine().ok();
        if let Some(engine) = &engine {
            if let Some(e) = engine.try_lock() {
                for channel in e.channels().keys() {
                    if !channels.contains(channel) {
                        channels.push(channel.clone());
                    }
                }
            }
        }
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
            inner.listener = None;
        });
        let dm = Rc::clone(&self.dm);
        let stores = Rc::clone(&self.inner);
        let dtx = tx.clone();
        let page = listener.clone();
        wasm_bindgen_futures::spawn_local(async move {
            while let Some(ev) = erx.next().await {
                if let GatewayEvent::Notice(Notice::Welcomed(w)) = &ev {
                    let _ = stores
                        .borrow_mut()
                        .reader
                        .meta_set("account_id", &hex::encode(&w.account_id));
                }
                let engine = dm.borrow().clone();
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
                        Ok(out) => dispatch(out, Some(&dtx), Some(&page)),
                        Err(err) => emit(
                            &page,
                            ClientEvent::DmError {
                                message: err.to_string(),
                            },
                        ),
                    }
                }
                if let Ok(value) = to_js(&client_event(ev)) {
                    let _ = page.call1(&JsValue::NULL, &value);
                }
            }
        });
        inner.commands = Some(tx);
        inner.listener = Some(listener);
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

    /// Revokes another device of this identity, signed by this device
    /// with the account password as reauth (identity spec 4.4). A device
    /// cannot revoke itself; that takes the phrase on a native device.
    ///
    /// # Errors
    /// On API failure, a wrong password, or when asked to revoke this device.
    #[wasm_bindgen(js_name = identityRevoke)]
    #[allow(clippy::await_holding_refcell_ref)] // see IdentityStores
    pub async fn identity_revoke(
        &self,
        device_id: String,
        password: String,
    ) -> Result<JsValue, JsError> {
        let target = parse_uuid(&device_id)?;
        let api = self.api.clone();
        let mut stores = self.identity.borrow_mut();
        let mut device = device(&mut stores, &self.device_id);
        device
            .revoke_with_device(&api, &password, &target)
            .await
            .map_err(js)?;
        let roster = device.roster().map_err(js)?;
        drop(stores);
        to_js(&roster_summary(&roster, &self.device_id))
    }

    // ---- Personal DMs (e2ee spec), through the DM engine ----

    /// Whether personal DMs work on this device: it holds an identity.
    #[must_use]
    #[wasm_bindgen(js_name = dmAvailable)]
    pub fn dm_available(&self) -> bool {
        self.dm_engine().is_ok()
    }

    /// The engine, built on first use from this device's identity and its
    /// own connection to the instance store.
    fn dm_engine(&self) -> Result<Rc<Mutex<DmEngine>>, JsError> {
        if let Some(engine) = self.dm.borrow().as_ref() {
            return Ok(Rc::clone(engine));
        }
        let (roster, seed, backup_key) = {
            let mut stores = self.identity.borrow_mut();
            let seed = stores
                .keys
                .get("device_key")
                .map_err(js)?
                .ok_or_else(|| JsError::new("no identity on this device"))?;
            let backup_key = stores.keys.get("backup_key").map_err(js)?;
            let roster = device(&mut stores, &self.device_id).roster().map_err(js)?;
            (roster, seed, backup_key)
        };
        let account = self
            .inner
            .borrow()
            .reader
            .meta_get("account_id")
            .map_err(js)?
            .map(|h| hex::decode(h).unwrap_or_default())
            .unwrap_or_default();
        let store = SqliteStore::open_on_vfs_keyed(
            &format!("lumia-{}-store.sqlite", self.host),
            VFS,
            self.store_key.as_deref(),
        )
        .map_err(js)?;
        let engine = DmEngine::new(
            self.api.clone(),
            store,
            account,
            self.device_id.clone(),
            &seed,
            backup_key,
            roster,
        )
        .map_err(js)?;
        let engine = Rc::new(Mutex::new(engine));
        *self.dm.borrow_mut() = Some(Rc::clone(&engine));
        Ok(engine)
    }

    /// Carries out outcomes from a page-initiated engine call.
    fn dispatch(&self, outcomes: Vec<Outcome>) {
        let inner = self.inner.borrow();
        dispatch(outcomes, inner.commands.as_ref(), inner.listener.as_ref());
    }

    /// The personal conversations this device knows, with whether it
    /// holds their group.
    ///
    /// # Errors
    /// Without an identity, or on a store failure.
    #[wasm_bindgen(js_name = dmChannels)]
    pub async fn dm_channels(&self) -> Result<JsValue, JsError> {
        let engine = self.dm_engine()?;
        let e = engine.lock().await;
        let mut out = Vec::new();
        for (channel, participants) in e.channels() {
            out.push(DmChannelSummary {
                channel_id: wire::display(channel),
                participant_account_ids: participants.iter().map(|p| wire::display(p)).collect(),
                member: e.is_member(channel).map_err(js)?,
            });
        }
        to_js(&out)
    }

    /// Opens (or finds) the conversation with `account_ids`; when this
    /// device opened a new one it creates the group and welcomes the
    /// others (`dmReady` follows on the listener).
    ///
    /// # Errors
    /// `FAILED_PRECONDITION` without an accepted request; or without an
    /// identity, or before the gateway loop started.
    #[wasm_bindgen(js_name = dmOpen)]
    pub async fn dm_open(&self, account_ids: Vec<String>) -> Result<JsValue, JsError> {
        let others = account_ids
            .iter()
            .map(|id| parse_uuid(id))
            .collect::<Result<Vec<_>, _>>()?;
        self.open_dm(others, false).await
    }

    async fn open_dm(&self, others: Vec<Vec<u8>>, accepter: bool) -> Result<JsValue, JsError> {
        let engine = self.dm_engine()?;
        let opened = self.api.open_personal_dm(others).await.map_err(js)?;
        let channel = opened
            .channel
            .ok_or_else(|| JsError::new("no channel in the response"))?;
        let mut e = engine.lock().await;
        e.record_channel(&channel).map_err(js)?;
        let member = e.is_member(&channel.channel_id).map_err(js)?;
        let creating = !member && (accepter || !opened.existed);
        if creating {
            if self.inner.borrow().commands.is_none() {
                return Err(JsError::new("not started"));
            }
            let outcomes = e.create_group(&channel.channel_id).await.map_err(js)?;
            drop(e);
            self.dispatch(outcomes);
        }
        to_js(&DmOpened {
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
    #[wasm_bindgen(js_name = dmRequest)]
    pub async fn dm_request(&self, account_id: String) -> Result<JsValue, JsError> {
        let to = parse_uuid(&account_id)?;
        let request = self.api.send_dm_request(to).await.map_err(js)?;
        to_js(&request_summary(&request))
    }

    /// Answers a DM request: `accept`, `decline` or `block`. Accepting
    /// opens the conversation and, as the accepter, creates its group.
    ///
    /// # Errors
    /// On API failure, an unknown response, or without an identity when accepting.
    #[wasm_bindgen(js_name = dmRespond)]
    pub async fn dm_respond(
        &self,
        request_id: String,
        response: String,
    ) -> Result<JsValue, JsError> {
        let state = match response.as_str() {
            "accept" => DmRequestState::Accepted,
            "decline" => DmRequestState::Declined,
            "block" => DmRequestState::Blocked,
            other => return Err(JsError::new(&format!("unknown response {other}"))),
        };
        let request = self
            .api
            .respond_dm_request(&request_id, state)
            .await
            .map_err(js)?;
        if state == DmRequestState::Accepted {
            self.open_dm(vec![request.from_account_id.clone()], true)
                .await
        } else {
            to_js(&request_summary(&request))
        }
    }

    /// Sends a message in a personal DM; `dmDelivered` follows on the
    /// listener once the ciphertext is acked.
    ///
    /// # Errors
    /// Without the group for the conversation, or before the loop started.
    #[wasm_bindgen(js_name = dmSend)]
    pub async fn dm_send(&self, channel_id: String, body: String) -> Result<(), JsError> {
        let channel = parse_uuid(&channel_id)?;
        let engine = self.dm_engine()?;
        let mut e = engine.lock().await;
        e.load_participants(&channel).map_err(js)?;
        let outcomes = e.send_text(&channel, &body).map_err(js)?;
        drop(e);
        self.dispatch(outcomes);
        Ok(())
    }

    /// Deletes one of this identity's messages in a personal DM.
    ///
    /// # Errors
    /// Without the group for the conversation, or before the loop started.
    #[wasm_bindgen(js_name = dmDelete)]
    pub async fn dm_delete(&self, channel_id: String, message_id: String) -> Result<(), JsError> {
        let channel = parse_uuid(&channel_id)?;
        let message = parse_uuid(&message_id)?;
        let engine = self.dm_engine()?;
        let mut e = engine.lock().await;
        e.load_participants(&channel).map_err(js)?;
        let outcomes = e.send_delete(&channel, message).map_err(js)?;
        drop(e);
        self.dispatch(outcomes);
        Ok(())
    }

    /// Rejoins a conversation by external commit (e2ee spec section 10).
    ///
    /// # Errors
    /// Without an identity, or when the instance holds no group info.
    #[wasm_bindgen(js_name = dmRejoin)]
    pub async fn dm_rejoin(&self, channel_id: String) -> Result<(), JsError> {
        let channel = parse_uuid(&channel_id)?;
        let engine = self.dm_engine()?;
        let mut e = engine.lock().await;
        e.load_participants(&channel).map_err(js)?;
        let outcomes = e.rejoin(&channel).await.map_err(js)?;
        drop(e);
        self.dispatch(outcomes);
        Ok(())
    }

    /// The conversation as this device can read it: the projection of its
    /// decrypted operations, oldest first, with counts of what it cannot.
    ///
    /// # Errors
    /// Without an identity, or on a store failure.
    #[wasm_bindgen(js_name = dmMessages)]
    pub async fn dm_messages(&self, channel_id: String) -> Result<JsValue, JsError> {
        let channel = parse_uuid(&channel_id)?;
        let engine = self.dm_engine()?;
        let e = engine.lock().await;
        let me = e.me().identity_id.clone();
        let mut projection = Projection::default();
        let mut before_join = 0u32;
        let mut undecryptable = 0u32;
        for row in e.rows(&channel).map_err(js)? {
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
        to_js(&DmLogSummary {
            messages,
            before_join,
            undecryptable,
        })
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
