//! The design system's pieces as Rune elements, the counterpart of LumiaKit's
//! `Components.swift`: avatar, card, field, buttons, notices. Every colour, distance and type
//! style is a token.
//!
//! The metrics are the design canvas's: the sign-in boards for the card, the field and the
//! buttons, the Quiet Split for the rest. A state a fill cannot say (a border or a shadow
//! that changes under the pointer) is a [`hover_layer`]. Two things the boards do are left
//! out: a rail tile lifting a point on hover, and the primary button darkening by a filter
//! in the profile card (it takes `accent-hover`, as the sign-in boards' does). A control
//! that cannot be used is drawn at half strength; the boards draw no disabled state.

use rune::{
    Align, Color, Cursor, El, Icon, Side, TextEdit, Tx, col, el, icon, row, text, text_input,
};

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
    el().size(size, size)
        .radius(size / 2.0)
        .fill(ground)
        .center()
        .a11y_hidden()
        .child(text(initials(name)).font(t::initials(size)).color(ink))
}

/// The status dot at the foot of an avatar `size` across: it overhangs the avatar's corner,
/// ringed in the colour of what the avatar sits on. A sibling of the avatar in its box.
pub fn status_dot(size: f32, color: Color, ring: Color) -> El {
    let at = size - t::STATUS_DOT + t::SPACE_2;
    el().absolute(at, at)
        .size(t::STATUS_DOT, t::STATUS_DOT)
        .radius(t::STATUS_DOT / 2.0)
        .fill(color)
        .border(2.0, ring)
}

/// A small tag beside a name, in the accent's tint: "You" in a member's row.
pub fn tag(words: &str, look: Look) -> El {
    el().px(t::SPACE_6)
        .py(t::SPACE_2)
        .radius(t::RADIUS_5)
        .fill(look.c.accent_soft)
        .child(text(words).font(t::TEXT_TAG_SMALL).color(look.c.accent_ink))
}

pub fn shadowed(mut element: El, layers: &[rune::Shadow]) -> El {
    for layer in layers {
        element = element.box_shadow(*layer);
    }
    element
}

/// What a control shows only under the pointer and a fill cannot say (another border,
/// another shadow): a box over the whole of it, to be given that look. The control is its
/// `hover_group`.
pub fn hover_layer(radius: f32) -> El {
    el().pin(Some(0.0), Some(0.0), Some(0.0), Some(0.0))
        .radius(radius)
        .group_opacity(0.0, 1.0)
        .a11y_hidden()
}

/// The sign-in card: a title, a line under it, then the form.
pub fn card(title: &str, subtitle: &str, look: Look) -> El {
    let c = look.c;
    let heading = col()
        .gap(t::SPACE_6)
        .child(text(title).font(t::TITLE_AUTH).color(c.ink).role("heading"))
        .child(
            text(subtitle)
                .font(t::TEXT_SUBTITLE)
                .color(c.ink_muted)
                .wrap(),
        );
    shadowed(
        col()
            .w(t::AUTH_WIDTH)
            .padding(t::SPACE_28)
            .gap(t::SPACE_18)
            .radius(t::RADIUS_22)
            .fill(c.surface)
            .border(1.0, c.panel_edge),
        look.shadows.card,
    )
    .child(heading)
}

