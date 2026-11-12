//! This person's profile and the status they chose.
//!
//! Not built yet.

use rune::El;

use crate::app::State;
use crate::tokens::icons;
use crate::ui::components::Look;
use crate::ui::conversation;

pub fn view(_s: &State, look: Look) -> El {
    conversation::vacant(
        "Profile",
        &icons::KEY,
        ("Not here yet", "The profile is not in this app yet."),
        look,
    )
}
