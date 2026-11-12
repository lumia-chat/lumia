//! Adding a server: create one, or join one from an invite.
//!
//! Not built yet.

use rune::El;

use crate::app::State;
use crate::tokens::icons;
use crate::ui::components::Look;
use crate::ui::conversation;

pub fn view(_s: &State, look: Look) -> El {
    conversation::vacant(
        "Add a server",
        &icons::PLUS,
        ("Not here yet", "Adding a server is not in this app yet."),
        look,
    )
}
