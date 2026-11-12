//! The two preferences the system carries out for the app (`app::State::set_appearance`
//! and `set_scroll_bars`, chosen on the Preferences page): light or dark, and when scroll
//! bars show.
//!
//! The window draws itself light or dark from the setting alone (`ui::view`), on every
//! platform. What this adds on macOS is the rest of the app: the tooltips, sheets and menus
//! AppKit draws for it follow the same choice, because the app's own appearance is set and
//! not only the window's colours.
//!
//! Scroll bars are different: Rune shows them as the system says and gives an app no say,
//! so on macOS the choice is made in the system's own terms, for this app alone
//! (`lumia-sparkle`'s `set_scroll_bars`, which says how), and Rune reads it as it would
//! read a change in System Settings. On Windows and Linux Rune has one behaviour, bars
//! that show while a list scrolls, and nothing here can change it; the Preferences page
//! does not draw the choice there.
//!
//! Only a real launch does any of it. A test, a headless picture and a scripted run leave
//! the machine's preferences alone.

use std::sync::{Mutex, PoisonError};

use lumia_sparkle as system;

use crate::app::State;
use crate::settings::{Appearance, ScrollBars};

/// What was last carried out, so each choice is made once.
static CARRIED: Mutex<Option<(Appearance, ScrollBars)>> = Mutex::new(None);

/// Carries out the preferences when they are not what was last carried out: at the first
/// poll of a launch, and after each change. Main thread (the poll's).
pub(super) fn follow(state: &State) {
    let wanted = (state.settings.appearance, state.settings.scroll_bars);
    let mut carried = CARRIED.lock().unwrap_or_else(PoisonError::into_inner);
    if *carried == Some(wanted) {
        return;
    }
    let dressed = system::set_appearance(match wanted.0 {
        Appearance::Light => system::Appearance::Light,
        Appearance::Dark => system::Appearance::Dark,
        Appearance::System => system::Appearance::System,
    });
    let scrolled = system::set_scroll_bars(match wanted.1 {
        ScrollBars::Automatic => system::ScrollBars::System,
        ScrollBars::WhenScrolling => system::ScrollBars::WhenScrolling,
        ScrollBars::Always => system::ScrollBars::Always,
    });
    super::trace(|| {
        format!(
            "preferences appearance={:?} set={dressed} scroll_bars={:?} set={scrolled}",
            wanted.0, wanted.1
        )
    });
    *carried = Some(wanted);
}
