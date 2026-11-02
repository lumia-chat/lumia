//! The gateway socket, per target: tungstenite over tokio natively, the
//! browser's `WebSocket` through gloo on the web. The gateway loop sees
//! only [`Socket`]: a stream of [`Frame`]s and a sink of binary messages.

use std::pin::Pin;
use std::task::{Context, Poll};

use futures_util::{Sink, Stream};

use crate::ClientError;

/// What arrives on the socket.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Frame {
    /// A binary message: a `ServerFrame`.
    Binary(Vec<u8>),
    /// The peer closed, with its reason.
    Close(String),
    /// Anything else (text, ping, pong); ignored.
    Other,
}

/// A connected gateway socket.
pub trait Socket:
    Stream<Item = Result<Frame, ClientError>> + Sink<Vec<u8>, Error = ClientError> + Unpin
{
}

impl<T> Socket for T where
    T: Stream<Item = Result<Frame, ClientError>> + Sink<Vec<u8>, Error = ClientError> + Unpin
{
}

/// A boxed socket: `Send` natively, where the loop runs on a thread pool;
/// the browser's socket is not, and runs on the worker's single thread.
#[cfg(not(target_arch = "wasm32"))]
pub type BoxSocket = Box<dyn Socket + Send>;
/// A boxed socket (see the native alias).
#[cfg(target_arch = "wasm32")]
pub type BoxSocket = Box<dyn Socket>;

/// Opens the gateway socket at `url`.
///
/// # Errors
/// Natively, a refused upgrade (401 or 403) is reported as the API error
/// `UNAUTHENTICATED` so the loop refreshes the token and tries again.
/// Phoenix answers 403 for every refused connect, so at this point an
/// expired access token is indistinguishable from a revoked session or
/// device; the refresh's answer (`SESSION_REVOKED`) is what tells them
/// apart, and that is where the loop stops. The browser does not expose
/// the status of a failed upgrade at all, so there every failure is a
/// transport error and the loop refreshes before its next attempt.
#[cfg(not(target_arch = "wasm32"))]
pub use native::upgrade_request;

pub async fn connect(url: &str, subprotocols: &[String]) -> Result<BoxSocket, ClientError> {
    #[cfg(not(target_arch = "wasm32"))]
    {
        native::connect(url, subprotocols).await
    }
    #[cfg(target_arch = "wasm32")]
    {
        web::connect(url, subprotocols).await
    }
}

// 401 and 403 alike: the status cannot say whether the token merely
// expired, so the loop refreshes and lets the refresh's answer decide.
#[cfg(not(target_arch = "wasm32"))]
fn refused(status: u16) -> ClientError {
    ClientError::Api {
        code: "UNAUTHENTICATED".into(),
        message: "gateway refused the access token (expired, or the session is revoked)".into(),
        status,
        details: std::collections::HashMap::new(),
    }
}

#[cfg(not(target_arch = "wasm32"))]
mod native {
    use super::{refused, BoxSocket, Context, Frame, Pin, Poll, Sink, Stream};
    use crate::ClientError;
    use tokio_tungstenite::tungstenite::{self, Message};
    use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};

    struct Native(WebSocketStream<MaybeTlsStream<tokio::net::TcpStream>>);

    impl Stream for Native {
        type Item = Result<Frame, ClientError>;

        fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
            Pin::new(&mut self.0).poll_next(cx).map(|item| {
                item.map(|r| match r {
                    Ok(Message::Binary(bytes)) => Ok(Frame::Binary(bytes.to_vec())),
                    Ok(Message::Close(frame)) => Ok(Frame::Close(format!("{frame:?}"))),
                    Ok(_) => Ok(Frame::Other),
                    Err(e) => Err(ClientError::Transport(e.to_string())),
                })
            })
        }
    }

    impl Sink<Vec<u8>> for Native {
        type Error = ClientError;

        fn poll_ready(
            mut self: Pin<&mut Self>,
            cx: &mut Context<'_>,
        ) -> Poll<Result<(), ClientError>> {
            Pin::new(&mut self.0)
                .poll_ready(cx)
                .map_err(|e| ClientError::Transport(e.to_string()))
        }

        fn start_send(mut self: Pin<&mut Self>, item: Vec<u8>) -> Result<(), ClientError> {
            Pin::new(&mut self.0)
                .start_send(Message::Binary(item.into()))
                .map_err(|e| ClientError::Transport(e.to_string()))
        }

        fn poll_flush(
            mut self: Pin<&mut Self>,
            cx: &mut Context<'_>,
        ) -> Poll<Result<(), ClientError>> {
            Pin::new(&mut self.0)
                .poll_flush(cx)
                .map_err(|e| ClientError::Transport(e.to_string()))
        }

        fn poll_close(
            mut self: Pin<&mut Self>,
            cx: &mut Context<'_>,
        ) -> Poll<Result<(), ClientError>> {
            Pin::new(&mut self.0)
                .poll_close(cx)
                .map_err(|e| ClientError::Transport(e.to_string()))
        }
    }

    /// The upgrade request with the subprotocols (the token among them)
    /// on it, for `tokio_tungstenite::connect_async`.
    ///
    /// # Errors
    /// When the URL or a subprotocol is malformed.
    pub fn upgrade_request(
        url: &str,
        subprotocols: &[String],
    ) -> Result<tungstenite::handshake::client::Request, ClientError> {
        use tungstenite::client::IntoClientRequest as _;
        let mut request = url
            .into_client_request()
            .map_err(|e| ClientError::Transport(e.to_string()))?;
        let header = subprotocols
            .join(", ")
            .parse()
            .map_err(|_| ClientError::Transport("bad subprotocol".into()))?;
        request
            .headers_mut()
            .insert("Sec-WebSocket-Protocol", header);
        Ok(request)
    }

    pub async fn connect(url: &str, subprotocols: &[String]) -> Result<BoxSocket, ClientError> {
        let request = upgrade_request(url, subprotocols)?;
        match tokio_tungstenite::connect_async(request).await {
            Ok((ws, _)) => Ok(Box::new(Native(ws))),
            Err(tungstenite::Error::Http(response))
                if response.status() == 401 || response.status() == 403 =>
            {
                Err(refused(response.status().as_u16()))
            }
            Err(e) => Err(ClientError::Transport(e.to_string())),
        }
    }
}

