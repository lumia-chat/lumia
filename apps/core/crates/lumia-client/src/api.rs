//! The HTTP API client (docs/protocol.md section 9) with protobuf bodies.
//!
//! Tokens live behind a shared handle so one rotation is seen by every
//! clone of the client, including the gateway loop that builds the socket
//! URL. An `UNAUTHENTICATED` or `TOKEN_EXPIRED` answer with a refresh token
//! on hand refreshes once and retries the call; the new pair goes to the
//! rotation hook so the caller can persist it.
//!
//! A refresh token is single use, and presenting one that was already
//! rotated revokes the whole session (protocol 12). So the clones rotate
//! one at a time, and a call that waited its turn behind another's
//! rotation takes the pair that one brought back instead of presenting
//! the token it retired.
//!
//! The clones are not the only holders of the pair. Another process on
//! the same stores (a phone's notification extension, a second copy of a
//! command) rotates it too and writes the new one where this process
//! would find it. So before a refresh token is presented, the stored pair
//! is read again through a hook: if it has moved, it is taken as it is.
//!
//! Every request is given a time to be answered in ([`REQUEST_TIMEOUT`],
//! longer for what carries more). Without one, a network that takes the
//! packets and answers nothing held a call for as long as TCP waits, which
//! is minutes, and whoever was signing out watched a spinner for them. A
//! call that runs out fails as any transport failure does.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use futures_util::lock::Mutex as AsyncMutex;
use lumia_core::proto::InstanceInfo;
use lumia_core::proto::{
    reauth_request, Account, AccountSnapshot, Ack, BackupKeyHandoff, ChallengePurpose,
    ChallengeRequest, ChallengeResponse, Channel, ChannelSnapshot, ClaimKeyPackageRequest,
    ClaimKeyPackageResponse, Command, CreateAccountRequest, CreateChannelRequest,
    CreateLinkBundleRequest, CreateLinkBundleResponse, CreateServerRequest, Error as ApiError,
    ErrorCode, GetGroupInfoResponse, GetLinkBundleResponse, GetRosterResponse, IdentityProof,
    IdentityProofSignInRequest, ListAccountEventsResponse, ListChannelsResponse,
    ListEventsResponse, ListMembersResponse, ListMessagesResponse, ListServersResponse,
    OpenPersonalDmRequest, OpenPersonalDmResponse, PasskeyRequirementRequest,
    PasswordSignInRequest, PublishBackupKeyHandoffRequest, PublishGroupInfoRequest,
    PublishKeyPackagesRequest, PublishRosterRequest, PushSubscription, ReauthRequest,
    ReauthResponse, RefreshRequest, RevokeDeviceRequest, Server, SignedRoster, TokenPair,
};
use lumia_core::proto::{
    AddChannelMembersRequest, AddChannelMembersResponse, ChannelIds, ListChannelMembersResponse,
    OpenServerDmRequest, UpdateMembershipRequest,
};
use lumia_core::proto::{
    CreateInviteRequest, EmailInviteRequest, HandoffResponse, Invite, InvitePreview,
    ListInvitesResponse, ListPasskeysResponse, MagicLinkCompleteRequest, MagicLinkRequest, Member,
    PasskeyAssertFinishRequest, PasskeyAssertStartRequest, PasskeyChallenge,
    PasskeyRegisterFinishRequest, SignedProfile, UpdateAccountRequest, UpdateServerRequest,
};
use lumia_core::proto::{
    DmRequest, DmRequestState, GetHistoryChunkResponse, GetIdentityBackupResponse,
    HistoryUsageResponse, ListHistoryManifestsResponse, PutHistoryChunkRequest,
    PutHistoryManifestRequest, PutIdentityBackupRequest, RespondDmRequestRequest,
    SendDmRequestRequest, SignedHistoryManifest,
};
use prost::Message;
use reqwest::header::{ACCEPT, AUTHORIZATION, CONTENT_TYPE};
use reqwest::StatusCode;

use crate::ClientError;

const PROTO: &str = "application/x-protobuf";
const REAUTH_HEADER: &str = "x-reauth-token";

/// How long a request may take from being sent to its answer's last byte,
/// before what it carries is counted in.
pub const REQUEST_TIMEOUT: Duration = Duration::from_secs(20);

/// The least a request with a body is assumed to upload in a second: a
/// picture or a chunk of history is allowed that much longer.
const UPLOAD_BYTES_PER_SECOND: usize = 16 * 1024;

/// What a download of history is allowed at least: a chunk is the one
/// answer that is large, and the phone asking for it may be on a slow link.
const CHUNK_TIMEOUT: Duration = Duration::from_secs(120);

/// What a farewell is allowed at most: the sign-out, and taking the push
/// subscription back before it. Signing out goes ahead whether the instance
/// hears of either or not, so nobody waits long for them.
const FAREWELL_TIMEOUT: Duration = Duration::from_secs(8);

/// The time one request has, given the client's own allowance, where it
/// goes and how many bytes it sends.
fn time_allowed(base: Duration, path: &str, sent: usize) -> Duration {
    let upload = Duration::from_secs((sent / UPLOAD_BYTES_PER_SECOND) as u64);
    if path.starts_with("/api/v1/auth/sign-out") || path.starts_with("/api/v1/push/subscription") {
        base.min(FAREWELL_TIMEOUT)
    } else if path.starts_with("/api/v1/history/chunks") {
        base.max(CHUNK_TIMEOUT) + upload
    } else {
        base + upload
    }
}

/// The current token pair for one instance.
#[derive(Debug, Clone, Default)]
pub struct Tokens {
    /// Bearer token for `/api/v1` and the gateway.
    pub access: Option<String>,
    /// Rotated at `POST /auth/refresh`.
    pub refresh: Option<String>,
}

/// Called with every new pair after a refresh, so the caller can persist it.
pub type RotateHook = Arc<dyn Fn(&TokenPair) + Send + Sync>;

/// Reads the pair as the store now holds it, when someone other than this
/// process has changed it there since this process last read or wrote it.
pub type StoredHook = Arc<dyn Fn() -> Option<Tokens> + Send + Sync>;

/// A client for one instance's HTTP API.
#[derive(Clone)]
pub struct Api {
    http: reqwest::Client,
    base: String,
    tokens: Arc<Mutex<Tokens>>,
    /// Held across a rotation, by whichever clone is making it.
    rotating: Arc<AsyncMutex<()>>,
    on_rotate: Option<RotateHook>,
    stored: Option<StoredHook>,
    /// What a request is allowed before what it carries is counted in.
    timeout: Duration,
    /// How many sign-ins have given this client a pair: a rotation keeps
    /// the session and does not count. The gateway loop reads it to tell a
    /// revocation of the session it holds from one of a session this
    /// client has already left for another.
    sign_ins: Arc<AtomicU64>,
}

impl std::fmt::Debug for Api {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Api")
            .field("base", &self.base)
            .field("signed_in", &self.tokens().access.is_some())
            .finish_non_exhaustive()
    }
}

impl Api {
    /// A client for the instance at `base`, such as `http://127.0.0.1:4000`.
    #[must_use]
    pub fn new(base: &str) -> Self {
        Self {
            http: reqwest::Client::new(),
            base: base.trim_end_matches('/').to_string(),
            tokens: Arc::new(Mutex::new(Tokens::default())),
            rotating: Arc::new(AsyncMutex::new(())),
            on_rotate: None,
            stored: None,
            timeout: REQUEST_TIMEOUT,
            sign_ins: Arc::new(AtomicU64::new(0)),
        }
    }

    /// Allows each request this long instead of [`REQUEST_TIMEOUT`].
    #[must_use]
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// Uses this access token for authenticated calls (no refresh possible).
    #[must_use]
    pub fn with_token(self, token: Option<String>) -> Self {
        self.with_tokens(Tokens {
            access: token,
            refresh: None,
        })
    }

    /// Uses this token pair; with a refresh token, expiry is handled.
    #[must_use]
    pub fn with_tokens(self, tokens: Tokens) -> Self {
        *self.tokens.lock().unwrap_or_else(PoisonError::into_inner) = tokens;
        self
    }

    /// Called with each rotated pair.
    #[must_use]
    pub fn on_rotate(mut self, hook: RotateHook) -> Self {
        self.on_rotate = Some(hook);
        self
    }

    /// Where to read the pair again before a refresh token is presented.
    #[must_use]
    pub fn stored(mut self, hook: StoredHook) -> Self {
        self.stored = Some(hook);
        self
    }

    /// Forgets the pair, as after a sign-out.
    pub fn clear_tokens(&self) {
        *self.tokens.lock().unwrap_or_else(PoisonError::into_inner) = Tokens::default();
    }

