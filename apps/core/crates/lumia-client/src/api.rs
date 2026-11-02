//! The HTTP API client (docs/protocol.md section 9) with protobuf bodies.

use lumia_core::proto::{
    AccountSnapshot, Channel, ChannelSnapshot, CreateAccountRequest, CreateChannelRequest,
    CreateServerRequest, Error as ApiError, ErrorCode, ListAccountEventsResponse,
    ListChannelsResponse, ListEventsResponse, ListServersResponse, PasswordSignInRequest,
    RefreshRequest, Server, TokenPair,
};
use prost::Message;
use reqwest::header::{ACCEPT, AUTHORIZATION, CONTENT_TYPE};

use crate::ClientError;

const PROTO: &str = "application/x-protobuf";

/// A client for one instance's HTTP API.
#[derive(Debug, Clone)]
pub struct Api {
    http: reqwest::Client,
    base: String,
    access_token: Option<String>,
}

impl Api {
    /// A client for the instance at `base`, such as `http://127.0.0.1:4000`.
    #[must_use]
    pub fn new(base: &str) -> Self {
        Self {
            http: reqwest::Client::new(),
            base: base.trim_end_matches('/').to_string(),
            access_token: None,
        }
    }

    /// Uses this access token for authenticated calls.
    #[must_use]
    pub fn with_token(mut self, token: Option<String>) -> Self {
        self.access_token = token;
        self
    }

    /// The gateway WebSocket URL for this instance.
    ///
    /// # Errors
    /// When not signed in.
    pub fn gateway_url(&self) -> Result<String, ClientError> {
        let token = self
            .access_token
            .as_deref()
            .ok_or(ClientError::NotSignedIn)?;
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

    /// `POST /auth/refresh`.
    ///
    /// # Errors
    /// On transport failure or an API error.
    pub async fn refresh(&self, refresh_token: &str) -> Result<TokenPair, ClientError> {
        self.post(
            "/api/v1/auth/refresh",
            &RefreshRequest {
                refresh_token: refresh_token.into(),
            },
        )
        .await
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

    async fn post<Req: Message, Resp: Message + Default>(
        &self,
        path: &str,
        req: &Req,
    ) -> Result<Resp, ClientError> {
        let mut builder = self
            .http
            .post(format!("{}{path}", self.base))
            .header(CONTENT_TYPE, PROTO)
            .header(ACCEPT, PROTO)
            .body(req.encode_to_vec());
        if let Some(token) = &self.access_token {
            builder = builder.header(AUTHORIZATION, format!("Bearer {token}"));
        }
        decode(builder.send().await?).await
    }

    async fn get<Resp: Message + Default>(&self, path: &str) -> Result<Resp, ClientError> {
        let mut builder = self
            .http
            .get(format!("{}{path}", self.base))
            .header(ACCEPT, PROTO);
        if let Some(token) = &self.access_token {
            builder = builder.header(AUTHORIZATION, format!("Bearer {token}"));
        }
        decode(builder.send().await?).await
    }
}

async fn decode<Resp: Message + Default>(response: reqwest::Response) -> Result<Resp, ClientError> {
    let status = response.status();
    let bytes = response.bytes().await?;
    if status.is_success() {
        Ok(Resp::decode(bytes.as_ref())?)
    } else {
        let error = ApiError::decode(bytes.as_ref()).unwrap_or_default();
        let code = ErrorCode::try_from(error.code)
            .map(|c| c.as_str_name().to_string())
            .unwrap_or_else(|_| "UNKNOWN".into());
        Err(ClientError::Api {
            code,
            message: error.message,
            status: status.as_u16(),
        })
    }
}
