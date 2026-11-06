//! The client for the web shell (plan step 6): `lumia_client::app::App`
//! in a dedicated worker, with its SQLite stores on the OPFS pool VFS
//! behind SQLite3 Multiple Ciphers (e2ee spec section 8). One [`Client`]
//! per instance.
//!
//! Marshalling only: calls are async and return promises, summaries and
//! events are the app's, serialized to plain JS objects; the listener
//! function gets `{kind: "..."}` objects in order. Nothing here is a
//! decision.

use std::cell::RefCell;
use std::sync::Arc;

use js_sys::Function;
use lumia_client::app::{App, EventSink, Opener, Place, StoreName};
use lumia_client::sqlite::SqliteStore;
use lumia_core::proto::PushKind;
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

fn to_js<T: Serialize>(value: &T) -> Result<JsValue, JsError> {
    serde_wasm_bindgen::to_value(value).map_err(|e| JsError::new(&e.to_string()))
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
        pub type WorkerGlobalScope;

        #[wasm_bindgen(method, getter)]
        pub fn origin(this: &WorkerGlobalScope) -> String;
    }
}

/// One instance's client in the worker.
#[wasm_bindgen]
pub struct Client {
    app: Arc<App>,
}

impl std::fmt::Debug for Client {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.app.fmt(f)
    }
}

#[wasm_bindgen]
impl Client {
    /// Opens the client for the instance at `base_url`. The stores live in
    /// OPFS as `lumia-<host>-store.sqlite`, `lumia-<host>-keys.sqlite`,
    /// `lumia-identity-store.sqlite` and `lumia-identity-keys.sqlite`,
    /// encrypted under `store_key` (32 bytes) when given. `shell` means
    /// the desktop shell: the device is NATIVE, not origin-scoped WEB.
    ///
    /// # Errors
    /// When the VFS cannot be installed or a store cannot be opened.
    pub async fn open(
        base_url: String,
        store_key: Option<Vec<u8>>,
        shell: bool,
    ) -> Result<Client, JsError> {
        ensure_vfs().await?;
        if let Some(k) = &store_key {
            if k.len() != 32 {
                return Err(JsError::new("the store key must be 32 bytes"));
            }
        }
        let host = lumia_client::app::host_of(&base_url);
        let opener: Opener = Arc::new(move |name| {
            let path = match name {
                StoreName::Instance => format!("lumia-{host}-store.sqlite"),
                StoreName::Keys => format!("lumia-{host}-keys.sqlite"),
                StoreName::IdentityStore => "lumia-identity-store.sqlite".to_string(),
                StoreName::IdentityKeys => "lumia-identity-keys.sqlite".to_string(),
            };
            SqliteStore::open_on_vfs_keyed(&path, VFS, store_key.as_deref())
        });
        let place = if shell {
            Place::Native
        } else {
            Place::Web { origin: origin() }
        };
        let app = App::open(&base_url, place, opener).map_err(js)?;
        Ok(Self { app })
    }

    /// The instance's host name (with the port when not the default).
    #[must_use]
    pub fn host(&self) -> String {
        self.app.host()
    }

    /// This device's id (UUID string).
    #[must_use]
    #[wasm_bindgen(js_name = deviceId)]
    pub fn device_id(&self) -> String {
        self.app.device_id()
    }

    /// Whether an access token is held for this instance.
    #[must_use]
    #[wasm_bindgen(js_name = signedIn)]
    pub fn signed_in(&self) -> bool {
        self.app.signed_in()
    }

    /// `GET /instance`: the trust card.
    ///
    /// # Errors
    /// On transport or API failure.
    #[wasm_bindgen(js_name = instanceInfo)]
    pub async fn instance_info(&self) -> Result<JsValue, JsError> {
        to_js(&self.app.instance_info().await.map_err(js)?)
    }

    /// Creates an account with a password and signs in.
    ///
    /// # Errors
    /// On API failure (`ALREADY_EXISTS`, `INVALID_ARGUMENT`).
    #[wasm_bindgen(js_name = createAccount)]
    pub async fn create_account(
        &self,
        handle: String,
        password: String,
        invite: Option<String>,
    ) -> Result<(), JsError> {
        self.app
            .create_account(&handle, &password, invite.as_deref())
            .await
            .map_err(js)
    }

