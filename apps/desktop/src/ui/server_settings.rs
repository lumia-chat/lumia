//! A server's settings (architecture 6.7 and 7.5, `app::server_settings`), as the "Server
//! settings" board draws them: the sections this account may use in a list beside the page
//! (`roles::sections`), and the one shown. Overview is its picture and name, what the tag
//! beside a name shows, the channels newcomers join (architecture 6.8), how long it keeps its
//! messages, and its files (specs/media.md 4.5); Roles and Members are `ui::roles` and
//! `ui::members`. Drawn in the server's main pane in place of the channel, for whoever the
//! menu under the server's name offers them to; a channel chosen in the sidebar goes back to
//! it.
//!
//! Overview's cards are the Profile page's vocabulary: cards, the photo row, a field, the
//! design system's segmented choice. "Tag beside names" is two pictured choices, saved as one
//! is chosen.

use lumia_client::app::ServerSummary;
use lumia_client::roles as core;
use rune::{El, ScrollSpec, Tx, col, el, row, text};

use super::components::{
    Look, card, check_row, error_text, field, panel, picture_choice, primary_button, role_tag,
    segmented, server_picture, small_button,
};
use super::home::page_column;
use crate::app::State;
use crate::app::roles::{Section, TagTone};
use crate::app::server_settings::{retention_choices, retention_words, shortens, usage_words};
use crate::tokens as t;
use crate::ui::scroll::WithScrollBar;

pub fn view(s: &State, server: &ServerSummary, look: Look) -> El {
    let content = match s.server_forms.settings.section {
        // Overview's cards are a sign-in card wide, as the board's are.
        Section::Overview => page_column().child(
            col()
                .w_full()
                .max_w(t::AUTH_WIDTH)
                .gap(t::SPACE_12)
                .child(overview(s, server, look))
                .child(tag_card(s, server, look))
                .child(super::channels::defaults_card(s, server, look))
                .child(history(s, server, look))
                .child(files(s, server, look)),
        ),
        Section::Roles => page_column().child(super::roles::section(s, server, look)),
        Section::Members => page_column().child(super::members::section(s, server, look)),
        // Not offered until the desktop's emoji screens (plan step 12).
        Section::Emoji => page_column(),
    };
    let header =
        super::conversation::header_with("Server settings", None, Some(&server.name), look);
    panel("panel", look).w_full().child(header).child(
        row()
            .w_full()
            .grow(1.0)
            .min_h(0.0)
            .child(super::roles::sections(s, server, look))
            .child(
                col()
                    .key("server-settings")
                    .grow(1.0)
                    .min_w(0.0)
                    .h_full()
                    .scroll(ScrollSpec {
                        stick_bottom: false,
                    })
                    .child(content)
                    .with_scroll_bar("server-settings", look),
            ),
    )
}

/// A tile's drawing of what the tag beside a name shows: two names, each with its tag, a
/// role's on its tone or a title's on the plain chip.
fn tag_sketch(titles: bool, look: Look) -> El {
    let c = look.c;
    let lines: [(&str, f32, &str, TagTone); 2] = if titles {
        [
            ("peach", 34.0, "Designer", TagTone::Neutral),
            ("mint", 26.0, "Engineer", TagTone::Neutral),
        ]
    } else {
        [
            ("peach", 34.0, "Admin", TagTone::Tone("indigo")),
            ("mint", 26.0, "Design", TagTone::Tone("peach")),
        ]
    };
    let mut sketch = col()
        .padding(t::SPACE_14)
        .gap(t::SPACE_12)
        .justify_center()
        .fill(c.surface);
    for (face, name, words, tone) in lines {
        sketch = sketch.child(
            row()
                .w_full()
                .gap(t::SPACE_8)
                .items_center()
                .child(
                    el().size(t::SKETCH_AVATAR, t::SKETCH_AVATAR)
                        .radius(t::RADIUS_PILL)
                        .fill(t::tone_named(face, c).0),
                )
                .child(
                    // The name's bar, as long as the board draws it under each face.
                    el().h(t::SKETCH_BAR)
                        .w(t::SPACE_2 * name / 2.0)
                        .radius(t::SKETCH_BAR / 2.0)
                        .fill(c.border_active),
                )
                .child(role_tag(words, tone, true, look)),
        );
    }
    sketch
}

