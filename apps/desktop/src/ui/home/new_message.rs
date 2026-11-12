//! Starting a personal conversation with someone.
//!
//! Not built yet.

use rune::El;

use crate::app::State;
use crate::tokens::icons;
use crate::ui::components::Look;
use crate::ui::conversation;

pub fn view(_s: &State, look: Look) -> El {
    conversation::vacant(
        "New message",
        &icons::COMPOSE,
        (
            "Not here yet",
            "Starting a conversation is not in this app yet.",
        ),
        look,
    )
}
