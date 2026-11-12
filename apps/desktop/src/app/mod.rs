//! The shell's state and what happens to it: the session, the route, where the shell is,
//! the forms, and the reactions to what the session reports. The views in `ui` only read
//! this and call back in.
//!
//! This file is the frame: the route, the place, the conversation on screen. Each part of
//! the product keeps its own state and reactions beside it: [`sign_in`], [`home`]
//! (personal conversations), [`server`] (making and managing servers) and [`account`]
//! (devices, profile, status).

pub mod account;
pub mod home;
pub mod links;
pub mod menu;
pub mod notices;
pub mod pictures;
pub mod server;
pub mod sign_in;

use std::path::PathBuf;
use std::sync::Arc;

use rune::{TextEdit, Tx, Waker};

use crate::session::{Session, Update};
use crate::settings::{self, Settings};
pub use sign_in::SignIn;

/// What the window shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Route {
    SignIn,
    /// Linking this computer to an identity (identity spec 4.2), from sign-in or from a
    /// signed-in account with no identity on this computer.
    Link,
    Shell,
}

/// Where the shell is: Home, or a server.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Place {
    Home,
    Server(String),
}

/// What Home's main pane shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum HomeView {
    /// The personal conversation on screen, or the empty state when none is.
    #[default]
    Conversations,
    Requests,
    NewMessage,
    Devices,
    Profile,
    AddServer,
}

/// A surface floating over the shell, anchored to the control that opened it. One at a time.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Popover {
    NewChannel,
    Invite,
    /// The menu under a server's name in the sidebar.
    ServerMenu,
    /// The menu the rail's self avatar opens.
    Account,
    /// Someone's profile card, for `State::person`.
    Person,
}

/// Whose profile card is open, and the control it hangs from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Person {
    pub account_id: String,
    pub anchor: String,
}

