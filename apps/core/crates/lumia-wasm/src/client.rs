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
use lumia_client::app::{
    App, EventSink, MentionSummary, MessageSummary, Opener, Place, QuoteSummary, StoreName,
};
use lumia_client::sqlite::SqliteStore;
use lumia_client::ClientError;
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

/// An API error with its details on the text, after the message, so the
/// page can read `details.mfa` and the like (bridge.ts parses them).
fn js_api(e: ClientError) -> JsError {
    if let ClientError::Api { details, .. } = &e {
        if !details.is_empty() {
            let mut pairs: Vec<String> = details.iter().map(|(k, v)| format!("{k}={v}")).collect();
            pairs.sort();
            return JsError::new(&format!("{e} details={}", pairs.join(",")));
        }
    }
    JsError::new(&e.to_string())
}

fn to_js<T: Serialize>(value: &T) -> Result<JsValue, JsError> {
    serde_wasm_bindgen::to_value(value).map_err(|e| JsError::new(&e.to_string()))
}

/// A page of messages, as `mentions` and `olderMessages` give it.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct MentionsPage {
    messages: Vec<MessageSummary>,
    has_more: bool,
}

/// A summary object from the page; `null` or `undefined` is `None`.
fn from_js<T: for<'de> serde::Deserialize<'de>>(value: JsValue) -> Result<Option<T>, JsError> {
    if value.is_null() || value.is_undefined() {
        return Ok(None);
    }
    serde_wasm_bindgen::from_value(value)
        .map(Some)
        .map_err(|e| JsError::new(&e.to_string()))
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
    /// encrypted under `store_key` (32 bytes) when given. The device is a
    /// WEB one, scoped to the page's origin.
    ///
    /// # Errors
    /// When the VFS cannot be installed or a store cannot be opened.
    pub async fn open(base_url: String, store_key: Option<Vec<u8>>) -> Result<Client, JsError> {
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
        let place = Place::Web { origin: origin() };
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

    /// `POST /invites/email`: a one-use invite good for a week, mailed to
    /// the address as a link (architecture 6.2).
    ///
    /// # Errors
    /// `PERMISSION_DENIED`, `FAILED_PRECONDITION` (no mail), `INVALID_ARGUMENT`; transport failure.
    #[wasm_bindgen(js_name = emailInvite)]
    pub async fn email_invite(&self, server_id: String, email: String) -> Result<JsValue, JsError> {
        to_js(
            &self
                .app
                .email_invite(&server_id, &email)
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

    /// The servers this client has on screen, whose presence it wants
    /// (protocol 6.7); sent with the chosen status.
    ///
    /// # Errors
    /// When not started or an id is malformed.
    #[wasm_bindgen(js_name = watchPresence)]
    pub fn watch_presence(&self, watch_server_ids: Vec<String>) -> Result<(), JsError> {
        self.app.watch_presence(&watch_server_ids).map_err(js)
    }

    /// Says this person is typing in a server's channel (protocol 6.7);
    /// call it on every edit of the composer, the core spaces them out.
    ///
    /// # Errors
    /// When not started or the id is malformed.
    #[wasm_bindgen(js_name = typing)]
    pub fn typing(&self, channel_id: &str) -> Result<(), JsError> {
        self.app.typing(channel_id).map_err(js)
    }

    /// The status this person chose and their custom status text.
    ///
    /// # Errors
    /// On a store failure.
    #[wasm_bindgen(js_name = status)]
    pub fn status(&self) -> Result<JsValue, JsError> {
        to_js(&self.app.status().map_err(js)?)
    }

    /// Chooses the status (`ONLINE`, `AWAY`, `DO_NOT_DISTURB`) and custom
    /// text; sent at once when the gateway is up, and on every welcome.
    ///
    /// # Errors
    /// On a store failure.
    #[wasm_bindgen(js_name = setStatus)]
    pub fn set_status(&self, status: String, custom_status: String) -> Result<(), JsError> {
        self.app.set_status(&status, &custom_status).map_err(js)
    }

    /// `POST /servers`: a server this account owns.
    ///
    /// # Errors
    /// `INVALID_ARGUMENT` for an empty name; transport failure.
    #[wasm_bindgen(js_name = createServer)]
    pub async fn create_server(&self, name: String) -> Result<JsValue, JsError> {
        to_js(&self.app.create_server(&name).await.map_err(js)?)
    }

    /// `PATCH /servers/{id}`: a server's name, how long it keeps messages in
    /// days (0 for ever), or both; one left out stays. The snapshot as it
    /// then is.
    ///
    /// # Errors
    /// `PERMISSION_DENIED` without `MANAGE_SERVER`; `INVALID_ARGUMENT` with
    /// its details (the page words it by `details.field`); transport failure.
    #[wasm_bindgen(js_name = updateServer)]
    pub async fn update_server(
        &self,
        server_id: String,
        name: Option<String>,
        retention_days: Option<u32>,
    ) -> Result<JsValue, JsError> {
        to_js(
            &self
                .app
                .update_server(&server_id, name.as_deref(), retention_days)
                .await
                .map_err(js_api)?,
        )
    }

    /// `PUT /servers/{id}/icon`: `picture` is a file's bytes as chosen. The
    /// snapshot as it then is.
    ///
    /// # Errors
    /// As `setAvatar`, and `PERMISSION_DENIED` without `MANAGE_SERVER`.
    #[wasm_bindgen(js_name = setServerIcon)]
    pub async fn set_server_icon(
        &self,
        server_id: String,
        picture: Vec<u8>,
        media_type: String,
    ) -> Result<JsValue, JsError> {
        to_js(
            &self
                .app
                .set_server_icon(&server_id, picture, &media_type)
                .await
                .map_err(js_api)?,
        )
    }

    /// `DELETE /servers/{id}/icon`.
    ///
    /// # Errors
    /// `PERMISSION_DENIED` without `MANAGE_SERVER`; transport failure.
    #[wasm_bindgen(js_name = removeServerIcon)]
    pub async fn remove_server_icon(&self, server_id: String) -> Result<JsValue, JsError> {
        to_js(
            &self
                .app
                .remove_server_icon(&server_id)
                .await
                .map_err(js_api)?,
        )
    }

    /// `POST /channels`: a text channel in a server this account may manage
    /// channels in; private when `private` is true (architecture 6.8).
    ///
    /// # Errors
    /// `PERMISSION_DENIED` without `manage_channels` there; transport failure.
    #[wasm_bindgen(js_name = createChannel)]
    pub async fn create_channel(
        &self,
        server_id: String,
        name: String,
        private: Option<bool>,
    ) -> Result<JsValue, JsError> {
        to_js(
            &self
                .app
                .create_channel(&server_id, &name, private.unwrap_or(false))
                .await
                .map_err(js_api)?,
        )
    }

    /// `GET /servers/{id}/channels/browse`: the public channels this account
    /// can see and is not in, with how many are in each.
    ///
    /// # Errors
    /// On transport or API failure.
    #[wasm_bindgen(js_name = browseChannels)]
    pub async fn browse_channels(&self, server_id: String) -> Result<JsValue, JsError> {
        to_js(&self.app.browse_channels(&server_id).await.map_err(js_api)?)
    }

    /// `POST /channels/{id}/join`. The snapshot as it then is.
    ///
    /// # Errors
    /// `NOT_FOUND` for a channel it may not join; transport failure.
    #[wasm_bindgen(js_name = joinChannel)]
    pub async fn join_channel(&self, channel_id: String) -> Result<JsValue, JsError> {
        to_js(&self.app.join_channel(&channel_id).await.map_err(js_api)?)
    }

    /// `POST /channels/{id}/leave`. The snapshot as it then is.
    ///
    /// # Errors
    /// `NOT_FOUND` when not in it; transport failure.
    #[wasm_bindgen(js_name = leaveChannel)]
    pub async fn leave_channel(&self, channel_id: String) -> Result<JsValue, JsError> {
        to_js(&self.app.leave_channel(&channel_id).await.map_err(js_api)?)
    }

    /// `GET /channels/{id}/members`: account ids, in the order they came in.
    ///
    /// # Errors
    /// `NOT_FOUND` for a channel it cannot see; transport failure.
    #[wasm_bindgen(js_name = channelMembers)]
    pub async fn channel_members(&self, channel_id: String) -> Result<JsValue, JsError> {
        to_js(
            &self
                .app
                .channel_members(&channel_id)
                .await
                .map_err(js_api)?,
        )
    }

    /// `POST /channels/{id}/members`: who was added and who invited.
    ///
    /// # Errors
    /// `PERMISSION_DENIED`, `NOT_FOUND`, or `INVALID_ARGUMENT` with
    /// `details.field` `account_ids`; transport failure.
    #[wasm_bindgen(js_name = addChannelMembers)]
    pub async fn add_channel_members(
        &self,
        channel_id: String,
        account_ids: Vec<String>,
    ) -> Result<JsValue, JsError> {
        to_js(
            &self
                .app
                .add_channel_members(&channel_id, &account_ids)
                .await
                .map_err(js_api)?,
        )
    }

    /// `DELETE /channels/{id}/members/{account_id}`.
    ///
    /// # Errors
    /// `PERMISSION_DENIED` without `MANAGE_CHANNELS`; transport failure.
    #[wasm_bindgen(js_name = removeChannelMember)]
    pub async fn remove_channel_member(
        &self,
        channel_id: String,
        account_id: String,
    ) -> Result<(), JsError> {
        self.app
            .remove_channel_member(&channel_id, &account_id)
            .await
            .map_err(js_api)
    }

    /// `DELETE /channel-invitations/{channel_id}`. The snapshot as it then is.
    ///
    /// # Errors
    /// `NOT_FOUND` without one; transport failure.
    #[wasm_bindgen(js_name = dismissChannelInvitation)]
    pub async fn dismiss_channel_invitation(&self, channel_id: String) -> Result<JsValue, JsError> {
        to_js(
            &self
                .app
                .dismiss_channel_invitation(&channel_id)
                .await
                .map_err(js_api)?,
        )
    }

    /// `PATCH /servers/{id}/membership`: this account's own switches there;
    /// one left out stays. The snapshot as it then is.
    ///
    /// # Errors
    /// `NOT_FOUND` for a server it is not in; transport failure.
    #[wasm_bindgen(js_name = updateMembership)]
    pub async fn update_membership(
        &self,
        server_id: String,
        channel_adds_by_invitation: Option<bool>,
        server_dms_off: Option<bool>,
    ) -> Result<JsValue, JsError> {
        to_js(
            &self
                .app
                .update_membership(&server_id, channel_adds_by_invitation, server_dms_off)
                .await
                .map_err(js_api)?,
        )
    }

    /// `POST /servers/{id}/dms`: the server DM with a member, to open.
    ///
    /// # Errors
    /// `PERMISSION_DENIED`, `FAILED_PRECONDITION` with `details.state`
    /// `server_dms_off`, `NOT_FOUND`; transport failure.
    #[wasm_bindgen(js_name = openServerDm)]
    pub async fn open_server_dm(
        &self,
        server_id: String,
        account_id: String,
    ) -> Result<JsValue, JsError> {
        to_js(
            &self
                .app
                .open_server_dm(&server_id, &account_id)
                .await
                .map_err(js_api)?,
        )
    }

    /// `PATCH /servers/{id}` with the default channels. The snapshot as it then is.
    ///
    /// # Errors
    /// `PERMISSION_DENIED` without `MANAGE_SERVER`; `INVALID_ARGUMENT` with
    /// `details.field` `default_channels`; transport failure.
    #[wasm_bindgen(js_name = setDefaultChannels)]
    pub async fn set_default_channels(
        &self,
        server_id: String,
        channel_ids: Vec<String>,
    ) -> Result<JsValue, JsError> {
        to_js(
            &self
                .app
                .set_default_channels(&server_id, &channel_ids)
                .await
                .map_err(js_api)?,
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

    /// `POST /auth/sign-in/password` with the passkey factor (protocol 6.1).
    ///
    /// # Errors
    /// `UNAUTHENTICATED`; transport failure.
    #[wasm_bindgen(js_name = signInWithPasskey)]
    pub async fn sign_in_with_passkey(
        &self,
        handle: String,
        password: String,
        challenge_id: String,
        credential_json: String,
    ) -> Result<(), JsError> {
        self.app
            .sign_in_with_passkey(&handle, &password, &challenge_id, &credential_json)
            .await
            .map_err(js_api)
    }

    /// Whether a password sign-in must also present one of the account's passkeys.
    ///
    /// # Errors
    /// On transport or API failure.
    #[wasm_bindgen(js_name = passkeyRequired)]
    pub async fn passkey_required(&self) -> Result<bool, JsError> {
        self.app.passkey_required().await.map_err(js)
    }

    /// `PUT /auth/passkeys/required`.
    ///
    /// # Errors
    /// `FAILED_PRECONDITION` with `state` `no_passkey`; transport failure.
    #[wasm_bindgen(js_name = setPasskeyRequired)]
    pub async fn set_passkey_required(&self, required: bool) -> Result<(), JsError> {
        self.app
            .set_passkey_required(required)
            .await
            .map_err(js_api)
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
            .map_err(js_api)
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

    /// `PATCH /account`: this account's handle, display name, or both; a
    /// value left out stays, an empty display name clears it. Resolves to
    /// the snapshot as it then is.
    ///
    /// # Errors
    /// `ALREADY_EXISTS` for a handle that is taken or held,
    /// `INVALID_ARGUMENT` for one that is not a handle.
    #[wasm_bindgen(js_name = updateAccount)]
    pub async fn update_account(
        &self,
        handle: Option<String>,
        display_name: Option<String>,
    ) -> Result<JsValue, JsError> {
        let snapshot = self
            .app
            .update_account(handle.as_deref(), display_name.as_deref())
            .await
            .map_err(js)?;
        to_js(&snapshot)
    }

    /// `PUT /account/avatar`: `picture` is a file's bytes as chosen (PNG,
    /// JPEG, WebP or GIF, at most 8 MB); the instance keeps its own square
    /// of it. Resolves to the snapshot as it then is.
    ///
    /// # Errors
    /// `INVALID_ARGUMENT` for what is not a picture it takes,
    /// `FAILED_PRECONDITION` on an instance that keeps none.
    #[wasm_bindgen(js_name = setAvatar)]
    pub async fn set_avatar(
        &self,
        picture: Vec<u8>,
        media_type: String,
    ) -> Result<JsValue, JsError> {
        to_js(
            &self
                .app
                .set_avatar(picture, &media_type)
                .await
                // With its details: the page words the refusal by `details.reason`.
                .map_err(js_api)?,
        )
    }

    /// `DELETE /account/avatar`. Resolves to the snapshot as it then is.
    ///
    /// # Errors
    /// On transport or API failure.
    #[wasm_bindgen(js_name = removeAvatar)]
    pub async fn remove_avatar(&self) -> Result<JsValue, JsError> {
        to_js(&self.app.remove_avatar().await.map_err(js)?)
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

    /// `GET /channels/{id}/messages`: scrollback from the instance, older
    /// than the store holds: `{messages, hasMore}`, the newest `limit` below
    /// `before_seq`, oldest first. Not stored.
    ///
    /// # Errors
    /// On transport or API failure.
    #[wasm_bindgen(js_name = olderMessages)]
    pub async fn older_messages(
        &self,
        channel_id: String,
        before_seq: f64,
        limit: u32,
    ) -> Result<JsValue, JsError> {
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let before_seq = before_seq.max(0.0) as u64;
        let (messages, has_more) = self
            .app
            .older_messages(&channel_id, before_seq, limit)
            .await
            .map_err(js)?;
        to_js(&MentionsPage { messages, has_more })
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

    /// Sends a message with its mentions (`[{accountId, everyone, start,
    /// end}]`) and what it quotes (`{messageId, text}` or null); returns
    /// the client-minted message id.
    ///
    /// # Errors
    /// When not started or an id is malformed.
    #[wasm_bindgen(js_name = sendMessageWith)]
    pub fn send_message_with(
        &self,
        channel_id: String,
        body: String,
        mentions: JsValue,
        reply_to: JsValue,
    ) -> Result<String, JsError> {
        let mentions: Vec<MentionSummary> = from_js(mentions)?.unwrap_or_default();
        let reply_to: Option<QuoteSummary> = from_js(reply_to)?;
        self.app
            .send_message_with(&channel_id, &body, &mentions, reply_to.as_ref())
            .map_err(js)
    }

    /// The mentions `body` makes of `members` (`MemberSummary` objects),
    /// and of everyone when `everyone`: `[{accountId, everyone, start,
    /// end}]`, UTF-8 byte ranges.
    ///
    /// # Errors
    /// When `members` is not a list of members.
    #[wasm_bindgen(js_name = findMentions)]
    pub fn find_mentions(
        &self,
        body: String,
        members: JsValue,
        everyone: bool,
    ) -> Result<JsValue, JsError> {
        let members: Vec<lumia_client::app::MemberSummary> = from_js(members)?.unwrap_or_default();
        to_js(&lumia_client::app::find_mentions(&body, &members, everyone))
    }

    /// Edits one of this account's messages in a server's channel: its new
    /// body and mentions (`[{accountId, everyone, start, end}]`); returns the
    /// command id a refusal's error names.
    ///
    /// # Errors
    /// When not started or an id is malformed.
    #[wasm_bindgen(js_name = editMessage)]
    pub fn edit_message(
        &self,
        channel_id: String,
        message_id: String,
        body: String,
        mentions: JsValue,
    ) -> Result<String, JsError> {
        let mentions: Vec<MentionSummary> = from_js(mentions)?.unwrap_or_default();
        self.app
            .edit_message(&channel_id, &message_id, &body, &mentions)
            .map_err(js)
    }

    /// Deletes a message of a server's channel: this account's own, or any
    /// for a moderator; returns the command id a refusal's error names.
    ///
    /// # Errors
    /// When not started or an id is malformed.
    #[wasm_bindgen(js_name = deleteMessage)]
    pub fn delete_message(
        &self,
        channel_id: String,
        message_id: String,
    ) -> Result<String, JsError> {
        self.app
            .delete_message(&channel_id, &message_id)
            .map_err(js)
    }

    /// Adds this account's reaction to a message, or takes it away.
    ///
    /// # Errors
    /// When not started or an id is malformed.
    pub fn react(
        &self,
        channel_id: String,
        message_id: String,
        emoji: String,
        remove: bool,
    ) -> Result<(), JsError> {
        self.app
            .react(&channel_id, &message_id, &emoji, remove)
            .map_err(js)
    }

    /// Every conversation's read state as this client counts it.
    ///
    /// # Errors
    /// When the states cannot be converted.
    #[wasm_bindgen(js_name = readStates)]
    pub fn read_states(&self) -> Result<JsValue, JsError> {
        to_js(&self.app.read_states())
    }

    /// The badge: what notifies this account and is unread.
    #[must_use]
    pub fn badge(&self) -> u32 {
        self.app.badge()
    }

    /// `POST /channels/{id}/read`: read through `seq`; the state as the
    /// instance then counts it.
    ///
    /// # Errors
    /// On transport or API failure.
    #[wasm_bindgen(js_name = markRead)]
    pub async fn mark_read(&self, channel_id: String, seq: f64) -> Result<JsValue, JsError> {
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let seq = seq.max(0.0) as u64;
        to_js(&self.app.mark_read(&channel_id, seq).await.map_err(js)?)
    }

    /// `GET /account/mentions`: `{messages, hasMore}`, newest first, in
    /// one server or all.
    ///
    /// # Errors
    /// On transport or API failure.
    pub async fn mentions(
        &self,
        server_id: Option<String>,
        before: Option<String>,
    ) -> Result<JsValue, JsError> {
        let (messages, has_more) = self
            .app
            .mentions(server_id.as_deref(), before.as_deref())
            .await
            .map_err(js)?;
        to_js(&MentionsPage { messages, has_more })
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
        self.app
            .dm_send(&channel_id, &body)
            .await
            .map(|_id| ())
            .map_err(js)
    }

    /// Sends a message in a personal DM quoting another (`{messageId,
    /// text}` or null).
    ///
    /// # Errors
    /// Without the group for the conversation, or before the loop started.
    #[wasm_bindgen(js_name = dmSendWith)]
    pub async fn dm_send_with(
        &self,
        channel_id: String,
        body: String,
        quote: JsValue,
    ) -> Result<(), JsError> {
        let quote: Option<QuoteSummary> = from_js(quote)?;
        self.app
            .dm_send_with(&channel_id, &body, quote.as_ref())
            .await
            .map(|_id| ())
            .map_err(js)
    }

    /// Adds this identity's reaction to a message of a personal DM, or
    /// takes it away.
    ///
    /// # Errors
    /// Without the group for the conversation, or before the loop started.
    #[wasm_bindgen(js_name = dmReact)]
    pub async fn dm_react(
        &self,
        channel_id: String,
        message_id: String,
        emoji: String,
        remove: bool,
    ) -> Result<(), JsError> {
        self.app
            .dm_react(&channel_id, &message_id, &emoji, remove)
            .await
            .map_err(js)
    }

    /// Edits one of this identity's messages in a personal DM.
    ///
    /// # Errors
    /// Without the group for the conversation, or before the loop started.
    #[wasm_bindgen(js_name = dmEdit)]
    pub async fn dm_edit(
        &self,
        channel_id: String,
        message_id: String,
        body: String,
    ) -> Result<(), JsError> {
        self.app
            .dm_edit(&channel_id, &message_id, &body)
            .await
            .map_err(js)
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
