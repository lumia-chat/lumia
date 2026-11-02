//! The HTTP API client (docs/protocol.md section 9) with protobuf bodies.
//!
//! Tokens live behind a shared handle so one rotation is seen by every
//! clone of the client, including the gateway loop that builds the socket
//! URL. An `UNAUTHENTICATED` or `TOKEN_EXPIRED` answer with a refresh token
//! on hand refreshes once and retries the call; the new pair goes to the
//! rotation hook so the caller can persist it.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, PoisonError};

use lumia_core::proto::{
    reauth_request, AccountSnapshot, Ack, ChallengePurpose, ChallengeRequest, ChallengeResponse,
    Channel, ChannelSnapshot, ClaimKeyPackageRequest, ClaimKeyPackageResponse, Command,
    CreateAccountRequest, CreateChannelRequest, CreateLinkBundleRequest, CreateLinkBundleResponse,
    CreateServerRequest, Error as ApiError, ErrorCode, GetGroupInfoResponse, GetLinkBundleResponse,
    GetRosterResponse, IdentityProof, IdentityProofSignInRequest, ListAccountEventsResponse,
    ListChannelsResponse, ListEventsResponse, ListServersResponse, OpenPersonalDmRequest,
    OpenPersonalDmResponse, PasswordSignInRequest, PublishGroupInfoRequest,
    PublishKeyPackagesRequest, PublishRosterRequest, ReauthRequest, ReauthResponse, RefreshRequest,
    RevokeDeviceRequest, Server, SignedRoster, TokenPair,
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
    ) -> Result<TokenPair, ClientError> {
        let req = CreateAccountRequest {
            handle: handle.into(),
            proof: Some(proof),
            ..Default::default()
        };
        self.post("/api/v1/accounts/by-identity", &req).await
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

    /// The gateway WebSocket URL for this instance, with the current token.
    ///
    /// # Errors
    /// When not signed in.
    pub fn gateway_url(&self) -> Result<String, ClientError> {
        let token = self.tokens().access.ok_or(ClientError::NotSignedIn)?;
        let ws = self.base.replacen("http", "ws", 1);
        Ok(format!(
            "{ws}/gateway/websocket?access_token={token}&vsn=1.0.0"
        ))
    }

    /// `POST /accounts` with a password.
    ///
    /// # Errors
    /// On transport failure or an API error.
    pub async fn create_account(
        &self,
        handle: &str,
        password: &str,
    ) -> Result<TokenPair, ClientError> {
        let req = CreateAccountRequest {
            handle: handle.into(),
            password: password.into(),
            ..Default::default()
        };
        self.post("/api/v1/accounts", &req).await
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

    /// `GET /channels/{id}/events?after_seq=&before_seq=`.
    ///
    /// # Errors
    /// On transport failure or an API error.
    pub async fn channel_events(
        &self,
        channel_id: &str,
        after: u64,
        before: u64,
    ) -> Result<ListEventsResponse, ClientError> {
        self.get(&format!(
            "/api/v1/channels/{channel_id}/events?after_seq={after}&before_seq={before}&limit=200"
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

    /// `POST /dms/personal`: opens (or finds) the personal DM with `other`.
    ///
    /// # Errors
    /// On transport failure or an API error.
    pub async fn open_personal_dm(
        &self,
        other: Vec<u8>,
    ) -> Result<OpenPersonalDmResponse, ClientError> {
        self.post(
            "/api/v1/dms/personal",
            &OpenPersonalDmRequest {
                participant_account_ids: vec![other],
            },
        )
        .await
    }

    /// `GET /accounts/{id}/roster`: another participant's roster.
    ///
    /// # Errors
    /// On transport failure or an API error (`NOT_FOUND` without a shared DM).
    pub async fn account_roster(&self, account_id: &str) -> Result<GetRosterResponse, ClientError> {
        self.get(&format!("/api/v1/accounts/{account_id}/roster"))
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
