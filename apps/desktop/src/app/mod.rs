//! The shell's state and what happens to it: the session, the route, where the shell is,
//! the forms, and the reactions to what the session reports. The views in `ui` only read
//! this and call back in.
//!
//! This file is the frame: the route, the place, the conversation on screen. Each part of
//! the product keeps its own state and reactions beside it: [`sign_in`], [`home`]
//! (personal conversations), [`server`] (making and managing servers) and [`account`]
//! (devices, profile, status).

pub mod account;
pub mod aside;
pub mod autoscroll;
pub mod channels;
pub mod home;
pub mod links;
pub mod menu;
pub mod notices;
pub mod pictures;
pub mod rail;
pub mod replies;
pub mod server;
pub mod server_settings;
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
    /// How Lumia looks on this computer (`settings`: appearance, how messages are drawn,
    /// scroll bars).
    Preferences,
    AddServer,
    /// The messages in every server that notify this account (architecture 6.6).
    Mentions,
}

/// A surface floating over the shell, anchored to the control that opened it. One at a time.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Popover {
    NewChannel,
    /// "New channel" from the Channels heading's menu, hung from its plus.
    NewChannelHere,
    Invite,
    /// The Channels heading's plus: create a channel, browse them.
    ChannelsMenu,
    /// "Add people" to the channel on screen.
    AddPeople,
    /// "Message someone" in the server on screen.
    NewDm,
    /// The menu under a server's name in the sidebar.
    ServerMenu,
    /// The menu the rail's self avatar opens.
    Account,
    /// Someone's profile card, for `State::person`.
    Person,
    /// The emoji picker, for the message `replies.picker` names.
    EmojiPicker,
    /// A message's "More actions" menu, for the message `replies.menu_for` names.
    MessageMenu,
    /// "Delete this message?", for the message `replies.deleting` names.
    DeleteMessage,
    /// A rail tile's menu, for the server or folder `rail.menu_for` names.
    RailMenu,
    /// "Move to folder", from a server's menu on the rail.
    MoveToFolder,
    /// A folder's name and tone, for the folder `rail.editing` names.
    FolderSettings,
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
    /// Being in channels, and server DMs (`channels`).
    pub channel_forms: channels::ChannelForms,
    pub account: account::Account,
    /// The system browser and the `lumia:` links it comes back on.
    pub links: links::Links,
    /// Whether the window is in front, and the notices owed the system.
    pub notices: notices::Notices,
    /// The avatars the session holds, decoded for drawing.
    pub pictures: pictures::Pictures,
    /// The pane the middle button scrolls while it is held (`autoscroll`).
    pub autoscroll: autoscroll::Autoscroll,
    /// The window's width in points, as the shell's layout last reported it (`aside`).
    pub(crate) window_width: Option<f32>,
    /// The aside over a narrow window's conversation, asked for by its toggle (`aside`).
    pub aside_open: bool,
    /// Quote replies, reactions and mentions (`replies`).
    pub replies: replies::Replies,
    /// The rail's drag, its menus and Folder settings (`rail`).
    pub rail: rail::RailUi,
    /// Whether the system shows times on a 24-hour clock, which the timeline's times
    /// follow. A real launch reads it from the system whenever the window comes in front
    /// (`platform`); a test and a headless picture keep the 12-hour clock.
    pub clock_24: bool,
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
            channel_forms: channels::ChannelForms::default(),
            account: account::Account::default(),
            links: links::Links::default(),
            notices: notices::Notices::default(),
            pictures: pictures::Pictures::default(),
            autoscroll: autoscroll::Autoscroll::default(),
            window_width: None,
            aside_open: false,
            replies: replies::Replies::default(),
            rail: rail::RailUi::default(),
            clock_24: false,
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
        self.close_server_settings();
        self.close_page();
        self.session.previewing = None;
        self.aside_open = false;
        self.place = Place::Server(server_id.to_string());
        self.placed = true;
        self.popover = None;
        self.draft.clear();
        self.replies = replies::Replies::default();
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
        self.close_server_settings();
        self.close_page();
        if self.session.joined(channel_id) {
            self.session.previewing = None;
        }
        if self.session.channel() != Some(channel_id) {
            self.replies = replies::Replies::default();
        }
        self.session.open_channel(channel_id);
        self.session.channel_members(channel_id);
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
        self.aside_open = false;
        let arriving = self.place != Place::Home || self.home_view != view;
        if self.place != Place::Home {
            self.session.close_conversation();
            self.draft.clear();
            self.replies = replies::Replies::default();
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
        self.channel_forms.dm_error.clear();
        self.person = Some(Person {
            account_id: account_id.to_string(),
            anchor: anchor.to_string(),
        });
        self.open_popover(Popover::Person);
    }

    /// The card's "Message privately": New message at Home, already asking for the
    /// conversation, so it opens there or the page says a request went out.
    pub fn message_person(&mut self) {
        let Some(person) = self.person.take() else {
            return;
        };
        self.go_home(HomeView::NewMessage);
        self.start_dm(&person.account_id);
    }

    // ---- Preferences ------------------------------------------------------------------
    //
    // Each takes effect at once and is kept for the next launch. They are this computer's.

    pub fn set_appearance(&mut self, appearance: settings::Appearance) {
        if self.settings.appearance != appearance {
            self.settings.appearance = appearance;
            self.settings.save(&self.data_dir);
        }
    }

    /// Writes a preset's values over the message settings (the font size stays).
    pub fn apply_preset(&mut self, preset: settings::Preset) {
        self.change_display(|settings| settings.apply(preset));
    }

    pub fn set_layout(&mut self, layout: settings::Layout) {
        self.change_display(|settings| settings.layout = layout);
    }

    pub fn set_spacing(&mut self, spacing: settings::Spacing) {
        self.change_display(|settings| settings.spacing = spacing);
    }

    pub fn set_avatars(&mut self, avatars: settings::Avatars) {
        self.change_display(|settings| settings.avatars = avatars);
    }

    pub fn set_font_size(&mut self, font_size: settings::FontSize) {
        self.change_display(|settings| settings.font_size = font_size);
    }

    /// Changes how messages are drawn and keeps it, when `change` changes anything.
    fn change_display(&mut self, change: impl FnOnce(&mut Settings)) {
        let before = (self.settings.display(), self.settings.font_size);
        change(&mut self.settings);
        if (self.settings.display(), self.settings.font_size) != before {
            self.settings.save(&self.data_dir);
            // The list keeps the rows it has built: these are drawn otherwise now.
            self.session.revision += 1;
        }
    }

    /// Follows the system's clock, 12 or 24 hours.
    pub fn set_clock_24(&mut self, clock_24: bool) {
        if self.clock_24 != clock_24 {
            self.clock_24 = clock_24;
            self.session.revision += 1;
        }
    }

    pub fn set_scroll_bars(&mut self, scroll_bars: settings::ScrollBars) {
        if self.settings.scroll_bars != scroll_bars {
            self.settings.scroll_bars = scroll_bars;
            self.settings.save(&self.data_dir);
        }
    }

    /// Sends the draft to the open conversation.
    pub fn send(&mut self, tx: &mut Tx) {
        if self.draft.is_composing() || self.draft.text.trim().is_empty() {
            return;
        }
        let reply = self.replies.replying.clone();
        if let Some(outside) = self.session.send_with(
            &self.draft.text,
            chrono::Utc::now().timestamp_micros(),
            reply.as_ref(),
        ) {
            self.draft.clear();
            self.draft_seen.clear();
            self.replies.replying = None;
            self.replies.mention = None;
            self.sent_outside(outside);
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
        let replied = self.tick_replies();
        expired || pictured || replied
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
        self.replies = replies::Replies::default();
        self.home = home::Home::default();
        self.server_forms = server::ServerForms::default();
        self.channel_forms = channels::ChannelForms::default();
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
                    .channel_summary(&last.channel_id)
                    .is_some_and(|c| c.server_id == last.server_id)
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
        // A channel left or taken away goes; one being read before joining stays.
        let still_there = self
            .session
            .channel()
            .is_some_and(|id| self.session.channel_summary(id).is_some());
        let elsewhere = self.server_settings_open() || self.server_page().is_some();
        if !still_there && !elsewhere {
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
            Update::Channel(update) => self.react_channel(update),
            Update::Identity(update) => self.react_identity(update),
            Update::Browser(update) => self.react_browser(update),
            Update::Notice(update) => self.react_notice(update),
            Update::Avatar(update) => self.react_avatar(update),
            Update::Gateway(event) => self.react_gateway(event),
            Update::Read(_)
            | Update::Rail(_)
            | Update::Members { .. }
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
pub fn poll(state: &mut State, now: f64, tx: &mut Tx) -> bool {
    // The middle button held over a pane: it moves a step each frame until the button is up.
    if let Some((pane, y)) = state.autoscroll_step(now) {
        tx.scroll_to_y(pane, y);
    }
    if state.autoscroll.active() {
        tx.wake_after(std::time::Duration::from_millis(16));
    }
    let drained = state.drain();
    let now = chrono::Utc::now().timestamp_micros();
    let ticked = state.tick(now);
    state.land(tx);
    let found = state.show_found(tx);
    if state.replies.flash.is_some()
        || state
            .replies
            .outside
            .as_ref()
            .is_some_and(|o| o.said.is_some())
    {
        tx.wake_after(std::time::Duration::from_millis(250));
    }
    // Nothing arrives when someone stops typing, so the window asks to be woken when the
    // first of them runs out, once for each such moment.
    let ends = state.session.typing_ends();
    if let Some(ends) = ends.filter(|ends| state.typing_wake != Some(*ends)) {
        let left = u64::try_from(ends - now).unwrap_or(0);
        tx.wake_after(std::time::Duration::from_micros(left));
    }
    state.typing_wake = ends;
    drained || ticked || found
}
