//! The session: one instance's client and the state the screens read, kept fresh by the
//! gateway's events. The desktop counterpart of LumiaKit's `Session.swift` and the web's
//! `client/state.ts`.
//!
//! Nothing here names the UI toolkit (architecture 13, "Fallback"). Calls that reach the
//! network run on the session's own runtime and come back as [`Update`]s in a queue; the
//! shell drains the queue on its own thread with [`Session::poll`], woken through [`Wake`].
//! Reads of the local store are synchronous: they are SQLite on this machine.
//!
//! This file is the account, its servers and the conversation on screen. What belongs to
//! one part of the product lives beside it and reports through its own update type:
//! [`dms`] (personal conversations), [`servers`] (making and joining servers, channels,
//! invites) and [`identity`] (devices, linking, the profile and status).

pub mod dms;
pub mod identity;
pub mod servers;

use std::collections::{HashMap, HashSet};
use std::future::Future;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex, PoisonError};

use lumia_client::ClientError;
use lumia_client::app::{
    self, App, AppEvent, ChannelSummary, InstanceSummary, MemberSummary, MessageSummary, Opener,
    Place, ServerSummary, SnapshotSummary, StoreName,
};
use lumia_client::sqlite::SqliteStore;

/// How many messages a page of a conversation is: what is read from the local store when it
/// opens, and each step further back, from the store and then from the instance.
pub const MESSAGE_PAGE: u32 = 200;

const PERSONAL_KINDS: [&str; 2] = ["PERSONAL_DM", "PERSONAL_GROUP_DM"];

/// Asks the shell to call [`Session::poll`] soon. Called from runtime threads.
pub type Wake = Arc<dyn Fn() + Send + Sync>;

/// Why a call failed, as a screen can act on it.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Failure {
    /// `lumia.v1.ErrorCode` as its name when the instance answered; empty otherwise.
    pub code: String,
    pub message: String,
    /// `Error.details`, keyed as docs/protocol.md documents per code (`mfa`, ...).
    pub details: HashMap<String, String>,
}

impl From<ClientError> for Failure {
    fn from(error: ClientError) -> Self {
        match error {
            ClientError::Api {
                code,
                message,
                details,
                ..
            } => Self {
                code,
                message,
                details,
            },
            ClientError::NotSignedIn => Self {
                code: "UNAUTHENTICATED".into(),
                message: "not signed in".into(),
                details: HashMap::new(),
            },
            other => Self {
                message: other.to_string(),
                ..Self::default()
            },
        }
    }
}

/// What a finished call or the gateway reports. [`Session::poll`] applies each to the session
/// and hands it on, so a screen can react to the ones it started.
#[derive(Debug, Clone)]
pub enum Update {
    /// The instance's trust card, asked for by [`Session::open`].
    Info(Result<InstanceSummary, Failure>),
    /// A sign-in or a new account.
    SignedIn(Result<(), Failure>),
    Snapshot(Result<SnapshotSummary, Failure>),
    Members {
        server_id: String,
        members: Vec<MemberSummary>,
    },
    /// A public profile looked up for someone the servers do not list; `None` when the
    /// instance would not show it.
    Profile {
        account_id: String,
        profile: Option<MemberSummary>,
    },
    /// A page of a channel's history from the instance, older than the local store holds,
    /// and whether still older messages remain.
    Older {
        channel_id: String,
        page: Result<(Vec<Entry>, bool), Failure>,
    },
    Gateway(AppEvent),
    SignedOut,
    Dm(dms::DmUpdate),
    Server(servers::ServerUpdate),
    Identity(identity::IdentityUpdate),
}

/// What is on screen to read and write: a server's channel, or a personal conversation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Conversation {
    Channel(String),
    Dm(String),
}

impl Conversation {
    /// The channel id, whichever kind it is.
    pub fn id(&self) -> &str {
        match self {
            Self::Channel(id) | Self::Dm(id) => id,
        }
    }
}

/// A message of the conversation on screen, as the timeline draws it, whichever kind of
/// conversation it came from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub id: String,
    /// Its place in the conversation's log; what a page further back is asked from.
    pub seq: u64,
    pub author_account_id: String,
    pub body: String,
    pub at_micros: i64,
    pub deleted: bool,
}