    /// `GET /invites/{code}`: what an invite link or code shows before joining. Public.
    ///
    /// # Errors
    /// `NOT_FOUND` for an unknown, expired or spent code; transport failure.
    #[wasm_bindgen(js_name = invitePreview)]
    pub async fn invite_preview(&self, link_or_code: String) -> Result<JsValue, JsError> {
        to_js(&self.app.invite_preview(&link_or_code).await.map_err(js)?)
    }

    /// `POST /invites`: an invite link for a server (0 uses is unlimited).
    ///
    /// # Errors
    /// `PERMISSION_DENIED` without `create_invites` there; transport failure.
    #[wasm_bindgen(js_name = createInvite)]
    pub async fn create_invite(
        &self,
        server_id: String,
        max_uses: u32,
        expires_in_hours: Option<u32>,
    ) -> Result<JsValue, JsError> {
        to_js(
            &self
                .app
                .create_invite(&server_id, max_uses, expires_in_hours)
                .await
                .map_err(js)?,
        )
    }

    /// `POST /invites/{code}/accept`: joins the invite's server; returns its id.
    ///
    /// # Errors
    /// `NOT_FOUND` for an unknown, expired or spent code; transport failure.
    #[wasm_bindgen(js_name = acceptInvite)]
    pub async fn accept_invite(&self, link_or_code: String) -> Result<String, JsError> {
        self.app.accept_invite(&link_or_code).await.map_err(js)
    }

    /// `GET /servers/{id}/invites`: the server's invites, newest first.
    ///
    /// # Errors
    /// `PERMISSION_DENIED` without `create_invites` there; transport failure.
    pub async fn invites(&self, server_id: String) -> Result<JsValue, JsError> {
        to_js(&self.app.invites(&server_id).await.map_err(js)?)
    }

    /// `DELETE /invites/{code}`: revokes an invite.
    ///
    /// # Errors
    /// `PERMISSION_DENIED` unless the creator or a manager; transport failure.
    #[wasm_bindgen(js_name = revokeInvite)]
    pub async fn revoke_invite(&self, link_or_code: String) -> Result<(), JsError> {
        self.app.revoke_invite(&link_or_code).await.map_err(js)
    }

    /// This client's status and the servers it has on screen (protocol 6.7).
    ///
    /// # Errors
    /// When not started or an id is malformed.
    #[wasm_bindgen(js_name = setPresence)]
    pub fn set_presence(
        &self,
        status: String,
        watch_server_ids: Vec<String>,
    ) -> Result<(), JsError> {
        self.app
            .set_presence(&status, &watch_server_ids)
            .map_err(js)
    }

    /// `POST /servers`: a server this account owns.
    ///
    /// # Errors
    /// `INVALID_ARGUMENT` for an empty name; transport failure.
    #[wasm_bindgen(js_name = createServer)]
    pub async fn create_server(&self, name: String) -> Result<JsValue, JsError> {
        to_js(&self.app.create_server(&name).await.map_err(js)?)
    }

    /// `POST /channels`: a text channel in a server this account may manage channels in.
    ///
    /// # Errors
    /// `PERMISSION_DENIED` without `manage_channels` there; transport failure.
    #[wasm_bindgen(js_name = createChannel)]
    pub async fn create_channel(
        &self,
        server_id: String,
        name: String,
    ) -> Result<JsValue, JsError> {
        to_js(
            &self
                .app
                .create_channel(&server_id, &name)
                .await
                .map_err(js)?,
        )
    }

    /// The creation options for a new passkey of this account (architecture 6.3).
    ///
    /// # Errors
    /// On transport or API failure.
    #[wasm_bindgen(js_name = passkeyRegisterStart)]
    pub async fn passkey_register_start(&self) -> Result<JsValue, JsError> {
        to_js(&self.app.passkey_register_start().await.map_err(js)?)
    }

    /// The credential the browser made, kept under `label`.
    ///
    /// # Errors
    /// `UNAUTHENTICATED` for a spent challenge, `ALREADY_EXISTS`; transport failure.
    #[wasm_bindgen(js_name = passkeyRegisterFinish)]
    pub async fn passkey_register_finish(
        &self,
        challenge_id: String,
        credential_json: String,
        label: String,
    ) -> Result<(), JsError> {
        self.app
            .passkey_register_finish(&challenge_id, &credential_json, &label)
            .await
            .map_err(js)
    }

