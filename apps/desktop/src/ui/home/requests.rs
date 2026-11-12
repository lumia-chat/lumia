//! The requests to start a personal conversation that wait for an answer.
//!
//! Not built yet.

use rune::El;

use crate::app::State;
use crate::tokens::icons;
use crate::ui::components::Look;
use crate::ui::conversation;

pub fn view(_s: &State, look: Look) -> El {
    conversation::vacant(
        "Requests",
        &icons::MENTION,
        ("Not here yet", "Requests are not in this app yet."),
        look,
    )
}
