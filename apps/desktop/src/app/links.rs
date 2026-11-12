//! The system browser and the `lumia:` links it comes back on (architecture 6.3).
//!
//! A passkey or the instance's OIDC provider signs in on the instance's own page in the
//! system browser, which returns to the app at `lumia://sign-in` with a one-time token.
//! The iOS app does this in an authentication sheet that hands the return to the code
//! that opened it and to nobody else. A link opened with the app has no such tie: anything
//! on the machine, and any web page, can open `lumia://sign-in?magic=…`. So every sign-in
//! the shell starts carries a `state` it minted, and a link is taken only when it brings
//! that state back, within ten minutes, to the instance it was started for, while nobody
//! is signed in. Without that a page could sign the app in to an account of the page's
//! choosing.
//!
//! The other way, a signed-in app opens the instance's web page as itself with a token it
//! minted ([`State::open_web_signed_in`]); the Devices screen makes a passkey there.

use std::time::{Duration, Instant};

use rune::Tx;

use super::{Route, State};
use crate::session::Failure;
use crate::session::browser::{BrowserUpdate, SIGN_IN_RETURN};

/// How long a sign-in started here may take in the browser. The instance keeps an OIDC
/// flow for ten minutes, and the token it hands back lives for minutes.
const PATIENCE: Duration = Duration::from_secs(600);

/// The longest link looked at. A sign-in link is under two hundred bytes.
const LONGEST: usize = 2048;

/// The longest value of a link's field.
const LONGEST_FIELD: usize = 512;

/// What a `lumia:` link asks for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Link {
    /// `lumia://sign-in?state=…&magic=…`: a sign-in made in the browser, with its token.
    SignIn { state: String, token: String },
    /// `lumia://sign-in?state=…&error=…`: the instance's provider refused the sign-in.
    Refused { state: String },
    /// `lumia://done`: the web page is finished with a session handed to it.
    Done,
}

/// Reads a link the system opened with the app. `None` for another scheme, a place the
/// app does not have, and anything malformed: the text comes from outside the app.
pub fn parse(url: &str) -> Option<Link> {
    let url = url.trim();
    if url.len() > LONGEST || url.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return None;
    }
    let (scheme, rest) = url.split_once("://")?;
    if !scheme.eq_ignore_ascii_case("lumia") {
        return None;
    }
    let rest = rest.split('#').next().unwrap_or_default();
    let (place, query) = rest.split_once('?').unwrap_or((rest, ""));
    let field = |name: &str| {
        query
            .split('&')
            .filter_map(|pair| pair.split_once('='))
            .find(|(key, _)| *key == name)
            .map(|(_, value)| value)
    };
    match place.trim_end_matches('/') {
        "sign-in" => {
            let state = word(field("state")?)?;
            match (field("magic"), field("error")) {
                (Some(token), _) => Some(Link::SignIn {
                    state,
                    token: word(token)?,
                }),
                (None, Some(_)) => Some(Link::Refused { state }),
                (None, None) => None,
            }
        }
        "done" => Some(Link::Done),
        _ => None,
    }
}

/// A field's value when it is what the instance puts there: a URL-safe token (base64url
/// or hex), which needs no escaping. Anything else is refused rather than decoded.
fn word(value: &str) -> Option<String> {
    let safe = |c: char| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | '~');
    (!value.is_empty() && value.len() <= LONGEST_FIELD && value.chars().all(safe))
        .then(|| value.to_string())
}

/// 128 random bits as hex, from the system; `None` when it has none to give.
fn mint() -> Option<String> {
    let mut bytes = [0u8; 16];
    getrandom::fill(&mut bytes).ok()?;
    Some(bytes.iter().map(|byte| format!("{byte:02x}")).collect())
}

/// A sign-in the shell sent the browser off to do.
#[derive(Debug)]
struct Asked {
    state: String,
    /// The instance it was started for.
    base_url: String,
    at: Instant,
}

/// The shell's dealings with the system browser.
#[derive(Debug, Default)]
pub struct Links {
    /// The newest sign-in sent to the browser that has not come back.
    asked: Option<Asked>,
    /// Pages the window owes the system browser, opened at its next poll: what a call
    /// that had to ask the instance first wants opened.
    pub to_open: Vec<String>,
    /// Waiting for the instance's token to open the web page signed in.
    pub handing_off: bool,
    /// Why the web page could not be opened signed in.
    pub handoff_error: String,
}