impl From<MessageSummary> for Entry {
    fn from(m: MessageSummary) -> Self {
        Self {
            id: m.message_id,
            seq: m.seq,
            author_account_id: m.author_account_id,
            body: m.body,
            at_micros: m.created_at_micros,
            deleted: m.deleted,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Connection {
    Idle,
    Connecting,
    Connected,
    Reconnecting { attempt: u32 },
    Disconnected { reason: String },
    Closed { reason: String },
}

impl Connection {
    pub fn label(&self) -> String {
        match self {
            Self::Idle => "not connected".into(),
            Self::Connecting => "connecting".into(),
            Self::Connected => "connected".into(),
            Self::Reconnecting { attempt } => format!("reconnecting (attempt {attempt})"),
            Self::Disconnected { reason } => format!("disconnected: {reason}"),
            Self::Closed { reason } => format!("closed: {reason}"),
        }
    }
}

/// How far back the open conversation has been read.
#[derive(Debug, Default)]
struct Reach {
    /// How many of the local store's newest messages are on screen.
    window: u32,
    /// The local store has nothing above what is on screen.
    local_done: bool,
    /// The instance said nothing older remains, or would not say.
    remote_done: bool,
    /// A page from the instance is on its way.
    loading: bool,
    /// The pages the instance gave, oldest first: what lies above the local store.
    history: Vec<Entry>,
}

/// A message this device sent that the store does not hold yet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pending {
    /// The client-minted id: the command's, and the message's once it is applied.
    pub id: String,
    pub channel_id: String,
    pub body: String,
    pub sent_at_micros: i64,
    /// Why the instance refused it, when it did.
    pub failed: Option<String>,
}

/// Where finished work is posted from runtime threads.
#[derive(Clone)]
struct Mailbox {
    sender: Sender<Update>,
    wake: Arc<Mutex<Option<Wake>>>,
}

impl Mailbox {
    fn post(&self, update: Update) {
        if self.sender.send(update).is_ok() {
            let wake = self
                .wake
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .clone();
            if let Some(wake) = wake {
                wake();
            }
        }
    }
}

pub struct Session {
    runtime: tokio::runtime::Runtime,
    mailbox: Mailbox,
    updates: Receiver<Update>,
    data_dir: PathBuf,
    app: Option<Arc<App>>,
    /// Channels the gateway loop has been asked for.
    subscribed: HashSet<String>,
    /// Snapshot requests that failed in a row, which spaces the next one out.
    load_failures: u32,
    /// Whether the members shown came from the instance since this launch, not from what
    /// was kept.
    members_fresh: bool,
    reach: Reach,
    watching: Vec<String>,

    /// The instance's base URL, as opened.
    pub base_url: String,
    /// The instance's name on disk (`host` or `host_port`); [`Session::address`] is the one to show.
    pub host: String,
    pub info: Option<InstanceSummary>,
    pub signed_in: bool,
    pub snapshot: Option<SnapshotSummary>,
    pub connection: Connection,
    /// Everyone this account shares a server with, by account id.
    pub people: HashMap<String, MemberSummary>,
    /// A server's members, by server id.
    pub members: HashMap<String, Vec<MemberSummary>>,
    /// Who is present in the servers on screen (protocol 6.7), by server then account:
    /// the status (`ONLINE`, `AWAY`, `DO_NOT_DISTURB`) and the custom status text.
    pub presence: HashMap<String, HashMap<String, (String, String)>>,
    /// Public profiles of people the servers do not list, by account id; `None` once the
    /// instance refused one, so it is not asked again.
    pub profiles: HashMap<String, Option<MemberSummary>>,
    /// What is on screen to read and write.
    pub conversation: Option<Conversation>,
    /// Its newest messages from the local store, oldest first.
    pub entries: Vec<Entry>,
    pub pending: Vec<Pending>,
    /// Bumped whenever `entries` or `pending` change, for a list that caches rows.
    pub revision: u64,
    /// Personal conversations.
    pub dm: dms::DmState,
    /// This device's identity, its roster and profile.
    pub identity: identity::IdentityState,
    pub last_error: Option<String>,
}

impl std::fmt::Debug for Session {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Session")
            .field("host", &self.host)
            .field("signed_in", &self.signed_in)
            .finish_non_exhaustive()
    }
}

impl Session {
    /// A session with no instance open, keeping its stores under `data_dir`.
    ///
    /// # Errors
    /// When the runtime cannot start.
    pub fn new(data_dir: &Path) -> std::io::Result<Self> {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .thread_name("lumia-session")
            .build()?;
        let (sender, updates) = mpsc::channel();
        Ok(Self {
            runtime,
            mailbox: Mailbox {
                sender,
                wake: Arc::default(),
            },
            updates,
            data_dir: data_dir.to_path_buf(),
            app: None,
            subscribed: HashSet::new(),
            load_failures: 0,
            members_fresh: false,
            reach: Reach::default(),
            watching: Vec::new(),
            base_url: String::new(),
            host: String::new(),
            info: None,
            signed_in: false,
            snapshot: None,
            connection: Connection::Idle,
            people: HashMap::new(),
            members: HashMap::new(),
            presence: HashMap::new(),
            profiles: HashMap::new(),
            conversation: None,
            entries: Vec::new(),
            pending: Vec::new(),
            revision: 0,
            dm: dms::DmState::default(),
            identity: identity::IdentityState::default(),
            last_error: None,
        })
    }

