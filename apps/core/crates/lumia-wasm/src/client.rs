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
    App, AttachmentInput, BlobSummary, EventSink, MentionSummary, MessageSummary, Opener,
    PixelsInput, Place, QuoteSummary, StoreName,
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

    /// Whether a server's deleted messages keep their place as "(deleted)"
    /// (architecture 6.7). Returns the snapshot as it then is.
    ///
    /// # Errors
    /// `PERMISSION_DENIED` without `MANAGE_SERVER`; transport failure.
    #[wasm_bindgen(js_name = setShowDeleted)]
    pub async fn set_show_deleted(
        &self,
        server_id: String,
        show: bool,
    ) -> Result<JsValue, JsError> {
        to_js(
            &self
                .app
                .set_show_deleted(&server_id, show)
                .await
                .map_err(js_api)?,
        )
    }

    /// How long a server keeps its attachments in days (0 for ever), and
    /// whether a full one lets its oldest go; one left out stays
    /// (specs/media.md 4.5). The snapshot as it then is.
    ///
    /// # Errors
    /// `PERMISSION_DENIED` without `MANAGE_SERVER`; transport failure.
    #[wasm_bindgen(js_name = setServerFiles)]
    pub async fn set_server_files(
        &self,
        server_id: String,
        attachment_retention_days: Option<u32>,
        delete_oldest_when_full: Option<bool>,
    ) -> Result<JsValue, JsError> {
        to_js(
            &self
                .app
                .set_server_files(
                    &server_id,
                    attachment_retention_days,
                    delete_oldest_when_full,
                )
                .await
                .map_err(js_api)?,
        )
    }

    /// What a server's files take, `{usedBytes, maxBytes}` (`maxBytes` 0 for
    /// no limit), for its settings.
    ///
    /// # Errors
    /// `PERMISSION_DENIED` without `MANAGE_SERVER`; transport failure.
    #[wasm_bindgen(js_name = serverStorage)]
    pub async fn server_storage(&self, server_id: String) -> Result<JsValue, JsError> {
        to_js(&self.app.server_storage(&server_id).await.map_err(js_api)?)
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

    /// What was typed into a conversation's composer and not sent, from
    /// the local store: `{body, reply}`, or `null` when nothing is kept.
    ///
    /// # Errors
    /// On a store failure or a malformed id.
    pub fn draft(&self, channel_id: String) -> Result<JsValue, JsError> {
        to_js(&self.app.draft(&channel_id).map_err(js)?)
    }

    /// Keeps what is typed in a conversation's composer, with the message
    /// it is a reply to (or `null`), in place of what was kept before;
    /// words that are empty or all blank keep nothing. In this browser
    /// only.
    ///
    /// # Errors
    /// On a store failure or a malformed id.
    #[wasm_bindgen(js_name = setDraft)]
    pub fn set_draft(
        &self,
        channel_id: String,
        body: String,
        reply: JsValue,
    ) -> Result<(), JsError> {
        let reply: Option<QuoteSummary> = from_js(reply)?;
        self.app
            .set_draft(&channel_id, &body, reply.as_ref())
            .map_err(js)
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

    /// Keeps `items` (`[{kind: "server", serverId} | {kind: "folder", folderId, name, tone,
    /// serverIds}]`) as this account's rail, whole (architecture 6.9). Returns the snapshot.
    ///
    /// # Errors
    /// When `items` is not a rail; `INVALID_ARGUMENT` with its field; transport failure.
    #[wasm_bindgen(js_name = arrangeRail)]
    pub async fn arrange_rail(&self, items: JsValue) -> Result<JsValue, JsError> {
        let items: Vec<lumia_client::rail::RailItem> = from_js(items)?.unwrap_or_default();
        to_js(&self.app.arrange_rail(&items).await.map_err(js_api)?)
    }

    /// Applies one rail edit (architecture 6.9) and answers the rail as it then is: `op` is
    /// `moveTo` (`id`, `folder`, `before`), `merge` (`dragged`, `onto`, `folderId`),
    /// `intoFolder` (`serverId`, `folder`), `outOfFolder` (`serverId`), `newFolder`
    /// (`serverId`, `folderId`), `deleteFolder` (`folder`), `setFolder` (`folder`, `name`,
    /// `tone`) or `step` (`id`, `by`). A new folder's id comes from `railNewFolderId`.
    ///
    /// # Errors
    /// When `items` is not a rail or `op` is not one of these.
    #[wasm_bindgen(js_name = railEdit)]
    pub fn rail_edit(&self, items: JsValue, op: String, args: JsValue) -> Result<JsValue, JsError> {
        use lumia_client::rail as r;
        #[derive(serde::Deserialize, Default)]
        #[serde(rename_all = "camelCase", default)]
        struct Args {
            id: String,
            folder: Option<String>,
            before: Option<String>,
            dragged: String,
            onto: String,
            folder_id: String,
            server_id: String,
            name: String,
            tone: String,
            by: i32,
        }
        let items: Vec<r::RailItem> = from_js(items)?.unwrap_or_default();
        let a: Args = from_js(args)?.unwrap_or_default();
        let next = match op.as_str() {
            "moveTo" => r::move_to(&items, &a.id, a.folder.as_deref(), a.before.as_deref()),
            "merge" => r::merge(&items, &a.dragged, &a.onto, &a.folder_id),
            "intoFolder" => r::into_folder(
                &items,
                &a.server_id,
                a.folder.as_deref().unwrap_or_default(),
            ),
            "outOfFolder" => r::out_of_folder(&items, &a.server_id),
            "newFolder" => r::new_folder(&items, &a.server_id, &a.folder_id),
            "deleteFolder" => r::delete_folder(&items, a.folder.as_deref().unwrap_or_default()),
            "setFolder" => r::set_folder(
                &items,
                a.folder.as_deref().unwrap_or_default(),
                &a.name,
                &a.tone,
            ),
            "step" => r::step(&items, &a.id, a.by),
            other => return Err(JsError::new(&format!("not a rail edit: {other}"))),
        };
        to_js(&next)
    }

    /// A new folder's id (UUIDv7).
    #[wasm_bindgen(js_name = railNewFolderId)]
    pub fn rail_new_folder_id(&self) -> String {
        lumia_client::rail::new_folder_id()
    }

    /// The tone an id is drawn in, as every shell hashes it.
    #[wasm_bindgen(js_name = railToneOf)]
    pub fn rail_tone_of(&self, id: String) -> String {
        lumia_client::rail::tone_of(&id)
    }

    /// How a folder with no name reads: "Acme, Lumia and 2 more".
    ///
    /// # Errors
    /// When `names` is not a list of strings.
    #[wasm_bindgen(js_name = railFolderWords)]
    pub fn rail_folder_words(&self, names: JsValue) -> Result<String, JsError> {
        let names: Vec<String> = from_js(names)?.unwrap_or_default();
        Ok(lumia_client::rail::folder_words(&names))
    }

    /// The largest message body the instance takes, in bytes of UTF-8.
    #[wasm_bindgen(js_name = maxMessageBytes)]
    pub fn max_message_bytes(&self) -> u32 {
        self.app.max_message_bytes()
    }

    /// The room `body` has under the instance's limit, for the composer's
    /// counter: `{shown, left}`, `left` negative when it is that many
    /// characters too long.
    ///
    /// # Errors
    /// Never in practice; marshalling.
    #[wasm_bindgen(js_name = messageRoom)]
    pub fn message_room(&self, body: String) -> Result<JsValue, JsError> {
        to_js(&lumia_client::body::room(
            &body,
            self.app.max_message_bytes(),
        ))
    }

    /// `body` read into blocks (protocol 14) with `mentions` (`[{start,
    /// end}]`, byte ranges) marked; ranges in the blocks are UTF-16, as
    /// JavaScript counts. `[{kind: "para", text, runs: [{start, end,
    /// style: {kind, index?}}], links: [{start, end, url}]} | {kind:
    /// "code", lang, text} | {kind: "quote", blocks} | {kind: "list",
    /// start, items: [[block]]}]`.
    ///
    /// # Errors
    /// When `mentions` is not a list of ranges.
    #[wasm_bindgen(js_name = parseBody)]
    pub fn parse_body(&self, body: String, mentions: JsValue) -> Result<JsValue, JsError> {
        let mentions: Vec<MentionSummary> = from_js(mentions)?.unwrap_or_default();
        let ranges: Vec<std::ops::Range<usize>> = mentions
            .iter()
            .map(|m| m.start as usize..m.end as usize)
            .collect();
        to_js(&lumia_client::body::display(&body, &ranges))
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

    // ---- Identity (identity spec 7: linked, created, or restored on the flagship's origin) ----

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

    /// The identity the signed-in account is bound to here, hex, or null.
    ///
    /// # Errors
    /// When not signed in, or on transport failure.
    #[wasm_bindgen(js_name = accountIdentity)]
    pub async fn account_identity(&self) -> Result<Option<String>, JsError> {
        self.app.account_identity().await.map_err(js_api)
    }

    /// Whether a reauth token is in hand for identity operations.
    #[must_use]
    #[wasm_bindgen(js_name = hasReauth)]
    pub fn has_reauth(&self) -> bool {
        self.app.has_reauth()
    }

    /// Reauth by the account's password.
    ///
    /// # Errors
    /// `UNAUTHENTICATED` for a wrong one.
    #[wasm_bindgen(js_name = reauthPassword)]
    pub async fn reauth_password(&self, password: String) -> Result<(), JsError> {
        self.app.reauth_password(&password).await.map_err(js_api)
    }

    /// Reauth by a TOTP code, or by the mailed code when `mailed`.
    ///
    /// # Errors
    /// `UNAUTHENTICATED` for a wrong one.
    #[wasm_bindgen(js_name = reauthCode)]
    pub async fn reauth_code(&self, code: String, mailed: bool) -> Result<(), JsError> {
        self.app.reauth_code(&code, mailed).await.map_err(js_api)
    }

    /// Mails six digits to the account's address for `reauthCode`.
    ///
    /// # Errors
    /// `FAILED_PRECONDITION` (`no_email`), `POLICY_DENIED`, `RATE_LIMITED`.
    #[wasm_bindgen(js_name = reauthMailCode)]
    pub async fn reauth_mail_code(&self) -> Result<(), JsError> {
        self.app.reauth_mail_code().await.map_err(js_api)
    }

    /// The URL of a fresh OIDC login landing at `returnTo` with `?reauth=`.
    ///
    /// # Errors
    /// `FAILED_PRECONDITION` (`no_oidc`).
    #[wasm_bindgen(js_name = reauthOidcUrl)]
    pub async fn reauth_oidc_url(&self, return_to: String) -> Result<String, JsError> {
        self.app.reauth_oidc_url(&return_to).await.map_err(js_api)
    }

    /// Holds the reauth token an OIDC login landed with.
    #[wasm_bindgen(js_name = reauthTake)]
    pub fn reauth_take(&self, token: String) {
        self.app.reauth_take(&token);
    }

    /// A new phrase held in memory, shown and confirmed before the identity
    /// is made: a browser keeps none (identity spec 7).
    ///
    /// # Errors
    /// When randomness is unavailable.
    #[wasm_bindgen(js_name = identityNewPhrase)]
    pub fn identity_new_phrase(&self) -> Result<String, JsError> {
        self.app.identity_new_phrase().map_err(js)
    }

    /// The words of the recovery phrase's wordlist that begin with `prefix`,
    /// at most four: what a phrase field offers as the person types.
    #[must_use]
    #[wasm_bindgen(js_name = phraseCompletions)]
    pub fn phrase_completions(&self, prefix: String) -> Vec<String> {
        lumia_core::identity::phrase::completions(&prefix, 4)
            .into_iter()
            .map(str::to_string)
            .collect()
    }

    /// Three positions, 1 to 24, to ask the person for.
    ///
    /// # Errors
    /// When there is no phrase to confirm.
    #[wasm_bindgen(js_name = identityPhraseQuiz)]
    pub async fn identity_phrase_quiz(&self) -> Result<Vec<u32>, JsError> {
        self.app.identity_phrase_quiz().await.map_err(js)
    }

    /// Checks the words for the quiz's positions.
    ///
    /// # Errors
    /// When there is no phrase or quiz.
    #[wasm_bindgen(js_name = identityPhraseConfirm)]
    pub async fn identity_phrase_confirm(&self, words: Vec<String>) -> Result<bool, JsError> {
        self.app.identity_phrase_confirm(words).await.map_err(js)
    }

    /// Sets up private messages here from the confirmed phrase, the account
    /// bound under the reauth in hand. Returns the identity id, hex.
    ///
    /// # Errors
    /// Without a confirmed phrase, without reauth, or when the account
    /// already has an identity.
    #[wasm_bindgen(js_name = identitySetup)]
    pub async fn identity_setup(&self, label: String) -> Result<String, JsError> {
        self.app.identity_setup(&label).await.map_err(js_api)
    }

    /// Starting over: the account moved to an identity made here.
    ///
    /// # Errors
    /// As `identitySetup`; `ALREADY_EXISTS`.
    #[wasm_bindgen(js_name = identityStartOver)]
    pub async fn identity_start_over(&self, label: String) -> Result<String, JsError> {
        self.app.identity_start_over(&label).await.map_err(js_api)
    }

    /// The devices restoring from `phrase` would stop, read first so the
    /// person can keep one they still have.
    ///
    /// # Errors
    /// A phrase that is not one; API failure.
    #[wasm_bindgen(js_name = identityRecoverDevices)]
    pub async fn identity_recover_devices(&self, phrase: String) -> Result<JsValue, JsError> {
        to_js(
            &self
                .app
                .identity_recover_devices(&phrase)
                .await
                .map_err(js_api)?,
        )
    }

    /// Restores from the phrase keeping the devices in `keep` (their ids).
    ///
    /// # Errors
    /// As `identityRecover`.
    #[wasm_bindgen(js_name = identityRecoverKeeping)]
    pub async fn identity_recover_keeping(
        &self,
        phrase: String,
        label: String,
        keep: Vec<String>,
    ) -> Result<JsValue, JsError> {
        to_js(
            &self
                .app
                .identity_recover_keeping(&phrase, &label, keep)
                .await
                .map_err(js_api)?,
        )
    }

    /// Restores the identity from its phrase here: only on the flagship's
    /// origin, which the page keeps to (identity spec 7).
    ///
    /// # Errors
    /// A phrase that is not the identity's; API failure.
    #[wasm_bindgen(js_name = identityRecover)]
    pub async fn identity_recover(
        &self,
        phrase: String,
        label: String,
    ) -> Result<JsValue, JsError> {
        to_js(
            &self
                .app
                .identity_recover(&phrase, &label)
                .await
                .map_err(js_api)?,
        )
    }

    /// Linking by prompt, as the new device (a browser is never the one that
    /// approves): opens a session, so every device that holds the identity
    /// prompts. `{stage, digits}`.
    ///
    /// # Errors
    /// When this browser already holds an identity; API failure.
    #[wasm_bindgen(js_name = linkStart)]
    pub async fn link_start(&self, label: String) -> Result<JsValue, JsError> {
        to_js(&self.app.link_start(&label).await.map_err(js_api)?)
    }

    /// Where this browser's link session stands, taking its next step:
    /// `waiting`, `compare` (show `digits`), `linked`, `declined`, `expired`.
    ///
    /// # Errors
    /// Without a session in progress; API failure.
    #[wasm_bindgen(js_name = linkProgress)]
    pub async fn link_progress(&self) -> Result<JsValue, JsError> {
        to_js(&self.app.link_progress().await.map_err(js_api)?)
    }

    /// Gives up this browser's link session.
    #[wasm_bindgen(js_name = linkCancel)]
    pub async fn link_cancel(&self) {
        self.app.link_cancel().await;
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

    // ---- Attachments and link cards (specs/media.md) ----

    /// What the instance takes of attachments, `{imageMaxBytes,
    /// fileMaxBytes, thumbnailMaxBytes}`, or null when it keeps none (and
    /// the composer offers no way to attach).
    ///
    /// # Errors
    /// On transport or API failure.
    #[wasm_bindgen(js_name = uploadLimits)]
    pub async fn upload_limits(&self) -> Result<JsValue, JsError> {
        to_js(&self.app.upload_limits().await.map_err(js_api)?)
    }

    /// Makes an attachment of a file's `bytes` and uploads it into the
    /// conversation. `options`: `{name, asFile?, alt?, durationMs?,
    /// videoWidth?, videoHeight?, pixels?: {rgba, width, height}, poster?:
    /// {rgba, width, height}}`, `rgba` a `Uint8Array` or
    /// `Uint8ClampedArray` (an `ImageData`'s `data`). Resolves to the draft
    /// (`{draftId, channelId, kind, name, mediaType, width, height,
    /// durationMs, placeholder, alt, sizeBytes}`, the placeholder in base64).
    ///
    /// # Errors
    /// When the file cannot be sent (in words), the instance keeps no
    /// attachments or refuses the upload, or the transport fails.
    pub async fn attach(
        &self,
        channel_id: String,
        bytes: Vec<u8>,
        options: JsValue,
    ) -> Result<JsValue, JsError> {
        let input = attachment_input(bytes, &options)?;
        to_js(&self.app.attach(&channel_id, input).await.map_err(js_api)?)
    }

    /// Sets a waiting attachment's alt text; resolves to its draft.
    ///
    /// # Errors
    /// When no attachment waits under `draft_id`.
    #[wasm_bindgen(js_name = setAttachmentAlt)]
    pub fn set_attachment_alt(&self, draft_id: String, alt: String) -> Result<JsValue, JsError> {
        to_js(&self.app.set_attachment_alt(&draft_id, &alt).map_err(js)?)
    }

    /// Takes a waiting attachment out of the composer (its blobs off the
    /// instance, best effort).
    #[wasm_bindgen(js_name = discardAttachment)]
    pub async fn discard_attachment(&self, draft_id: String) {
        self.app.discard_attachment(&draft_id).await;
    }

    /// `sendMessageWith`, with the waiting attachments `attachments` names
    /// (draft ids) and the cards of `cardUrls` (the links of `cardLinks`
    /// the person kept). Returns the message id.
    ///
    /// # Errors
    /// When not started, an id is malformed, or an attachment is not one
    /// waiting for this channel.
    #[wasm_bindgen(js_name = sendMessageWithMedia)]
    pub fn send_message_with_media(
        &self,
        channel_id: String,
        body: String,
        mentions: JsValue,
        reply_to: JsValue,
        attachments: JsValue,
        card_urls: JsValue,
    ) -> Result<String, JsError> {
        let mentions: Vec<MentionSummary> = from_js(mentions)?.unwrap_or_default();
        let reply_to: Option<QuoteSummary> = from_js(reply_to)?;
        let attachments: Vec<String> = from_js(attachments)?.unwrap_or_default();
        let card_urls: Vec<String> = from_js(card_urls)?.unwrap_or_default();
        self.app
            .send_message_with_media(
                &channel_id,
                &body,
                &mentions,
                reply_to.as_ref(),
                &attachments,
                &card_urls,
            )
            .map_err(js)
    }

    /// `editMessage`, dropping the attachments `removeAttachmentIds` names
    /// and, when `cardUrls` is an array (not null), keeping only those
    /// links' cards. Returns the command id.
    ///
    /// # Errors
    /// When not started or an id is malformed.
    #[wasm_bindgen(js_name = editMessageWithMedia)]
    pub fn edit_message_with_media(
        &self,
        channel_id: String,
        message_id: String,
        body: String,
        mentions: JsValue,
        remove_attachment_ids: JsValue,
        card_urls: JsValue,
    ) -> Result<String, JsError> {
        let mentions: Vec<MentionSummary> = from_js(mentions)?.unwrap_or_default();
        let removed: Vec<String> = from_js(remove_attachment_ids)?.unwrap_or_default();
        let card_urls: Option<Vec<String>> = from_js(card_urls)?;
        self.app
            .edit_message_with_media(
                &channel_id,
                &message_id,
                &body,
                &mentions,
                &removed,
                card_urls.as_deref(),
            )
            .map_err(js)
    }

    /// `dmSendWith`, with the waiting attachments `attachments` names. The
    /// web builds no cards in personal conversations (specs/media.md 6.5),
    /// so `cardUrls` finds none; it is here for one shape on every client.
    /// Resolves to the message id.
    ///
    /// # Errors
    /// Without the group for the conversation, before the loop started, or
    /// when an attachment is not one waiting for this conversation.
    #[wasm_bindgen(js_name = dmSendWithMedia)]
    pub async fn dm_send_with_media(
        &self,
        channel_id: String,
        body: String,
        quote: JsValue,
        attachments: JsValue,
        card_urls: JsValue,
    ) -> Result<String, JsError> {
        let quote: Option<QuoteSummary> = from_js(quote)?;
        let attachments: Vec<String> = from_js(attachments)?.unwrap_or_default();
        let card_urls: Vec<String> = from_js(card_urls)?.unwrap_or_default();
        self.app
            .dm_send_with_media(&channel_id, &body, quote.as_ref(), &attachments, &card_urls)
            .await
            .map_err(js)
    }

    /// `dmEdit`, dropping the attachments `removeAttachmentIds` names and,
    /// when `cardUrls` is an array, keeping only those links' cards.
    ///
    /// # Errors
    /// Without the group for the conversation, or before the loop started.
    #[wasm_bindgen(js_name = dmEditWithMedia)]
    pub async fn dm_edit_with_media(
        &self,
        channel_id: String,
        message_id: String,
        body: String,
        remove_attachment_ids: JsValue,
        card_urls: JsValue,
    ) -> Result<(), JsError> {
        let removed: Vec<String> = from_js(remove_attachment_ids)?.unwrap_or_default();
        let card_urls: Option<Vec<String>> = from_js(card_urls)?;
        self.app
            .dm_edit_with_media(
                &channel_id,
                &message_id,
                &body,
                &removed,
                card_urls.as_deref(),
            )
            .await
            .map_err(js)
    }

    /// The links of a message body that may have cards, at most three.
    #[must_use]
    #[wasm_bindgen(js_name = cardLinks)]
    pub fn card_links(&self, body: String) -> Vec<String> {
        App::card_links(&body)
    }

    /// The links a composer previews in a conversation, at most three: none
    /// where it cannot have cards (a server channel of an instance that
    /// makes none, and a personal one on the web), so nothing is shown
    /// waiting for one.
    #[wasm_bindgen(js_name = previewLinks)]
    pub fn preview_links(&self, channel_id: String, body: String) -> Vec<String> {
        self.app.preview_links(&channel_id, &body)
    }

    /// The card to show under a link while composing, or null: the
    /// instance's in a server channel; none in a personal one on the web.
    ///
    /// # Errors
    /// The instance's refusal (`RATE_LIMITED`) or the transport.
    #[wasm_bindgen(js_name = cardPreview)]
    pub async fn card_preview(&self, channel_id: String, url: String) -> Result<JsValue, JsError> {
        to_js(
            &self
                .app
                .card_preview(&channel_id, &url)
                .await
                .map_err(js_api)?,
        )
    }

    /// An attachment's or a card's blob (`{blobId, sizeBytes, key, nonce,
    /// hash}` as a summary gave it), fetched, opened and kept in memory.
    /// Check `attachmentDrawable` before decoding a picture.
    ///
    /// # Errors
    /// `NOT_FOUND` when this account cannot read it; when it does not open
    /// as named; the transport.
    #[wasm_bindgen(js_name = attachmentBytes)]
    pub async fn attachment_bytes(&self, blob: JsValue) -> Result<Vec<u8>, JsError> {
        let blob: BlobSummary = from_js(blob)?.ok_or_else(|| JsError::new("no blob was named"))?;
        self.app.attachment_bytes(&blob).await.map_err(js_api)
    }

    /// A server card's picture, by its `imageUrl`.
    ///
    /// # Errors
    /// For a path that is not a card picture of this instance; `NOT_FOUND`.
    #[wasm_bindgen(js_name = cardImage)]
    pub async fn card_image(&self, image_url: String) -> Result<Vec<u8>, JsError> {
        self.app.card_image(&image_url).await.map_err(js_api)
    }

    /// The embed of a link in a message's text (specs/media.md 6.6), or
    /// null: `{provider, providerName, kind, url, blueskyHandle,
    /// blueskyRkey, needsParent, aspectWidth, aspectHeight, fixedHeight,
    /// resizes, maxWidth, label}`. Nothing is fetched.
    ///
    /// # Errors
    /// Only when the summary cannot be made a JS object.
    #[wasm_bindgen(js_name = embedFor)]
    pub fn embed_for(&self, url: String) -> Result<JsValue, JsError> {
        to_js(&App::embed_for(&url))
    }

    /// The address of a Bluesky post's embed from its account's DID and
    /// its record key; undefined when either is not one.
    #[must_use]
    #[wasm_bindgen(js_name = blueskyEmbedUrl)]
    pub fn bluesky_embed_url(&self, did: String, rkey: String) -> Option<String> {
        App::bluesky_embed_url(&did, &rkey)
    }

    /// The origins an embed loads from: the page's `frame-src`.
    #[must_use]
    #[wasm_bindgen(js_name = embedOrigins)]
    pub fn embed_origins(&self) -> Vec<String> {
        App::embed_origins()
    }

    /// The height a post's embed reported (specs/media.md 6.6), from what its
    /// window posted as text (a string as posted, an object as its JSON),
    /// held to 50..=5000; undefined for any other message or site.
    #[must_use]
    #[wasm_bindgen(js_name = reportedEmbedHeight)]
    pub fn reported_embed_height(&self, provider: String, message: String) -> Option<u32> {
        App::reported_embed_height(&provider, &message)
    }

    /// A Bluesky handle's DID, asked of Bluesky's public API (which sends
    /// CORS headers) on the press of a post named by handle.
    ///
    /// Undefined when Bluesky has no account for it (the card opens the
    /// browser).
    ///
    /// # Errors
    /// Bluesky gave no answer: the press may be made again.
    #[wasm_bindgen(js_name = resolveBlueskyHandle)]
    pub async fn resolve_bluesky_handle(&self, handle: String) -> Result<Option<String>, JsError> {
        self.app.resolve_bluesky_handle(&handle).await.map_err(js)
    }

    /// Whether a picture someone sent may be decoded: its header names a
    /// size within the bounds (specs/media.md 5).
    #[must_use]
    #[wasm_bindgen(js_name = attachmentDrawable)]
    pub fn attachment_drawable(&self, bytes: Vec<u8>) -> bool {
        App::attachment_drawable(&bytes)
    }

    /// An attachment's placeholder (base64, as a summary carries it) as a
    /// tiny picture, `{width, height, rgba}` with `rgba` a
    /// `Uint8ClampedArray` for an `ImageData`; null for none.
    #[must_use]
    #[wasm_bindgen(js_name = placeholderImage)]
    pub fn placeholder_image(&self, placeholder: String) -> JsValue {
        use base64::Engine as _;
        let Ok(bytes) = base64::engine::general_purpose::STANDARD.decode(placeholder) else {
            return JsValue::NULL;
        };
        let Some(image) = App::placeholder_image(&bytes) else {
            return JsValue::NULL;
        };
        let out = js_sys::Object::new();
        let _ = js_sys::Reflect::set(&out, &"width".into(), &image.width.into());
        let _ = js_sys::Reflect::set(&out, &"height".into(), &image.height.into());
        let rgba = js_sys::Uint8ClampedArray::from(image.rgba.as_slice());
        let _ = js_sys::Reflect::set(&out, &"rgba".into(), &rgba);
        out.into()
    }
}

/// The scalar options of `attach`.
#[derive(serde::Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct AttachOptions {
    name: String,
    as_file: bool,
    alt: String,
    duration_ms: Option<u32>,
    video_width: u32,
    video_height: u32,
}

/// `attach`'s options read: the scalars by serde, the pixels as typed arrays.
fn attachment_input(bytes: Vec<u8>, options: &JsValue) -> Result<AttachmentInput, JsError> {
    let o: AttachOptions = from_js(options.clone())?.unwrap_or_default();
    Ok(AttachmentInput {
        bytes,
        name: o.name,
        as_file: o.as_file,
        pixels: pixels_of(options, "pixels")?,
        poster: pixels_of(options, "poster")?,
        duration_ms: o.duration_ms,
        video_width: o.video_width,
        video_height: o.video_height,
        alt: o.alt,
    })
}

/// `options[field]` as `{rgba, width, height}`, or none.
fn pixels_of(options: &JsValue, field: &str) -> Result<Option<PixelsInput>, JsError> {
    if !options.is_object() {
        return Ok(None);
    }
    let value = js_sys::Reflect::get(options, &field.into()).unwrap_or(JsValue::UNDEFINED);
    if value.is_null() || value.is_undefined() {
        return Ok(None);
    }
    let get = |name: &str| js_sys::Reflect::get(&value, &name.into()).unwrap_or(JsValue::UNDEFINED);
    let rgba = get("rgba");
    if !rgba.is_object() {
        return Err(JsError::new(&format!(
            "{field}.rgba is not an array of bytes"
        )));
    }
    let dimension = |name: &str| {
        get(name)
            .as_f64()
            .filter(|n| n.fract() == 0.0 && *n >= 0.0 && *n <= f64::from(u32::MAX))
            .map(|n| n as u32)
            .ok_or_else(|| JsError::new(&format!("{field}.{name} is not a size")))
    };
    Ok(Some(PixelsInput {
        rgba: js_sys::Uint8Array::new(&rgba).to_vec(),
        width: dimension("width")?,
        height: dimension("height")?,
    }))
}