impl Links {
    /// Whether a sign-in for the instance at `base_url` is out in the browser.
    pub fn waiting(&self, base_url: &str) -> bool {
        self.asked
            .as_ref()
            .is_some_and(|asked| asked.base_url == base_url && asked.at.elapsed() < PATIENCE)
    }
}

impl State {
    /// Sends the system browser to sign in: to the instance's OIDC provider, or to its web
    /// sign-in page for a passkey. The browser comes back on a `lumia:` link
    /// ([`State::open_url`]).
    pub fn sign_in_through_browser(&mut self, oidc: bool, tx: &mut Tx) {
        let Some(page) = self.begin_browser_sign_in(oidc) else {
            return;
        };
        if let Err(error) = tx.open_url(&page) {
            self.links.asked = None;
            self.sign_in.error = format!("Could not open the browser: {error}");
        }
    }

    /// Starts a sign-in through the browser and returns the page to send it to: mints the
    /// `state` the return must bring back and remembers it for the open instance. `None`
    /// with no instance open, or (with the reason on the card) no randomness to mint from.
    pub fn begin_browser_sign_in(&mut self, oidc: bool) -> Option<String> {
        self.sign_in.error.clear();
        let Some(state) = mint() else {
            self.sign_in.error =
                "This computer gave no random number to start a sign-in with.".into();
            return None;
        };
        let back = format!("{SIGN_IN_RETURN}?state={state}");
        let page = self.session.browser_sign_in_url(oidc, &back)?;
        self.links.asked = Some(Asked {
            state,
            base_url: self.session.base_url.clone(),
            at: Instant::now(),
        });
        Some(page)
    }

    /// Opens the instance's web page signed in as this account, landing on `next`
    /// (`devices`, where a passkey is made). The page opens once the instance has minted
    /// the token; [`Links::handing_off`] and [`Links::handoff_error`] say how that goes.
    pub fn open_web_signed_in(&mut self, next: &str) {
        if self.links.handing_off || !self.session.signed_in {
            return;
        }
        self.links.handing_off = true;
        self.links.handoff_error.clear();
        self.session.web_handoff(next);
    }

    /// A link the system opened with the app. True when it was one the app acts on, so the
    /// window should come forward.
    pub fn open_url(&mut self, url: &str) -> bool {
        self.open_url_at(url, Instant::now())
    }

    /// [`State::open_url`] at a given moment, for a test of a sign-in that took too long.
    pub fn open_url_at(&mut self, url: &str, now: Instant) -> bool {
        let (state, token) = match parse(url) {
            Some(Link::SignIn { state, token }) => (state, Some(token)),
            Some(Link::Refused { state }) => (state, None),
            Some(Link::Done) => return true,
            None => return false,
        };
        // A signed-in app does not sign in again, whoever asks.
        if self.route != Route::SignIn || self.session.signed_in || !self.session.is_open() {
            return false;
        }
        let expected = self.links.asked.as_ref().is_some_and(|asked| {
            asked.state == state
                && asked.base_url == self.session.base_url
                && now.saturating_duration_since(asked.at) < PATIENCE
        });
        if !expected {
            self.sign_in.error =
                "That sign-in did not start in this app, or it took too long. Try again.".into();
            return true;
        }
        self.links.asked = None;
        match token {
            Some(token) => {
                self.sign_in.error.clear();
                self.sign_in.busy = true;
                self.session.sign_in_with_token(&token);
            }
            None => self.sign_in.error = "That sign-in was not accepted.".into(),
        }
        true
    }

    pub(super) fn react_browser(&mut self, update: &BrowserUpdate) {
        match update {
            BrowserUpdate::SignedIn(Ok(())) => self.signed_in(&Ok(())),
            BrowserUpdate::SignedIn(Err(failure)) => {
                self.sign_in.busy = false;
                self.sign_in.error = browser_refusal(failure);
            }
            BrowserUpdate::Handoff(Ok(page)) => {
                self.links.handing_off = false;
                self.links.to_open.push(page.clone());
            }
            BrowserUpdate::Handoff(Err(failure)) => {
                self.links.handing_off = false;
                self.links.handoff_error = failure.message.clone();
            }
        }
    }
}

/// The sign-in screen's wording when the token the browser came back with was refused, as
/// the iOS app words it.
fn browser_refusal(failure: &Failure) -> String {
    match failure.code.as_str() {
        "UNAUTHENTICATED" => "That sign-in was not accepted.".into(),
        _ => failure.message.clone(),
    }
}
