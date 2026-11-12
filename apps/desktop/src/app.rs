//! The shell's state and what happens to it: the session, the route, the forms, and the
//! reactions to what the session reports. The views in `ui` only read this and call back in.

use std::path::PathBuf;
use std::sync::Arc;

use rune::{TextEdit, Tx, Waker};
use rune_kit::MarkdownStyle;

use crate::session::{Failure, Session, Update};
use crate::settings::{self, Settings};
use crate::tokens;

/// What the window shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Route {
    SignIn,
    Shell,
}

/// The sign-in screen: the instance, then an account on it.
#[derive(Debug)]
pub struct SignIn {
    pub instance: TextEdit,
    pub handle: TextEdit,
    pub password: TextEdit,
    pub totp: TextEdit,
    /// A new account instead of an existing one.
    pub creating: bool,
    /// The account asked for its second factor.
    pub needs_totp: bool,
    /// Waiting for the instance's trust card.
    pub reaching: bool,
    /// Waiting for the instance's answer to a sign-in.
    pub busy: bool,
    pub error: String,
}

impl SignIn {
    fn new(instance: &str) -> Self {
        Self {
            instance: TextEdit::new(instance),
            handle: TextEdit::new(""),
            password: TextEdit::new("").secure(),
            totp: TextEdit::new(""),
            creating: false,
            needs_totp: false,
            reaching: false,
            busy: false,
            error: String::new(),
        }
    }
}

pub struct State {
    pub session: Session,
    pub settings: Settings,
    data_dir: PathBuf,
    pub route: Route,
    pub sign_in: SignIn,
    /// The server on screen.
    pub server: Option<String>,
    pub draft: TextEdit,
    /// Message-body styles for light and dark, built once.
    pub markdown: [MarkdownStyle; 2],
}

impl State {
    /// A state over `data_dir` with nothing opened, for `init` and for tests.
    ///
    /// # Panics
    /// When the session's runtime cannot start, which leaves nothing to show.
    pub fn new(data_dir: PathBuf) -> Self {
        let session = Session::new(&data_dir)
            .unwrap_or_else(|error| panic!("lumia: starting the client runtime: {error}"));
        let settings = Settings::load(&data_dir);
        Self {
            session,
            sign_in: SignIn::new(settings.instance.as_deref().unwrap_or_default()),
            settings,
            data_dir,
            route: Route::SignIn,
            server: None,
            draft: TextEdit::new(""),
            markdown: [tokens::markdown(false), tokens::markdown(true)],
        }
    }

    // ---- Sign in ------------------------------------------------------------------

    /// Opens the instance in the field and asks for its trust card.
    pub fn connect(&mut self) {
        self.sign_in.error.clear();
        let url = self
            .sign_in
            .instance
            .text
            .trim()
            .trim_end_matches('/')
            .to_string();
        match self.session.open(&url) {
            Ok(()) => {
                self.sign_in.reaching = true;
                self.settings.instance = Some(url);
                self.settings.save(&self.data_dir);
            }
            Err(error) => self.sign_in.error = error,
        }
    }

    /// Back to the instance field.
    pub fn change_instance(&mut self) {
        self.session.close();
        self.sign_in.error.clear();
        self.sign_in.needs_totp = false;
        self.sign_in.busy = false;
    }

    /// Signs in, or creates the account, with what the form holds.
    pub fn submit(&mut self) {
        let form = &mut self.sign_in;
        if form.busy || form.handle.text.trim().is_empty() || form.password.text.is_empty() {
            return;
        }
        form.error.clear();
        form.busy = true;
        if form.creating {
            self.session
                .create_account(&form.handle.text, &form.password.text);
        } else {
            let code = form.needs_totp.then_some(form.totp.text.trim());
            self.session
                .sign_in(&form.handle.text, &form.password.text, code);
        }
    }

    pub fn sign_out(&mut self) {
        self.session.sign_out();
        self.server = None;
        self.draft.clear();
        self.route = Route::SignIn;
    }

    // ---- The shell ------------------------------------------------------------------

    /// Puts a server on screen with the channel it last had, else its first.
    pub fn select_server(&mut self, server_id: &str) {
        self.server = Some(server_id.to_string());
        self.session.watch_presence(&[server_id.to_string()]);
        let first = self
            .session
            .channels_of(server_id)
            .next()
            .map(|c| c.channel_id.clone());
        match first {
            Some(channel) => self.session.open_channel(&channel),
            None => self.session.close_channel(),
        }
    }

    pub fn select_channel(&mut self, channel_id: &str) {
        self.session.open_channel(channel_id);
        self.draft.clear();
    }

    /// Sends the draft to the open channel.
    pub fn send(&mut self, tx: &mut Tx) {
        if self.draft.is_composing() || self.draft.text.trim().is_empty() {
            return;
        }
        if self
            .session
            .send(&self.draft.text, chrono::Utc::now().timestamp_micros())
        {
            self.draft.clear();
            tx.scroll_to_bottom("messages");
        }
    }

    /// Keeps the server and channel on screen ones the snapshot still has.
    fn settle(&mut self) {
        let servers = self.session.servers();
        let server = match &self.server {
            Some(id) if servers.iter().any(|s| s.server_id == *id) => Some(id.clone()),
            _ => servers.first().map(|s| s.server_id.clone()),
        };
        let Some(server) = server else {
            self.server = None;
            self.session.close_channel();
            return;
        };
        let open = self.session.channel.clone();
        let still_there = open.as_deref().is_some_and(|id| {
            self.session
                .channels_of(&server)
                .any(|c| c.channel_id == id)
        });
        if self.server.as_deref() != Some(server.as_str()) || !still_there {
            self.select_server(&server);
        }
    }

