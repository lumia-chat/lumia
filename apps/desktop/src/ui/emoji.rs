//! The emoji picker (architecture 6.6): a search field, "Frequently used" from this
//! computer's own reactions, and every emoji by Unicode's groups, from the `emojis` crate
//! (fully qualified, default skin tone; a search matches its name and shortcodes), up to
//! [`NEWEST`]: the systems' emoji fonts lag Unicode, and an emoji the font does not have
//! draws as an empty box. Choosing one reacts to the message it was opened for, or, opened
//! from the composer's emoji button, goes in at the caret, the picker staying open for more.
//!
//! In a server's conversation the server's own emoji are a section of their own headed by
//! its name, after "Frequently used" (which may hold them, there) and ahead of Unicode's
//! groups, each its picture at [`CUSTOM_SIZE`] named by its `:name:`; a search lists the
//! server's matches first, then Unicode's, in one grid (the "Emoji picker" board). In a
//! personal conversation there are none (specs/emoji.md 1).
//!
//! Opened from a message it is modal, as every popover is. Opened from the composer it is
//! not: Rune's modal takes the focus and makes the window behind it inert, and the
//! composer's picker keeps the caret in the field while it is open. A press anywhere else
//! lands on a clear layer under it that puts it away, and Escape does from the field or
//! the picker.

use lumia_client::app::EmojiSummary;
use rune::{
    Align, Cursor, El, Key, ScrollSpec, Side, SurfaceFit, col, el, icon, row, text, text_input,
};

use super::components::{Look, shadowed};
use super::scroll::WithScrollBar;
use crate::app::State;
use crate::app::emoji::{COMPOSER_EMOJI, aspect};
use crate::app::pictures::EmojiFace;
use crate::tokens::{self as t, icons};

/// A server's emoji in a picker's cell: the board's 28 points.
const CUSTOM_SIZE: f32 = t::SPACE_28;

/// One of a server's emoji fitted in a square of `size` points, at its shape: its picture
/// when held and decoded (its first frame under Reduce Motion), nothing while it is on its
/// way, and its `:name:`, cut to the square, when the picture could not be had. Named by
/// what holds it, so hidden here.
pub fn picture(s: &State, emoji: &EmojiSummary, size: f32, look: Look) -> El {
    let shape = aspect(emoji);
    let (w, h) = if shape >= 1.0 {
        (size, size / shape)
    } else {
        (size * shape, size)
    };
    let drawn = match s.emoji_face(emoji, look.still) {
        EmojiFace::Picture(image) => el()
            .size(w, h)
            .a11y_hidden()
            .pixels_fit(image.pixels(), SurfaceFit::Fit),
        EmojiFace::Waiting => el().size(w, h).a11y_hidden(),
        EmojiFace::Name => text(format!(":{}:", emoji.name))
            .font(t::TEXT_TAG_SMALL)
            .color(look.c.ink_muted)
            .truncate()
            .max_w(size)
            .a11y_hidden(),
    };
    el().size(size, size).center().a11y_hidden().child(drawn)
}

/// One of a server's emoji drawn at a height of `height` points and as wide as its shape: a
/// reaction chip's. None when its picture could not be had: the chip says its `:name:`.
pub fn picture_tall(s: &State, emoji: &EmojiSummary, height: f32, look: Look) -> Option<El> {
    let w = height * aspect(emoji);
    let drawn = el().size(w, height).a11y_hidden();
    match s.emoji_face(emoji, look.still) {
        EmojiFace::Picture(image) => Some(drawn.pixels_fit(image.pixels(), SurfaceFit::Fit)),
        EmojiFace::Waiting => Some(drawn),
        EmojiFace::Name => None,
    }
}

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
        .on_press_tx(move |s: &mut State, tx| {
            s.pick_emoji(&pick);
            keep_caret(s, tx);
        })
        .child(text(emoji.to_string()).font(t::TITLE_CHANNEL))
}

/// After a choice in the composer's picker, which stays open: the caret is the field's.
fn keep_caret(s: &State, tx: &mut rune::Tx) {
    if s.composer_picker_open() {
        tx.focus("composer", false);
    }
}

