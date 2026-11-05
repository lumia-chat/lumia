//! The HTTP API client (docs/protocol.md section 9) with protobuf bodies.
//!
//! Tokens live behind a shared handle so one rotation is seen by every
//! clone of the client, including the gateway loop that builds the socket
//! URL. An `UNAUTHENTICATED` or `TOKEN_EXPIRED` answer with a refresh token
//! on hand refreshes once and retries the call; the new pair goes to the
//! rotation hook so the caller can persist it.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, PoisonError};

use lumia_core::proto::InstanceInfo;
use lumia_core::proto::{
    reauth_request, Account, AccountSnapshot, Ack, ChallengePurpose, ChallengeRequest,
    ChallengeResponse, Channel, ChannelSnapshot, ClaimKeyPackageRequest, ClaimKeyPackageResponse,
    Command, CreateAccountRequest, CreateChannelRequest, CreateLinkBundleRequest,
    CreateLinkBundleResponse, CreateServerRequest, Error as ApiError, ErrorCode,
    GetGroupInfoResponse, GetLinkBundleResponse, GetRosterResponse, IdentityProof,
    IdentityProofSignInRequest, ListAccountEventsResponse, ListChannelsResponse,
    ListEventsResponse, ListMembersResponse, ListServersResponse, OpenPersonalDmRequest,
    OpenPersonalDmResponse, PasswordSignInRequest, PublishGroupInfoRequest,
    PublishKeyPackagesRequest, PublishRosterRequest, PushSubscription, ReauthRequest,
    ReauthResponse, RefreshRequest, RevokeDeviceRequest, Server, SignedRoster, TokenPair,
};
use lumia_core::proto::{
    CreateInviteRequest, Invite, InvitePreview, ListInvitesResponse, ListPasskeysResponse,
    MagicLinkCompleteRequest, MagicLinkRequest, Member, PasskeyAssertFinishRequest,
    PasskeyAssertStartRequest, PasskeyChallenge, PasskeyRegisterFinishRequest, SignedProfile,
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

/// A client for one instance's HTTP API.
#[derive(Clone)]
pub struct Api {
    http: reqwest::Client,
    base: String,
    tokens: Arc<Mutex<Tokens>>,
    on_rotate: Option<RotateHook>,
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
            on_rotate: None,
        }
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

    /// Replaces the pair, as after a sign-in.
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
        let refresh = self.tokens().refresh.ok_or(ClientError::NotSignedIn)?;
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
                false,
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
            .send_once(reqwest::Method::GET, "/api/v1/instance", None, None, false)
            .await?;
        decoded(status, &bytes)
    }

    /// `POST /auth/sign-in/password` with a second factor.
    ///
    /// # Errors
    /// On transport failure or an API error (`FAILED_PRECONDITION{mfa}`
    /// without the code, `UNAUTHENTICATED` with a wrong one).
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
    ) -> Result<TokenPair, ClientError> {
        let req = CreateAccountRequest {
            handle: handle.into(),
            email: email.into(),
            password: password.into(),
            invite_code: invite_code.into(),
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
    ) -> Result<Channel, ClientError> {
        let req = CreateChannelRequest {
            server_id,
            name: name.into(),
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

    /// `GET /accounts/{id}`: another account's public profile, when the
    /// caller may see it (a shared server or personal DM, or a request).
    ///
    /// # Errors
    /// `NOT_FOUND` when not visible; transport failure.
    pub async fn account(&self, account_id: &str) -> Result<Account, ClientError> {
        self.get(&format!("/api/v1/accounts/{account_id}")).await
    }

    /// `GET /servers/{id}/members`: a page of members with their accounts,
    /// optionally those whose handle or nickname starts with `query`.
    ///
    /// # Errors
    /// On transport failure or an API error.
    pub async fn members(
        &self,
        server_id: &str,
        query: &str,
    ) -> Result<ListMembersResponse, ClientError> {
        let q = url_encode(query);
        self.get(&format!(
            "/api/v1/servers/{server_id}/members?page_size=100&query={q}"
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
        let (status, bytes) = self
            .send_once(method.clone(), path, body.clone(), extra, true)
            .await?;
        if status != StatusCode::UNAUTHORIZED || self.tokens().refresh.is_none() {
            return Ok((status, bytes));
        }
        match api_error(status, &bytes) {
            ClientError::Api { code, .. }
                if code == "TOKEN_EXPIRED" || code == "UNAUTHENTICATED" =>
            {
                self.refresh_now().await?;
                self.send_once(method, path, body, extra, true).await
            }
            _ => Ok((status, bytes)),
        }
    }

    async fn send_once(
        &self,
        method: reqwest::Method,
        path: &str,
        body: Option<Vec<u8>>,
        extra: Option<(&str, &str)>,
        authenticated: bool,
    ) -> Result<(StatusCode, Vec<u8>), ClientError> {
        let mut builder = self
            .http
            .request(method, format!("{}{path}", self.base))
            .header(ACCEPT, PROTO);
        if let Some(body) = body {
            builder = builder.header(CONTENT_TYPE, PROTO).body(body);
        }
        if let Some((name, value)) = extra {
            builder = builder.header(name, value);
        }
        if authenticated {
            if let Some(token) = self.tokens().access {
                builder = builder.header(AUTHORIZATION, format!("Bearer {token}"));
            }
        }
        let response = builder.send().await?;
        let status = response.status();
        let bytes = response.bytes().await?;
        Ok((status, bytes.to_vec()))
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
mod tests {
    use super::page_limit;

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
