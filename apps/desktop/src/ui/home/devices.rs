//! The identity's devices, and linking this computer to it.
//!
//! Not built yet.

use rune::El;

use crate::app::State;
use crate::tokens::icons;
use crate::ui::components::Look;
use crate::ui::conversation;

pub fn view(_s: &State, look: Look) -> El {
    conversation::vacant(
        "Devices",
        &icons::SHIELD,
        ("Not here yet", "Devices are not in this app yet."),
        look,
    )
}
