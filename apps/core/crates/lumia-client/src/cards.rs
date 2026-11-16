//! Fetching a link's card on a native client (media spec 6.3, for 6.5's personal DMs):
//! the page's head, then its image, under the same rules the instance keeps.
//!
//! Every hop is vetted before anything connects: `http` or `https`, port 80 or 443, the host
//! resolved here and refused if any address it resolves to is not
//! [`cards::address_allowed`], and the connection made to those vetted addresses only (a
//! client built per hop, with its resolver overridden for the host and every other name
//! refused, no proxy, redirects off and followed here). This keeps a link in a message from
//! reading the sharer's own router or anything else on their network. The clients share one
//! TLS configuration, built once.
//!
//! Native only: a browser cannot read other sites (CORS), so the web client sends no cards
//! in personal DMs (6.5) and has no fetcher.

use std::net::{IpAddr, SocketAddr};
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use lumia_core::cards::{self, CardDraft, MAX_HTML_BYTES, MAX_IMAGE_BYTES, MAX_REDIRECTS};
use reqwest::header::{
    HeaderValue, ACCEPT, ACCEPT_LANGUAGE, CONTENT_LENGTH, CONTENT_TYPE, LOCATION,
};
use reqwest::{redirect, StatusCode};
use url::{Host, Url};

/// What the fetcher calls itself (6.3).
pub const USER_AGENT: &str = concat!(
    "LumiaBot/",
    env!("CARGO_PKG_VERSION"),
    " (+https://lumia.chat/bot)"
);

/// How long a fetch may take, all of it: the page with its redirects, and its image with
/// its own. What is left of it when the page is read is the image's.
pub const TIMEOUT: Duration = Duration::from_secs(5);

/// A card as fetched: what the page's head gave, its `url` the link as given, and its
/// image's bytes as served, unprocessed (the thumbnail is made elsewhere). `image` is
/// `None` when the page named none or it could not be fetched within the rules.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FetchedCard {
    pub card: CardDraft,
    pub image: Option<Vec<u8>>,
}

/// Why a page was not fetched.
#[derive(Debug, thiserror::Error)]
pub enum CardError {
    /// The link is not an absolute URL with a host.
    #[error("not a link: {0}")]
    BadUrl(String),
    /// A scheme other than `http` and `https`.
    #[error("only http and https links are fetched, not {0}")]
    Scheme(String),
    /// A port other than 80 and 443.
    #[error("only ports 80 and 443 are fetched, not {0}")]
    Port(u16),
    /// The host is, or resolves to, an address a card is never fetched from.
    #[error("{host} is at {address}, which is not fetched from")]
    Address { host: String, address: IpAddr },
    /// The host did not resolve.
    #[error("{0} did not resolve")]
    Resolve(String),
    /// More than [`MAX_REDIRECTS`] redirects, or one without a usable `Location`.
    #[error("too many redirects, or a redirect to nowhere")]
    Redirect,
    /// The page answered with a status that is neither success nor a redirect.
    #[error("the page answered {0}")]
    Status(u16),
    /// The fetch took longer than [`TIMEOUT`].
    #[error("the page took longer than 5 seconds")]
    Timeout,
    /// The connection failed.
    #[error("transport: {0}")]
    Transport(String),
}

impl From<reqwest::Error> for CardError {
    fn from(e: reqwest::Error) -> Self {
        if e.is_timeout() {
            Self::Timeout
        } else {
            Self::Transport(e.to_string())
        }
    }
}

/// Fetches the card for `url` (6.3): `Ok(None)` when the page was read and gives no card
/// (it is not HTML, or its head has no title), an error when it could not be read or was
/// refused. `accept_language` is the sharer's locale (`en-CA`, or a full header value);
/// empty sends none.
pub async fn fetch_card(
    url: &str,
    accept_language: &str,
) -> Result<Option<FetchedCard>, CardError> {
    fetch_with(&Rules::spec(), url, accept_language).await
}

/// The rules a fetch is held to. Only tests loosen them, to reach a local server.
struct Rules {
    ports: Vec<u16>,
    allowed: fn(IpAddr) -> bool,
    timeout: Duration,
}

