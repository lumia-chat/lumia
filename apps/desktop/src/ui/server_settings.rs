//! A server's settings (architecture 6.7, `app::server_settings`): its picture and name, the
//! channels newcomers join (architecture 6.8), and how long it keeps its messages. Drawn in the server's main pane in place of the channel,
//! for whoever the menu under the server's name offers them to; a channel chosen in the
//! sidebar goes back to it.
//!
//! No board draws this page. It is the Profile page's vocabulary: cards, the photo row, a
//! field, the design system's segmented choice.

use lumia_client::app::ServerSummary;
use rune::{El, Tx, col, row, text};

use super::components::{
    Look, card, error_text, field, primary_button, segmented, server_picture, small_button,
};
use super::home::{action, page_column, titled_page};
use crate::app::State;
use crate::app::server_settings::{retention_choices, retention_words, shortens};
use crate::tokens as t;

pub fn view(s: &State, server: &ServerSummary, look: Look) -> El {
    let content = page_column()
        .child(overview(s, server, look))
        .child(super::channels::defaults_card(s, server, look))
        .child(history(s, server, look));
    titled_page(
        "server-settings",
        "Server settings",
        &server.name,
        content,
        look,
    )
}

fn quiet(words: &str, look: Look) -> El {
    text(words)
        .font(t::TEXT_META)
        .color(look.c.ink_secondary)
        .wrap()
        .w_full()
}

fn save_name(s: &mut State, _: &mut Tx) {
    s.save_server_name();
}

/// The server's picture and its name.
fn overview(s: &State, server: &ServerSummary, look: Look) -> El {
    let form = &s.server_forms.settings;
    let mut card = card(
        "Overview",
        "How the server shows on the rail and in invites.",
        look,
    )
    .key("server-overview")
    .w_full()
    .child(picture(s, server, look))
    .child(field(
        "server-name",
        "Name",
        &form.name,
        "Acme",
        |s| &mut s.server_forms.settings.name,
        save_name,
        look,
    ));
    if !form.name_saving.error.is_empty() {
        card = card.child(error_text(&form.name_saving.error, look));
    } else if form.name_saving.saved {
        card = card.child(quiet("Saved.", look));
    }
    let name = form.name.text.trim();
    card.child(action(primary_button(
        "save-server-name",
        "Save",
        !form.name_saving.busy && !name.is_empty() && name != server.name,
        State::save_server_name,
        look,
    )))
}

/// The picture: what it is now, a way to choose another, and a way to have none. The
/// instance makes its own square of the file, as it does an account's photo.
fn picture(s: &State, server: &ServerSummary, look: Look) -> El {
    let form = &s.server_forms.settings.picture;
    let has = !server.icon_url.is_empty();
    let mut buttons = row().gap(t::SPACE_8).items_center().child(small_button(
        "choose-server-picture",
        if form.busy {
            "Sending…"
        } else if has {
            "Change picture…"
        } else {
            "Choose a picture…"
        },
        !form.busy,
        |s, tx| s.choose_server_picture(tx),
        look,
    ));
    if has {
        buttons = buttons.child(small_button(
            "remove-server-picture",
            "Remove",
            !form.busy,
            |s, _| s.remove_server_picture(),
            look,
        ));
    }
    let words = col()
        .grow(1.0)
        .min_w(0.0)
        .gap(t::SPACE_6)
        .child(buttons)
        .child(if !form.error.is_empty() {
            error_text(&form.error, look)
        } else if form.saved {
            quiet("Saved.", look)
        } else {
            quiet(
                "A PNG, JPEG, WebP or GIF up to 8 MB. Everyone sees its middle, as a circle.",
                look,
            )
        });
    row()
        .key("server-picture")
        .w_full()
        .gap(t::SPACE_14)
        .items_center()
        .child(server_picture(s, server, t::AVATAR_CARD, look.c.ink, look))
        .child(words)
}

/// How long the server keeps its messages, with what a shorter time does said first.
fn history(s: &State, server: &ServerSummary, look: Look) -> El {
    let form = &s.server_forms.settings;
    let choices: Vec<(u32, String)> = retention_choices(server.retention_days)
        .into_iter()
        .map(|days| (days, retention_words(days)))
        .collect();
    let options: Vec<(u32, &str)> = choices.iter().map(|(d, w)| (*d, w.as_str())).collect();
    let mut card = card(
        "Message history",
        "How long this server keeps its messages.",
        look,
    )
    .key("server-history")
    .w_full()
    .child(segmented(
        "retention",
        "Keep messages",
        &options,
        form.retention,
        |s: &mut State, days: u32| s.choose_retention(days),
        look,
    ));
    if shortens(server.retention_days, form.retention) {
        card = card.child(error_text(
            &format!(
                "Messages older than {} will be deleted for everyone. This cannot be undone.",
                retention_words(form.retention)
            ),
            look,
        ));
    }
    if !form.retention_saving.error.is_empty() {
        card = card.child(error_text(&form.retention_saving.error, look));
    } else if form.retention_saving.saved {
        card = card.child(quiet("Saved.", look));
    }
    card.child(action(primary_button(
        "save-retention",
        "Save",
        !form.retention_saving.busy && form.retention != server.retention_days,
        State::save_server_retention,
        look,
    )))
}
