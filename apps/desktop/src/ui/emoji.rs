//! The emoji picker (architecture 6.6): a search field, "Frequently used" from this
//! computer's own reactions, and every emoji by Unicode's groups, from the `emojis` crate
//! (fully qualified, default skin tone; a search matches its name and shortcodes), up to
//! [`NEWEST`]: the systems' emoji fonts lag Unicode, and an emoji the font does not have
//! draws as an empty box. Choosing one reacts to the message it was opened for.

use rune::{Align, Cursor, El, ScrollSpec, Side, col, el, icon, row, text, text_input};

use super::components::{Look, shadowed};
use super::scroll::WithScrollBar;
use crate::app::State;
use crate::tokens::{self as t, icons};

/// The picker's width: eight emoji across with the room around them.
const COLUMNS: usize = 8;
const CELL: f32 = t::ICON_BUTTON;
const WIDTH: f32 = CELL * COLUMNS as f32 + 2.0 * t::SPACE_10;
/// Its list's height: about seven rows.
const LIST_HEIGHT: f32 = CELL * 7.0;
/// The newest emoji offered: Unicode 16, which current macOS, iOS and Windows draw (this
/// Mac's macOS 27 has none of Unicode 18's), as does the Noto Color Emoji that Linux builds
/// carry (`build.rs`).
const NEWEST: (u32, u32) = (16, 0);

/// Whether the system's emoji font can be expected to draw `e`.
fn drawable(e: &emojis::Emoji) -> bool {
    let v = e.unicode_version();
    (v.major(), v.minor()) <= NEWEST
}

/// How many a search shows at most.
const FOUND: usize = 64;

/// A group's name as the picker heads it.
fn group_name(group: emojis::Group) -> &'static str {
    use emojis::Group;
    match group {
        Group::SmileysAndEmotion => "Smileys & emotion",
        Group::PeopleAndBody => "People & body",
        Group::AnimalsAndNature => "Animals & nature",
        Group::FoodAndDrink => "Food & drink",
        Group::TravelAndPlaces => "Travel & places",
        Group::Activities => "Activities",
        Group::Objects => "Objects",
        Group::Symbols => "Symbols",
        Group::Flags => "Flags",
    }
}

/// The emoji a search finds: those whose name or a shortcode holds every word of it.
fn search(wanted: &str) -> Vec<&'static emojis::Emoji> {
    let words: Vec<String> = wanted.split_whitespace().map(str::to_lowercase).collect();
    emojis::iter()
        .filter(|e| drawable(e))
        .filter(|e| {
            let name = e.name().to_lowercase();
            words.iter().all(|w| {
                name.contains(w.as_str()) || e.shortcodes().any(|code| code.contains(w.as_str()))
            })
        })
        .take(FOUND)
        .collect()
}

/// One emoji to choose.
fn cell(key: String, emoji: &str, name: &str, look: Look) -> El {
    let c = look.c;
    let pick = emoji.to_string();
    el().key(key)
        .size(CELL, CELL)
        .radius(t::RADIUS_8)
        .center()
        .role("button")
        .label(name.to_string())
        .help(name.to_string())
        .hover(c.hover_icon)
        .cursor(Cursor::Pointer)
        .focusable(true)
        .focus_ring(2.0, 0.0, c.focus)
        .on_press(move |s: &mut State| s.pick_emoji(&pick))
        .child(text(emoji.to_string()).font(t::TITLE_CHANNEL))
}

/// Emoji in rows of [`COLUMNS`].
fn grid(key: String, cells: Vec<El>) -> El {
    let mut grid = col().key(key).w_full().role("list");
    let mut cells = cells.into_iter().peekable();
    let mut n = 0;
    while cells.peek().is_some() {
        let mut line = row().key(format!("r{n}"));
        for cell in cells.by_ref().take(COLUMNS) {
            line = line.child(cell);
        }
        grid = grid.child(line);
        n += 1;
    }
    grid
}

fn heading(words: &str, look: Look) -> El {
    text(words.to_string())
        .font(t::TEXT_SECTION)
        .color(look.c.ink_muted)
        .px(t::SPACE_4)
        .pt(t::SPACE_8)
        .pb(t::SPACE_4)
        .role("heading")
}

/// The picker, hung from the control that opened it.
pub fn picker(s: &State, look: Look) -> Option<El> {
    let c = look.c;
    let picker = s.replies.picker.as_ref()?;
    let field = row()
        .key("emoji-search-box")
        .w_full()
        .h(t::CONTROL_SMALL)
        .px(t::SPACE_12)
        .gap(t::SPACE_8)
        .items_center()
        .radius(t::RADIUS_10)
        .fill(c.field)
        .border(1.0, c.border_composer)
        .child(icon(&icons::SEARCH, t::SPACE_16).color(c.ink_quiet))
        .child(
            text_input(&picker.search, t::TEXT_MENU)
                .key("emoji-search")
                .label("Search emoji")
                .placeholder("Search emoji", c.ink_muted)
                .color(c.ink_field)
                .caret(c.accent, c.accent_soft)
                .grow(1.0)
                .min_w(0.0)
                .on_edit(|s: &mut State| match &mut s.replies.picker {
                    Some(picker) => &mut picker.search,
                    // Gone between the frame and the edit: an edit to nothing kept.
                    None => &mut s.member_filter,
                }),
        );
    let wanted = picker.search.text.trim();
    let mut list = col().w_full().px(t::SPACE_10).pb(t::SPACE_10);
    if wanted.is_empty() {
        let frequent = s
            .frequent_emoji()
            .into_iter()
            .enumerate()
            .map(|(n, e)| {
                let name = emojis::get(&e).map_or(e.clone(), |found| found.name().to_string());
                cell(format!("f{n}"), &e, &name, look)
            })
            .collect();
        list = list
            .child(heading("Frequently used", look))
            .child(grid("frequent".into(), frequent));
        for (g, group) in emojis::Group::iter().enumerate() {
            let cells = group
                .emojis()
                .filter(|e| drawable(e))
                .enumerate()
                .map(|(n, e)| cell(format!("e{n}"), e.as_str(), e.name(), look))
                .collect();
            list = list
                .child(heading(group_name(group), look))
                .child(grid(format!("group-{g}"), cells));
        }
    } else {
        let found = search(wanted);
        if found.is_empty() {
            list = list.child(
                text("No emoji match that.")
                    .font(t::TEXT_META)
                    .color(c.ink_quiet)
                    .padding(t::SPACE_10),
            );
        } else {
            let cells = found
                .iter()
                .enumerate()
                .map(|(n, e)| cell(format!("s{n}"), e.as_str(), e.name(), look))
                .collect();
            list = list
                .child(el().h(t::SPACE_8))
                .child(grid("found".into(), cells));
        }
    }
    let surface = col()
        .key("emoji-picker")
        .w(WIDTH)
        .radius(t::RADIUS_14)
        .fill(c.surface)
        .border(1.0, c.line_strong)
        .role("dialog")
        .label("Add reaction")
        .child(el().w_full().padding(t::SPACE_10).child(field))
        .child(
            col()
                .key("emoji-list")
                .w_full()
                .h(LIST_HEIGHT)
                .scroll(ScrollSpec {
                    stick_bottom: false,
                })
                .child(list)
                .with_scroll_bar("emoji-list", look),
        );
    Some(
        shadowed(surface, look.shadows.floating)
            .anchored_fit(picker.anchor.clone(), Side::Bottom, Align::End, t::SPACE_6)
            .modal(true, State::close_popover),
    )
}