    /// The request options to sign in with a passkey, for a handle when given.
    ///
    /// # Errors
    /// On transport or API failure.
    #[wasm_bindgen(js_name = passkeyAssertStart)]
    pub async fn passkey_assert_start(&self, handle_or_email: String) -> Result<JsValue, JsError> {
        to_js(
            &self
                .app
                .passkey_assert_start(&handle_or_email)
                .await
                .map_err(js)?,
        )
    }

    /// The assertion signs this browser in.
    ///
    /// # Errors
    /// `UNAUTHENTICATED`; transport failure.
    #[wasm_bindgen(js_name = signInPasskey)]
    pub async fn sign_in_passkey(
        &self,
        challenge_id: String,
        credential_json: String,
    ) -> Result<(), JsError> {
        self.app
            .sign_in_passkey(&challenge_id, &credential_json)
            .await
            .map_err(js)
    }

    /// This account's passkeys.
    ///
    /// # Errors
    /// On transport or API failure.
    pub async fn passkeys(&self) -> Result<JsValue, JsError> {
        to_js(&self.app.passkeys().await.map_err(js)?)
    }

    /// Removes one of this account's passkeys.
    ///
    /// # Errors
    /// `NOT_FOUND`; transport failure.
    #[wasm_bindgen(js_name = removePasskey)]
    pub async fn remove_passkey(&self, passkey_id: String) -> Result<(), JsError> {
        self.app.remove_passkey(&passkey_id).await.map_err(js)
    }

    /// Asks the instance to mail a sign-in link to `email`.
    ///
    /// # Errors
    /// `POLICY_DENIED` when the instance sends no mail, `RATE_LIMITED`; transport failure.
    #[wasm_bindgen(js_name = requestMagicLink)]
    pub async fn request_magic_link(&self, email: String) -> Result<(), JsError> {
        self.app.request_magic_link(&email).await.map_err(js)
    }

    /// Signs in with the token from a magic-link mail (the code, or the whole link).
    ///
    /// # Errors
    /// `UNAUTHENTICATED` for an unknown, used or expired token; transport failure.
    #[wasm_bindgen(js_name = signInMagicLink)]
    pub async fn sign_in_magic_link(&self, token: String) -> Result<(), JsError> {
        self.app.sign_in_magic_link(&token).await.map_err(js)
    }

    /// A one-time sign-in token for this account, for a native app that
    /// signed in through this page to take back by its return URL.
    ///
    /// # Errors
    /// On transport or API failure.
    #[wasm_bindgen(js_name = handoffToken)]
    pub async fn handoff_token(&self) -> Result<String, JsError> {
        self.app.handoff_token().await.map_err(js)
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
        self.app
            .sign_in(&handle, &password, totp_code.as_deref())
            .await
            .map_err(js)
    }

    /// Signs out: revokes the session and forgets the tokens.
    ///
    /// # Errors
    /// On a store failure.
    #[wasm_bindgen(js_name = signOut)]
    pub async fn sign_out(&self) -> Result<(), JsError> {
        self.app.sign_out().await.map_err(js)
    }

    /// `GET /account/snapshot`.
    ///
    /// # Errors
    /// On transport or API failure.
    pub async fn snapshot(&self) -> Result<JsValue, JsError> {
        to_js(&self.app.snapshot().await.map_err(js)?)
    }

    /// `GET /accounts/{id}`: a public profile, when visible.
    ///
    /// # Errors
    /// `NOT_FOUND` when not visible; transport failure.
    pub async fn account(&self, account_id: String) -> Result<JsValue, JsError> {
        to_js(&self.app.account(&account_id).await.map_err(js)?)
    }

    /// `GET /servers/{id}/members`.
    ///
    /// # Errors
    /// On transport or API failure.
    pub async fn members(&self, server_id: String, query: String) -> Result<JsValue, JsError> {
        to_js(&self.app.members(&server_id, &query).await.map_err(js)?)
    }

    /// The newest messages of a channel from the local store, oldest first.
    ///
    /// # Errors
    /// On a store failure or a malformed id.
    pub fn messages(&self, channel_id: String, limit: u32) -> Result<JsValue, JsError> {
        to_js(&self.app.messages(&channel_id, limit).map_err(js)?)
    }