/// "Tag beside names": roles or titles, saved as it is chosen (architecture 7.5).
fn tag_card(s: &State, server: &ServerSummary, look: Look) -> El {
    let form = &s.server_forms.settings;
    let titles = s.tag_titles_shown(server);
    let mut choices = row()
        .key("tag-choices")
        .w_full()
        .gap(t::SPACE_16)
        .items_start()
        .role("radiogroup")
        .label(core::TAG_HEADING);
    for (key, name, caption, value) in [
        (
            "tag-roles",
            core::TAG_ROLES_LABEL,
            core::TAG_ROLES_CAPTION,
            false,
        ),
        (
            "tag-titles",
            core::TAG_TITLES_LABEL,
            core::TAG_TITLES_CAPTION,
            true,
        ),
    ] {
        choices = choices.child(picture_choice(
            key,
            name,
            Some(caption),
            tag_sketch(value, look),
            titles == value,
            move |s: &mut State| s.choose_tag_titles(value),
            look,
        ));
    }
    let mut card = card(core::TAG_HEADING, core::TAG_SUBTITLE, look)
        .key("server-tag")
        .w_full()
        .child(choices);
    if !form.tag.error.is_empty() {
        card = card.child(error_text(&form.tag.error, look));
    }
    card.child(quiet(core::TAG_NOTE, look))
}

fn quiet(words: &str, look: Look) -> El {
    text(words)
        .font(t::read(t::TEXT_META))
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
    card.child(primary_button(
        "save-server-name",
        "Save",
        !form.name_saving.busy && !name.is_empty() && name != server.name,
        State::save_server_name,
        look,
    ))
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
        // The rail's tile across, in a line, as the board draws it.
        .child(
            el().child(server_picture(s, server, t::TILE_SIZE, look.c.ink, look))
                .child(
                    el().pin(Some(0.0), Some(0.0), Some(0.0), Some(0.0))
                        .radius(t::TILE_SIZE / 2.0)
                        .border(1.0, look.c.line_strong)
                        .a11y_hidden(),
                ),
        )
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
    card = card.child(primary_button(
        "save-retention",
        "Save",
        !form.retention_saving.busy && form.retention != server.retention_days,
        State::save_server_retention,
        look,
    ));
    // Saved as it is turned, apart from the retention's Save (architecture 6.7).
    card = card.child(check_row(
        "show-deleted",
        "Show deleted messages",
        Some("Off, a deleted message disappears for everyone. On, it stays in its place as “(deleted)”."),
        server.show_deleted,
        State::toggle_show_deleted,
        look,
    ));
    if !form.show_deleted.error.is_empty() {
        card = card.child(error_text(&form.show_deleted.error, look));
    }
    card
}

/// The server's files: what they take, how long attachments are kept, and what happens when
/// the server is full, with what a shorter time and letting the oldest go do said first.
fn files(s: &State, server: &ServerSummary, look: Look) -> El {
    let form = &s.server_forms.settings;
    let choices: Vec<(u32, String)> = retention_choices(server.attachment_retention_days)
        .into_iter()
        .map(|days| (days, retention_words(days)))
        .collect();
    let options: Vec<(u32, &str)> = choices.iter().map(|(d, w)| (*d, w.as_str())).collect();
    let mut card = card(
        "Files",
        "The attachments sent in this server's channels.",
        look,
    )
    .key("server-files")
    .w_full();
    card = card.child(match (&form.storage, form.storage_error.is_empty()) {
        (Some(storage), _) => quiet(&usage_words(storage), look),
        (None, false) => error_text(&form.storage_error, look),
        (None, true) => quiet("Counting…", look),
    });
    card = card.child(segmented(
        "attachment-retention",
        "Keep attachments",
        &options,
        form.attachment_retention,
        |s: &mut State, days: u32| s.choose_attachment_retention(days),
        look,
    ));
    if shortens(server.attachment_retention_days, form.attachment_retention) {
        card = card.child(error_text(
            &format!(
                "Attachments older than {} will be deleted for everyone. Their messages stay. This cannot be undone.",
                retention_words(form.attachment_retention)
            ),
            look,
        ));
    }
    card = card.child(segmented(
        "when-full",
        "When the server is full",
        &[
            (false, "Refuse new uploads"),
            (true, "Delete the oldest attachments"),
        ],
        form.delete_oldest_when_full,
        |s: &mut State, delete: bool| s.choose_when_full(delete),
        look,
    ));
    card = card.child(if form.delete_oldest_when_full {
        quiet(
            "When an upload would not fit, the oldest messages’ attachments are deleted for everyone to make room.",
            look,
        )
    } else {
        quiet(
            "When an upload would not fit, it is refused until files are deleted.",
            look,
        )
    });
    if !form.files_saving.error.is_empty() {
        card = card.child(error_text(&form.files_saving.error, look));
    } else if form.files_saving.saved {
        card = card.child(quiet("Saved.", look));
    }
    card.child(primary_button(
        "save-files",
        "Save",
        !form.files_saving.busy
            && (form.attachment_retention != server.attachment_retention_days
                || form.delete_oldest_when_full != server.delete_oldest_when_full),
        State::save_server_files,
        look,
    ))
}
