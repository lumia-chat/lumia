//! The design system's pieces as Rune elements, the counterpart of LumiaKit's
//! `Components.swift`: avatar, card, field, buttons, notices. Every colour, distance and type
//! style is a token.

use rune::{Cursor, El, Icon, TextEdit, col, el, icon, row, text, text_input};

use crate::app::State;
use crate::tokens::{self as t, Colors, Shadows};

/// The colours and shadows of the window's appearance, handed down the view.
#[derive(Clone, Copy)]
pub struct Look {
    pub c: &'static Colors,
    pub shadows: &'static Shadows,
    pub dark: bool,
}

impl Look {
    pub fn of(dark: bool) -> Self {
        Self {
            c: t::colors(dark),
            shadows: t::shadows(dark),
            dark,
        }
    }
}

/// Up to two initials of a name, as the other clients draw them.
pub fn initials(name: &str) -> String {
    let letters: String = name
        .split_whitespace()
        .take(2)
        .filter_map(|word| word.chars().next())
        .flat_map(char::to_uppercase)
        .collect();
    if letters.is_empty() {
        "?".into()
    } else {
        letters
    }
}

/// A person's avatar: their initials on the tone their id hashes to.
pub fn avatar(name: &str, id: &str, size: f32, look: Look) -> El {
    let (ground, ink) = t::tone(id, look.c);
    let style = if size >= t::AVATAR_MD {
        t::TEXT_AUTHOR
    } else {
        t::TEXT_TAG
    };
    el().size(size, size)
        .radius(size / 2.0)
        .fill(ground)
        .center()
        .a11y_hidden()
        .child(text(initials(name)).font(style).color(ink))
}

fn shadowed(mut element: El, layers: &[rune::Shadow]) -> El {
    for layer in layers {
        element = element.box_shadow(*layer);
    }
    element
}

/// The sign-in card: a title, a line under it, then the form.
pub fn card(title: &str, subtitle: &str, look: Look) -> El {
    let c = look.c;
    let heading = col()
        .gap(t::SPACE_4)
        .child(text(title).font(t::TITLE_AUTH).color(c.ink).role("heading"))
        .child(
            text(subtitle)
                .font(t::TEXT_SUBTITLE)
                .color(c.ink_secondary)
                .wrap(),
        );
    shadowed(
        col()
            .w(t::AUTH_WIDTH)
            .padding(t::SPACE_24)
            .gap(t::SPACE_16)
            .radius(t::RADIUS_16)
            .fill(c.surface)
            .border(1.0, c.panel_edge),
        look.shadows.card,
    )
    .child(heading)
}

/// A labelled single-line field bound to a `TextEdit` in the state.
pub fn field(
    key: &'static str,
    label: &str,
    edit: &TextEdit,
    placeholder: &str,
    access: impl for<'a> Fn(&'a mut State) -> &'a mut TextEdit + 'static,
    submit: fn(&mut State, &mut rune::Tx),
    look: Look,
) -> El {
    let c = look.c;
    let input = text_input(edit, t::TEXT_BODY)
        .key(key)
        .single_line()
        .label(label)
        .placeholder(placeholder, c.ink_muted)
        .color(c.ink_field)
        .caret(c.accent, c.accent_soft)
        .w_full()
        .on_edit(access)
        .on_submit(submit);
    let frame = row()
        .w_full()
        .h(t::CONTROL_HEIGHT)
        .px(t::SPACE_12)
        .items_center()
        .radius(t::RADIUS_10)
        .fill(c.field)
        .border(1.0, c.border_form)
        .focus_within_ring(2.0, 0.0, c.border_focus)
        .child(input);
    col()
        .w_full()
        .gap(t::SPACE_6)
        .child(
            text(label)
                .font(t::TEXT_LABEL)
                .color(c.ink_body)
                .a11y_hidden(),
        )
        .child(frame)
}

/// The primary action of a card: the accent fill, full width.
pub fn primary_button(
    key: &'static str,
    label: &str,
    enabled: bool,
    press: fn(&mut State),
    look: Look,
) -> El {
    let c = look.c;
    let mut button = row()
        .key(key)
        .w_full()
        .h(t::CONTROL_HEIGHT)
        .center()
        .radius(t::RADIUS_12)
        .fill(c.accent)
        .role("button")
        .label(label)
        .disabled(!enabled)
        .child(text(label).font(t::TEXT_BUTTON).color(c.on_accent));
    if enabled {
        button = shadowed(button, look.shadows.primary)
            .hover(c.accent_hover)
            .cursor(Cursor::Pointer)
            .focusable(true)
            .focus_ring(2.0, 2.0, c.focus)
            .on_press(press);
    } else {
        button = button.opacity(0.5);
    }
    button
}

/// A quiet text button: a link in the accent.
pub fn link_button(key: &'static str, label: &str, press: fn(&mut State), look: Look) -> El {
    let c = look.c;
    el().key(key)
        .role("button")
        .label(label)
        .cursor(Cursor::Pointer)
        .focusable(true)
        .focus_ring(2.0, 2.0, c.focus)
        .radius(t::RADIUS_5)
        .on_press(press)
        .child(text(label).font(t::TEXT_LINK_SM).color(c.link))
}

/// What went wrong, under the field it concerns.
pub fn error_text(message: &str, look: Look) -> El {
    text(message)
        .font(t::TEXT_META)
        .color(look.c.dnd)
        .wrap()
        .w_full()
        .role("alert")
}

/// A centred empty state: a glyph, a title, a line of what to do.
pub fn empty_state(glyph: &'static Icon, title: &str, body: &str, look: Look) -> El {
    let c = look.c;
    col()
        .w_full()
        .grow(1.0)
        .center()
        .gap(t::SPACE_8)
        .padding(t::SPACE_24)
        .child(
            el().size(t::TILE_SIZE, t::TILE_SIZE)
                .radius(t::RADIUS_14)
                .fill(c.inset)
                .center()
                .child(icon(glyph, 20.0).color(c.ink_quiet)),
        )
        .child(text(title).font(t::TEXT_AUTHOR).color(c.ink))
        .child(
            text(body)
                .font(t::TEXT_COMPACT)
                .color(c.ink_secondary)
                .wrap()
                .max_w(t::AUTH_WIDTH),
        )
}

/// A floating panel on the app ground: the conversation, the members.
pub fn panel(key: &'static str, look: Look) -> El {
    shadowed(
        col()
            .key(key)
            .h_full()
            .radius(t::RADIUS_18)
            .fill(look.c.surface)
            .border(1.0, look.c.panel_edge),
        look.shadows.panel,
    )
    .clip_rounded(t::RADIUS_18)
}