    /// Starts the gateway loop on this worker; `listener` is called with
    /// each event object, in order, until `{kind: "closed"}` or `stop`.
    ///
    /// # Errors
    /// When not signed in, already started, or an id is malformed.
    pub fn start(&self, channel_ids: Vec<String>, listener: Function) -> Result<(), JsError> {
        let sink: EventSink = Arc::new(move |event| {
            if let Ok(value) = to_js(&event) {
                let _ = listener.call1(&JsValue::NULL, &value);
            }
        });
        self.app.start(&channel_ids, sink).map_err(js)
    }

    /// Stops the gateway loop; the store comes back once it ends.
    pub fn stop(&self) {
        self.app.stop();
    }

    /// Subscribes to a channel from the store's cursor.
    ///
    /// # Errors
    /// When not started or the id is malformed.
    pub fn subscribe(&self, channel_id: String) -> Result<(), JsError> {
        self.app.subscribe(&channel_id).map_err(js)
    }

    /// Sends a message; returns the client-minted message id.
    ///
    /// # Errors
    /// When not started or the id is malformed.
    #[wasm_bindgen(js_name = sendMessage)]
    pub fn send_message(&self, channel_id: String, body: String) -> Result<String, JsError> {
        self.app.send_message(&channel_id, &body).map_err(js)
    }

    /// The page came back to the foreground: ask for every head.
    ///
    /// # Errors
    /// When not started.
    pub fn foreground(&self) -> Result<(), JsError> {
        self.app.foreground().map_err(js)
    }

    // ---- Identity (a web device is linked from a native one) ----

    /// Whether this device holds an identity roster.
    #[must_use]
    #[wasm_bindgen(js_name = hasIdentity)]
    pub fn has_identity(&self) -> bool {
        self.app.has_identity()
    }

    /// Step 1 of linking this device: the request for another device to approve.
    ///
    /// # Errors
    /// On a store or keystore failure.
    #[wasm_bindgen(js_name = identityLinkRequest)]
    pub async fn identity_link_request(&self, label: String) -> Result<String, JsError> {
        self.app.identity_link_request(&label).await.map_err(js)
    }

    /// Step 7: claims the bundle the approving device uploaded, then signs in.
    ///
    /// # Errors
    /// `NOT_FOUND` for a wrong or used code.
    #[wasm_bindgen(js_name = identityClaim)]
    pub async fn identity_claim(&self, code: String) -> Result<JsValue, JsError> {
        to_js(&self.app.identity_claim(&code).await.map_err(js)?)
    }

    /// Signs in by identity proof with the roster this device holds.
    ///
    /// # Errors
    /// On API failure, or when the device is revoked in the held roster.
    #[wasm_bindgen(js_name = signInIdentity)]
    pub async fn sign_in_identity(&self) -> Result<(), JsError> {
        self.app.sign_in_identity().await.map_err(js)
    }

    /// The roster this device holds.
    ///
    /// # Errors
    /// When there is none.
    /// The identity's signed profile, if one was set on this device.
    ///
    /// # Errors
    /// On a store failure.
    #[wasm_bindgen(js_name = identityProfile)]
    pub async fn identity_profile(&self) -> Result<JsValue, JsError> {
        to_js(&self.app.identity_profile().await.map_err(js)?)
    }

    /// Signs a new profile with this device's key and, when signed in, sends it.
    ///
    /// # Errors
    /// When this device holds no identity, or when the instance refuses it.
    #[wasm_bindgen(js_name = identitySetProfile)]
    pub async fn identity_set_profile(
        &self,
        display_name: String,
        pronouns: String,
        bio: String,
        avatar_url: String,
    ) -> Result<JsValue, JsError> {
        to_js(
            &self
                .app
                .identity_set_profile(&display_name, &pronouns, &bio, &avatar_url)
                .await
                .map_err(js)?,
        )
    }

    #[wasm_bindgen(js_name = identityShow)]
    pub async fn identity_show(&self) -> Result<JsValue, JsError> {
        to_js(&self.app.identity_show().await.map_err(js)?)
    }

    /// Revokes another device, signed by this one with the password as reauth.
    ///
    /// # Errors
    /// On API failure, a wrong password, or when asked to revoke this device.
    #[wasm_bindgen(js_name = identityRevoke)]
    pub async fn identity_revoke(
        &self,
        device_id: String,
        password: String,
    ) -> Result<JsValue, JsError> {
        to_js(
            &self
                .app
                .identity_revoke(&device_id, &password)
                .await
                .map_err(js)?,
        )
    }

