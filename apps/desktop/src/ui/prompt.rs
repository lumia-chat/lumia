//! The prompt over the window when another device of the account asks to be linked
//! (identity spec 4.2.1), on this computer, which holds the identity: a card on a shade,
//! centred over whatever is on screen, one at a time. `app::prompts` says which prompt and
//! in what words. It holds the focus until it is answered: Escape and a press outside do not
//! put it away, "Not me" does.
//!
//! "They match" approves without this computer's own unlock, as Devices' "Approve a device"
//! does: the shell has no way to ask for one (`app::prompts`).

use rune::{El, col, el, icon, row, text};

use super::components::{Look, error_text, primary_button, shadowed, toned_notice, wide_button};
use super::link::{digits, waiting};
use crate::app::State;
use crate::app::prompts::{self, Ended};
use crate::tokens::{self as t, icons};
use lumia_client::app::LinkPrompt;

/// The prompt on screen, or how the last one ended; nothing when no device is asking.
pub fn view(s: &State, look: Look) -> Option<El> {
    let card = match &s.prompts.ended {
        Some(ended) => ended_card(ended, look),
        None => prompt_card(s, s.prompts.current()?, look),
    };
    Some(
        col()
            .key("prompt-shade")
            .pin(Some(0.0), Some(0.0), Some(0.0), Some(0.0))
            .fill(look.c.scrim)
            .center()
            .child(card),
    )
}

/// The card: the shield on its tile beside the title, then what the stage adds.
fn frame(title: &str, look: Look) -> El {
    let c = look.c;
    let heading = row()
        .w_full()
        .gap(t::SPACE_12)
        .items_center()
        .child(
            el().size(t::GLYPH_TILE, t::GLYPH_TILE)
                .radius(t::RADIUS_12)
                .fill(c.accent_soft)
                .center()
                .child(icon(&icons::SHIELD, t::SPACE_18).color(c.accent_ink)),
        )
        .child(
            text(title)
                .font(t::TITLE_SERVER)
                .color(c.ink)
                .role("heading")
                .wrap()
                .grow(1.0)
                .min_w(0.0),
        );
    shadowed(
        col()
            .key("link-prompt")
            .w(t::AUTH_WIDTH)
            .padding(t::SPACE_28)
            .gap(t::SPACE_18)
            .radius(t::RADIUS_22)
            .fill(c.surface)
            .border(1.0, c.panel_edge),
        look.shadows.card,
    )
    .no_window_drag()
    // Answered only by its buttons: put away by Escape, it would be asked again in two
    // seconds, and declining by a stray key would stop a link the person meant.
    .modal(false, |_: &mut State| {})
    .role("dialog")
    .label(title)
    .child(heading)
}

fn prompt_card(s: &State, prompt: &LinkPrompt, look: Look) -> El {
    let c = look.c;
    let busy = s.prompts.busy;
    let mut card = frame(&prompts::title(prompt), look);
    match (prompt.stage.as_str(), prompts::line(prompt)) {
        ("refused", Some(line)) => {
            card = card.child(toned_notice(&icons::KEY, line, t::caution(c)));
        }
        (_, Some(line)) => {
            card = card.child(
                text(line)
                    .font(t::TEXT_BODY)
                    .color(c.ink_secondary)
                    .wrap()
                    .w_full(),
            );
        }
        (_, None) => {}
    }
    match prompt.stage.as_str() {
        "waiting" => card = card.child(waiting("Waiting for the other device", look)),
        "compare" => card = card.child(digits("prompt-digits", &prompt.digits, look)),
        _ => {}
    }
    if !s.prompts.error.is_empty() {
        card = card.child(error_text(&s.prompts.error, look));
    }
    let not_me = |label: &str| {
        wide_button(
            "prompt-not-me",
            None,
            label,
            !busy,
            |s, _| s.decline_prompt(),
            look,
        )
    };
    let actions = col().w_full().gap(t::SPACE_8);
    card.child(match prompt.stage.as_str() {
        "new" => actions
            .child(primary_button(
                "prompt-continue",
                "Continue",
                !busy,
                State::answer_prompt,
                look,
            ))
            .child(not_me("Not me")),
        "compare" => actions
            .child(primary_button(
                "prompt-match",
                if busy { "Approving…" } else { "They match" },
                !busy,
                State::confirm_prompt,
                look,
            ))
            .child(wide_button(
                "prompt-mismatch",
                None,
                "They don’t match",
                !busy,
                |s, _| s.mismatch_prompt(),
                look,
            )),
        _ => actions.child(not_me("Not me")),
    })
}

/// How the prompt ended, for a moment: "Pixel 9 is linked.", "Linking was stopped."
fn ended_card(ended: &Ended, look: Look) -> El {
    frame(&ended.words, look).child(primary_button(
        "prompt-done",
        "Done",
        true,
        State::close_ended,
        look,
    ))
}
