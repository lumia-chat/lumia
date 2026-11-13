//! What makes the shell an app on its platform, around the window Rune opens: one running
//! copy, the links the system hands it, the menu bar, the window's remembered frame and
//! system notifications. The decisions are the state's (`app::links`, `app::notices`,
//! `app::menu`); this is where they meet the system, and the only place that does.
//!
//! Built for macOS. What the others get today, and what they need:
//!
//! - **Links.** macOS hands a `lumia:` link to the running app as an Apple event, which
//!   Rune turns into `on_open_url`; the bundle declares the scheme
//!   (`scripts/bundle-macos.sh`). Windows and Linux pass a link to a new process as an
//!   argument, which [`instance`] hands to the running one; each needs the scheme
//!   registered at install. The Linux package's launcher entry does that
//!   (`packaging/linux/chat.lumia.desktop`, `x-scheme-handler/lumia`), and nobody has
//!   followed a link through it; on Windows it is a registry key an installer would
//!   write, and there is no installer.
//! - **Menus.** Only macOS has a menu bar; elsewhere [`sync_menu`] finds none and the
//!   same actions are on screen.
//! - **Updates.** On macOS the bundled app updates itself with Sparkle ([`updates`]),
//!   which adds two items to the application menu; a bare binary has neither. Windows and
//!   Linux have no updater yet.
//! - **The frame.** AppKit saves and restores it (`rune-mac`); Windows and Linux open at
//!   the default size and need their own saved frame in `settings.json`.
//! - **Notifications.** `rune-notify` posts on macOS from a bundled app and on Windows
//!   once the process has registered an identity (`rune_notify::register`, not called
//!   yet); on Linux it posts nothing, and the shell needs the desktop's notification
//!   service over D-Bus. A conversation's notice is taken back once it is read, here or on
//!   another device.
//! - **The badge.** On macOS the Dock icon carries the count of what is unread and for this
//!   account (`lumia-sparkle`'s `set_badge`); Windows and Linux show none yet.
//! - **Preferences.** The window's own light or dark is the shell's on every platform;
//!   on macOS the app's appearance follows it too, and the choice of when scroll bars
//!   show is carried out through the system's setting ([`system`]). Elsewhere Rune shows
//!   scroll bars one way and the choice is not offered.
//! - **Staying open.** On macOS closing the window leaves the app running, as Mac apps
//!   do, so messages keep arriving and a click on the Dock icon brings the window back.
//!   Elsewhere closing the window quits, until there is a tray icon to come back from.

pub mod instance;
mod system;
pub mod updates;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock, PoisonError};

use rune::{App, AppHandle, MenuError, Tx};
use rune_notify::{Click, Notification};

use crate::app::{self, Route, State, menu};

/// The app's reverse-DNS name: the bundle identifier on macOS, and the data directory's
/// name. The one Lumia has always had on the Mac, so an installed app's preferences, its
/// notification settings and its update feed carry over to this shell.
pub const APP_ID: &str = "chat.lumia";

/// The window's size at a first launch: the sign-in card with every way in an instance can
/// offer fits without scrolling, and the Quiet Split has room for its aside. Later launches
/// open at the size the person left it.
pub const DEFAULT_SIZE: (f32, f32) = (1280.0, 900.0);
/// The size a headless run draws at, whatever was remembered: pictures and scripted runs
/// stay comparable from one machine and one launch to the next.
pub const HEADLESS_SIZE: (f32, f32) = (1180.0, 760.0);
/// The smallest the window goes.
pub const MIN_SIZE: (f32, f32) = (880.0, 560.0);

/// The name AppKit saves the window's frame under.
const FRAME: &str = "Lumia";
/// The user-info key a notification carries its conversation in.
const CHANNEL: &str = "channel";