    /// Sets how runtime threads ask for a [`Session::poll`].
    pub fn set_wake(&self, wake: Wake) {
        *self
            .mailbox
            .wake
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = Some(wake);
    }

    /// Runs `work` on the session's runtime and queues what it returns.
    pub(crate) fn task(&self, work: impl Future<Output = Update> + Send + 'static) {
        let mailbox = self.mailbox.clone();
        self.runtime.spawn(async move { mailbox.post(work.await) });
    }

    /// [`Session::task`] for a call whose future is not `Send`: the core's identity and
    /// personal-conversation calls hold a store connection across their awaits. `make`
    /// builds the future on a blocking thread of the runtime, which then drives it there.
    pub(crate) fn task_local<F: Future<Output = Update>>(
        &self,
        make: impl FnOnce() -> F + Send + 'static,
    ) {
        let mailbox = self.mailbox.clone();
        let runtime = self.runtime.handle().clone();
        self.runtime
            .spawn_blocking(move || mailbox.post(runtime.block_on(make())));
    }

    /// The open instance's client, for a call a submodule makes.
    pub(crate) fn client(&self) -> Option<Arc<App>> {
        self.app.clone()
    }

    // ---- Opening and signing in ---------------------------------------------------

    /// Opens the client for the instance at `url` (its stores under `<data>/<host>/`, the
    /// device's identity under `<data>/identity/`, the layout `lumia-ffi` uses) and asks for
    /// its trust card, which arrives as [`Update::Info`].
    ///
    /// The stores are SQLite files in the clear: the at-rest key from the platform keystore
    /// (architecture 12) is not built for this shell yet.
    ///
    /// # Errors
    /// When the address is not an `http` or `https` URL, or a store cannot be opened.
    pub fn open(&mut self, url: &str) -> Result<(), String> {
        let url = url.trim().trim_end_matches('/');
        if !(url.starts_with("http://") || url.starts_with("https://"))
            || url.split("://").nth(1).is_none_or(str::is_empty)
        {
            return Err("Enter the instance address, starting with http:// or https://".into());
        }
        self.close();
        let dir = self.data_dir.join(app::host_of(url));
        let identity_dir = self.data_dir.join("identity");
        for d in [&dir, &identity_dir] {
            std::fs::create_dir_all(d).map_err(|e| format!("{}: {e}", d.display()))?;
        }
        let opener: Opener = Arc::new(move |name| {
            SqliteStore::open(&match name {
                StoreName::Instance => dir.join("store.sqlite"),
                StoreName::Keys => dir.join("keys.sqlite"),
                StoreName::IdentityStore => identity_dir.join("store.sqlite"),
                StoreName::IdentityKeys => identity_dir.join("keys.sqlite"),
            })
        });
        let app = App::open(url, Place::Native, opener).map_err(|e| e.to_string())?;
        self.base_url = url.to_string();
        self.host = app.host();
        self.signed_in = app.signed_in();
        self.app = Some(Arc::clone(&app));
        if self.signed_in {
            self.restore();
        }
        self.task(async move { Update::Info(app.instance_info().await.map_err(Failure::from)) });
        Ok(())
    }

