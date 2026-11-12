//! The updater (architecture section 13, "Distribution"): on macOS, Sparkle, which reads
//! the release feed named in the bundle's `Info.plist`, checks each download against the
//! public key beside it, and installs what the person accepts. The bridge to it is the
//! `lumia-sparkle` crate, the one place in the workspace with `unsafe` in it; this module is
//! where the shell holds the updater and what the menu's two items do with it.
//!
//! Only a real launch of the bundled app has one. Sparkle is loaded from the bundle at run
//! time, so a bare `cargo run` binary, a test and a headless run have no updater and the
//! menu has no update items; Windows and Linux have none until their own updaters exist
//! (plan step 9). The daily check of the feed is the one request the app makes to anything
//! but the person's instances, and "Check for updates automatically" is how they turn it
//! off.

use std::cell::OnceCell;
use std::time::Duration;

use lumia_sparkle::Updater;
use rune::Tx;

use crate::app::State;
use crate::app::menu::{UpdateActions, Updates};

thread_local! {
    /// The updater, on the thread that started it: the main one, where Rune also runs the
    /// menu's actions and the poll. It cannot be anywhere else (it is not `Send`), which
    /// is the point: Sparkle's objects belong to the main thread.
    static UPDATER: OnceCell<Updater> = const { OnceCell::new() };
}

/// How often the menu looks again while a check is under way, to enable "Check for
/// updates…" when it ends. Nothing is asked for while one can be made.
const RECHECK: Duration = Duration::from_millis(500);

/// What the menu's update items do.
pub const ACTIONS: UpdateActions = UpdateActions {
    check,
    toggle_automatic,
};

/// Starts the updater on this thread, which must be the main one, once per launch. From
/// here Sparkle checks on its own schedule. Without Sparkle this does nothing, and a
/// debug build's trace says which kind of nothing; a bundle that should have had an
/// updater says so on standard error.
pub fn start() {
    UPDATER.with(|cell| {
        if cell.get().is_some() {
            return;
        }
        match Updater::start() {
            Ok(updater) => {
                super::trace(|| "updater started".to_string());
                let _ = cell.set(updater);
            }
            Err(why) => {
                if why.is_fault() {
                    eprintln!("lumia: no updater: {why}");
                }
                super::trace(|| format!("updater absent: {why}"));
            }
        }
    });
}

/// What the updater says, for the menu. None when there is none on this thread.
pub fn standing() -> Option<Updates> {
    UPDATER.with(|cell| {
        cell.get().map(|updater| Updates {
            can_check: updater.can_check_for_updates(),
            automatic: updater.checks_automatically(),
        })
    })
}

/// Asks for another poll while a check is under way, so the menu hears it end.
pub fn follow(tx: &mut Tx) {
    if standing().is_some_and(|updates| !updates.can_check) {
        tx.wake_after(RECHECK);
    }
}

/// Lumia › Check for updates…: a check the person asked for. Sparkle's own windows show
/// what it finds.
fn check(state: &mut State, tx: &mut Tx) {
    UPDATER.with(|cell| {
        if let Some(updater) = cell.get() {
            updater.check_for_updates();
        }
    });
    super::sync_menu(state);
    follow(tx);
}

/// Lumia › Check for updates automatically: the daily check, on or off. Sparkle keeps the
/// choice in the app's preferences.
fn toggle_automatic(state: &mut State, _tx: &mut Tx) {
    UPDATER.with(|cell| {
        if let Some(updater) = cell.get() {
            updater.set_checks_automatically(!updater.checks_automatically());
        }
    });
    super::sync_menu(state);
}
