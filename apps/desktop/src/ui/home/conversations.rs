//! The personal conversation on screen, and its details as the aside.
//!
//! Not built yet.

use rune::El;

use crate::app::State;
use crate::tokens::icons;
use crate::ui::components::Look;
use crate::ui::conversation;

pub fn view(_s: &State, look: Look) -> El {
    conversation::vacant(
        "Home",
        &icons::MAIL,
        (
            "Not here yet",
            "Personal conversations are not in this app yet.",
        ),
        look,
    )
}

/// The conversation's details, as the aside panel.
pub fn aside(_s: &State, _look: Look) -> Option<El> {
    None
}