    /// Puts back what the core kept of the account at its last snapshot (architecture 13,
    /// "Local-first"): the servers, the channels and the members' names, and starts the
    /// gateway from the store's cursors. The window has its last state before the instance
    /// answers, and without it answering at all. A fresh snapshot is still asked for.
    fn restore(&mut self) {
        let Some(app) = self.app.clone() else { return };
        let Ok(Some(snapshot)) = app.kept_snapshot() else {
            return;
        };
        for server in &snapshot.servers {
            let members = app.kept_members(&server.server_id).unwrap_or_default();
            for member in &members {
                self.people
                    .insert(member.account_id.clone(), member.clone());
            }
            self.members.insert(server.server_id.clone(), members);
        }
        let channels = server_channels(&snapshot);
        self.snapshot = Some(snapshot);
        self.start(&channels);
    }

    /// Whether an instance is open.
    pub fn is_open(&self) -> bool {
        self.app.is_some()
    }

    /// Stops the gateway loop and forgets the open instance; its stores stay on disk.
    pub fn close(&mut self) {
        if let Some(app) = self.app.take() {
            app.stop();
        }
        self.subscribed.clear();
        self.members_fresh = false;
        self.watching.clear();
        self.base_url.clear();
        self.host.clear();
        self.info = None;
        self.signed_in = false;
        self.forget_account();
    }

    fn forget_account(&mut self) {
        self.snapshot = None;
        self.connection = Connection::Idle;
        self.people.clear();
        self.members.clear();
        self.presence.clear();
        self.profiles.clear();
        self.conversation = None;
        self.reach = Reach::default();
        self.entries.clear();
        self.pending.clear();
        self.dm = dms::DmState::default();
        self.identity = identity::IdentityState::default();
        self.revision += 1;
    }

    /// Signs in with a password, and a TOTP code when the account asks for one; the answer
    /// is [`Update::SignedIn`].
    pub fn sign_in(&self, handle: &str, password: &str, totp_code: Option<&str>) {
        let Some(app) = self.app.clone() else { return };
        let (handle, password, code) = (
            handle.trim().to_string(),
            password.to_string(),
            totp_code.map(str::to_string),
        );
        self.task(async move {
            Update::SignedIn(
                app.sign_in(&handle, &password, code.as_deref())
                    .await
                    .map_err(Failure::from),
            )
        });
    }

    /// Creates an account with a password and signs in; the answer is [`Update::SignedIn`].
    pub fn create_account(&self, handle: &str, password: &str) {
        let Some(app) = self.app.clone() else { return };
        let (handle, password) = (handle.trim().to_string(), password.to_string());
        self.task(async move {
            Update::SignedIn(
                app.create_account(&handle, &password, None)
                    .await
                    .map_err(Failure::from),
            )
        });
    }

    /// Revokes the session on the instance and forgets the tokens; [`Update::SignedOut`]
    /// follows once the instance has been told.
    pub fn sign_out(&mut self) {
        let Some(app) = self.app.clone() else { return };
        app.stop();
        self.subscribed.clear();
        self.signed_in = false;
        self.forget_account();
        self.task(async move {
            // The tokens are forgotten locally whatever the instance answers.
            let _ = app.sign_out().await;
            Update::SignedOut
        });
    }

    // ---- Loading ------------------------------------------------------------------

    /// Asks for the account snapshot ([`Update::Snapshot`]); applying it starts the gateway
    /// on every server channel and asks for each server's members.
    pub fn load(&self) {
        self.load_after(std::time::Duration::ZERO);
    }

    fn load_after(&self, wait: std::time::Duration) {
        let Some(app) = self.client() else { return };
        self.task(async move {
            tokio::time::sleep(wait).await;
            Update::Snapshot(app.snapshot().await.map_err(Failure::from))
        });
    }

    fn start(&mut self, channels: &[String]) {
        let Some(app) = self.app.clone() else { return };
        if app.started() {
            for id in channels {
                if self.subscribed.insert(id.clone()) {
                    // A channel that appeared after the loop started.
                    let _ = app.subscribe(id);
                }
            }
            return;
        }
        let mailbox = self.mailbox.clone();
        let sink: app::EventSink = Arc::new(move |event| mailbox.post(Update::Gateway(event)));
        // `App::start` spawns the loop on the runtime it is called from.
        let _inside = self.runtime.enter();
        match app.start(channels, sink) {
            Ok(()) => {
                self.subscribed = channels.iter().cloned().collect();
                self.connection = Connection::Connecting;
            }
            Err(error) => self.last_error = Some(error.to_string()),
        }
    }