/// The running app, for what arrives off the window's thread or outside a handler: a
/// later launch, a clicked notification, a window to bring forward, the menu to refresh.
static HANDLE: OnceLock<AppHandle<State>> = OnceLock::new();
/// Whether a real window is being run. False in a headless run and in tests, where there
/// is no frame to remember.
static WINDOWED: AtomicBool = AtomicBool::new(false);
static FRAMED: AtomicBool = AtomicBool::new(false);
/// Whether the system's clock has been read since launch (`follow_clock`).
static CLOCK_READ: AtomicBool = AtomicBool::new(false);
/// What decides the menu bar's items: where the app stands (`menu::standing`), and what
/// the updater says when there is one.
type MenuFor = ((bool, bool), Option<menu::Updates>);
/// What the menu bar's items were last made for.
static MENU: Mutex<Option<MenuFor>> = Mutex::new(None);
/// The notifications posted and not yet taken back, by id.
static POSTED: Mutex<Vec<String>> = Mutex::new(Vec::new());
/// The count last put on the Dock icon.
static BADGE: Mutex<Option<u32>> = Mutex::new(None);

/// The size to open the window at: the one remembered from the last launch on macOS,
/// else the default. A headless run always draws [`HEADLESS_SIZE`].
pub fn window_size(headless: bool) -> (f32, f32) {
    if headless {
        return HEADLESS_SIZE;
    }
    rune_mac::saved_size(FRAME).map_or(DEFAULT_SIZE, |(width, height)| {
        (width.max(MIN_SIZE.0), height.max(MIN_SIZE.1))
    })
}

/// Adds the shell's hooks to the app: links, focus, the Account menu, and on macOS a
/// process that outlives its window.
pub fn install(app: App<State>) -> App<State> {
    let mut app = app.on_open_url(open_url).on_focus(focus);
    // The update items join when the updater has started, which needs the app running.
    for item in menu::items((false, false), None) {
        app = app.menu_item(item);
    }
    if cfg!(target_os = "macos") {
        app = app.keep_running();
    }
    // For measuring a launch (architecture 17's cold-start row): `LUMIA_FIRST_FRAME` set,
    // the window's first frame on the GPU prints a line; set to `quit`, the app then exits.
    if let Some(mode) = std::env::var_os("LUMIA_FIRST_FRAME") {
        let quit = mode == "quit";
        app = app.on_ready(move |_| {
            println!("lumia-first-frame");
            if quit {
                std::process::exit(0);
            }
        });
    }
    app
}

/// Connects the running app to what reaches it from outside: later launches through
/// `hold`, clicks on its notifications, and (where the system passes links as arguments)
/// the links this launch came with. Call once, before the app runs. A `headless` run is
/// connected to nothing.
pub fn attach(
    handle: AppHandle<State>,
    hold: &mut instance::Hold,
    links: Vec<String>,
    headless: bool,
) {
    if headless {
        // A scripted run still fires menu items and links; nothing reaches it from outside.
        let _ = HANDLE.set(handle);
        return;
    }
    WINDOWED.store(true, Ordering::Relaxed);
    let later = handle.clone();
    hold.listen(move |launch| {
        for link in launch.links {
            later.update(move |state, tx| open_url(state, &link, tx));
        }
        // A launch brings the window forward, as opening a running app does.
        later.activate(launch.token);
    });
    let clicks = handle.clone();
    rune_notify::on_click(move |click| clicked(&clicks, &click));
    // macOS sends this launch's link as an Apple event and Rune reads Windows' from the
    // arguments itself; on the other Unixes it is ours to pass on.
    if cfg!(all(unix, not(target_os = "macos"))) {
        for link in links {
            handle.update(move |state, tx| open_url(state, &link, tx));
        }
    }
    dev_browser(&handle);
    // The updater is Sparkle's, and Sparkle's objects are the main thread's: started from
    // inside the loop, where the application object exists. Its items join the menu then.
    handle.update(|state, _| {
        updates::start();
        sync_menu(state);
    });
    trace(|| format!("notifications can_post={}", rune_notify::available()));
    let _ = HANDLE.set(handle);
}

/// A debug build with `LUMIA_DEV_BROWSER=FILE` starts a sign-in through the browser at
/// launch and writes the page to FILE instead of opening it, so a check with no hands on
/// the window (`tests/live_app.rs`) can come back on the link as a browser would.
fn dev_browser(handle: &AppHandle<State>) {
    if !cfg!(debug_assertions) {
        return;
    }
    let Some(file) = std::env::var_os("LUMIA_DEV_BROWSER") else {
        return;
    };
    handle.update(move |state, _| {
        let page = state.begin_browser_sign_in(false).unwrap_or_default();
        if let Err(error) = std::fs::write(&file, page) {
            eprintln!("lumia: LUMIA_DEV_BROWSER: {error}");
        }
    });
}