    /// The tokens in use right now.
    #[must_use]
    pub fn tokens(&self) -> Tokens {
        self.tokens
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    /// Takes the pair a sign-in brought: a session of its own, which the
    /// gateway loop can tell from the one it was holding.
    pub fn take_pair(&self, pair: &TokenPair) {
        self.set_tokens(pair);
        self.sign_ins.fetch_add(1, Ordering::SeqCst);
    }

    /// How many sign-ins have given this client a pair (see [`Api::take_pair`]).
    #[must_use]
    pub fn sign_ins(&self) -> u64 {
        self.sign_ins.load(Ordering::SeqCst)
    }

    /// Ends a session this client held and has already left for another,
    /// by that session's own pair, without touching the one held now. An
    /// access token that has run out is rotated once with the session's
    /// refresh token, then the session is signed out.
    ///
    /// # Errors
    /// On transport failure, or when the instance refuses both the access
    /// token and the refresh.
    pub async fn revoke(&self, held: Tokens) -> Result<(), ClientError> {
        const SIGN_OUT: &str = "/api/v1/auth/sign-out";
        let Some(access) = held.access else {
            return Ok(());
        };
        let (status, bytes) = self
            .send_once(reqwest::Method::POST, SIGN_OUT, None, None, Some(access))
            .await?;
        if status != StatusCode::UNAUTHORIZED {
            return answered(status, &bytes);
        }
        let Some(refresh) = held.refresh else {
            return Err(api_error(status, &bytes));
        };
        let body = RefreshRequest {
            refresh_token: refresh,
        }
        .encode_to_vec();
        let (status, bytes) = self
            .send_once(
                reqwest::Method::POST,
                "/api/v1/auth/refresh",
                Some(body),
                None,
                None,
            )
            .await?;
        let pair: TokenPair = decoded(status, &bytes)?;
        let (status, bytes) = self
            .send_once(
                reqwest::Method::POST,
                SIGN_OUT,
                None,
                None,
                Some(pair.access_token),
            )
            .await?;
        answered(status, &bytes)
    }

    /// Replaces the pair, as after a sign-in or a rotation.
    pub fn set_tokens(&self, pair: &TokenPair) {
        *self.tokens.lock().unwrap_or_else(PoisonError::into_inner) = Tokens {
            access: Some(pair.access_token.clone()),
            refresh: Some(pair.refresh_token.clone()),
        };
    }

    /// The base URL.
    #[must_use]
    pub fn base(&self) -> &str {
        &self.base
    }

    /// The host this client talks to, as challenges must be bound to it.
    #[must_use]
    pub fn host(&self) -> String {
        url::Url::parse(&self.base)
            .ok()
            .and_then(|u| u.host_str().map(str::to_string))
            .unwrap_or_default()
    }

    /// Rotates the pair at `POST /auth/refresh` and tells the hook.
    ///
    /// # Errors
    /// When there is no refresh token, or the instance refuses it
    /// (`TOKEN_EXPIRED`, `SESSION_REVOKED`).
    pub async fn refresh_now(&self) -> Result<(), ClientError> {
        self.refresh_after(None).await
    }

    /// [`Api::refresh_now`] for a call that was refused: `sent_with` is the
    /// refresh token held when that call went out. If the pair has been
    /// rotated since (another call was refused first and has already been
    /// to `/auth/refresh`), there is nothing to do but try again with it.
    async fn refresh_after(&self, sent_with: Option<&str>) -> Result<(), ClientError> {
        let _turn = self.rotating.lock().await;
        let refresh = self.tokens().refresh.ok_or(ClientError::NotSignedIn)?;
        if sent_with.is_some_and(|sent| sent != refresh) {
            return Ok(());
        }
        // Another process on these stores may have rotated the pair since
        // this one last looked: its pair is the live one, and presenting
        // the one held here would be the reuse that revokes the session.
        if let Some(stored) = self.stored.as_ref().and_then(|read| read()) {
            if stored.refresh.as_deref().is_some_and(|r| r != refresh) {
                *self.tokens.lock().unwrap_or_else(PoisonError::into_inner) = stored;
                return Ok(());
            }
        }
        let body = RefreshRequest {
            refresh_token: refresh,
        }
        .encode_to_vec();
        let (status, bytes) = self
            .send_once(
                reqwest::Method::POST,
                "/api/v1/auth/refresh",
                Some(body),
                None,
                None,
            )
            .await?;
        let pair: TokenPair = decoded(status, &bytes)?;
        self.set_tokens(&pair);
        if let Some(hook) = &self.on_rotate {
            hook(&pair);
        }
        Ok(())
    }

    /// `POST /auth/challenge`.
    ///
    /// # Errors
    /// On transport failure or an API error.
    pub async fn challenge(
        &self,
        purpose: ChallengePurpose,
    ) -> Result<ChallengeResponse, ClientError> {
        self.post(
            "/api/v1/auth/challenge",
            &ChallengeRequest {
                purpose: purpose as i32,
            },
        )
        .await
    }

    /// `POST /accounts/by-identity`.
    ///
    /// # Errors
    /// On transport failure or an API error.
    pub async fn create_account_by_identity(
        &self,
        handle: &str,
        proof: IdentityProof,
        profile: Option<SignedProfile>,
    ) -> Result<TokenPair, ClientError> {
        let req = CreateAccountRequest {
            handle: handle.into(),
            proof: Some(proof),
            profile,
            ..Default::default()
        };
        self.post("/api/v1/accounts/by-identity", &req).await
    }

    /// `PATCH /account`: the caller's own handle and display name on this
    /// instance; `None` leaves one as it is, and an empty display name
    /// clears it.
    ///
    /// # Errors
    /// `ALREADY_EXISTS` for a handle another account has or holds,
    /// `INVALID_ARGUMENT` for one that is not a handle; transport failure.
    pub async fn update_account(
        &self,
        handle: Option<String>,
        display_name: Option<String>,
    ) -> Result<Account, ClientError> {
        let req = UpdateAccountRequest {
            handle,
            display_name,
        };
        let (status, bytes) = self
            .send(
                reqwest::Method::PATCH,
                "/api/v1/account",
                Some(req.encode_to_vec()),
                None,
            )
            .await?;
        decoded(status, &bytes)
    }

    /// `PUT /account/avatar`: `picture` is a file's bytes as chosen; the
    /// instance keeps its own square of it and names it in the account.
    ///
    /// # Errors
    /// `INVALID_ARGUMENT` (`details.reason`: `unsupported`, `too_large`,
    /// `invalid`), `FAILED_PRECONDITION` (`details.state`: `blobs`) on an
    /// instance that keeps none, `RATE_LIMITED`; transport failure.
    pub async fn put_avatar(
        &self,
        picture: Vec<u8>,
        media_type: &str,
    ) -> Result<Account, ClientError> {
        let (status, bytes) = self
            .send(
                reqwest::Method::PUT,
                "/api/v1/account/avatar",
                Some(picture),
                Some((CONTENT_TYPE.as_str(), media_type)),
            )
            .await?;
        decoded(status, &bytes)
    }

    /// `PATCH /servers/{server_id}`: the server's name, its retention in
    /// days (0 for ever), or both; `None` leaves one as it is.
    ///
    /// # Errors
    /// `PERMISSION_DENIED` without `MANAGE_SERVER`; `INVALID_ARGUMENT` with
    /// `details.field` `name`; transport failure.
    pub async fn update_server(
        &self,
        server_id: &str,
        name: Option<String>,
        retention_days: Option<u32>,
    ) -> Result<Server, ClientError> {
        let req = UpdateServerRequest {
            name,
            retention_days,
            default_channels: None,
        };
        let (status, bytes) = self
            .send(
                reqwest::Method::PATCH,
                &format!("/api/v1/servers/{server_id}"),
                Some(req.encode_to_vec()),
                None,
            )
            .await?;
        decoded(status, &bytes)
    }

    /// `PUT /servers/{server_id}/icon`: `picture` is a file's bytes as
    /// chosen, taken as the account's picture is.
    ///
    /// # Errors
    /// As [`Api::put_avatar`], and `PERMISSION_DENIED` without `MANAGE_SERVER`.
    pub async fn put_server_icon(
        &self,
        server_id: &str,
        picture: Vec<u8>,
        media_type: &str,
    ) -> Result<Server, ClientError> {
        let (status, bytes) = self
            .send(
                reqwest::Method::PUT,
                &format!("/api/v1/servers/{server_id}/icon"),
                Some(picture),
                Some((CONTENT_TYPE.as_str(), media_type)),
            )
            .await?;
        decoded(status, &bytes)
    }

    /// `DELETE /servers/{server_id}/icon`.
    ///
    /// # Errors
    /// `PERMISSION_DENIED` without `MANAGE_SERVER`; transport failure.
    pub async fn delete_server_icon(&self, server_id: &str) -> Result<Server, ClientError> {
        let (status, bytes) = self
            .send(
                reqwest::Method::DELETE,
                &format!("/api/v1/servers/{server_id}/icon"),
                None,
                None,
            )
            .await?;
        decoded(status, &bytes)
    }

    /// `DELETE /account/avatar`.
    ///
    /// # Errors
    /// On transport failure or an API error.
    pub async fn delete_avatar(&self) -> Result<Account, ClientError> {
        let (status, bytes) = self
            .send(
                reqwest::Method::DELETE,
                "/api/v1/account/avatar",
                None,
                None,
            )
            .await?;
        decoded(status, &bytes)
    }

    /// `GET /avatars/{address}`: a picture's bytes, at most `limit` of
    /// them. `path` is an `Account.avatar_url` of this instance. Asked for
    /// without the session: the address is the way in, and the answer is
    /// the same for everyone.
    ///
    /// # Errors
    /// `NOT_FOUND` for an address nothing is at; a picture over `limit`;
    /// transport failure.
    pub async fn avatar(&self, path: &str, limit: usize) -> Result<Vec<u8>, ClientError> {
        let response = self
            .http
            .get(format!("{}{path}", self.base))
            .timeout(time_allowed(self.timeout, path, 0))
            .send()
            .await?;
        let status = response.status();
        let bytes = response.bytes().await?;
        if !status.is_success() {
            return Err(api_error(status, &bytes));
        }
        if bytes.len() > limit {
            return Err(ClientError::State("the picture is too large".into()));
        }
        Ok(bytes.to_vec())
    }

    /// `PUT /account/profile`: a new signed profile for the caller's account.
    ///
    /// # Errors
    /// `IDENTITY_PROOF_INVALID` for a profile the roster does not vouch for; transport failure.
    pub async fn put_profile(&self, profile: SignedProfile) -> Result<(), ClientError> {
        self.empty(
            reqwest::Method::PUT,
            "/api/v1/account/profile",
            Some(profile.encode_to_vec()),
        )
        .await
    }

    /// `POST /auth/sign-in/identity`.
    ///
    /// # Errors
    /// On transport failure or an API error.
    pub async fn sign_in_identity(&self, proof: IdentityProof) -> Result<TokenPair, ClientError> {
        self.post(
            "/api/v1/auth/sign-in/identity",
            &IdentityProofSignInRequest { proof: Some(proof) },
        )
        .await
    }

    /// `POST /auth/reauth` with a password; the token is single use and
    /// bound to this session (identity spec 10).
    ///
    /// # Errors
    /// On transport failure or an API error.
    pub async fn reauth_password(&self, password: &str) -> Result<ReauthResponse, ClientError> {
        self.post(
            "/api/v1/auth/reauth",
            &ReauthRequest {
                credential: Some(reauth_request::Credential::Password(password.into())),
            },
        )
        .await
    }

    /// `POST /identity/roster`.
    ///
    /// # Errors
    /// On transport failure or an API error (including `POLICY_DENIED`).
    pub async fn publish_roster(&self, roster: SignedRoster) -> Result<(), ClientError> {
        self.post_empty(
            "/api/v1/identity/roster",
            &PublishRosterRequest {
                roster: Some(roster),
            },
        )
        .await
    }

    /// `POST /identity/backup-key-handoff`: the new backup key after a root
    /// rotation, sealed to the identity's other devices, for their account
    /// logs here.
    ///
    /// # Errors
    /// On transport failure or an API error.
    pub async fn publish_backup_key_handoff(
        &self,
        handoff: BackupKeyHandoff,
    ) -> Result<(), ClientError> {
        self.post_empty(
            "/api/v1/identity/backup-key-handoff",
            &PublishBackupKeyHandoffRequest {
                handoff: Some(handoff),
            },
        )
        .await
    }

    /// `POST /identity/devices/revoke`: a device-signed roster that adds a
    /// revocation, with the reauth token the policy requires.
    ///
    /// # Errors
    /// On transport failure or an API error (`POLICY_DENIED` without reauth).
    pub async fn revoke_device(
        &self,
        roster: SignedRoster,
        reauth_token: &str,
    ) -> Result<(), ClientError> {
        self.post_empty(
            "/api/v1/identity/devices/revoke",
            &RevokeDeviceRequest {
                roster: Some(roster),
                reauth_token: reauth_token.into(),
            },
        )
        .await
    }

    /// `POST /identity/link`: binds the signed-in account to the identity
    /// the proof is for, under reauth.
    ///
    /// # Errors
    /// On transport failure or an API error (`CONFLICT` when bound elsewhere).
    pub async fn link_identity(
        &self,
        proof: IdentityProof,
        reauth_token: &str,
    ) -> Result<(), ClientError> {
        let (status, bytes) = self
            .send(
                reqwest::Method::POST,
                "/api/v1/identity/link",
                Some(proof.encode_to_vec()),
                Some((REAUTH_HEADER, reauth_token)),
            )
            .await?;
        if status.is_success() {
            Ok(())
        } else {
            Err(api_error(status, &bytes))
        }
    }

    /// `GET /identity/roster`.
    ///
    /// # Errors
    /// On transport failure or an API error.
    pub async fn get_roster(&self) -> Result<GetRosterResponse, ClientError> {
        self.get("/api/v1/identity/roster").await
    }

    /// `POST /identity/link-bundles`.
    ///
    /// # Errors
    /// On transport failure or an API error.
    pub async fn create_link_bundle(
        &self,
        req: CreateLinkBundleRequest,
    ) -> Result<CreateLinkBundleResponse, ClientError> {
        self.post("/api/v1/identity/link-bundles", &req).await
    }

    /// `GET /identity/link-bundles/{code}?device_id=` (public).
    ///
    /// # Errors
    /// On transport failure or an API error.
    pub async fn get_link_bundle(
        &self,
        code: &str,
        device_id: &str,
    ) -> Result<GetLinkBundleResponse, ClientError> {
        self.get(&format!(
            "/api/v1/identity/link-bundles/{code}?device_id={device_id}"
        ))
        .await
    }

    /// The gateway WebSocket URL for this instance. The token is not in
    /// it (it would land in every access log); `gateway_subprotocols`
    /// carries it on the upgrade.
    #[must_use]
    pub fn gateway_url(&self) -> String {
        let ws = self.base.replacen("http", "ws", 1);
        format!("{ws}/gateway/websocket?vsn=1.0.0")
    }

    /// The `Sec-WebSocket-Protocol` values for the upgrade: `phoenix`,
    /// which the server answers with, and the bearer token the way
    /// Phoenix reads it (`base64url.bearer.phx.` + unpadded base64 of the
    /// token; protocol section 6.1).
    ///
    /// # Errors
    /// When not signed in.
    pub fn gateway_subprotocols(&self) -> Result<Vec<String>, ClientError> {
        use base64::Engine as _;
        let token = self.tokens().access.ok_or(ClientError::NotSignedIn)?;
        let encoded = base64::engine::general_purpose::STANDARD_NO_PAD.encode(token);
        Ok(vec![
            "phoenix".to_string(),
            format!("base64url.bearer.phx.{encoded}"),
        ])
    }

    /// `GET /instance`: the trust card (public).
    ///
    /// # Errors
    /// On transport failure or an API error.
    pub async fn instance_info(&self) -> Result<InstanceInfo, ClientError> {
        let (status, bytes) = self
            .send_once(reqwest::Method::GET, "/api/v1/instance", None, None, None)
            .await?;
        decoded(status, &bytes)
    }

    /// `POST /auth/sign-in/password` with a second factor.
    ///
    /// # Errors
    /// On transport failure or an API error (`FAILED_PRECONDITION{mfa}`
    /// without the code, `UNAUTHENTICATED` with a wrong one).
    /// `POST /auth/sign-in/password` with the passkey factor: an assertion
    /// made for an assert challenge by one of the account's passkeys.
    ///
    /// # Errors
    /// On transport failure or an API error.
    pub async fn sign_in_with_passkey(
        &self,
        handle_or_email: &str,
        password: &str,
        device_id: Vec<u8>,
        challenge_id: &str,
        credential_json: &str,
    ) -> Result<TokenPair, ClientError> {
        let req = PasswordSignInRequest {
            handle_or_email: handle_or_email.into(),
            password: password.into(),
            device_id,
            passkey_challenge_id: challenge_id.into(),
            passkey_credential_json: credential_json.into(),
            ..Default::default()
        };
        self.post("/api/v1/auth/sign-in/password", &req).await
    }

    /// `PUT /auth/passkeys/required`: the passkey as the second factor of a
    /// password sign-in.
    ///
    /// # Errors
    /// On transport failure or an API error.
    pub async fn set_passkey_required(&self, required: bool) -> Result<(), ClientError> {
        self.empty(
            reqwest::Method::PUT,
            "/api/v1/auth/passkeys/required",
            Some(PasskeyRequirementRequest { required }.encode_to_vec()),
        )
        .await
    }

    pub async fn sign_in_with_totp(
        &self,
        handle_or_email: &str,
        password: &str,
        device_id: Vec<u8>,
        totp_code: &str,
    ) -> Result<TokenPair, ClientError> {
        let req = PasswordSignInRequest {
            handle_or_email: handle_or_email.into(),
            password: password.into(),
            device_id,
            totp_code: totp_code.into(),
            ..Default::default()
        };
        self.post("/api/v1/auth/sign-in/password", &req).await
    }

    /// `POST /auth/sign-out`.
    ///
    /// # Errors
    /// On transport failure or an API error.
    pub async fn sign_out(&self) -> Result<(), ClientError> {
        self.empty(reqwest::Method::POST, "/api/v1/auth/sign-out", None)
            .await
    }

    /// `POST /accounts` with a password.
    ///
    /// # Errors
    /// On transport failure or an API error.
    pub async fn create_account(
        &self,
        handle: &str,
        email: &str,
        password: &str,
        invite_code: &str,
        device_id: Vec<u8>,
    ) -> Result<TokenPair, ClientError> {
        let req = CreateAccountRequest {
            handle: handle.into(),
            email: email.into(),
            password: password.into(),
            invite_code: invite_code.into(),
            device_id,
            ..Default::default()
        };
        self.post("/api/v1/accounts", &req).await
    }

    /// `GET /invites/{code}`: what an invite shows before joining. Public.
    ///
    /// # Errors
    /// `NOT_FOUND` for an unknown, expired or spent code; transport failure.
    pub async fn invite_preview(&self, code: &str) -> Result<InvitePreview, ClientError> {
        self.get(&format!("/api/v1/invites/{}", url_encode(code)))
            .await
    }

    /// `POST /invites`.
    ///
    /// # Errors
    /// `PERMISSION_DENIED` without `CREATE_INVITES`; transport failure.
    /// `POST /invites/email`: an invite mailed to an address.
    ///
    /// # Errors
    /// On transport failure or an API error.
    pub async fn email_invite(&self, req: &EmailInviteRequest) -> Result<Invite, ClientError> {
        self.post("/api/v1/invites/email", req).await
    }

    pub async fn create_invite(&self, req: &CreateInviteRequest) -> Result<Invite, ClientError> {
        self.post("/api/v1/invites", req).await
    }

    /// `POST /invites/{code}/accept`: joins the caller to the invite's server.
    ///
    /// # Errors
    /// `NOT_FOUND` for an unknown, expired or spent code; transport failure.
    pub async fn accept_invite(&self, code: &str) -> Result<Member, ClientError> {
        let (status, bytes) = self
            .send(
                reqwest::Method::POST,
                &format!("/api/v1/invites/{}/accept", url_encode(code)),
                None,
                None,
            )
            .await?;
        decoded(status, &bytes)
    }

    /// `GET /servers/{id}/invites`: the server's invites, newest first.
    ///
    /// # Errors
    /// `PERMISSION_DENIED` without `CREATE_INVITES`; transport failure.
    pub async fn invites(&self, server_id: &str) -> Result<ListInvitesResponse, ClientError> {
        self.get(&format!("/api/v1/servers/{server_id}/invites"))
            .await
    }

    /// `DELETE /invites/{code}`.
    ///
    /// # Errors
    /// `PERMISSION_DENIED` unless the creator or a manager; transport failure.
    pub async fn delete_invite(&self, code: &str) -> Result<(), ClientError> {
        self.empty(
            reqwest::Method::DELETE,
            &format!("/api/v1/invites/{}", url_encode(code)),
            None,
        )
        .await
    }

    /// `POST /auth/sign-in/magic-link`: asks for a mail; 202 either way.
    ///
    /// # Errors
    /// `POLICY_DENIED` when the instance sends no mail, `RATE_LIMITED`; transport failure.
    pub async fn magic_link_request(&self, email: &str) -> Result<(), ClientError> {
        self.post_empty(
            "/api/v1/auth/sign-in/magic-link",
            &MagicLinkRequest {
                email: email.into(),
            },
        )
        .await
    }

    /// `POST /auth/sign-in/magic-link/complete`: the token from the mail.
    ///
    /// # Errors
    /// `UNAUTHENTICATED` for an unknown, used or expired token; transport failure.
    pub async fn magic_link_complete(
        &self,
        token: &str,
        device_id: Vec<u8>,
    ) -> Result<TokenPair, ClientError> {
        self.post(
            "/api/v1/auth/sign-in/magic-link/complete",
            &MagicLinkCompleteRequest {
                token: token.into(),
                device_id,
            },
        )
        .await
    }

    /// `POST /auth/handoff`: a one-time sign-in token for the caller's account.
    ///
    /// # Errors
    /// On transport failure or an API error.
    pub async fn handoff(&self) -> Result<HandoffResponse, ClientError> {
        let (status, bytes) = self
            .send(reqwest::Method::POST, "/api/v1/auth/handoff", None, None)
            .await?;
        decoded(status, &bytes)
    }

    /// `POST /auth/passkey/register/start`: the creation options for the caller.
    ///
    /// # Errors
    /// On transport failure or an API error.
    pub async fn passkey_register_start(&self) -> Result<PasskeyChallenge, ClientError> {
        let (status, bytes) = self
            .send(
                reqwest::Method::POST,
                "/api/v1/auth/passkey/register/start",
                None,
                None,
            )
            .await?;
        decoded(status, &bytes)
    }

    /// `POST /auth/passkey/register/finish`: the credential the platform made.
    ///
    /// # Errors
    /// `UNAUTHENTICATED` for a spent challenge, `ALREADY_EXISTS`; transport failure.
    pub async fn passkey_register_finish(
        &self,
        challenge_id: &str,
        credential_json: &str,
        label: &str,
    ) -> Result<(), ClientError> {
        self.post_empty(
            "/api/v1/auth/passkey/register/finish",
            &PasskeyRegisterFinishRequest {
                challenge_id: challenge_id.into(),
                credential_json: credential_json.into(),
                label: label.into(),
            },
        )
        .await
    }

    /// `POST /auth/passkey/assert/start`: the request options, for a handle when given.
    ///
    /// # Errors
    /// On transport failure or an API error.
    pub async fn passkey_assert_start(
        &self,
        handle_or_email: &str,
    ) -> Result<PasskeyChallenge, ClientError> {
        self.post(
            "/api/v1/auth/passkey/assert/start",
            &PasskeyAssertStartRequest {
                handle_or_email: handle_or_email.into(),
            },
        )
        .await
    }

    /// `POST /auth/passkey/assert/finish`: the assertion signs the device in.
    ///
    /// # Errors
    /// `UNAUTHENTICATED`; transport failure.
    pub async fn passkey_assert_finish(
        &self,
        challenge_id: &str,
        credential_json: &str,
        device_id: Vec<u8>,
    ) -> Result<TokenPair, ClientError> {
        self.post(
            "/api/v1/auth/passkey/assert/finish",
            &PasskeyAssertFinishRequest {
                challenge_id: challenge_id.into(),
                credential_json: credential_json.into(),
                device_id,
            },
        )
        .await
    }

    /// `GET /auth/passkeys`.
    ///
    /// # Errors
    /// On transport failure or an API error.
    pub async fn passkeys(&self) -> Result<ListPasskeysResponse, ClientError> {
        self.get("/api/v1/auth/passkeys").await
    }

    /// `DELETE /auth/passkeys/{id}`.
    ///
    /// # Errors
    /// `NOT_FOUND`; transport failure.
    pub async fn delete_passkey(&self, passkey_id: &str) -> Result<(), ClientError> {
        self.empty(
            reqwest::Method::DELETE,
            &format!("/api/v1/auth/passkeys/{}", url_encode(passkey_id)),
            None,
        )
        .await
    }

    /// `POST /auth/sign-in/password`.
    ///
    /// # Errors
    /// On transport failure or an API error.
    pub async fn sign_in(
        &self,
        handle_or_email: &str,
        password: &str,
        device_id: Vec<u8>,
    ) -> Result<TokenPair, ClientError> {
        let req = PasswordSignInRequest {
            handle_or_email: handle_or_email.into(),
            password: password.into(),
            device_id,
            ..Default::default()
        };
        self.post("/api/v1/auth/sign-in/password", &req).await
    }

    /// `GET /account/snapshot`.
    ///
    /// # Errors
    /// On transport failure or an API error.
    pub async fn account_snapshot(&self) -> Result<AccountSnapshot, ClientError> {
        self.get("/api/v1/account/snapshot").await
    }

    /// `GET /account/events?after_seq=`.
    ///
    /// # Errors
    /// On transport failure or an API error.
    pub async fn account_events(
        &self,
        after_seq: u64,
    ) -> Result<ListAccountEventsResponse, ClientError> {
        self.get(&format!(
            "/api/v1/account/events?after_seq={after_seq}&limit=200"
        ))
        .await
    }

    /// `GET /servers`.
    ///
    /// # Errors
    /// On transport failure or an API error.
    pub async fn servers(&self) -> Result<ListServersResponse, ClientError> {
        self.get("/api/v1/servers").await
    }

    /// `POST /servers`.
    ///
    /// # Errors
    /// On transport failure or an API error.
    pub async fn create_server(&self, name: &str) -> Result<Server, ClientError> {
        self.post(
            "/api/v1/servers",
            &CreateServerRequest { name: name.into() },
        )
        .await
    }

    /// `POST /channels`.
    ///
    /// # Errors
    /// On transport failure or an API error.
    pub async fn create_channel(
        &self,
        server_id: Vec<u8>,
        name: &str,
        private: bool,
    ) -> Result<Channel, ClientError> {
        let req = CreateChannelRequest {
            server_id,
            name: name.into(),
            private,
            ..Default::default()
        };
        self.post("/api/v1/channels", &req).await
    }

    /// `GET /servers/{id}/channels`.
    ///
    /// # Errors
    /// On transport failure or an API error.
    pub async fn channels(&self, server_id: &str) -> Result<ListChannelsResponse, ClientError> {
        self.get(&format!("/api/v1/servers/{server_id}/channels"))
            .await
    }

    /// `GET /servers/{id}/channels/browse`: the public channels the caller
    /// can see and is not in, with their member counts (architecture 6.8).
    ///
    /// # Errors
    /// On transport failure or an API error.
    pub async fn browse_channels(
        &self,
        server_id: &str,
    ) -> Result<ListChannelsResponse, ClientError> {
        self.get(&format!("/api/v1/servers/{server_id}/channels/browse"))
            .await
    }

    /// `POST /channels/{id}/join`.
    ///
    /// # Errors
    /// `NOT_FOUND` for a channel the caller may not join; transport failure.
    pub async fn join_channel(&self, channel_id: &str) -> Result<Channel, ClientError> {
        let (status, bytes) = self
            .send(
                reqwest::Method::POST,
                &format!("/api/v1/channels/{channel_id}/join"),
                None,
                None,
            )
            .await?;
        decoded(status, &bytes)
    }

    /// `POST /channels/{id}/leave`.
    ///
    /// # Errors
    /// `NOT_FOUND` when not in it; transport failure.
    pub async fn leave_channel(&self, channel_id: &str) -> Result<(), ClientError> {
        self.empty(
            reqwest::Method::POST,
            &format!("/api/v1/channels/{channel_id}/leave"),
            None,
        )
        .await
    }

    /// `GET /channels/{id}/members`.
    ///
    /// # Errors
    /// `NOT_FOUND` for a channel the caller cannot see; transport failure.
    pub async fn channel_members(
        &self,
        channel_id: &str,
    ) -> Result<ListChannelMembersResponse, ClientError> {
        self.get(&format!("/api/v1/channels/{channel_id}/members"))
            .await
    }

    /// `POST /channels/{id}/members`: `account_ids` as 16-byte ids.
    ///
    /// # Errors
    /// `PERMISSION_DENIED` or `NOT_FOUND` for a caller who may not add there,
    /// `INVALID_ARGUMENT` with `details.field` `account_ids`; transport failure.
    pub async fn add_channel_members(
        &self,
        channel_id: &str,
        account_ids: Vec<Vec<u8>>,
    ) -> Result<AddChannelMembersResponse, ClientError> {
        self.post(
            &format!("/api/v1/channels/{channel_id}/members"),
            &AddChannelMembersRequest { account_ids },
        )
        .await
    }

    /// `DELETE /channels/{id}/members/{account_id}`.
    ///
    /// # Errors
    /// `PERMISSION_DENIED` without `MANAGE_CHANNELS`; transport failure.
    pub async fn remove_channel_member(
        &self,
        channel_id: &str,
        account_id: &str,
    ) -> Result<(), ClientError> {
        self.empty(
            reqwest::Method::DELETE,
            &format!("/api/v1/channels/{channel_id}/members/{account_id}"),
            None,
        )
        .await
    }

    /// `DELETE /channel-invitations/{channel_id}`.
    ///
    /// # Errors
    /// `NOT_FOUND` without one; transport failure.
    pub async fn dismiss_channel_invitation(&self, channel_id: &str) -> Result<(), ClientError> {
        self.empty(
            reqwest::Method::DELETE,
            &format!("/api/v1/channel-invitations/{channel_id}"),
            None,
        )
        .await
    }

    /// `PATCH /servers/{id}/membership`: the caller's own switches there.
    ///
    /// # Errors
    /// `NOT_FOUND` for a server it is not in; transport failure.
    pub async fn update_membership(
        &self,
        server_id: &str,
        req: &UpdateMembershipRequest,
    ) -> Result<Member, ClientError> {
        let (status, bytes) = self
            .send(
                reqwest::Method::PATCH,
                &format!("/api/v1/servers/{server_id}/membership"),
                Some(req.encode_to_vec()),
                None,
            )
            .await?;
        decoded(status, &bytes)
    }

    /// `POST /servers/{id}/dms`: the server DM with `account_id` (16 bytes).
    ///
    /// # Errors
    /// `PERMISSION_DENIED` without `START_SERVER_DMS`, `FAILED_PRECONDITION`
    /// with `server_dms_off`, `NOT_FOUND`; transport failure.
    pub async fn open_server_dm(
        &self,
        server_id: &str,
        account_id: Vec<u8>,
    ) -> Result<Channel, ClientError> {
        self.post(
            &format!("/api/v1/servers/{server_id}/dms"),
            &OpenServerDmRequest { account_id },
        )
        .await
    }

    /// `PATCH /servers/{id}` with only the default channels.
    ///
    /// # Errors
    /// `PERMISSION_DENIED` without `MANAGE_SERVER`, `INVALID_ARGUMENT` with
    /// `default_channels`; transport failure.
    pub async fn set_default_channels(
        &self,
        server_id: &str,
        channel_ids: Vec<Vec<u8>>,
    ) -> Result<Server, ClientError> {
        let req = UpdateServerRequest {
            default_channels: Some(ChannelIds { channel_ids }),
            ..Default::default()
        };
        let (status, bytes) = self
            .send(
                reqwest::Method::PATCH,
                &format!("/api/v1/servers/{server_id}"),
                Some(req.encode_to_vec()),
                None,
            )
            .await?;
        decoded(status, &bytes)
    }

    /// `GET /accounts/{id}`: another account's public profile, when the
    /// caller may see it (a shared server or personal DM, or a request).
    ///
    /// # Errors
    /// `NOT_FOUND` when not visible; transport failure.
    pub async fn account(&self, account_id: &str) -> Result<Account, ClientError> {
        self.get(&format!("/api/v1/accounts/{account_id}")).await
    }

    /// `GET /servers/{id}/members`: a page of members with their accounts,
    /// optionally those whose handle or nickname starts with `query`. `cursor`
    /// is the last page's `next_cursor`, empty for the first page.
    ///
    /// # Errors
    /// On transport failure or an API error.
    pub async fn members(
        &self,
        server_id: &str,
        query: &str,
        cursor: &str,
    ) -> Result<ListMembersResponse, ClientError> {
        let (q, c) = (url_encode(query), url_encode(cursor));
        self.get(&format!(
            "/api/v1/servers/{server_id}/members?page_size=200&query={q}&cursor={c}"
        ))
        .await
    }

    /// `POST /channels/{id}/commands`: the HTTP path for a command, which
    /// shares the idempotency table with the gateway (protocol section 7).
    ///
    /// # Errors
    /// On transport failure or an API error.
    pub async fn post_command(
        &self,
        channel_id: &str,
        command: &Command,
    ) -> Result<Ack, ClientError> {
        self.post(&format!("/api/v1/channels/{channel_id}/commands"), command)
            .await
    }

    /// `GET /channels/{id}/events?after_seq=&limit=`: events with
    /// `seq > after`, and when `before > 0` only those below it. The
    /// instance pages by `after_seq` and `limit` (at most 200), so the
    /// upper bound becomes the limit rather than a parameter.
    ///
    /// # Errors
    /// On transport failure or an API error.
    pub async fn channel_events(
        &self,
        channel_id: &str,
        after: u64,
        before: u64,
    ) -> Result<ListEventsResponse, ClientError> {
        let limit = page_limit(after, before);
        self.get(&format!(
            "/api/v1/channels/{channel_id}/events?after_seq={after}&limit={limit}"
        ))
        .await
    }

    /// `GET /channels/{id}/messages`: scrollback, the newest `limit` messages
    /// with `seq` strictly below `before_seq` (0 for the head), oldest first,
    /// and whether older ones remain.
    ///
    /// # Errors
    /// On transport failure or an API error.
    pub async fn list_messages(
        &self,
        channel_id: &str,
        before_seq: u64,
        limit: u32,
    ) -> Result<ListMessagesResponse, ClientError> {
        self.get(&format!(
            "/api/v1/channels/{channel_id}/messages?before_seq={before_seq}&limit={limit}"
        ))
        .await
    }

    /// `GET /channels/{id}/snapshot`.
    ///
    /// # Errors
    /// On transport failure or an API error.
    pub async fn channel_snapshot(&self, channel_id: &str) -> Result<ChannelSnapshot, ClientError> {
        self.get(&format!("/api/v1/channels/{channel_id}/snapshot"))
            .await
    }

    // ---- Personal DMs and the MLS delivery service (e2ee spec section 11) ----

    /// `POST /dms/personal`: opens (or finds) the personal DM with `others`
    /// (one other: the pair's DM; more: a new personal group DM).
    ///
    /// # Errors
    /// On transport failure or an API error (`FAILED_PRECONDITION{state:
    /// "request"}` without an accepted DM request).
    pub async fn open_personal_dm(
        &self,
        others: Vec<Vec<u8>>,
    ) -> Result<OpenPersonalDmResponse, ClientError> {
        self.post(
            "/api/v1/dms/personal",
            &OpenPersonalDmRequest {
                participant_account_ids: others,
            },
        )
        .await
    }

    /// `POST /dm-requests`.
    ///
    /// # Errors
    /// On transport failure or an API error.
    pub async fn send_dm_request(&self, to: Vec<u8>) -> Result<DmRequest, ClientError> {
        self.post(
            "/api/v1/dm-requests",
            &SendDmRequestRequest { to_account_id: to },
        )
        .await
    }

    /// `POST /dm-requests/{id}/respond`.
    ///
    /// # Errors
    /// On transport failure or an API error.
    pub async fn respond_dm_request(
        &self,
        request_id: &str,
        response: DmRequestState,
    ) -> Result<DmRequest, ClientError> {
        self.post(
            &format!("/api/v1/dm-requests/{request_id}/respond"),
            &RespondDmRequestRequest {
                response: response as i32,
                ..Default::default()
            },
        )
        .await
    }

    /// `PUT /identity/backup`.
    ///
    /// # Errors
    /// On transport failure or an API error.
    pub async fn put_identity_backup(
        &self,
        version: u64,
        ciphertext: Vec<u8>,
        nonce: Vec<u8>,
    ) -> Result<(), ClientError> {
        self.empty(
            reqwest::Method::PUT,
            "/api/v1/identity/backup",
            Some(
                PutIdentityBackupRequest {
                    version,
                    ciphertext,
                    nonce,
                }
                .encode_to_vec(),
            ),
        )
        .await
    }

    /// `GET /identity/backup`.
    ///
    /// # Errors
    /// On transport failure or an API error (`NOT_FOUND` when none).
    pub async fn get_identity_backup(&self) -> Result<GetIdentityBackupResponse, ClientError> {
        self.get("/api/v1/identity/backup").await
    }

    /// `GET /accounts/{id}/roster`: another participant's roster.
    ///
    /// # Errors
    /// On transport failure or an API error (`NOT_FOUND` without a shared DM).
    pub async fn account_roster(&self, account_id: &str) -> Result<GetRosterResponse, ClientError> {
        self.get(&format!("/api/v1/accounts/{account_id}/roster"))
            .await
    }

    /// `GET /identity/rosters/by-root/{root}` (public): the roster whose
    /// current root public key is `root`, for recovery after a rotation.
    ///
    /// # Errors
    /// On transport failure or an API error (`NOT_FOUND` when no identity
    /// here has that root).
    pub async fn roster_by_root(&self, root: &[u8]) -> Result<GetRosterResponse, ClientError> {
        use base64::Engine as _;
        let root = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(root);
        self.get(&format!("/api/v1/identity/rosters/by-root/{root}"))
            .await
    }

    /// `POST /mls/key-packages`.
    ///
    /// # Errors
    /// On transport failure or an API error.
    pub async fn publish_key_packages(
        &self,
        key_packages: Vec<Vec<u8>>,
        last_resort_key_package: Vec<u8>,
    ) -> Result<(), ClientError> {
        self.post_empty(
            "/api/v1/mls/key-packages",
            &PublishKeyPackagesRequest {
                key_packages,
                last_resort_key_package,
            },
        )
        .await
    }

    /// `POST /mls/key-packages/claim`: one package per device of `account_id`
    /// (this device excluded when claiming its own identity's).
    ///
    /// # Errors
    /// On transport failure or an API error.
    pub async fn claim_key_packages(
        &self,
        account_id: Vec<u8>,
    ) -> Result<ClaimKeyPackageResponse, ClientError> {
        self.post(
            "/api/v1/mls/key-packages/claim",
            &ClaimKeyPackageRequest {
                account_id,
                device_id: Vec::new(),
            },
        )
        .await
    }

    /// `POST /mls/key-packages/claim` for one device of `account_id`.
    ///
    /// # Errors
    /// On transport failure or an API error.
    pub async fn claim_key_package_for(
        &self,
        account_id: Vec<u8>,
        device_id: Vec<u8>,
    ) -> Result<ClaimKeyPackageResponse, ClientError> {
        self.post(
            "/api/v1/mls/key-packages/claim",
            &ClaimKeyPackageRequest {
                account_id,
                device_id,
            },
        )
        .await
    }

    // ---- History backup (e2ee spec section 9) ----

    /// `PUT /history/chunks/{chunk_id}`.
    ///
    /// # Errors
    /// On transport failure or an API error (`RESOURCE_EXHAUSTED` over quota).
    pub async fn put_history_chunk(
        &self,
        chunk_id: &str,
        channel_id: Vec<u8>,
        ciphertext: Vec<u8>,
        nonce: Vec<u8>,
    ) -> Result<(), ClientError> {
        self.empty(
            reqwest::Method::PUT,
            &format!("/api/v1/history/chunks/{chunk_id}"),
            Some(
                PutHistoryChunkRequest {
                    channel_id,
                    ciphertext,
                    nonce,
                }
                .encode_to_vec(),
            ),
        )
        .await
    }

    /// `GET /history/chunks/{chunk_id}`.
    ///
    /// # Errors
    /// On transport failure or an API error.
    pub async fn get_history_chunk(
        &self,
        chunk_id: &str,
    ) -> Result<GetHistoryChunkResponse, ClientError> {
        self.get(&format!("/api/v1/history/chunks/{chunk_id}"))
            .await
    }

    /// `PUT /push/subscription`: this device's push subscription of one
    /// kind, replacing the previous (architecture 15.5).
    ///
    /// # Errors
    /// `INVALID_ARGUMENT` for a malformed subscription; transport failure.
    pub async fn put_push_subscription(
        &self,
        subscription: &PushSubscription,
    ) -> Result<(), ClientError> {
        self.empty(
            reqwest::Method::PUT,
            "/api/v1/push/subscription",
            Some(subscription.encode_to_vec()),
        )
        .await
    }

    /// `DELETE /push/subscription?kind=`.
    ///
    /// # Errors
    /// On transport failure or an API error.
    pub async fn delete_push_subscription(&self, kind: &str) -> Result<(), ClientError> {
        self.empty(
            reqwest::Method::DELETE,
            &format!("/api/v1/push/subscription?kind={}", url_encode(kind)),
            None,
        )
        .await
    }

    /// `DELETE /history/chunks/{chunk_id}`.
    ///
    /// # Errors
    /// On transport failure or an API error.
    pub async fn delete_history_chunk(&self, chunk_id: &str) -> Result<(), ClientError> {
        self.empty(
            reqwest::Method::DELETE,
            &format!("/api/v1/history/chunks/{chunk_id}"),
            None,
        )
        .await
    }

    /// `PUT /history/manifest`.
    ///
    /// # Errors
    /// On transport failure or an API error.
    pub async fn put_history_manifest(
        &self,
        manifest: SignedHistoryManifest,
    ) -> Result<(), ClientError> {
        self.empty(
            reqwest::Method::PUT,
            "/api/v1/history/manifest",
            Some(
                PutHistoryManifestRequest {
                    manifest: Some(manifest),
                }
                .encode_to_vec(),
            ),
        )
        .await
    }

    /// `GET /history/manifests`.
    ///
    /// # Errors
    /// On transport failure or an API error.
    pub async fn list_history_manifests(
        &self,
    ) -> Result<ListHistoryManifestsResponse, ClientError> {
        self.get("/api/v1/history/manifests").await
    }

    /// `DELETE /history`: every chunk and manifest of the identity here.
    ///
    /// # Errors
    /// On transport failure or an API error.
    pub async fn delete_history(&self) -> Result<(), ClientError> {
        self.empty(reqwest::Method::DELETE, "/api/v1/history", None)
            .await
    }

    /// `GET /history/usage`.
    ///
    /// # Errors
    /// On transport failure or an API error.
    pub async fn history_usage(&self) -> Result<HistoryUsageResponse, ClientError> {
        self.get("/api/v1/history/usage").await
    }

    /// `POST /mls/group-info`.
    ///
    /// # Errors
    /// On transport failure or an API error.
    pub async fn publish_group_info(
        &self,
        channel_id: Vec<u8>,
        group_info: Vec<u8>,
        epoch: u64,
    ) -> Result<(), ClientError> {
        self.post_empty(
            "/api/v1/mls/group-info",
            &PublishGroupInfoRequest {
                channel_id,
                group_info,
                epoch,
            },
        )
        .await
    }

    /// `GET /mls/group-info/{channel_id}`.
    ///
    /// # Errors
    /// On transport failure or an API error.
    pub async fn group_info(&self, channel_id: &str) -> Result<GetGroupInfoResponse, ClientError> {
        self.get(&format!("/api/v1/mls/group-info/{channel_id}"))
            .await
    }

    async fn post<Req: Message, Resp: Message + Default>(
        &self,
        path: &str,
        req: &Req,
    ) -> Result<Resp, ClientError> {
        let (status, bytes) = self
            .send(reqwest::Method::POST, path, Some(req.encode_to_vec()), None)
            .await?;
        decoded(status, &bytes)
    }

    async fn post_empty<Req: Message>(&self, path: &str, req: &Req) -> Result<(), ClientError> {
        let (status, bytes) = self
            .send(reqwest::Method::POST, path, Some(req.encode_to_vec()), None)
            .await?;
        if status.is_success() {
            Ok(())
        } else {
            Err(api_error(status, &bytes))
        }
    }

    async fn empty(
        &self,
        method: reqwest::Method,
        path: &str,
        body: Option<Vec<u8>>,
    ) -> Result<(), ClientError> {
        let (status, bytes) = self.send(method, path, body, None).await?;
        if status.is_success() {
            Ok(())
        } else {
            Err(api_error(status, &bytes))
        }
    }

    async fn get<Resp: Message + Default>(&self, path: &str) -> Result<Resp, ClientError> {
        let (status, bytes) = self.send(reqwest::Method::GET, path, None, None).await?;
        decoded(status, &bytes)
    }

    /// One call, with one refresh-and-retry when the access token is refused
    /// and a refresh token is on hand.
    async fn send(
        &self,
        method: reqwest::Method,
        path: &str,
        body: Option<Vec<u8>>,
        extra: Option<(&str, &str)>,
    ) -> Result<(StatusCode, Vec<u8>), ClientError> {
        // The pair this call goes out with: if it is refused and the pair has
        // changed by then, someone else has rotated it and the retry takes theirs.
        let mut sent_with = self.tokens().refresh;
        let mut answer = self
            .send_once(
                method.clone(),
                path,
                body.clone(),
                extra,
                self.tokens().access,
            )
            .await?;
        // Twice at most: a pair taken from another call or another process
        // may itself have run out, and then it is this call's to rotate. A
        // call that presents credentials of its own is refused for those,
        // never for the pair, which it is not to spend.
        for _ in 0..2 {
            if answer.0 != StatusCode::UNAUTHORIZED
                || sent_with.is_none()
                || presents_credentials(path)
            {
                break;
            }
            match api_error(answer.0, &answer.1) {
                ClientError::Api { code, .. }
                    if code == "TOKEN_EXPIRED" || code == "UNAUTHENTICATED" =>
                {
                    self.refresh_after(sent_with.as_deref()).await?;
                    sent_with = self.tokens().refresh;
                    answer = self
                        .send_once(
                            method.clone(),
                            path,
                            body.clone(),
                            extra,
                            self.tokens().access,
                        )
                        .await?;
                }
                _ => break,
            }
        }
        Ok(answer)
    }

    async fn send_once(
        &self,
        method: reqwest::Method,
        path: &str,
        body: Option<Vec<u8>>,
        extra: Option<(&str, &str)>,
        bearer: Option<String>,
    ) -> Result<(StatusCode, Vec<u8>), ClientError> {
        let sent = body.as_ref().map_or(0, Vec::len);
        let mut builder = self
            .http
            .request(method, format!("{}{path}", self.base))
            .timeout(time_allowed(self.timeout, path, sent))
            .header(ACCEPT, PROTO);
        // A body is protobuf unless the extra header says what else it is.
        let typed = extra.filter(|(name, _)| name.eq_ignore_ascii_case(CONTENT_TYPE.as_str()));
        if let Some(body) = body {
            builder = builder
                .header(CONTENT_TYPE, typed.map_or(PROTO, |(_, value)| value))
                .body(body);
        }
        if let Some((name, value)) = extra.filter(|_| typed.is_none()) {
            builder = builder.header(name, value);
        }
        if let Some(token) = bearer {
            builder = builder.header(AUTHORIZATION, format!("Bearer {token}"));
        }
        let response = builder.send().await?;
        let status = response.status();
        let bytes = response.bytes().await?;
        Ok((status, bytes.to_vec()))
    }
}

/// Whether a call presents credentials of its own (a password, a magic
/// link's token, an identity's proof, a passkey's assertion, a refresh
/// token, a new account's): its 401 is about those, not about the session's
/// access token, so it is not retried with a refreshed pair.
fn presents_credentials(path: &str) -> bool {
    path.starts_with("/api/v1/auth/sign-in/")
        || path.starts_with("/api/v1/auth/passkey/assert/")
        || path == "/api/v1/auth/refresh"
        || path == "/api/v1/accounts"
        || path == "/api/v1/accounts/by-identity"
}

/// An answer with nothing in it to decode: success, or the error it carries.
fn answered(status: StatusCode, bytes: &[u8]) -> Result<(), ClientError> {
    if status.is_success() {
        Ok(())
    } else {
        Err(api_error(status, bytes))
    }
}

fn decoded<Resp: Message + Default>(status: StatusCode, bytes: &[u8]) -> Result<Resp, ClientError> {
    if status.is_success() {
        Ok(Resp::decode(bytes)?)
    } else {
        Err(api_error(status, bytes))
    }
}

/// The instance's page size for event listings.
const PAGE: u64 = 200;

/// How many events to ask for after `after`: a full page, or just enough
/// to reach `before` (exclusive) when the caller already holds what lies
/// beyond it. Never zero, so a request always makes progress.
fn page_limit(after: u64, before: u64) -> u64 {
    if before > after + 1 {
        (before - after - 1).min(PAGE)
    } else {
        PAGE
    }
}

fn api_error(status: StatusCode, bytes: &[u8]) -> ClientError {
    let error = ApiError::decode(bytes).unwrap_or_default();
    let code = ErrorCode::try_from(error.code)
        .map(|c| c.as_str_name().to_string())
        .unwrap_or_else(|_| "UNKNOWN".into());
    ClientError::Api {
        code,
        message: error.message,
        status: status.as_u16(),
        details: error.details.into_iter().collect::<HashMap<_, _>>(),
    }
}

/// Percent-encodes a query value (RFC 3986 unreserved characters pass).
fn url_encode(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for b in value.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char);
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    use lumia_core::proto::{Error as ApiError, ErrorCode, RefreshRequest, TokenPair};
    use prost::Message;