    fn load_members(&self, servers: &[ServerSummary]) {
        let Some(app) = &self.app else { return };
        for server in servers {
            let (app, server_id) = (Arc::clone(app), server.server_id.clone());
            self.task(async move {
                let members = app.members(&server_id, "").await.unwrap_or_default();
                Update::Members { server_id, members }
            });
        }
    }

    /// Reads the open conversation's newest messages from the local store, drops the
    /// pending sends it now holds, and looks up authors nobody has named yet.
    pub(crate) fn read_entries(&mut self) {
        let Some(conversation) = self.conversation.clone() else {
            return;
        };
        let read = match &conversation {
            Conversation::Channel(id) => match &self.app {
                // What the instance gave of older history sits above the local window.
                Some(app) => app
                    .messages(id, self.reach.window.max(MESSAGE_PAGE))
                    .map(|rows| {
                        let local = rows.into_iter().map(Entry::from);
                        self.reach.history.iter().cloned().chain(local).collect()
                    })
                    .map_err(|e| e.to_string()),
                None => return,
            },
            Conversation::Dm(id) => self.read_dm_entries(id),
        };
        match read {
            Ok(entries) => self.set_entries(entries),
            Err(error) => self.last_error = Some(error),
        }
        self.revision += 1;
    }

    /// Replaces what the open conversation shows.
    pub(crate) fn set_entries(&mut self, entries: Vec<Entry>) {
        let here = self.conversation.as_ref().map(Conversation::id);
        self.pending.retain(|p| {
            Some(p.channel_id.as_str()) != here || !entries.iter().any(|e| e.id == p.id)
        });
        let strangers: HashSet<&str> = entries
            .iter()
            .map(|e| e.author_account_id.as_str())
            .filter(|id| {
                !id.is_empty() && !self.people.contains_key(*id) && !self.profiles.contains_key(*id)
            })
            .collect();
        let strangers: Vec<String> = strangers.into_iter().map(str::to_string).collect();
        self.entries = entries;
        for id in strangers {
            self.look_up(&id);
        }
    }

    /// Asks the instance for someone's public profile, once.
    pub(crate) fn look_up(&mut self, account_id: &str) {
        let Some(app) = self.app.clone() else { return };
        if account_id.is_empty()
            || account_id == self.me()
            || self.people.contains_key(account_id)
            || self.profiles.contains_key(account_id)
        {
            return;
        }
        // Held as refused until the answer says otherwise, so it is asked once.
        self.profiles.insert(account_id.to_string(), None);
        let account_id = account_id.to_string();
        self.task(async move {
            let profile = app.account(&account_id).await.ok();
            Update::Profile {
                account_id,
                profile,
            }
        });
    }

    // ---- What the screens do --------------------------------------------------------

    /// Puts a server's channel on screen, from the local store.
    pub fn open_channel(&mut self, channel_id: &str) {
        self.open_conversation(Conversation::Channel(channel_id.to_string()));
    }

    /// Puts a conversation on screen, from the local store.
    pub fn open_conversation(&mut self, conversation: Conversation) {
        if self.conversation.as_ref() == Some(&conversation) {
            return;
        }
        self.conversation = Some(conversation);
        self.entries.clear();
        self.reach = Reach {
            window: MESSAGE_PAGE,
            ..Reach::default()
        };
        self.read_entries();
    }