    // ---- What the session reports -----------------------------------------------------

    fn react(&mut self, update: &Update) {
        match update {
            Update::Info(Ok(_)) => self.sign_in.reaching = false,
            Update::Info(Err(failure)) => {
                self.sign_in.reaching = false;
                self.sign_in.error = format!("Could not reach that instance: {}", failure.message);
            }
            Update::SignedIn(Ok(())) => {
                self.sign_in.busy = false;
                self.sign_in.password.clear();
                self.sign_in.totp.clear();
                self.sign_in.needs_totp = false;
                self.route = Route::Shell;
            }
            Update::SignedIn(Err(failure)) => {
                self.sign_in.busy = false;
                self.refused(failure);
            }
            Update::Snapshot(Ok(_)) => self.settle(),
            Update::Snapshot(Err(_)) => {
                // The instance no longer knows this session.
                if !self.session.signed_in && self.route == Route::Shell {
                    self.route = Route::SignIn;
                }
            }
            Update::Members { .. } | Update::Gateway(_) | Update::SignedOut => {}
        }
    }

    /// The sign-in screen's wording for a refusal, as the other shells word it.
    fn refused(&mut self, failure: &Failure) {
        let form = &mut self.sign_in;
        let mfa = failure.details.get("mfa").map(String::as_str);
        form.error = match failure.code.as_str() {
            // The passkey factor needs the browser's ceremony (architecture 6.3), which
            // this shell does not hand off to yet.
            "FAILED_PRECONDITION" if mfa == Some("passkey") => {
                "This account asks for its passkey at sign-in, which this app cannot do yet. Sign in from the web for now.".into()
            }
            "FAILED_PRECONDITION" if !form.needs_totp && !form.creating => {
                form.needs_totp = true;
                String::new()
            }
            "UNAUTHENTICATED" if form.needs_totp => "That code was not accepted.".into(),
            "UNAUTHENTICATED" => "Wrong handle or password.".into(),
            "ALREADY_EXISTS" => "That handle is taken.".into(),
            _ => failure.message.clone(),
        };
    }

    /// Applies what the session finished and reacts to it; true when anything changed.
    pub fn drain(&mut self) -> bool {
        let updates = self.session.poll();
        for update in &updates {
            self.react(update);
        }
        !updates.is_empty()
    }

    /// Applies one update as if the instance had sent it. For tests and scenes.
    pub fn receive(&mut self, update: &Update) {
        self.session.apply(update);
        self.react(update);
    }
}

/// `AppSpec::init`: reopens the saved instance, straight into the shell when signed in.
pub fn init() -> State {
    let mut state = State::new(settings::data_dir());
    // A debug build signs in from the environment, for the smoke run (scripts/smoke.sh):
    // LUMIA_DEV_INSTANCE, _HANDLE, _PASSWORD, as the SwiftUI app's debug build does.
    if cfg!(debug_assertions)
        && let (Ok(instance), Ok(handle), Ok(password)) = (
            std::env::var("LUMIA_DEV_INSTANCE"),
            std::env::var("LUMIA_DEV_HANDLE"),
            std::env::var("LUMIA_DEV_PASSWORD"),
        )
    {
        state.sign_in.instance = TextEdit::new(&instance);
        state.connect();
        if state.session.signed_in {
            state.route = Route::Shell;
            state.session.load();
        } else {
            state.sign_in.handle = TextEdit::new(&handle);
            state.sign_in.password.replace_text(&password);
            state.submit();
        }
        return state;
    }
    if state.settings.instance.is_some() {
        state.connect();
        if state.session.signed_in {
            state.route = Route::Shell;
            state.session.load();
        }
    }
    state
}

/// The `live` scene, for a headless picture of the real thing (`--snap out.png --scene live`
/// with the `LUMIA_DEV_*` environment): waits, on the wall clock, until the instance has
/// answered and the open channel has its messages, or ten seconds. A headless run has a
/// virtual clock and one frame, so nothing else would give the network time to answer.
pub fn scene_live(state: &mut State) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    let mut settled = None;
    while std::time::Instant::now() < deadline {
        if state.drain() {
            settled = None;
        }
        let loaded = state.route == Route::Shell
            && state.session.connection == crate::session::Connection::Connected
            && state.session.snapshot.as_ref().is_some_and(|snap| {
                snap.servers
                    .iter()
                    .all(|x| state.session.members.contains_key(&x.server_id))
            });
        // Loaded, and quiet for a moment: the channel's replay has landed.
        match (loaded, settled) {
            (true, Some(since))
                if std::time::Instant::now() - since > std::time::Duration::from_millis(600) =>
            {
                return;
            }
            (true, None) => settled = Some(std::time::Instant::now()),
            (false, _) => settled = None,
            _ => {}
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    eprintln!(
        "lumia: the live scene gave up waiting; connection {:?}, error {:?}",
        state.session.connection, state.session.last_error
    );
}

/// `AppSpec::setup`: the session's runtime threads wake the window through `waker`.
pub fn setup(state: &mut State, waker: Waker) {
    state.session.set_wake(Arc::clone(&waker));
    // Anything that finished between `init` and here is already queued.
    waker();
}

/// `AppSpec::poll`: before every frame, on the window's thread.
pub fn poll(state: &mut State, _now: f64, _tx: &mut Tx) -> bool {
    state.drain()
}