    // ---- Push ----

    /// Registers this browser's Web Push subscription with the instance.
    ///
    /// # Errors
    /// `INVALID_ARGUMENT` for a malformed subscription; transport failure.
    #[wasm_bindgen(js_name = pushSubscribe)]
    pub async fn push_subscribe(
        &self,
        endpoint: String,
        p256dh: String,
        auth: String,
    ) -> Result<(), JsError> {
        self.app
            .push_subscribe(PushKind::WebPush, &endpoint, &p256dh, &auth)
            .await
            .map_err(js)
    }

    /// Removes this browser's Web Push subscription from the instance.
    ///
    /// # Errors
    /// On transport or API failure.
    #[wasm_bindgen(js_name = pushUnsubscribe)]
    pub async fn push_unsubscribe(&self) -> Result<(), JsError> {
        self.app.push_unsubscribe("web_push").await.map_err(js)
    }

    // ---- Personal DMs ----

    /// Whether personal DMs work on this device: it holds an identity.
    #[must_use]
    #[wasm_bindgen(js_name = dmAvailable)]
    pub fn dm_available(&self) -> bool {
        self.app.dm_available()
    }

    /// The personal conversations this device knows.
    ///
    /// # Errors
    /// Without an identity, or on a store failure.
    #[wasm_bindgen(js_name = dmChannels)]
    pub async fn dm_channels(&self) -> Result<JsValue, JsError> {
        to_js(&self.app.dm_channels().await.map_err(js)?)
    }

    /// Opens (or finds) the conversation with `account_ids`.
    ///
    /// # Errors
    /// `FAILED_PRECONDITION` without an accepted request; without an
    /// identity; before the gateway loop started.
    #[wasm_bindgen(js_name = dmOpen)]
    pub async fn dm_open(&self, account_ids: Vec<String>) -> Result<JsValue, JsError> {
        to_js(&self.app.dm_open(&account_ids).await.map_err(js)?)
    }

    /// Sends a DM request to an account.
    ///
    /// # Errors
    /// `ALREADY_EXISTS` when the pair already has a personal DM.
    #[wasm_bindgen(js_name = dmRequest)]
    pub async fn dm_request(&self, account_id: String) -> Result<JsValue, JsError> {
        to_js(&self.app.dm_request(&account_id).await.map_err(js)?)
    }

    /// Answers a DM request: `accept`, `decline` or `block`.
    ///
    /// # Errors
    /// On API failure, an unknown response, or without an identity when accepting.
    #[wasm_bindgen(js_name = dmRespond)]
    pub async fn dm_respond(
        &self,
        request_id: String,
        response: String,
    ) -> Result<JsValue, JsError> {
        match self
            .app
            .dm_respond(&request_id, &response)
            .await
            .map_err(js)?
        {
            Ok(opened) => to_js(&opened),
            Err(request) => to_js(&request),
        }
    }

    /// Sends a message in a personal DM.
    ///
    /// # Errors
    /// Without the group for the conversation, or before the loop started.
    #[wasm_bindgen(js_name = dmSend)]
    pub async fn dm_send(&self, channel_id: String, body: String) -> Result<(), JsError> {
        self.app.dm_send(&channel_id, &body).await.map_err(js)
    }

    /// Deletes one of this identity's messages in a personal DM.
    ///
    /// # Errors
    /// Without the group for the conversation, or before the loop started.
    #[wasm_bindgen(js_name = dmDelete)]
    pub async fn dm_delete(&self, channel_id: String, message_id: String) -> Result<(), JsError> {
        self.app
            .dm_delete(&channel_id, &message_id)
            .await
            .map_err(js)
    }

    /// Rejoins a conversation by external commit.
    ///
    /// # Errors
    /// Without an identity, or when the instance holds no group info.
    #[wasm_bindgen(js_name = dmRejoin)]
    pub async fn dm_rejoin(&self, channel_id: String) -> Result<(), JsError> {
        self.app.dm_rejoin(&channel_id).await.map_err(js)
    }

    /// The conversation as this device can read it.
    ///
    /// # Errors
    /// Without an identity, or on a store failure.
    #[wasm_bindgen(js_name = dmMessages)]
    pub async fn dm_messages(&self, channel_id: String) -> Result<JsValue, JsError> {
        to_js(&self.app.dm_messages(&channel_id).await.map_err(js)?)
    }
}
