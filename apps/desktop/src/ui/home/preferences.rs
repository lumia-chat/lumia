//! Preferences: how Lumia looks on this computer. Three choices, each taking effect as it
//! is made and kept in `settings.json` for the next launch: light or dark (or as the
//! system is), how much room messages take, and when a list shows its scroll bar. They
//! are this computer's, not the account's: nothing here is sent to the instance.
//!
//! The boards' account menu has Preferences and no board draws the page, so it is a Home
//! page in the vocabulary of Profile and Devices.
//!
//! The scroll bar's card is drawn on macOS only: there the choice is carried out by the
//! system's own setting, made for this app alone (`platform::system`). Rune gives an app
//! no say in it anywhere else, and a control that does nothing is worse than none.

use rune::El;

use super::devices::{page, page_column};
use crate::app::State;
use crate::settings::{Appearance, Density, ScrollBars};
use crate::ui::components::{Look, card, segmented};

pub fn view(s: &State, look: Look) -> El {
    let mut content = page_column()
        .child(appearance(s, look))
        .child(density(s, look));
    if scroll_bars_can_be_chosen() {
        content = content.child(scroll_bars(s, look));
    }
    page(
        "preferences",
        "Preferences",
        "How Lumia looks on this computer",
        content,
        look,
    )
}

/// Whether this platform lets the app say when its scroll bars show.
pub fn scroll_bars_can_be_chosen() -> bool {
    cfg!(target_os = "macos")
}

fn appearance(s: &State, look: Look) -> El {
    card(
        "Appearance",
        "Light, dark, or whichever this computer is in.",
        look,
    )
    .key("appearance-card")
    .w_full()
    .child(segmented(
        "appearance",
        "Appearance",
        &[
            (Appearance::Light, "Light"),
            (Appearance::Dark, "Dark"),
            (Appearance::System, "System"),
        ],
        s.settings.appearance,
        |s: &mut State, choice| s.set_appearance(choice),
        look,
    ))
}

fn density(s: &State, look: Look) -> El {
    card(
        "Messages",
        "Comfortable gives each message room. Compact shows more of a conversation at once.",
        look,
    )
    .key("density-card")
    .w_full()
    .child(segmented(
        "density",
        "Message density",
        &[
            (Density::Comfortable, "Comfortable"),
            (Density::Compact, "Compact"),
        ],
        s.settings.density,
        |s: &mut State, choice| s.set_density(choice),
        look,
    ))
}

fn scroll_bars(s: &State, look: Look) -> El {
    card(
        "Scroll bars",
        "When a list shows its scroll bar. Automatic follows this Mac's own setting.",
        look,
    )
    .key("scroll-bars-card")
    .w_full()
    .child(segmented(
        "scroll-bars",
        "Show scroll bars",
        &[
            (ScrollBars::Automatic, "Automatic"),
            (ScrollBars::WhenScrolling, "When scrolling"),
            (ScrollBars::Always, "Always"),
        ],
        s.settings.scroll_bars,
        |s: &mut State, choice| s.set_scroll_bars(choice),
        look,
    ))
}