impl Rules {
    fn spec() -> Self {
        Self {
            ports: vec![80, 443],
            allowed: cards::address_allowed,
            timeout: TIMEOUT,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Want {
    Page,
    Image,
}

async fn fetch_with(
    rules: &Rules,
    url: &str,
    accept_language: &str,
) -> Result<Option<FetchedCard>, CardError> {
    // One deadline for the page and its image together (6.3).
    let deadline = tokio::time::Instant::now() + rules.timeout;
    let page = tokio::time::timeout_at(deadline, get(rules, url, accept_language, Want::Page))
        .await
        .map_err(|_| CardError::Timeout)??;
    let Some((at, html)) = page else {
        return Ok(None);
    };
    let Some(mut card) = cards::parse_head(&html, at.as_str()) else {
        return Ok(None);
    };
    card.url = url.to_owned();
    let image = if card.image_url.is_empty() {
        None
    } else {
        let fetch = get(rules, &card.image_url, accept_language, Want::Image);
        match tokio::time::timeout_at(deadline, fetch).await {
            Ok(Ok(Some((_, bytes)))) => Some(bytes),
            Ok(Ok(None)) => None,
            Ok(Err(e)) => {
                tracing::debug!(url = %card.image_url, error = %e, "card image not fetched");
                None
            }
            Err(_) => {
                tracing::debug!(url = %card.image_url, "card image timed out");
                None
            }
        }
    };
    Ok(Some(FetchedCard { card, image }))
}

/// GETs `url`, following redirects by the rules: the address finally read and its body, or
/// `None` when what it served is not wanted (a page that is not HTML, an image too large).
async fn get(
    rules: &Rules,
    url: &str,
    accept_language: &str,
    want: Want,
) -> Result<Option<(Url, Vec<u8>)>, CardError> {
    let mut url = Url::parse(url.trim()).map_err(|_| CardError::BadUrl(url.to_owned()))?;
    let accept = match want {
        Want::Page => "text/html,application/xhtml+xml;q=0.9,*/*;q=0.1",
        Want::Image => "image/*,*/*;q=0.1",
    };
    let language = HeaderValue::from_str(accept_language.trim())
        .ok()
        .filter(|v| !v.is_empty());
    for hop in 0..=MAX_REDIRECTS {
        let (client, host) = vetted(rules, &url).await?;
        let mut request = client.get(url.clone()).header(ACCEPT, accept);
        if let Some(language) = &language {
            request = request.header(ACCEPT_LANGUAGE, language.clone());
        }
        let mut response = request.send().await?;
        // The connection went to a vetted address; say so again where it can be seen.
        if let Some(address) = response.remote_addr().map(|a| a.ip()) {
            if !(rules.allowed)(address) {
                return Err(CardError::Address { host, address });
            }
        }
        let status = response.status();
        if matches!(
            status,
            StatusCode::MOVED_PERMANENTLY
                | StatusCode::FOUND
                | StatusCode::SEE_OTHER
                | StatusCode::TEMPORARY_REDIRECT
                | StatusCode::PERMANENT_REDIRECT
        ) {
            if hop == MAX_REDIRECTS {
                return Err(CardError::Redirect);
            }
            url = response
                .headers()
                .get(LOCATION)
                .and_then(|l| l.to_str().ok())
                .and_then(|l| url.join(l.trim()).ok())
                .ok_or(CardError::Redirect)?;
            continue;
        }
        if !status.is_success() {
            return Err(CardError::Status(status.as_u16()));
        }
        let body = match want {
            Want::Page => {
                if !is_html(&response) {
                    return Ok(None);
                }
                Some(read_head(&mut response).await?)
            }
            Want::Image => read_image(&mut response).await?,
        };
        return Ok(body.map(|body| (url, body)));
    }
    Err(CardError::Redirect)
}

/// A client that can reach `url`'s host only at addresses vetted here, and the host's name.
async fn vetted(rules: &Rules, url: &Url) -> Result<(reqwest::Client, String), CardError> {
    let scheme = url.scheme();
    if !matches!(scheme, "http" | "https") {
        return Err(CardError::Scheme(scheme.to_owned()));
    }
    let port = url
        .port_or_known_default()
        .ok_or_else(|| CardError::BadUrl(url.to_string()))?;
    if !rules.ports.contains(&port) {
        return Err(CardError::Port(port));
    }
    // No timeout of its own: the fetch's one deadline covers every hop.
    let builder = reqwest::Client::builder()
        .redirect(redirect::Policy::none())
        .no_proxy()
        .user_agent(USER_AGENT)
        .tls_backend_preconfigured(tls()?)
        .dns_resolver(Arc::new(Refuse));
    let vet = |host: &str, address: IpAddr| {
        if (rules.allowed)(address) {
            Ok(())
        } else {
            Err(CardError::Address {
                host: host.to_owned(),
                address,
            })
        }
    };
    let (builder, host) = match url.host() {
        Some(Host::Ipv4(ip)) => {
            vet(&ip.to_string(), ip.into())?;
            (builder, ip.to_string())
        }
        Some(Host::Ipv6(ip)) => {
            vet(&ip.to_string(), ip.into())?;
            (builder, ip.to_string())
        }
        Some(Host::Domain(name)) => {
            let addresses: Vec<SocketAddr> = tokio::net::lookup_host((name, port))
                .await
                .map_err(|_| CardError::Resolve(name.to_owned()))?
                .collect();
            if addresses.is_empty() {
                return Err(CardError::Resolve(name.to_owned()));
            }
            for address in &addresses {
                vet(name, address.ip())?;
            }
            (builder.resolve_to_addrs(name, &addresses), name.to_owned())
        }
        None => return Err(CardError::BadUrl(url.to_string())),
    };
    Ok((builder.build()?, host))
}

/// The TLS configuration every hop's client shares, built once: what reqwest builds for
/// itself (the platform's verifier, TLS 1.2 and 1.3, `h2` and `http/1.1`), but not again
/// per client, which on Linux would read and parse the system's certificates every hop.
fn tls() -> Result<rustls::ClientConfig, CardError> {
    static TLS: OnceLock<Result<rustls::ClientConfig, String>> = OnceLock::new();
    TLS.get_or_init(|| {
        let provider = rustls::crypto::CryptoProvider::get_default()
            .cloned()
            .unwrap_or_else(|| Arc::new(rustls::crypto::aws_lc_rs::default_provider()));
        let verifier =
            rustls_platform_verifier::Verifier::new(provider.clone()).map_err(|e| e.to_string())?;
        let mut config = rustls::ClientConfig::builder_with_provider(provider)
            .with_protocol_versions(rustls::ALL_VERSIONS)
            .map_err(|e| e.to_string())?
            .dangerous()
            .with_custom_certificate_verifier(Arc::new(verifier))
            .with_no_client_auth();
        config.alpn_protocols = vec![b"h2".to_vec(), b"http/1.1".to_vec()];
        Ok(config)
    })
    .clone()
    .map_err(CardError::Transport)
}

/// A resolver that resolves nothing: every name a fetch connects to is overridden with the
/// addresses vetted for it, so a lookup that reaches this one is refused.
struct Refuse;

impl reqwest::dns::Resolve for Refuse {
    fn resolve(&self, name: reqwest::dns::Name) -> reqwest::dns::Resolving {
        let name = name.as_str().to_owned();
        Box::pin(async move { Err(format!("{name} was not vetted for this fetch").into()) })
    }
}

fn is_html(response: &reqwest::Response) -> bool {
    let Some(value) = response
        .headers()
        .get(CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
    else {
        return false;
    };
    let essence = value.split(';').next().unwrap_or_default().trim();
    essence.eq_ignore_ascii_case("text/html")
        || essence.eq_ignore_ascii_case("application/xhtml+xml")
}

/// A page's body up to the end of its head or [`MAX_HTML_BYTES`], whichever is first. The
/// head has ended where [`cards::head_ended`] says, so a `</head` in a comment or a script
/// does not end it; that reads the whole body, so it is asked only when what has just
/// arrived holds `</head` or `<body`, and at most [`MAX_HEAD_CHECKS`] times.
async fn read_head(response: &mut reqwest::Response) -> Result<Vec<u8>, CardError> {
    let mut body = Vec::new();
    let mut checks = 0;
    while let Some(chunk) = response.chunk().await? {
        let from = body.len().saturating_sub(HEAD_END_OVERLAP);
        body.extend_from_slice(&chunk);
        if body.len() >= MAX_HTML_BYTES {
            break;
        }
        if may_end_head(body.get(from..).unwrap_or_default()) {
            checks += 1;
            if checks > MAX_HEAD_CHECKS || cards::head_ended(&body) {
                break;
            }
        }
    }
    body.truncate(MAX_HTML_BYTES);
    Ok(body)
}

/// What can end a head.
const HEAD_ENDS: [&[u8]; 2] = [b"</head", b"<body"];
/// How much of what was read before a chunk is searched again with it: a whole `</head`,
/// so one that arrived as the last bytes of a chunk, its name not yet known to be whole, is
/// asked about again when the next arrives.
const HEAD_END_OVERLAP: usize = 6;
/// The most times a page's head is asked whether it has ended. A page with more `</head`
/// and `<body` in its comments and scripts than this, each in its own chunk, is read up to
/// where it was when this ran out, as a search for the bytes would have read it, rather than
/// scanned from its start once per chunk.
const MAX_HEAD_CHECKS: usize = 16;

/// Whether `bytes` hold `</head` or `<body`, in any case.
fn may_end_head(bytes: &[u8]) -> bool {
    HEAD_ENDS.iter().any(|end| {
        bytes
            .windows(end.len())
            .any(|w| w.eq_ignore_ascii_case(end))
    })
}

/// An image's body, or `None` when it is larger than [`MAX_IMAGE_BYTES`].
async fn read_image(response: &mut reqwest::Response) -> Result<Option<Vec<u8>>, CardError> {
    let declared = response
        .headers()
        .get(CONTENT_LENGTH)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.trim().parse::<u64>().ok());
    if declared.is_some_and(|len| len > MAX_IMAGE_BYTES as u64) {
        return Ok(None);
    }
    let mut body = Vec::new();
    while let Some(chunk) = response.chunk().await? {
        body.extend_from_slice(&chunk);
        if body.len() > MAX_IMAGE_BYTES {
            return Ok(None);
        }
    }
    Ok(Some(body))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Mutex;

    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    use super::*;

    /// What a test server answers for a path, given its own port.
    type Route = fn(&str, u16) -> Vec<u8>;

    struct Server {
        port: u16,
        connections: Arc<AtomicUsize>,
        requests: Arc<Mutex<Vec<String>>>,
    }

    /// A tiny HTTP/1.1 server on 127.0.0.1 answering each request with `route`.
    async fn serve(route: Route) -> Server {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let connections = Arc::new(AtomicUsize::new(0));
        let requests = Arc::new(Mutex::new(Vec::new()));
        let (count, seen) = (connections.clone(), requests.clone());
        tokio::spawn(async move {
            while let Ok((mut socket, _)) = listener.accept().await {
                count.fetch_add(1, Ordering::SeqCst);
                let seen = seen.clone();
                tokio::spawn(async move {
                    let mut request = Vec::new();
                    let mut buf = [0u8; 1024];
                    while !request.windows(4).any(|w| w == b"\r\n\r\n") {
                        match socket.read(&mut buf).await {
                            Ok(0) | Err(_) => return,
                            Ok(n) => request.extend_from_slice(&buf[..n]),
                        }
                    }
                    let request = String::from_utf8_lossy(&request).into_owned();
                    let path = request.split(' ').nth(1).unwrap_or("/").to_owned();
                    seen.lock().unwrap().push(request);
                    // Written in pieces, a pause between each: what a reader gets as
                    // separate chunks, or waits for.
                    let answer = route(&path, port);
                    for (i, piece) in split_on(&answer, PAUSE).into_iter().enumerate() {
                        if i > 0 {
                            tokio::time::sleep(PAUSE_FOR).await;
                        }
                        let _ = socket.write_all(piece).await;
                    }
                    if path == "/forever" || path == "/silent" || path.starts_with("/held") {
                        tokio::time::sleep(Duration::from_secs(60)).await;
                    }
                    let _ = socket.shutdown().await;
                });
            }
        });
        Server {
            port,
            connections,
            requests,
        }
    }

    /// Where a route's answer pauses, for [`PAUSE_FOR`].
    const PAUSE: &[u8] = b"\0PAUSE\0";
    const PAUSE_FOR: Duration = Duration::from_millis(100);

    fn split_on<'a>(bytes: &'a [u8], marker: &[u8]) -> Vec<&'a [u8]> {
        let mut pieces = Vec::new();
        let mut rest = bytes;
        while let Some(at) = rest.windows(marker.len()).position(|w| w == marker) {
            pieces.push(&rest[..at]);
            rest = &rest[at + marker.len()..];
        }
        pieces.push(rest);
        pieces
    }

    fn response(status: &str, headers: &[(&str, &str)], body: &[u8]) -> Vec<u8> {
        let mut out = format!("HTTP/1.1 {status}\r\nconnection: close\r\n");
        for (name, value) in headers {
            out.push_str(&format!("{name}: {value}\r\n"));
        }
        out.push_str(&format!("content-length: {}\r\n\r\n", body.len()));
        let mut out = out.into_bytes();
        out.extend_from_slice(body);
        out
    }

    fn redirect_to(location: &str) -> Vec<u8> {
        response("302 Found", &[("location", location)], b"")
    }

    const PAGE: &str = "<html><head><title>Local</title>\
        <meta property=\"og:title\" content=\"A local page\">\
        <meta property=\"og:image\" content=\"/image.png\"></head><body>hello</body></html>";

    fn site(path: &str, port: u16) -> Vec<u8> {
        let html = [("content-type", "text/html; charset=utf-8")];
        match path {
            "/" | "/page" => response("200 OK", &html, PAGE.as_bytes()),
            "/image.png" => response("200 OK", &[("content-type", "image/png")], b"\x89PNG fake"),
            "/hop/5" => redirect_to("/page"),
            p if p.starts_with("/hop/") => {
                let n: u32 = p["/hop/".len()..].parse().unwrap();
                redirect_to(&format!("http://127.0.0.1:{port}/hop/{}", n + 1))
            }
            "/loop" => redirect_to("/loop"),
            "/to-private" => redirect_to("http://10.0.0.1/"),
            "/to-ftp" => redirect_to("ftp://127.0.0.1/"),
            "/to-port" => redirect_to("http://127.0.0.1:22/"),
            "/json" => response(
                "200 OK",
                &[("content-type", "application/json")],
                b"{\"title\":\"no\"}",
            ),
            "/no-title" => response("200 OK", &html, b"<head><meta name=x content=y></head>"),
            "/missing" => response("404 Not Found", &html, b"<title>gone</title>"),
            "/big-image" => response(
                "200 OK",
                &html,
                b"<head><title>Big</title><meta property=og:image content=/huge.png></head>",
            ),
            "/huge.png" => {
                b"HTTP/1.1 200 OK\r\ncontent-type: image/png\r\ncontent-length: 6000000\r\n\r\nPNG"
                    .to_vec()
            }
            // A head and then a body that never ends: reading stops at the head.
            "/forever" => {
                let mut out = b"HTTP/1.1 200 OK\r\ncontent-type: text/html\r\n\r\n".to_vec();
                out.extend_from_slice(b"<head><title>Endless</title></head><body>");
                out
            }
            "/silent" => Vec::new(),
            // A `</head` that does not end the head arrives first, the rest later.
            "/held/comment" => [
                &b"HTTP/1.1 200 OK\r\ncontent-type: text/html\r\n\r\n"[..],
                b"<head><!-- </head> -->",
                PAUSE,
                b"<meta property=og:title content=\"After the comment\"></head><body>",
            ]
            .concat(),
            "/held/script" => [
                &b"HTTP/1.1 200 OK\r\ncontent-type: text/html\r\n\r\n"[..],
                b"<head><script>document.write('<BODY>')</script>",
                PAUSE,
                b"<title>After the script</title></head><body>",
            ]
            .concat(),
            // A page and then its image, each 1.5 s in coming.
            "/slow" => [
                PAUSE.repeat(15),
                response(
                    "200 OK",
                    &html,
                    b"<head><title>Slow</title><meta property=og:image content=/slow.png>",
                ),
            ]
            .concat(),
            "/slow.png" => [
                PAUSE.repeat(15),
                response("200 OK", &[("content-type", "image/png")], b"\x89PNG slow"),
            ]
            .concat(),
            _ => response("404 Not Found", &[], b""),
        }
    }

    /// The spec's rules, but for `port` and loopback: the test server is local.
    fn local(port: u16) -> Rules {
        Rules {
            ports: vec![80, 443, port],
            allowed: |ip| ip.is_loopback(),
            timeout: TIMEOUT,
        }
    }

    #[tokio::test]
    async fn loopback_is_refused_without_connecting() {
        let server = serve(site).await;
        let rules = Rules {
            ports: vec![server.port],
            ..Rules::spec()
        };
        for url in [
            format!("http://127.0.0.1:{}/", server.port),
            format!("http://localhost:{}/", server.port),
            format!("http://[::ffff:127.0.0.1]:{}/", server.port),
        ] {
            let got = fetch_with(&rules, &url, "en").await;
            assert!(
                matches!(got, Err(CardError::Address { .. })),
                "{url}: {got:?}"
            );
        }
        assert_eq!(server.connections.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn the_spec_refuses_before_any_lookup_or_connection() {
        let refused = [
            ("ftp://example.com/", "Scheme"),
            ("file:///etc/passwd", "Scheme"),
            ("javascript:alert(1)", "Scheme"),
            ("http://example.com:8080/", "Port"),
            ("https://example.com:22/", "Port"),
            ("http://127.0.0.1/", "Address"),
            ("http://[::1]/", "Address"),
            ("http://169.254.169.254/latest/meta-data/", "Address"),
            ("http://[fd00:ec2::254]/", "Address"),
            ("http://[::ffff:10.0.0.1]/", "Address"),
            ("http://2130706433/", "Address"),
            ("http://0x7f.1/", "Address"),
            ("not a url", "BadUrl"),
        ];
        for (url, want) in refused {
            let got = fetch_card(url, "en").await;
            let kind = match &got {
                Err(CardError::Scheme(_)) => "Scheme",
                Err(CardError::Port(_)) => "Port",
                Err(CardError::Address { .. }) => "Address",
                Err(CardError::BadUrl(_)) => "BadUrl",
                _ => "other",
            };
            assert_eq!(kind, want, "{url}: {got:?}");
        }
    }

    #[tokio::test]
    async fn a_page_and_its_image_are_fetched() {
        let server = serve(site).await;
        let url = format!("http://127.0.0.1:{}/page", server.port);
        let got = fetch_with(&local(server.port), &url, "fr-CA")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(got.card.url, url);
        assert_eq!(got.card.title, "A local page");
        assert_eq!(
            got.card.image_url,
            format!("http://127.0.0.1:{}/image.png", server.port)
        );
        assert_eq!(got.image.as_deref(), Some(&b"\x89PNG fake"[..]));
        let requests = server.requests.lock().unwrap();
        assert_eq!(requests.len(), 2);
        for request in requests.iter() {
            let lower = request.to_ascii_lowercase();
            assert!(lower.contains(&format!("user-agent: {}", USER_AGENT.to_ascii_lowercase())));
            assert!(lower.contains("accept-language: fr-ca"));
            assert!(!lower.contains("cookie"));
        }
    }

    #[tokio::test]
    async fn a_host_name_is_connected_to_at_its_vetted_address() {
        let server = serve(site).await;
        let url = format!("http://localhost:{}/", server.port);
        let got = fetch_with(&local(server.port), &url, "").await.unwrap();
        assert_eq!(got.map(|c| c.card.title), Some("A local page".into()));
    }

    #[tokio::test]
    async fn five_redirects_are_followed_and_a_sixth_is_not() {
        let server = serve(site).await;
        let rules = local(server.port);
        let five = format!("http://127.0.0.1:{}/hop/1", server.port);
        let got = fetch_with(&rules, &five, "en").await.unwrap().unwrap();
        // The card's url is the link; its image resolves against the page it ended at.
        assert_eq!(got.card.url, five);
        assert_eq!(got.card.title, "A local page");
        let six = format!("http://127.0.0.1:{}/hop/0", server.port);
        assert!(matches!(
            fetch_with(&rules, &six, "en").await,
            Err(CardError::Redirect)
        ));
        let endless = format!("http://127.0.0.1:{}/loop", server.port);
        assert!(matches!(
            fetch_with(&rules, &endless, "en").await,
            Err(CardError::Redirect)
        ));
    }

    #[tokio::test]
    async fn every_hop_is_held_to_the_rules() {
        let server = serve(site).await;
        let rules = local(server.port);
        let at = |path: &str| format!("http://127.0.0.1:{}{path}", server.port);
        let got = fetch_with(&rules, &at("/to-private"), "en").await;
        assert!(matches!(got, Err(CardError::Address { .. })), "{got:?}");
        let got = fetch_with(&rules, &at("/to-ftp"), "en").await;
        assert!(matches!(got, Err(CardError::Scheme(_))), "{got:?}");
        let got = fetch_with(&rules, &at("/to-port"), "en").await;
        assert!(matches!(got, Err(CardError::Port(22))), "{got:?}");
    }

    #[tokio::test]
    async fn what_gives_no_card_is_none_and_a_failure_is_an_error() {
        let server = serve(site).await;
        let rules = local(server.port);
        let at = |path: &str| format!("http://127.0.0.1:{}{path}", server.port);
        assert_eq!(fetch_with(&rules, &at("/json"), "en").await.unwrap(), None);
        assert_eq!(
            fetch_with(&rules, &at("/no-title"), "en").await.unwrap(),
            None
        );
        assert!(matches!(
            fetch_with(&rules, &at("/missing"), "en").await,
            Err(CardError::Status(404))
        ));
    }

    #[tokio::test]
    async fn an_image_over_the_limit_is_left_out() {
        let server = serve(site).await;
        let url = format!("http://127.0.0.1:{}/big-image", server.port);
        let got = fetch_with(&local(server.port), &url, "en")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(got.card.title, "Big");
        assert_eq!(got.image, None);
    }

    #[tokio::test]
    async fn reading_stops_at_the_end_of_the_head() {
        let server = serve(site).await;
        let url = format!("http://127.0.0.1:{}/forever", server.port);
        // The page never ends; a fetch that waited for its end would run into
        // the time limit and fail. Long enough not to race a loaded machine.
        let rules = Rules {
            timeout: Duration::from_secs(30),
            ..local(server.port)
        };
        let got = fetch_with(&rules, &url, "en").await.unwrap();
        assert_eq!(got.map(|c| c.card.title), Some("Endless".into()));
    }

    #[tokio::test]
    async fn a_page_that_never_answers_times_out() {
        let server = serve(site).await;
        let rules = Rules {
            timeout: Duration::from_millis(300),
            ..local(server.port)
        };
        // The server answers nothing to this path and holds the connection.
        let url = format!("http://127.0.0.1:{}/silent", server.port);
        let got = fetch_with(&rules, &url, "en").await;
        assert!(matches!(got, Err(CardError::Timeout)), "{got:?}");
    }

    #[tokio::test]
    async fn a_head_end_in_a_comment_or_a_script_does_not_stop_the_read() {
        let server = serve(site).await;
        // The pages hold their connections after their heads: a read that waited for more
        // would run into the time limit and fail. Long enough not to race a loaded machine.
        let rules = Rules {
            timeout: Duration::from_secs(30),
            ..local(server.port)
        };
        for (path, title) in [
            ("/held/comment", "After the comment"),
            ("/held/script", "After the script"),
        ] {
            let url = format!("http://127.0.0.1:{}{path}", server.port);
            let got = fetch_with(&rules, &url, "en").await.unwrap();
            assert_eq!(got.map(|c| c.card.title), Some(title.into()), "{path}");
        }
    }

    #[test]
    fn a_head_end_split_across_chunks_is_still_seen() {
        let page = b"<head><title>x</title></head><body>";
        for cut in 1..page.len() {
            let (first, second) = page.split_at(cut);
            let from = first.len().saturating_sub(HEAD_END_OVERLAP);
            let seen = may_end_head(first) && cards::head_ended(first)
                || may_end_head(&page[from..]) && cards::head_ended(page);
            assert!(seen, "cut at {cut}: {:?}", std::str::from_utf8(second));
        }
    }

    #[tokio::test]
    async fn the_page_and_its_image_share_one_deadline() {
        let server = serve(site).await;
        // The page takes 1.5 s and its image 1.5 s more: either is within 2.5 s, both are
        // not, and the page has a second to spare on a busy machine.
        let rules = Rules {
            timeout: Duration::from_millis(2500),
            ..local(server.port)
        };
        let url = format!("http://127.0.0.1:{}/slow", server.port);
        let got = fetch_with(&rules, &url, "en").await.unwrap().unwrap();
        assert_eq!(got.card.title, "Slow");
        assert_eq!(got.image, None);
    }

    #[test]
    fn every_hop_shares_one_tls_configuration() {
        let (a, b) = (tls().unwrap(), tls().unwrap());
        assert!(Arc::ptr_eq(a.crypto_provider(), b.crypto_provider()));
        assert_eq!(a.alpn_protocols, [b"h2".to_vec(), b"http/1.1".to_vec()]);
    }
}