pub struct State {
    pub session: Session,
    pub settings: Settings,
    pub(crate) data_dir: PathBuf,
    pub route: Route,
    pub sign_in: SignIn,
    pub place: Place,
    pub home_view: HomeView,
    /// Whether the shell has chosen its first place from a snapshot.
    placed: bool,
    pub popover: Option<Popover>,
    /// Whose card `Popover::Person` shows.
    pub person: Option<Person>,
    /// What "Find a member" holds; the side panel lists the members it matches.
    pub member_filter: TextEdit,
    pub draft: TextEdit,
    /// What the draft held when [`State::tick`] last looked: a change since is typing.
    draft_seen: String,
    /// The moment the window has asked to be woken at, to stop showing someone typing.
    typing_wake: Option<i64>,
    pub home: home::Home,
    pub server_forms: server::ServerForms,
    pub account: account::Account,
    /// The system browser and the `lumia:` links it comes back on.
    pub links: links::Links,
    /// Whether the window is in front, and the notices owed the system.
    pub notices: notices::Notices,
    /// The avatars the session holds, decoded for drawing.
    pub pictures: pictures::Pictures,
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
            place: Place::Home,
            home_view: HomeView::default(),
            placed: false,
            popover: None,
            person: None,
            member_filter: TextEdit::new(""),
            draft: TextEdit::new(""),
            draft_seen: String::new(),
            typing_wake: None,
            home: home::Home::default(),
            server_forms: server::ServerForms::default(),
            account: account::Account::default(),
            links: links::Links::default(),
            notices: notices::Notices::default(),
            pictures: pictures::Pictures::default(),
        }
    }

    /// Where this state keeps what it remembers between launches.
    pub fn data_dir(&self) -> &std::path::Path {
        &self.data_dir
    }

    // ---- Where the shell is -----------------------------------------------------------

    /// The server on screen, when the shell is in one.
    pub fn server_id(&self) -> Option<&str> {
        match &self.place {
            Place::Server(id) => Some(id),
            Place::Home => None,
        }
    }

    /// Puts a server on screen with its first channel.
    pub fn select_server(&mut self, server_id: &str) {
        self.place = Place::Server(server_id.to_string());
        self.placed = true;
        self.popover = None;
        self.draft.clear();
        self.member_filter.clear();
        self.session.watch_presence(&[server_id.to_string()]);
        let first = self
            .session
            .channels_of(server_id)
            .next()
            .map(|c| c.channel_id.clone());
        match first {
            Some(channel) => {
                self.session.open_channel(&channel);
                self.remember_channel();
            }
            None => self.session.close_conversation(),
        }
    }

    pub fn select_channel(&mut self, channel_id: &str) {
        self.session.open_channel(channel_id);
        self.draft.clear();
        self.remember_channel();
    }

    /// Keeps the channel on screen as the one a launch returns to.
    fn remember_channel(&mut self) {
        let (Some(server_id), Some(channel_id)) = (self.server_id(), self.session.channel()) else {
            return;
        };
        let last = settings::LastChannel {
            server_id: server_id.to_string(),
            channel_id: channel_id.to_string(),
        };
        if self.settings.last_channel.as_ref() != Some(&last) {
            self.settings.last_channel = Some(last);
            self.settings.save(&self.data_dir);
        }
    }

    /// Goes to Home, showing `view`. No server is on screen there, so none is watched.
    pub fn go_home(&mut self, view: HomeView) {
        let arriving = self.place != Place::Home || self.home_view != view;
        if self.place != Place::Home {
            self.session.close_conversation();
            self.draft.clear();
        }
        self.place = Place::Home;
        self.home_view = view;
        if arriving {
            self.account_opened(view);
        }
        self.placed = true;
        self.popover = None;
        self.session.watch_presence(&[]);
    }

    pub fn open_popover(&mut self, popover: Popover) {
        self.popover = Some(popover);
    }

    pub fn close_popover(&mut self) {
        self.popover = None;
    }

    /// Opens the menu under the server's name.
    pub fn open_server_menu(&mut self) {
        if self.server_id().is_some() {
            self.open_popover(Popover::ServerMenu);
        }
    }

    /// Opens the profile card of `account_id` beside the control keyed `anchor`.
    pub fn open_person(&mut self, account_id: &str, anchor: &str) {
        self.person = Some(Person {
            account_id: account_id.to_string(),
            anchor: anchor.to_string(),
        });
        self.open_popover(Popover::Person);
    }

    /// The card's "Message": New message at Home, already asking for the conversation, so
    /// it opens there or the page says a request went out.
    pub fn message_person(&mut self) {
        let Some(person) = self.person.take() else {
            return;
        };
        self.go_home(HomeView::NewMessage);
        self.start_dm(&person.account_id);
    }

    /// Sends the draft to the open conversation.
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

    /// What the passing of time does, apart from anything the session reports: says this
    /// person is typing when the draft has changed since the last look (the composer has
    /// no hook of its own for an edit), and stops showing those whose typing has run out
    /// by `now_micros`. True when that changed what is on screen.
    pub fn tick(&mut self, now_micros: i64) -> bool {
        if self.draft.text != self.draft_seen {
            self.draft_seen.clone_from(&self.draft.text);
            // Emptied is not typing: a send, or another channel.
            if !self.draft.text.trim().is_empty() {
                self.session.typed();
            }
        }
        let expired = self.session.expire_typing(now_micros);
        let pictured = self.pictures.sync(&self.session.avatars);
        expired || pictured
    }

    /// Enters the shell for a session the stores already hold, at launch: with what the
    /// core kept of the account on screen at once when it kept anything, and a fresh
    /// snapshot asked for either way.
    pub fn resume(&mut self) {
        self.route = Route::Shell;
        if self.session.snapshot.is_some() {
            self.settle();
        }
        self.session.load();
    }

    /// Reads a page further back in the open conversation; true when that changed what is
    /// on screen (a page came from the local store, or one is now on its way).
    pub fn load_older(&mut self) -> bool {
        let before = (self.session.entries.len(), self.session.loading_older());
        self.session.load_older();
        before != (self.session.entries.len(), self.session.loading_older())
    }

    /// Forgets where the shell was, on the way out of it.
    pub(crate) fn leave_shell(&mut self) {
        self.place = Place::Home;
        self.home_view = HomeView::default();
        self.placed = false;
        self.popover = None;
        self.draft.clear();
        self.home = home::Home::default();
        self.server_forms = server::ServerForms::default();
        self.account = account::Account::default();
    }

    /// Keeps the place and the channel on screen ones the snapshot still has. The first
    /// snapshot chooses the place as the other shells do: the first server when there is
    /// one and no personal conversation, Home otherwise.
    fn settle(&mut self) {
        let servers = self.session.servers();
        if !self.placed {
            self.placed = true;
            // Back to the channel last on screen, when the account still has it.
            if let Some(last) = self.settings.last_channel.clone()
                && self
                    .session
                    .channels_of(&last.server_id)
                    .any(|c| c.channel_id == last.channel_id)
            {
                self.select_server(&last.server_id);
                self.select_channel(&last.channel_id);
                return;
            }
            let personal = self.session.personal_channels().next().is_some();
            if let (Some(first), false) = (servers.first(), personal) {
                let first = first.server_id.clone();
                self.select_server(&first);
            }
            return;
        }
        let Place::Server(server) = self.place.clone() else {
            return;
        };
        if !servers.iter().any(|s| s.server_id == server) {
            // The server on screen is gone (left, or removed from).
            match servers.first().map(|s| s.server_id.clone()) {
                Some(first) => self.select_server(&first),
                None => self.go_home(HomeView::default()),
            }
            return;
        }
        let still_there = self.session.channel().is_some_and(|id| {
            self.session
                .channels_of(&server)
                .any(|c| c.channel_id == id)
        });
        if !still_there {
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
            Update::SignedIn(answer) => self.signed_in(answer),
            Update::Snapshot(Ok(_)) => {
                self.settle();
                // A server or a channel just made may be listed now.
                self.open_made();
            }
            Update::Snapshot(Err(_)) => {
                // The instance no longer knows this session, or has revoked it.
                if !self.session.signed_in && self.route == Route::Shell {
                    self.leave_shell();
                    self.route = Route::SignIn;
                    self.sign_in.error = SIGNED_OUT.into();
                }
            }
            Update::Dm(update) => self.react_dm(update),
            Update::Server(update) => self.react_server(update),
            Update::Identity(update) => self.react_identity(update),
            Update::Browser(update) => self.react_browser(update),
            Update::Notice(update) => self.react_notice(update),
            Update::Avatar(update) => self.react_avatar(update),
            Update::Gateway(event) => self.react_gateway(event),
            Update::Members { .. }
            | Update::Older { .. }
            | Update::Profile { .. }
            | Update::SignedOut => {}
        }
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

/// What the sign-in card says to someone the instance has sent back to it.
pub const SIGNED_OUT: &str = "You were signed out of this computer. Sign in again to carry on.";

/// `AppSpec::init`: reopens the saved instance, straight into the shell when signed in.
pub fn init() -> State {
    let mut state = State::new(settings::data_dir());
    // A debug build signs in from the environment, for the smoke run (scripts/smoke.sh):
    // LUMIA_DEV_INSTANCE, _HANDLE, _PASSWORD, as the iOS app's debug build does.
    if cfg!(debug_assertions)
        && let (Ok(instance), Ok(handle), Ok(password)) = (
            std::env::var("LUMIA_DEV_INSTANCE"),
            std::env::var("LUMIA_DEV_HANDLE"),
            std::env::var("LUMIA_DEV_PASSWORD"),
        )
    {
        state.sign_in.instance = TextEdit::new(&instance);
        state.connect();
        if !state.session.signed_in {
            state.sign_in.handle = TextEdit::new(&handle);
            state.sign_in.password.replace_text(&password);
            state.submit();
        }
        return state;
    }
    if state.settings.instance.is_some() {
        state.connect();
    }
    state
}

/// `AppSpec::setup`: the session's runtime threads wake the window through `waker`.
pub fn setup(state: &mut State, waker: Waker) {
    state.session.set_wake(Arc::clone(&waker));
    state.pictures.start(Arc::clone(&waker));
    // Anything that finished between `init` and here is already queued.
    waker();
}

/// `AppSpec::poll`: before every frame, on the window's thread.
pub fn poll(state: &mut State, _now: f64, tx: &mut Tx) -> bool {
    let drained = state.drain();
    let now = chrono::Utc::now().timestamp_micros();
    let ticked = state.tick(now);
    // Nothing arrives when someone stops typing, so the window asks to be woken when the
    // first of them runs out, once for each such moment.
    let ends = state.session.typing_ends();
    if let Some(ends) = ends.filter(|ends| state.typing_wake != Some(*ends)) {
        let left = u64::try_from(ends - now).unwrap_or(0);
        tx.wake_after(std::time::Duration::from_micros(left));
    }
    state.typing_wake = ends;
    drained || ticked
}