    /// Reads a page further back in the open channel: from the local store while it has
    /// more, then from the instance (which is not stored; it is history from before this
    /// device's store begins). Does nothing while a page is on its way, at the beginning
    /// of the channel, or in a personal conversation, whose whole log is already read.
    pub fn load_older(&mut self) {
        let Some(Conversation::Channel(channel)) = self.conversation.clone() else {
            return;
        };
        if self.reach.loading {
            return;
        }
        if !self.reach.local_done {
            let before = self.entries.len();
            self.reach.window += MESSAGE_PAGE;
            self.read_entries();
            let local = self.entries.len() - self.reach.history.len();
            self.reach.local_done = local < self.reach.window as usize;
            if self.entries.len() > before {
                return;
            }
        }
        let (Some(app), Some(first), false) =
            (self.client(), self.entries.first(), self.reach.remote_done)
        else {
            return;
        };
        let before_seq = first.seq;
        self.reach.loading = true;
        self.task(async move {
            let page = app
                .older_messages(&channel, before_seq, MESSAGE_PAGE)
                .await
                .map(|(rows, more)| (rows.into_iter().map(Entry::from).collect(), more))
                .map_err(Failure::from);
            Update::Older {
                channel_id: channel,
                page,
            }
        });
    }

    /// Whether a page of older messages is being fetched from the instance.
    pub fn loading_older(&self) -> bool {
        self.reach.loading
    }

    /// Whether the open conversation is known to begin at its first message on screen.
    pub fn at_beginning(&self) -> bool {
        self.reach.local_done && self.reach.remote_done
    }

    /// Takes the conversation off screen.
    pub fn close_conversation(&mut self) {
        if self.conversation.take().is_some() {
            self.entries.clear();
            self.reach = Reach::default();
            self.revision += 1;
        }
    }

    /// The server channel on screen, when that is what is on screen.
    pub fn channel(&self) -> Option<&str> {
        match &self.conversation {
            Some(Conversation::Channel(id)) => Some(id),
            _ => None,
        }
    }

    /// Sends a message to the open conversation; it shows as pending until the store holds
    /// it. False when it could not even be queued (nothing open, or the gateway loop is not
    /// running), so the caller keeps the draft.
    pub fn send(&mut self, body: &str, now_micros: i64) -> bool {
        let body = body.trim();
        if body.is_empty() {
            return false;
        }
        let channel = match self.conversation.clone() {
            Some(Conversation::Channel(id)) => id,
            Some(Conversation::Dm(id)) => return self.send_dm(&id, body, now_micros),
            None => return false,
        };
        let Some(app) = &self.app else {
            return false;
        };
        self.revision += 1;
        match app.send_message(&channel, body) {
            Ok(id) => {
                self.pending.push(Pending {
                    id,
                    channel_id: channel,
                    body: body.to_string(),
                    sent_at_micros: now_micros,
                    failed: None,
                });
                true
            }
            Err(error) => {
                self.last_error = Some(format!("Not sent: {error}"));
                false
            }
        }
    }

    /// Drops a send the instance refused.
    pub fn dismiss(&mut self, pending_id: &str) {
        self.pending.retain(|p| p.id != pending_id);
        self.revision += 1;
    }

    /// Names the servers on screen, whose presence this client wants (protocol 6.7); none
    /// for a screen without one. Sent again on every welcome, since a connection starts with
    /// nothing watched.
    pub fn watch_presence(&mut self, server_ids: &[String]) {
        if self.watching == server_ids {
            return;
        }
        self.watching = server_ids.to_vec();
        self.send_watch();
    }

    fn send_watch(&self) {
        if let (Some(app), Connection::Connected) = (&self.app, &self.connection) {
            let _ = app.watch_presence(&self.watching);
        }
    }

    // ---- Reading ------------------------------------------------------------------

    /// The instance as a person would write it: the base URL without its scheme.
    pub fn address(&self) -> &str {
        self.base_url
            .split_once("://")
            .map_or(self.base_url.as_str(), |(_, rest)| rest)
    }

    pub fn me(&self) -> &str {
        self.snapshot.as_ref().map_or("", |s| s.account_id.as_str())
    }

    pub fn self_name(&self) -> &str {
        let Some(s) = &self.snapshot else {
            return "You";
        };
        [s.display_name.as_str(), s.handle.as_str()]
            .into_iter()
            .find(|n| !n.is_empty())
            .unwrap_or("You")
    }

    /// A person's name: from the shared servers, else the start of the id.
    pub fn name_of(&self, account_id: &str) -> String {
        if !account_id.is_empty() && account_id == self.me() {
            return self.self_name().to_string();
        }
        match self.person(account_id) {
            Some(person) => display_name(person),
            None if account_id.is_empty() => "Someone".into(),
            None => account_id.chars().take(8).collect(),
        }
    }