#[cfg(target_arch = "wasm32")]
mod web {
    use super::{BoxSocket, Context, Frame, Pin, Poll, Sink, Stream};
    use crate::ClientError;
    use gloo_net::websocket::futures::WebSocket;
    use gloo_net::websocket::{Message, WebSocketError};

    struct Web(WebSocket);

    impl Stream for Web {
        type Item = Result<Frame, ClientError>;

        fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
            Pin::new(&mut self.0).poll_next(cx).map(|item| {
                item.map(|r| match r {
                    Ok(Message::Bytes(bytes)) => Ok(Frame::Binary(bytes)),
                    Ok(Message::Text(_)) => Ok(Frame::Other),
                    Err(WebSocketError::ConnectionClose(close)) => {
                        Ok(Frame::Close(format!("{} {}", close.code, close.reason)))
                    }
                    Err(e) => Err(ClientError::Transport(e.to_string())),
                })
            })
        }
    }

    impl Sink<Vec<u8>> for Web {
        type Error = ClientError;

        fn poll_ready(
            mut self: Pin<&mut Self>,
            cx: &mut Context<'_>,
        ) -> Poll<Result<(), ClientError>> {
            Pin::new(&mut self.0)
                .poll_ready(cx)
                .map_err(|e| ClientError::Transport(e.to_string()))
        }

        fn start_send(mut self: Pin<&mut Self>, item: Vec<u8>) -> Result<(), ClientError> {
            Pin::new(&mut self.0)
                .start_send(Message::Bytes(item))
                .map_err(|e| ClientError::Transport(e.to_string()))
        }

        fn poll_flush(
            mut self: Pin<&mut Self>,
            cx: &mut Context<'_>,
        ) -> Poll<Result<(), ClientError>> {
            Pin::new(&mut self.0)
                .poll_flush(cx)
                .map_err(|e| ClientError::Transport(e.to_string()))
        }

        fn poll_close(
            mut self: Pin<&mut Self>,
            cx: &mut Context<'_>,
        ) -> Poll<Result<(), ClientError>> {
            Pin::new(&mut self.0)
                .poll_close(cx)
                .map_err(|e| ClientError::Transport(e.to_string()))
        }
    }

    pub async fn connect(url: &str, subprotocols: &[String]) -> Result<BoxSocket, ClientError> {
        let ws = WebSocket::open_with_protocols(url, subprotocols)
            .map_err(|e| ClientError::Transport(e.to_string()))?;
        Ok(Box::new(Web(ws)))
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    #[test]
    fn the_upgrade_carries_the_subprotocols() {
        let request = super::upgrade_request(
            "ws://localhost:4000/gateway/websocket?vsn=1.0.0",
            &["phoenix".to_string(), "base64url.bearer.phx.abc".to_string()],
        )
        .unwrap();
        let header = request
            .headers()
            .get("sec-websocket-protocol")
            .and_then(|v| v.to_str().ok())
            .unwrap();
        assert_eq!(header, "phoenix, base64url.bearer.phx.abc");
        assert_eq!(request.uri().path(), "/gateway/websocket");
    }
}