/// A debug build with `LUMIA_DEV_TRACE=FILE` appends a line to FILE for what it does with
/// the system that leaves nothing to see from outside: whether it may post notifications,
/// each notice it hands over, whether the updater started, what the menu bar's items were
/// made for, and which preferences the system was asked to carry out.
fn trace(line: impl FnOnce() -> String) {
    use std::io::Write;

    if !cfg!(debug_assertions) {
        return;
    }
    let Some(file) = std::env::var_os("LUMIA_DEV_TRACE") else {
        return;
    };
    let written = std::fs::File::options()
        .create(true)
        .append(true)
        .open(file)
        .and_then(|mut file| writeln!(file, "{}", line()));
    if let Err(error) = written {
        eprintln!("lumia: LUMIA_DEV_TRACE: {error}");
    }
}

/// `App::on_open_url`: a link the system opened with the app. The text comes from
/// outside; `State::open_url` decides what, if anything, it may do.
pub fn open_url(state: &mut State, url: &str, _tx: &mut Tx) {
    // Windows delivers a clicked notification as a link on the app's scheme.
    if let Some(click) = rune_notify::click_from_url(url) {
        if let Some(channel) = click.get(CHANNEL) {
            state.open_notice(channel);
        }
        raise();
    } else if state.open_url(url) {
        raise();
    }
}

/// `App::on_focus`: the window gained or lost the keyboard focus.
pub fn focus(state: &mut State, focused: bool, _tx: &mut Tx) {
    state.window_focused(focused);
    // Read again each time the window comes in front: System Settings, where the clock is
    // changed, was in front instead.
    if focused && WINDOWED.load(Ordering::Relaxed) {
        follow_clock(state);
    }
}

/// Has the timeline's times follow the system's clock, 12 or 24 hours. Only a real window:
/// a test or a headless picture keeps the 12-hour clock whatever this machine's is.
fn follow_clock(state: &mut State) {
    CLOCK_READ.store(true, Ordering::Relaxed);
    if let Some(clock_24) = lumia_sparkle::clock_24_hour() {
        state.set_clock_24(clock_24);
    }
}

/// `AppSpec::poll`: the state's own poll, then what it owes the system.
pub fn poll(state: &mut State, now: f64, tx: &mut Tx) -> bool {
    let changed = app::poll(state, now, tx);
    settle(state, tx) || changed
}

/// What the state owes the system after a poll: a clicked notice's conversation, pages to
/// open in the browser, notices to post, the frame and the menu. True when the state
/// changed. Apart from [`poll`] so a test can run it over a state it filled itself.
pub fn settle(state: &mut State, tx: &mut Tx) -> bool {
    let mut changed = state.settle_notice();
    for page in std::mem::take(&mut state.links.to_open) {
        if let Err(error) = tx.open_url(&page) {
            state.links.handoff_error = format!("Could not open the browser: {error}");
            changed = true;
        }
    }
    notify(state);
    remember_frame();
    if WINDOWED.load(Ordering::Relaxed) {
        system::follow(state);
        badge(state);
        if !CLOCK_READ.load(Ordering::Relaxed) {
            follow_clock(state);
            changed = true;
        }
    }
    sync_menu(state);
    updates::follow(tx);
    changed
}

/// Shows the window and brings it in front.
fn raise() {
    if let Some(handle) = HANDLE.get() {
        handle.open();
    }
}

/// A notification was clicked: its conversation, and the window in front.
fn clicked(handle: &AppHandle<State>, click: &Click) {
    if let Some(channel) = click.get(CHANNEL).map(str::to_string) {
        handle.update(move |state, _| state.open_notice(&channel));
    }
    handle.open();
}

/// A notification's id: one per conversation, so a newer message replaces the older
/// notice instead of stacking under it.
fn notice_id(channel_id: &str) -> String {
    format!("lumia-{channel_id}")
}