    use super::{page_limit, time_allowed, Api, Tokens, REQUEST_TIMEOUT};

    /// An instance as far as tokens go, on a local port: it knows one
    /// session, whose pair is (`a1`, `r1`) with `a1` expired. A request
    /// with `a1` is refused as expired; `r1` rotates to (`a2`, `r2`), slowly
    /// enough for a second call to be refused meanwhile; `r1` presented
    /// again is the reuse the protocol answers by revoking the session.
    /// Returns its address and the refresh tokens presented to it, in order.
    fn instance() -> (String, Arc<Mutex<Vec<String>>>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let presented = Arc::new(Mutex::new(Vec::new()));
        let seen = Arc::clone(&presented);
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let (mut stream, seen) = (stream.unwrap(), Arc::clone(&seen));
                std::thread::spawn(move || {
                    let mut request = Vec::new();
                    let mut chunk = [0u8; 4096];
                    let (head, body) = loop {
                        let n = stream.read(&mut chunk).unwrap();
                        request.extend_from_slice(&chunk[..n]);
                        let Some(end) = request.windows(4).position(|w| w == b"\r\n\r\n") else {
                            continue;
                        };
                        let head = String::from_utf8_lossy(&request[..end]).to_lowercase();
                        let length = head
                            .lines()
                            .find_map(|l| l.strip_prefix("content-length: "))
                            .map_or(0, |l| l.trim().parse::<usize>().unwrap());
                        if request.len() >= end + 4 + length {
                            break (head, request[end + 4..end + 4 + length].to_vec());
                        }
                    };
                    let refused = |code: ErrorCode| {
                        let error = ApiError {
                            code: code as i32,
                            message: code.as_str_name().to_lowercase(),
                            ..Default::default()
                        };
                        (401, error.encode_to_vec())
                    };
                    let (status, answer) = if head.starts_with("post /api/v1/auth/refresh") {
                        let token = RefreshRequest::decode(body.as_slice())
                            .unwrap()
                            .refresh_token;
                        let first = {
                            let mut seen = seen.lock().unwrap();
                            seen.push(token.clone());
                            seen.len() == 1
                        };
                        std::thread::sleep(Duration::from_millis(150));
                        match token.as_str() {
                            "r1" if first => {
                                let pair = TokenPair {
                                    access_token: "a2".into(),
                                    refresh_token: "r2".into(),
                                    ..Default::default()
                                };
                                (200, pair.encode_to_vec())
                            }
                            _ => refused(ErrorCode::SessionRevoked),
                        }
                    } else if head.starts_with("post /api/v1/auth/sign-in/password") {
                        refused(ErrorCode::Unauthenticated)
                    } else if head.contains("authorization: bearer a2") {
                        (200, Vec::new())
                    } else {
                        refused(ErrorCode::TokenExpired)
                    };
                    let reason = if status == 200 { "OK" } else { "Unauthorized" };
                    let head = format!(
                        "HTTP/1.1 {status} {reason}\r\ncontent-type: application/x-protobuf\r\ncontent-length: {}\r\nconnection: close\r\n\r\n",
                        answer.len()
                    );
                    stream.write_all(head.as_bytes()).unwrap();
                    stream.write_all(&answer).unwrap();
                });
            }
        });
        (base, presented)
    }

    /// Two calls go out with an expired access token and both are refused.
    /// Each used to present the refresh token, and the second, arriving
    /// after the first had rotated it, was a reuse: the instance revoked
    /// the session and the person was signed out for doing nothing.
    #[tokio::test(flavor = "multi_thread")]
    async fn calls_refused_together_rotate_the_pair_once() {
        let (base, presented) = instance();
        let api = Api::new(&base).with_tokens(Tokens {
            access: Some("a1".into()),
            refresh: Some("r1".into()),
        });
        let (first, second) = (api.clone(), api.clone());
        let (first, second) = tokio::join!(
            tokio::spawn(async move { first.account_snapshot().await }),
            tokio::spawn(async move { second.account_snapshot().await }),
        );
        first.unwrap().expect("the first call is answered");
        second.unwrap().expect("and so is the second");
        assert_eq!(*presented.lock().unwrap(), ["r1"]);
        assert_eq!(api.tokens().refresh.as_deref(), Some("r2"));
    }

    /// Another process on the same stores (a phone's notification
    /// extension) has rotated the pair and written it where this one finds
    /// it. This process still holds the retired pair in memory: it takes
    /// the stored one instead of presenting its own, which the instance
    /// would answer by revoking the session. Here the stored pair has run
    /// out as well, so it is then rotated, once, as the live pair it is.
    #[tokio::test(flavor = "multi_thread")]
    async fn a_pair_rotated_by_another_process_is_taken_from_the_store() {
        let (base, presented) = instance();
        let api = Api::new(&base)
            .with_tokens(Tokens {
                access: Some("a0".into()),
                refresh: Some("r0".into()),
            })
            .stored(Arc::new(|| {
                Some(Tokens {
                    access: Some("a1".into()),
                    refresh: Some("r1".into()),
                })
            }));
        api.account_snapshot().await.expect("the call is answered");
        assert_eq!(*presented.lock().unwrap(), ["r1"], "never `r0`");
        assert_eq!(api.tokens().refresh.as_deref(), Some("r2"));
    }

    /// A sign-in with the wrong password, made while the last session's pair
    /// is still held (a sign-out on its way, as the desktop's live test hit
    /// on a runner). Its refusal is about the password: it used to be taken
    /// for an expired token, the held pair was presented, and the person read
    /// "session revoked" for a typo.
    #[tokio::test(flavor = "multi_thread")]
    async fn a_refused_sign_in_is_not_retried_with_the_held_pair() {
        let (base, presented) = instance();
        let api = Api::new(&base).with_tokens(Tokens {
            access: Some("a1".into()),
            refresh: Some("r1".into()),
        });
        let answer = api.sign_in("ada", "wrong", vec![1; 16]).await;
        assert!(
            matches!(&answer, Err(crate::ClientError::Api { code, .. }) if code == "UNAUTHENTICATED"),
            "{answer:?}"
        );
        assert!(presented.lock().unwrap().is_empty(), "no pair presented");
    }

    /// The gateway loop rotates on its own account, with no call of its to
    /// retry. Two rotations asked for outright take turns: the second
    /// presents the pair the first brought back, never the one it retired.
    #[tokio::test(flavor = "multi_thread")]
    async fn rotations_asked_for_outright_take_turns() {
        let (base, presented) = instance();
        let api = Api::new(&base).with_tokens(Tokens {
            access: Some("a1".into()),
            refresh: Some("r1".into()),
        });
        let (first, second) = (api.clone(), api.clone());
        let _ = tokio::join!(
            tokio::spawn(async move { first.refresh_now().await }),
            tokio::spawn(async move { second.refresh_now().await }),
        );
        assert_eq!(*presented.lock().unwrap(), ["r1", "r2"]);
    }

    /// An instance that takes a request and never answers it, as a network
    /// that swallows packets looks from here. The call used to wait for as
    /// long as TCP does; it now fails once its time is up.
    #[tokio::test(flavor = "multi_thread")]
    async fn a_request_nobody_answers_fails_when_its_time_is_up() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        std::thread::spawn(move || {
            let mut held = Vec::new();
            for stream in listener.incoming() {
                held.push(stream.unwrap());
            }
        });
        let api = Api::new(&base)
            .with_token(Some("a1".into()))
            .with_timeout(Duration::from_millis(200));
        let started = std::time::Instant::now();
        let answer = api.account_snapshot().await;
        assert!(
            matches!(answer, Err(crate::ClientError::Transport(_))),
            "{answer:?}"
        );
        assert!(started.elapsed() < Duration::from_secs(5));
        // Signing out is refused the same way and no slower.
        let started = std::time::Instant::now();
        assert!(api.sign_out().await.is_err());
        assert!(started.elapsed() < Duration::from_secs(5));
    }

    #[test]
    fn what_a_request_carries_buys_it_time() {
        let base = REQUEST_TIMEOUT;
        assert_eq!(time_allowed(base, "/api/v1/account/snapshot", 0), base);
        // A picture of two megabytes at the slowest link allowed for.
        assert_eq!(
            time_allowed(base, "/api/v1/account/avatar", 2 * 1024 * 1024),
            base + Duration::from_secs(128)
        );
        // History is the large answer, coming or going.
        assert_eq!(
            time_allowed(base, "/api/v1/history/chunks/c1", 0),
            Duration::from_secs(120)
        );
        // The farewell is short, and shorter still for a client told to be.
        assert_eq!(
            time_allowed(base, "/api/v1/auth/sign-out", 0),
            Duration::from_secs(8)
        );
        assert_eq!(
            time_allowed(Duration::from_secs(1), "/api/v1/auth/sign-out", 0),
            Duration::from_secs(1)
        );
        // So is taking the push subscription back before it.
        assert_eq!(
            time_allowed(base, "/api/v1/push/subscription?kind=relay_apns", 0),
            Duration::from_secs(8)
        );
    }

    #[test]
    fn the_upper_bound_becomes_the_limit() {
        // No bound: a full page.
        assert_eq!(page_limit(10, 0), 200);
        // Events 11..=14 are wanted: four of them.
        assert_eq!(page_limit(10, 15), 4);
        // A bound wider than a page is still one page.
        assert_eq!(page_limit(10, 1000), 200);
        // A bound that leaves nothing to fetch still asks for something.
        assert_eq!(page_limit(10, 11), 200);
        assert_eq!(page_limit(10, 10), 200);
    }
}