    /// What is known of someone: from the shared servers, else a looked-up profile.
    pub fn person(&self, account_id: &str) -> Option<&MemberSummary> {
        self.people
            .get(account_id)
            .or_else(|| self.profiles.get(account_id)?.as_ref())
    }

    pub fn servers(&self) -> &[ServerSummary] {
        self.snapshot.as_ref().map_or(&[], |s| s.servers.as_slice())
    }

    /// A server's channels, without the personal conversations a snapshot also lists.
    pub fn channels_of<'a>(
        &'a self,
        server_id: &'a str,
    ) -> impl Iterator<Item = &'a ChannelSummary> {
        let all = self
            .snapshot
            .as_ref()
            .map_or(&[][..], |s| s.channels.as_slice());
        all.iter()
            .filter(move |c| c.server_id == server_id && !PERSONAL_KINDS.contains(&c.kind.as_str()))
    }

    /// The personal conversations the snapshot lists, in its order.
    pub fn personal_channels(&self) -> impl Iterator<Item = &ChannelSummary> {
        let all = self
            .snapshot
            .as_ref()
            .map_or(&[][..], |s| s.channels.as_slice());
        all.iter()
            .filter(|c| PERSONAL_KINDS.contains(&c.kind.as_str()))
    }

    pub fn channel_summary(&self, channel_id: &str) -> Option<&ChannelSummary> {
        self.snapshot
            .as_ref()?
            .channels
            .iter()
            .find(|c| c.channel_id == channel_id)
    }

    /// The open conversation's sends that the store does not hold yet.
    pub fn pending_here(&self) -> impl Iterator<Item = &Pending> {
        let here = self.conversation.as_ref().map(Conversation::id);
        self.pending
            .iter()
            .filter(move |p| Some(p.channel_id.as_str()) == here)
    }

    /// An account's presence in a server, when it is there.
    pub fn presence_of(&self, server_id: &str, account_id: &str) -> Option<&(String, String)> {
        self.presence.get(server_id)?.get(account_id)
    }

    // ---- The queue ------------------------------------------------------------------

    /// Applies everything that finished since the last call and returns it in order.
    pub fn poll(&mut self) -> Vec<Update> {
        let updates: Vec<Update> = self.updates.try_iter().collect();
        for update in &updates {
            self.apply(update);
        }
        updates
    }

    /// Applies one update. Public so a test can stand in for the instance.
    pub fn apply(&mut self, update: &Update) {
        match update {
            Update::Info(Ok(info)) => self.info = Some(info.clone()),
            Update::Info(Err(_)) => self.info = None,
            Update::SignedIn(Ok(())) => {
                self.signed_in = true;
                self.load();
            }
            Update::SignedIn(Err(_)) => {}
            Update::Snapshot(Ok(snapshot)) => {
                let channels = server_channels(snapshot);
                // Members are asked for again only when the servers changed, and once
                // after a launch that began from what was kept.
                let servers_changed = self.snapshot.as_ref().is_none_or(|old| {
                    old.servers
                        .iter()
                        .map(|s| &s.server_id)
                        .ne(snapshot.servers.iter().map(|s| &s.server_id))
                });
                self.snapshot = Some(snapshot.clone());
                self.load_failures = 0;
                self.last_error = None;
                self.start(&channels);
                if servers_changed || !self.members_fresh {
                    self.members_fresh = true;
                    self.load_members(&snapshot.servers);
                }
                self.dm_snapshot_applied();
            }
            Update::Snapshot(Err(failure)) => {
                self.last_error = Some(failure.message.clone());
                if failure.code == "UNAUTHENTICATED" {
                    self.signed_in = false;
                } else if self.snapshot.is_none() {
                    // Nothing was kept from an earlier launch, so there is nothing to
                    // show until one arrives: ask again after 2 s, then 4, up to 30.
                    self.load_failures += 1;
                    self.load_after(std::time::Duration::from_secs(
                        (1u64 << self.load_failures.min(5)).min(30),
                    ));
                }
            }
            Update::Members { server_id, members } => {
                for member in members {
                    self.people
                        .insert(member.account_id.clone(), member.clone());
                }
                self.members.insert(server_id.clone(), members.clone());
            }
            Update::Profile {
                account_id,
                profile,
            } => {
                self.profiles.insert(account_id.clone(), profile.clone());
            }
            Update::Older { channel_id, page } => {
                if self.channel() != Some(channel_id.as_str()) {
                    return;
                }
                self.reach.loading = false;
                match page {
                    Ok((older, more)) => {
                        self.reach.remote_done = !*more || older.is_empty();
                        let mut history = older.clone();
                        history.append(&mut self.reach.history);
                        self.reach.history = history;
                        self.read_entries();
                    }
                    Err(failure) => {
                        // Not asked again for this channel until it is reopened.
                        self.reach.remote_done = true;
                        self.last_error = Some(failure.message.clone());
                    }
                }
            }
            Update::Gateway(event) => self.on_gateway(event),
            Update::SignedOut => {}
            Update::Dm(update) => self.apply_dm(update),
            Update::Server(update) => self.apply_server(update),
            Update::Identity(update) => self.apply_identity(update),
        }
    }

    fn on_gateway(&mut self, event: &AppEvent) {
        match event {
            AppEvent::Welcomed { .. } => {
                self.connection = Connection::Connected;
                self.presence.clear();
                self.send_watch();
                self.load();
            }
            AppEvent::Applied { channel_id, .. } | AppEvent::SnapshotTaken { channel_id, .. } => {
                if self.channel() == Some(channel_id.as_str()) {
                    self.read_entries();
                }
            }
            AppEvent::AccountApplied { .. } => self.load(),
            AppEvent::Error {
                command_id,
                message,
                ..
            } => {
                if let Some(pending) = self.pending.iter_mut().find(|p| p.id == *command_id) {
                    pending.failed = Some(message.clone());
                    self.revision += 1;
                }
            }
            AppEvent::Reconnecting { attempt, .. } => {
                self.connection = Connection::Reconnecting { attempt: *attempt }
            }
            AppEvent::Disconnected { reason, .. } => {
                self.connection = Connection::Disconnected {
                    reason: reason.clone(),
                }
            }
            AppEvent::Closed { reason } => {
                self.connection = Connection::Closed {
                    reason: reason.clone(),
                }
            }
            AppEvent::PresenceDiff {
                server_id,
                joined,
                updated,
                left_account_ids,
            } => {
                let server = self.presence.entry(server_id.clone()).or_default();
                for p in joined.iter().chain(updated) {
                    server.insert(
                        p.account_id.clone(),
                        (p.status.clone(), p.custom_status.clone()),
                    );
                }
                for id in left_account_ids {
                    server.remove(id);
                }
                // Someone present whom the member list does not have joined since it was
                // read: read it again.
                let listed = self.members.get(server_id);
                let stranger = joined.iter().any(|p| {
                    listed.is_some_and(|all| all.iter().all(|m| m.account_id != p.account_id))
                });
                if stranger {
                    let server = self.servers().iter().find(|s| s.server_id == *server_id);
                    if let Some(server) = server.cloned() {
                        self.load_members(&[server]);
                    }
                }
            }
            AppEvent::DmError { .. }
            | AppEvent::DmNote { .. }
            | AppEvent::DmSubscribed { .. }
            | AppEvent::DmOperation { .. }
            | AppEvent::DmReady { .. }
            | AppEvent::DmDelivered { .. } => self.on_dm_event(event),
            AppEvent::Acked { .. } | AppEvent::SubscriptionFailed { .. } => {}
        }
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        if let Some(app) = &self.app {
            app.stop();
        }
    }
}

/// The channels of a snapshot that belong to servers: what the gateway subscribes to from
/// the start (personal conversations are the DM engine's to subscribe).
fn server_channels(snapshot: &SnapshotSummary) -> Vec<String> {
    snapshot
        .channels
        .iter()
        .filter(|c| !PERSONAL_KINDS.contains(&c.kind.as_str()))
        .map(|c| c.channel_id.clone())
        .collect()
}

/// The name a person goes by: display name, else nickname, else handle, else the id's start.
pub fn display_name(person: &MemberSummary) -> String {
    [&person.display_name, &person.nickname, &person.handle]
        .into_iter()
        .find(|n| !n.is_empty())
        .cloned()
        .unwrap_or_else(|| person.account_id.chars().take(8).collect())
}