/// Posts the notices the state owes, and takes back the ones that no longer say anything:
/// a conversation's once it is on screen in front of the person, and all of them when the
/// account signs out. Nothing posts unless `rune-notify` can (a bundled app on macOS).
fn notify(state: &mut State) {
    let owed = std::mem::take(&mut state.notices.to_post);
    let mut posted = POSTED.lock().unwrap_or_else(PoisonError::into_inner);
    if state.route != Route::Shell {
        let all: Vec<&str> = posted.iter().map(String::as_str).collect();
        rune_notify::remove(&all);
        posted.clear();
        return;
    }
    if state.notices.in_front
        && let Some(conversation) = &state.session.conversation
    {
        let id = notice_id(conversation.id());
        if let Some(index) = posted.iter().position(|p| *p == id) {
            rune_notify::remove(&[&id]);
            posted.swap_remove(index);
        }
    }
    // Read on another device (or here): nothing unread is left to tell of.
    let read: Vec<String> = posted
        .iter()
        .filter(|id| {
            id.strip_prefix("lumia-")
                .is_some_and(|channel| state.session.unread(channel).0 == 0)
        })
        .cloned()
        .collect();
    if !read.is_empty() {
        let ids: Vec<&str> = read.iter().map(String::as_str).collect();
        rune_notify::remove(&ids);
        posted.retain(|id| !read.contains(id));
    }
    // Only an app the system knows can post or ask it anything (a bundled one on macOS).
    if !rune_notify::available() {
        for notice in owed {
            trace(|| {
                format!(
                    "notice {} not posted: this app cannot post",
                    notice.channel_id
                )
            });
        }
        return;
    }
    for notice in owed {
        // The system's own word on whether the person is looking.
        if rune_notify::app_focused() {
            trace(|| {
                format!(
                    "notice {} not posted: the app is in front",
                    notice.channel_id
                )
            });
            continue;
        }
        let id = notice_id(&notice.channel_id);
        let note = Notification::new(&id, notice.title)
            .body(notice.body)
            .info(CHANNEL, notice.channel_id.as_str());
        let went = rune_notify::post(&note);
        trace(|| format!("notice {} posted={went}", notice.channel_id));
        if went && !posted.contains(&id) {
            posted.push(id);
        }
    }
}

/// Puts the count of what is unread and for this account on the Dock icon, when it changed;
/// none while signed out.
fn badge(state: &State) {
    let count = if state.route == Route::Shell {
        state.session.badge()
    } else {
        0
    };
    let mut last = BADGE.lock().unwrap_or_else(PoisonError::into_inner);
    if *last == Some(count) {
        return;
    }
    let label = (count > 0).then(|| count.to_string());
    let set = lumia_sparkle::set_badge(label.as_deref());
    trace(|| format!("badge {count} set={set}"));
    *last = Some(count);
}

/// Has AppKit save the window's frame and restore it at the next launch. Asked until the
/// window exists; nothing to do off macOS, headless or in a test.
fn remember_frame() {
    if WINDOWED.load(Ordering::Relaxed) && !FRAMED.load(Ordering::Relaxed) {
        FRAMED.store(rune_mac::remember_frame(FRAME), Ordering::Relaxed);
    }
}

/// Makes the menu bar's items for where the app stands and what the updater says, when
/// either changed.
fn sync_menu(state: &State) {
    let Some(handle) = HANDLE.get() else { return };
    let standing = (menu::standing(state), updates::standing());
    let mut last = MENU.lock().unwrap_or_else(PoisonError::into_inner);
    if *last == Some(standing) {
        return;
    }
    let updater = standing.1.map(|updates| (updates, updates::ACTIONS));
    match handle.set_menu(menu::items(standing.0, updater)) {
        // No menu bar on this platform: nothing to keep in step.
        Ok(()) | Err(MenuError::Unsupported) => {
            trace(|| format!("menu account={:?} updates={:?}", standing.0, standing.1));
            *last = Some(standing);
        }
        // Not running yet; asked again at the next poll.
        Err(_) => {}
    }
}