/// One of the server's emoji to choose, named by its `:name:`.
fn custom_cell(s: &State, key: String, emoji: &EmojiSummary, look: Look) -> El {
    let c = look.c;
    let name = format!(":{}:", emoji.name);
    let id = emoji.emoji_id.clone();
    el().key(key)
        .size(CELL, CELL)
        .radius(t::RADIUS_8)
        .center()
        .role("button")
        .label(name.clone())
        .help(name)
        .hover(c.hover_icon)
        .cursor(Cursor::Pointer)
        .focusable(true)
        .focus_ring(2.0, 0.0, c.focus)
        .on_press_tx(move |s: &mut State, tx| {
            s.pick_custom(&id);
            keep_caret(s, tx);
        })
        .child(picture(s, emoji, CUSTOM_SIZE, look))
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
                })
                // The composer's is not modal: Escape in its search puts it away here.
                .on_input_key(|s: &mut State, key, tx| {
                    if key.key == Key::Escape && s.composer_picker_open() {
                        s.close_composer_picker(tx);
                        return true;
                    }
                    false
                }),
        );
    let wanted = picker.search.text.trim();
    let server = s.session.emoji_server();
    let mut list = col().w_full().px(t::SPACE_10).pb(t::SPACE_10);
    if wanted.is_empty() {
        let frequent = s
            .frequent_emoji()
            .into_iter()
            .enumerate()
            .map(|(n, e)| match s.frequent_custom(&e) {
                Some(custom) => custom_cell(s, format!("f{n}"), &custom, look),
                None => {
                    let name = emojis::get(&e).map_or(e.clone(), |found| found.name().to_string());
                    cell(format!("f{n}"), &e, &name, look)
                }
            })
            .collect();
        list = list
            .child(heading("Frequently used", look))
            .child(grid("frequent".into(), frequent));
        if let Some(server) = server.filter(|server| !server.emoji.is_empty()) {
            let cells = server
                .emoji
                .iter()
                .map(|e| custom_cell(s, format!("c-{}", e.emoji_id), e, look))
                .collect();
            list = list
                .child(heading(&server.name, look))
                .child(grid("server".into(), cells));
        }
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
        let custom = s.custom_found(wanted);
        if found.is_empty() && custom.is_empty() {
            list = list.child(
                text("No emoji match that.")
                    .font(t::TEXT_META)
                    .color(c.ink_quiet)
                    .padding(t::SPACE_10),
            );
        } else {
            let cells = custom
                .iter()
                .map(|e| custom_cell(s, format!("c-{}", e.emoji_id), e, look))
                .chain(
                    found
                        .iter()
                        .enumerate()
                        .map(|(n, e)| cell(format!("s{n}"), e.as_str(), e.name(), look)),
                )
                .collect();
            list = list
                .child(el().h(t::SPACE_8))
                .child(grid("found".into(), cells));
        }
    }
    let composer = picker.composer;
    let surface = col()
        .key("emoji-picker")
        .w(WIDTH)
        .radius(t::RADIUS_14)
        .fill(c.surface)
        .border(1.0, c.line_strong)
        .role("dialog")
        .label(if composer { "Emoji" } else { "Add reaction" })
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
    if composer {
        // Over the composer, its end on the button's, with the clear layer under it.
        let picker = shadowed(surface, look.shadows.floating).anchored_fit(
            COMPOSER_EMOJI,
            Side::Top,
            Align::End,
            t::SPACE_6,
        );
        let outside = el()
            .key("emoji-picker-outside")
            .pin(Some(0.0), Some(0.0), Some(0.0), Some(0.0))
            .a11y_hidden()
            .on_press_tx(State::close_composer_picker);
        let layer = el()
            .key("emoji-picker-layer")
            .pin(Some(0.0), Some(0.0), Some(0.0), Some(0.0))
            .child(outside)
            .child(picker);
        return Some(layer);
    }
    Some(
        shadowed(surface, look.shadows.floating)
            .anchored_fit(picker.anchor.clone(), Side::Bottom, Align::End, t::SPACE_6)
            .modal(true, State::close_popover),
    )
}

/// The composer's emoji button, at the field's end before Send: the picker, hung from it.
pub fn composer_button(look: Look) -> El {
    super::media::glyph_button(
        COMPOSER_EMOJI,
        &icons::EMOJI,
        "Emoji",
        t::ICON_BUTTON,
        |s: &mut State, _| s.open_composer_picker(),
        look,
    )
}