/// A labelled single-line field bound to a `TextEdit` in the state.
pub fn field(
    key: impl Into<String>,
    label: &str,
    edit: &TextEdit,
    placeholder: &str,
    access: impl for<'a> Fn(&'a mut State) -> &'a mut TextEdit + 'static,
    submit: impl Fn(&mut State, &mut rune::Tx) + 'static,
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
    // Focused, the border takes `border-focus` and the `ring-field` glow goes round it. An
    // element draws one ring, so the border's is on a box that lies over the frame's border.
    let inner = row()
        .w_full()
        .h_full()
        .px(t::SPACE_14)
        .items_center()
        .radius(t::RADIUS_12)
        .focus_within_ring(1.0, -1.0, c.border_focus)
        .child(input);
    let mut frame = row()
        .w_full()
        .h(t::CONTROL_HEIGHT)
        .radius(t::RADIUS_12)
        .fill(c.field)
        .border(1.0, c.border_form)
        .child(inner);
    if let Some(glow) = look.shadows.ring_field.first() {
        frame = frame.focus_within_ring(glow.spread, 0.0, glow.color);
    }
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
    key: impl Into<String>,
    label: &str,
    enabled: bool,
    press: impl Fn(&mut State) + 'static,
    look: Look,
) -> El {
    let c = look.c;
    let mut button = row()
        .key(key)
        .w_full()
        .h(t::CONTROL_PRIMARY)
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

/// The one thing to do next in a pane or a row: the accent fill, as wide as its words, at
/// the height of the profile card's primary button on the Quiet Split.
pub fn action_button(
    key: impl Into<String>,
    label: &str,
    enabled: bool,
    press: impl Fn(&mut State) + 'static,
    look: Look,
) -> El {
    let c = look.c;
    let button = row()
        .key(key)
        .h(t::CONTROL_PANE)
        .px(t::SPACE_18)
        .center()
        .radius(t::RADIUS_12)
        .fill(c.accent)
        .role("button")
        .label(label)
        .disabled(!enabled)
        .child(text(label).font(t::TEXT_BUTTON_PANE).color(c.on_accent));
    if enabled {
        button
            .hover(c.accent_hover)
            .cursor(Cursor::Pointer)
            .focusable(true)
            .focus_ring(2.0, 2.0, c.focus)
            .on_press(press)
    } else {
        button.opacity(0.5)
    }
}

/// A quiet text button: a link in the accent.
pub fn link_button(
    key: impl Into<String>,
    label: &str,
    press: impl Fn(&mut State) + 'static,
    look: Look,
) -> El {
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
                .child(icon(glyph, t::SPACE_18).color(c.ink_quiet)),
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

/// An outlined button's size: the boards draw three.
#[derive(Clone, Copy)]
enum Outlined {
    /// The sign-in card's: `control-height`, as wide as the card, a glyph before its label.
    Card,
    /// Beside a pane's primary action: 40 high, as the one in the Quiet Split's profile card.
    Pane,
    /// In a row or a notice: 38 high at `radius-10`, as "View all members" in the side panel.
    Small,
}

fn outlined(
    key: impl Into<String>,
    glyph: Option<&'static Icon>,
    label: &str,
    enabled: bool,
    size: Outlined,
    press: impl Fn(&mut State, &mut Tx) + 'static,
    look: Look,
) -> El {
    let c = look.c;
    let (height, pad, radius, edge, style, ink, hover) = match size {
        Outlined::Card | Outlined::Pane => (
            if matches!(size, Outlined::Card) {
                t::CONTROL_HEIGHT
            } else {
                t::CONTROL_PANE
            },
            t::SPACE_18,
            t::RADIUS_12,
            c.border_form,
            t::TEXT_BUTTON_SECONDARY,
            c.ink_field,
            c.hover_secondary,
        ),
        Outlined::Small => (
            t::CONTROL_SMALL,
            t::SPACE_14,
            t::RADIUS_10,
            c.border,
            t::TEXT_LABEL,
            c.ink_body,
            c.hover_icon,
        ),
    };
    let mut button = row()
        .key(key)
        .h(height)
        .px(pad)
        .gap(t::SPACE_10)
        .center()
        .radius(radius)
        .fill(c.surface)
        .border(1.0, edge)
        .role("button")
        .label(label)
        .disabled(!enabled);
    if matches!(size, Outlined::Card) {
        button = button.w_full();
    }
    if let Some(glyph) = glyph {
        button = button.child(icon(glyph, t::SPACE_18).color(ink));
    }
    // Under the pointer the label of a small one turns `ink-field`, and the border of the
    // others `border-hover-form`: a second border over the first, shown with the hover.
    let mut words = text(label).font(style).color(ink);
    if matches!(size, Outlined::Small) {
        words = words.group_color(c.ink_field);
    }
    button = button.child(words);
    if enabled {
        if !matches!(size, Outlined::Small) {
            button = button.child(hover_layer(radius).border(1.0, c.border_hover_form));
        }
        button
            .hover(hover)
            .hover_group()
            .cursor(Cursor::Pointer)
            .focusable(true)
            .focus_ring(2.0, 2.0, c.focus)
            .on_press_tx(press)
    } else {
        button.opacity(0.5)
    }
}

/// A secondary way through a card: outlined, as wide as the card, a glyph before its label.
pub fn wide_button(
    key: impl Into<String>,
    glyph: Option<&'static Icon>,
    label: &str,
    enabled: bool,
    press: impl Fn(&mut State, &mut Tx) + 'static,
    look: Look,
) -> El {
    outlined(key, glyph, label, enabled, Outlined::Card, press, look)
}

/// The second action beside a pane's primary one: an outlined button, as wide as its label.
pub fn secondary_button(
    key: impl Into<String>,
    label: &str,
    enabled: bool,
    press: impl Fn(&mut State) + 'static,
    look: Look,
) -> El {
    let press = move |s: &mut State, _: &mut Tx| press(s);
    outlined(key, None, label, enabled, Outlined::Pane, press, look)
}

/// A small outlined button, for a row or a notice.
pub fn small_button(
    key: impl Into<String>,
    label: &str,
    enabled: bool,
    press: impl Fn(&mut State, &mut Tx) + 'static,
    look: Look,
) -> El {
    outlined(key, None, label, enabled, Outlined::Small, press, look)
}

/// A button that is only a glyph, with a name for assistive technology and a tooltip.
pub fn icon_button(
    key: impl Into<String>,
    glyph: &'static Icon,
    label: &str,
    press: impl Fn(&mut State) + 'static,
    look: Look,
) -> El {
    let c = look.c;
    el().key(key)
        .size(t::ICON_BUTTON, t::ICON_BUTTON)
        .radius(t::RADIUS_9)
        .center()
        .role("button")
        .label(label)
        .help(label)
        .hover(c.hover_icon)
        .hover_group()
        .cursor(Cursor::Pointer)
        .focusable(true)
        .focus_ring(2.0, 0.0, c.focus)
        .no_window_drag()
        .on_press(press)
        .child(
            icon(glyph, t::SPACE_16)
                .color(c.ink_quiet)
                .group_color(c.ink_field),
        )
}

/// The small heading over a group of rows. `top` is the room above it: 4 in the sidebar, 8
/// over a side panel's first group and 14 over its later ones.
pub fn section_label(label: impl Into<String>, top: f32, look: Look) -> El {
    text(label)
        .font(t::TEXT_SECTION)
        .color(look.c.ink_muted)
        .px(t::SPACE_10)
        .pt(top)
        .pb(t::SPACE_6)
}

/// The rule after a title, before the quieter line that says what the place is about.
pub fn title_rule(look: Look) -> El {
    el().w(1.0)
        .h(t::HEADER_GLYPH)
        .mx(t::SPACE_6)
        .fill(look.c.line_strong)
}

/// The quieter line beside a title: a channel's topic, what a page is about.
pub fn topic(words: &str, look: Look) -> El {
    text(words)
        .font(t::TEXT_COMPACT)
        .color(look.c.ink_muted)
        .truncate()
        .grow(1.0)
        .min_w(0.0)
}

/// A quiet note with a glyph: something to know, not something wrong.
pub fn notice(glyph: &'static Icon, words: impl Into<String>, look: Look) -> El {
    let c = look.c;
    row()
        .w_full()
        .px(t::SPACE_14)
        .py(t::SPACE_10)
        .gap(t::SPACE_10)
        .radius(t::RADIUS_12)
        .fill(c.inset)
        .child(icon(glyph, t::SPACE_16).color(c.ink_quiet))
        .child(
            text(words)
                .font(t::TEXT_COMPACT)
                .color(c.ink_secondary)
                .wrap()
                .grow(1.0)
                .min_w(0.0),
        )
}

/// A row of choices where one is chosen, as the design's segmented tabs: `options` are
/// `(value, label)`, `pick` gets the value pressed.
pub fn segmented<V: Copy + PartialEq + 'static>(
    key: &str,
    label: &str,
    options: &[(V, &str)],
    selected: V,
    pick: impl Fn(&mut State, V) + Clone + 'static,
    look: Look,
) -> El {
    let c = look.c;
    let mut track = row()
        .key(key)
        .w_full()
        .padding(t::SPACE_4)
        .gap(t::SPACE_4)
        .radius(t::RADIUS_12)
        .fill(c.track)
        .role("radiogroup")
        .label(label);
    for (i, (value, name)) in options.iter().enumerate() {
        let (value, on) = (*value, *value == selected);
        let pick = pick.clone();
        let mut segment = row()
            .key(format!("{key}-{i}"))
            .grow(1.0)
            .h(t::SEGMENT_HEIGHT)
            .center()
            .radius(t::RADIUS_9)
            .role("radio")
            .label(*name)
            .checked(on)
            .cursor(Cursor::Pointer)
            .focusable(true)
            .focus_ring(2.0, 0.0, c.focus)
            .on_press(move |s: &mut State| pick(s, value))
            .child(
                text(*name)
                    .font(t::TEXT_TAB)
                    .color(if on { c.ink } else { c.ink_quiet }),
            );
        if on {
            segment = shadowed(segment.fill(c.tab_active), look.shadows.tab);
        }
        track = track.child(segment);
    }
    track
}

/// A surface floating beside the control keyed `anchor`, with a title, a line under it and
/// whatever `content` adds. It traps focus, and Escape or a press outside closes it. Return
/// it from `ui::overlay` so it is mounted at the root.
///
/// Its metrics are the profile card's on the Quiet Split, the boards' one dialog. Its shadow
/// is `shadow-popover`: the card's own, deeper one has no token.
pub fn popover(
    key: &'static str,
    anchor: &str,
    title: &str,
    subtitle: &str,
    width: f32,
    look: Look,
) -> El {
    let c = look.c;
    shadowed(
        col()
            .key(key)
            .w(width)
            .padding(t::SPACE_18)
            .gap(t::SPACE_14)
            .radius(t::RADIUS_18)
            .fill(c.surface)
            .border(1.0, c.line_strong),
        look.shadows.popover,
    )
    .anchored_fit(anchor, Side::Bottom, Align::Start, t::SPACE_6)
    .modal(true, State::close_popover)
    .role("dialog")
    .label(title)
    .child(
        col()
            .gap(t::SPACE_4)
            .child(
                text(title)
                    .font(t::TITLE_SERVER)
                    .color(c.ink)
                    .role("heading"),
            )
            .child(
                text(subtitle)
                    .font(t::TEXT_SUBTITLE)
                    .color(c.ink_muted)
                    .wrap(),
            ),
    )
}
