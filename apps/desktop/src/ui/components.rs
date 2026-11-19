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

use std::rc::Rc;

use rune::{
    Align, Color, Cursor, El, Icon, Image, Side, Span, SurfaceFit, TextEdit, Tx, col, el, icon,
    row, text, text_input,
};

use lumia_client::app::ServerSummary;

use crate::app::State;
use crate::app::roles::TagTone;
use crate::tokens::{self as t, Colors, Shadows, icons};

/// The colours and shadows of the window's appearance, handed down the view.
#[derive(Clone, Copy)]
pub struct Look {
    pub c: &'static Colors,
    pub shadows: &'static Shadows,
    pub dark: bool,
    /// The system asks for reduced motion: a GIF in a message is drawn still until pressed
    /// (specs/media.md 5).
    pub still: bool,
}

impl Look {
    pub fn of(dark: bool) -> Self {
        Self {
            c: t::colors(dark),
            shadows: t::shadows(dark),
            dark,
            still: false,
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

/// [`avatar`] for someone the state knows: their picture when this computer holds it. The
/// initials stay underneath while it decodes, which paints nothing, and go once it is drawn:
/// a picture with transparency would show them through.
pub fn avatar_of(s: &State, name: &str, id: &str, size: f32, look: Look) -> El {
    match s.picture_of(id) {
        Some(picture) if drawn(picture) => {
            let (ground, _) = t::tone(id, look.c);
            el().size(size, size)
                .radius(size / 2.0)
                .fill(ground)
                .a11y_hidden()
                .child(circled(picture, size))
        }
        Some(picture) => avatar(name, id, size, look).child(circled(picture, size)),
        None => avatar(name, id, size, look),
    }
}

/// Whether a picture has decoded and can stand without initials under it.
pub fn drawn(picture: &Image) -> bool {
    picture.is_ready() && picture.error().is_none()
}

/// A picture as a circle `size` across, over whatever it is a child of.
pub fn circled(picture: &Image, size: f32) -> El {
    el().key("picture")
        .absolute(0.0, 0.0)
        .size(size, size)
        .radius(size / 2.0)
        .pixels_fit(picture.pixels(), SurfaceFit::Fill)
}

/// A server as the rail draws it, `size` across: its initials on the tile's colour, and its
/// picture over them when this computer holds it.
pub fn server_picture(s: &State, server: &ServerSummary, size: f32, ink: Color, look: Look) -> El {
    let tile = el()
        .size(size, size)
        .radius(size / 2.0)
        .fill(look.c.tile)
        .center()
        .a11y_hidden();
    let picture = s.picture_at(&server.icon_url);
    let tile = if picture.is_some_and(drawn) {
        tile
    } else {
        tile.child(
            text(initials(&server.name))
                .font(if size > t::TILE_SIZE {
                    t::initials(size)
                } else {
                    t::TEXT_MONOGRAM_REST
                })
                .color(ink),
        )
    };
    match picture {
        Some(picture) => tile.child(circled(picture, size)),
        None => tile,
    }
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

/// A count on the accent (the design system's badge): `small` in a row of the sidebar and
/// on a quick tile, the larger on the rail. Assistive technology reads the row's label, so
/// the count itself is hidden from it.
pub fn count_badge(count: u32, small: bool, look: Look) -> El {
    let c = look.c;
    let size = if small { t::BADGE_SMALL } else { t::BADGE };
    el().h(size)
        .min_w(size)
        .px(t::SPACE_4)
        .radius(t::RADIUS_PILL)
        .fill(c.accent)
        .center()
        .a11y_hidden()
        .child(
            text(if count > 99 {
                "99+".to_string()
            } else {
                count.to_string()
            })
            .font(t::TEXT_COUNT)
            .color(c.on_accent),
        )
}

/// A small tag beside a name, in the accent's tint: "You" in a member's row.
pub fn tag(words: &str, look: Look) -> El {
    el().px(t::SPACE_6)
        .py(t::SPACE_2)
        .radius(t::RADIUS_5)
        .fill(look.c.accent_soft)
        .child(text(words).font(t::TEXT_TAG_SMALL).color(look.c.accent_ink))
}

/// The larger accent tag (the design system's `lu-tag--md`): "You", "Owner" or "Guest" beside
/// the name on a profile card.
pub fn mark(words: &str, look: Look) -> El {
    el().px(t::TAG_PAD)
        .py(t::SPACE_2)
        .radius(t::RADIUS_6)
        .fill(look.c.accent_soft)
        .child(text(words).font(t::TEXT_TAG).color(look.c.accent_ink))
}

/// The ground and ink a tag is drawn in: a role's tone, or the plain chip for a role with no
/// colour and for a title.
pub fn tag_colors(tone: TagTone, look: Look) -> (Color, Color) {
    match tone {
        TagTone::Tone(name) => t::tone_named(name, look.c),
        TagTone::Neutral => (look.c.chip, look.c.ink_secondary),
    }
}

/// A member's tag beside their name (architecture 7.5): a role on its tone or a title on the
/// plain chip, one line, cut with an ellipsis where its row has no room. `small` is a member
/// row's (`lu-tag--sm`); the other a message's and a card's (`lu-tag--md`).
pub fn role_tag(words: &str, tone: TagTone, small: bool, look: Look) -> El {
    let (ground, ink) = tag_colors(tone, look);
    let chip = el().fill(ground).min_w(0.0);
    let chip = if small {
        chip.px(t::SPACE_6).py(t::SPACE_2 / 2.0).radius(t::RADIUS_5)
    } else {
        chip.px(t::TAG_PAD).py(t::SPACE_2).radius(t::RADIUS_6)
    };
    chip.child(
        text(words)
            .font(if small { t::TEXT_TAG_ROW } else { t::TEXT_TAG })
            .color(ink)
            .truncate(),
    )
}

/// A role's colour as a dot, in the ink its tag's words are written in; a ring with nothing
/// in it for a role with no colour.
pub fn role_dot(tone: Option<&str>, look: Look) -> El {
    let dot = el()
        .size(t::ROLE_DOT, t::ROLE_DOT)
        .radius(t::ROLE_DOT / 2.0)
        .a11y_hidden();
    match tone {
        Some(tone) => dot.fill(t::tone_named(tone, look.c).1),
        None => dot.border(1.5, look.c.ink_faint),
    }
}

/// `element` with a token's shadow layers. The tokens list them as CSS does, the first on
/// top, and Rune paints them in the order given, so they go on last first: the current
/// server's ring is a gap in the rail's colour over a wider ring of the accent.
pub fn shadowed(mut element: El, layers: &[rune::Shadow]) -> El {
    for layer in layers.iter().rev() {
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
    field_with(key, label, edit, placeholder, None, access, submit, look)
}

/// [`field`] for a role's name or a title: what is typed past 32 characters is not kept, as
/// the web's field takes no more.
pub fn field_of_label(
    key: impl Into<String>,
    label: &str,
    edit: &TextEdit,
    placeholder: &str,
    access: impl for<'a> Fn(&'a mut State) -> &'a mut TextEdit + 'static,
    submit: impl Fn(&mut State, &mut rune::Tx) + 'static,
    look: Look,
) -> El {
    let access = Rc::new(access);
    let kept = Rc::clone(&access);
    field_built(
        key,
        label,
        edit,
        placeholder,
        None,
        move |s| access(s),
        submit,
        Some(Box::new(move |s: &mut State| {
            crate::app::role_settings::keep_to_label(kept(s));
        })),
        look,
    )
}

/// The button at the end of a password field that shows what was typed, and hides it
/// again: the board's eye, 38 points at `radius-9`.
pub fn reveal_button(
    key: impl Into<String>,
    revealed: bool,
    press: impl Fn(&mut State) + 'static,
    look: Look,
) -> El {
    let c = look.c;
    let label = if revealed {
        "Hide password"
    } else {
        "Show password"
    };
    el().key(key)
        .size(t::CONTROL_SMALL, t::CONTROL_SMALL)
        .radius(t::RADIUS_9)
        .center()
        .role("button")
        .label(label)
        .help(label)
        .hover(c.hover_icon)
        .cursor(Cursor::Pointer)
        .focusable(true)
        .focus_ring(2.0, 0.0, c.focus)
        .on_press(press)
        .child(
            icon(
                if revealed {
                    &t::icons::EYE_OFF
                } else {
                    &t::icons::EYE
                },
                t::SPACE_18,
            )
            .color(c.ink_quiet),
        )
}

/// [`field`] with a control at its end, inside the frame.
#[allow(clippy::too_many_arguments)]
pub fn field_with(
    key: impl Into<String>,
    label: &str,
    edit: &TextEdit,
    placeholder: &str,
    trail: Option<El>,
    access: impl for<'a> Fn(&'a mut State) -> &'a mut TextEdit + 'static,
    submit: impl Fn(&mut State, &mut rune::Tx) + 'static,
    look: Look,
) -> El {
    field_built(
        key,
        label,
        edit,
        placeholder,
        trail,
        access,
        submit,
        None,
        look,
    )
}

/// What a field does after each change to its words.
type Changed = Option<Box<dyn Fn(&mut State)>>;

#[allow(clippy::too_many_arguments)]
fn field_built(
    key: impl Into<String>,
    label: &str,
    edit: &TextEdit,
    placeholder: &str,
    trail: Option<El>,
    access: impl for<'a> Fn(&'a mut State) -> &'a mut TextEdit + 'static,
    submit: impl Fn(&mut State, &mut rune::Tx) + 'static,
    changed: Changed,
    look: Look,
) -> El {
    let c = look.c;
    let mut input = text_input(edit, t::TEXT_BODY)
        .key(key)
        .single_line()
        .label(label)
        .placeholder(placeholder, c.ink_muted)
        .color(c.ink_field)
        .caret(c.accent, c.accent_soft)
        .w_full()
        .on_edit(access)
        .on_submit(submit);
    if let Some(changed) = changed {
        input = input.on_input_change(move |s: &mut State, _: &mut Tx| changed(s));
    }
    // Focused, the border takes `border-focus` and the `ring-field` glow goes round it. An
    // element draws one ring, so the border's is on a box that lies over the frame's border.
    let mut inner = row()
        .w_full()
        .h_full()
        .pl(t::SPACE_14)
        .pr(if trail.is_some() {
            t::SPACE_4
        } else {
            t::SPACE_14
        })
        .gap(t::SPACE_4)
        .items_center()
        .radius(t::RADIUS_12)
        .focus_within_ring(1.0, -1.0, c.border_focus)
        .child(input.grow(1.0).min_w(0.0));
    if let Some(trail) = trail {
        inner = inner.child(trail);
    }
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
    primary_button_tx(key, label, enabled, move |s, _| press(s), look)
}

/// [`primary_button`] for an action that needs the frame's `Tx` (a file dialog).
pub fn primary_button_tx(
    key: impl Into<String>,
    label: &str,
    enabled: bool,
    press: impl Fn(&mut State, &mut Tx) + 'static,
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
            .on_press_tx(press);
    } else {
        button = button.opacity(0.5);
    }
    button
}

/// A card's primary action when it cannot be undone (starting over): [`primary_button`] in
/// `danger`. The board darkens it under the pointer to `#B91C1C`, which no token names, so
/// it keeps its fill there.
pub fn danger_button(
    key: impl Into<String>,
    label: &str,
    enabled: bool,
    press: impl Fn(&mut State) + 'static,
    look: Look,
) -> El {
    let c = look.c;
    let button = row()
        .key(key)
        .w_full()
        .h(t::CONTROL_PRIMARY)
        .center()
        .radius(t::RADIUS_12)
        .fill(c.danger)
        .role("button")
        .label(label)
        .disabled(!enabled)
        .child(text(label).font(t::TEXT_BUTTON).color(c.on_accent));
    if enabled {
        button
            .cursor(Cursor::Pointer)
            .focusable(true)
            .focus_ring(2.0, 2.0, c.focus)
            .on_press(press)
    } else {
        button.opacity(0.5)
    }
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

/// [`action_button`] for what cannot be undone ("Delete role"): `danger`, 40 high, as wide
/// as its words.
pub fn danger_action(
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
        .fill(c.danger)
        .role("button")
        .label(label)
        .disabled(!enabled)
        .child(text(label).font(t::TEXT_BUTTON_PANE).color(c.on_accent));
    if enabled {
        button
            .cursor(Cursor::Pointer)
            .focusable(true)
            .focus_ring(2.0, 2.0, c.focus)
            .on_press(press)
    } else {
        button.opacity(0.5)
    }
}

/// A pane's outlined button in the danger ink ("Kick"): `danger` words in a `danger-line`
/// edge, on `danger-soft` under the pointer.
pub fn danger_outline(
    key: impl Into<String>,
    words: &str,
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
        .fill(c.surface)
        .border(1.0, c.danger_line)
        .role("button")
        .label(label)
        .disabled(!enabled)
        .child(text(words).font(t::TEXT_BUTTON_SECONDARY).color(c.danger));
    if enabled {
        button
            .child(hover_layer(t::RADIUS_12).border(1.0, c.danger))
            .hover(c.danger_soft)
            .hover_group()
            .cursor(Cursor::Pointer)
            .focusable(true)
            .focus_ring(2.0, 2.0, c.focus)
            .on_press(press)
    } else {
        button.opacity(0.5)
    }
}

/// The design system's text button: its words underlined in the secondary ink, beside what
/// it changes ("Edit" beside a card's title). `label` names it for assistive technology.
pub fn text_button(
    key: impl Into<String>,
    words: &str,
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
        .radius(t::RADIUS_6)
        .on_press(press)
        .child(
            text(words)
                .font(t::TEXT_LINK_SM)
                .color(c.ink_secondary)
                .spans([Span::new(0, words.len()).underline(c.underline)])
                .a11y_hidden(),
        )
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
        button = if matches!(size, Outlined::Small) {
            button
                .gap(t::SPACE_8)
                .child(icon(glyph, t::SPACE_16).color(ink).group_color(c.ink_field))
        } else {
            button.child(icon(glyph, t::SPACE_18).color(ink))
        };
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

/// [`small_button`] with a glyph before its words: "New role", "Add members".
pub fn small_button_with(
    key: impl Into<String>,
    glyph: &'static Icon,
    label: &str,
    enabled: bool,
    press: impl Fn(&mut State, &mut Tx) + 'static,
    look: Look,
) -> El {
    outlined(
        key,
        Some(glyph),
        label,
        enabled,
        Outlined::Small,
        press,
        look,
    )
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

/// A menu and a profile card are this wide on the boards (260 and 300); the design system
/// has no token for either.
pub const MENU_WIDTH: f32 = 260.0;
pub const CARD_WIDTH: f32 = 300.0;
/// The @ list over the composer: a card's width and a card avatar's more (372), room for a
/// name, a handle and "Not in #channel" on one line. The web's list is as wide.
pub const AVATAR_CARD_WIDTH: f32 = CARD_WIDTH + t::AVATAR_CARD;

/// A menu floating beside the control keyed `anchor`, on the board's account menu's metrics.
/// It traps focus, and Escape or a press outside closes it. Fill it with [`menu_item`]s and
/// return it from `ui::overlay`.
pub fn menu(
    key: &'static str,
    anchor: &str,
    label: &str,
    side: Side,
    align: Align,
    look: Look,
) -> El {
    let c = look.c;
    shadowed(
        col()
            .key(key)
            .w(MENU_WIDTH)
            .padding(t::SPACE_6)
            .gap(t::SPACE_2)
            .radius(t::RADIUS_14)
            .fill(c.surface)
            .border(1.0, c.line_strong),
        look.shadows.floating,
    )
    .anchored_fit(anchor, side, align, t::SPACE_6)
    .modal(true, State::close_popover)
    .role("menu")
    .label(label)
}

/// A row of a [`menu`]: a glyph or whatever `lead` is, the words, and `trail` at the end.
pub fn menu_item(
    key: impl Into<String>,
    lead: Option<El>,
    label: &str,
    trail: Option<El>,
    press: impl Fn(&mut State) + 'static,
    look: Look,
) -> El {
    let c = look.c;
    let mut item = row()
        .key(key)
        .w_full()
        .h(t::MENU_ITEM_HEIGHT)
        .px(t::SPACE_10)
        .gap(t::SPACE_10)
        .items_center()
        .radius(t::RADIUS_8)
        .role("menuitem")
        .label(label)
        .hover(c.hover_member)
        .cursor(Cursor::Pointer)
        .focusable(true)
        .focus_ring(2.0, 0.0, c.focus)
        .on_press(press);
    if let Some(lead) = lead {
        item = item.child(lead);
    }
    item = item.child(
        text(label)
            .font(t::TEXT_MENU)
            .color(c.ink_body)
            .truncate()
            .grow(1.0)
            .min_w(0.0),
    );
    match trail {
        Some(trail) => item.child(trail),
        None => item,
    }
}

/// The glyph a [`menu_item`] leads with.
pub fn menu_glyph(glyph: &'static Icon, look: Look) -> El {
    icon(glyph, t::SPACE_16).color(look.c.ink_body)
}

/// The rule between a [`menu`]'s groups.
pub fn menu_rule(look: Look) -> El {
    el().h(1.0)
        .mx(t::SPACE_6)
        .my(t::SPACE_2)
        .fill(look.c.line)
        .a11y_hidden()
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

/// A notice in a tone (`t::secure`, `t::caution`): what kind of conversation this is, said so
/// it is seen. The quiet notice's measures, on the tone's ground with its edge.
pub fn toned_notice(glyph: &'static Icon, words: impl Into<String>, tone: t::Caution) -> El {
    row()
        .w_full()
        .px(t::SPACE_14)
        .py(t::SPACE_10)
        .gap(t::SPACE_10)
        .radius(t::RADIUS_12)
        .fill(tone.fill)
        .border(1.0, tone.edge)
        .child(icon(glyph, t::SPACE_16).color(tone.glyph))
        .child(
            text(words)
                .font(t::TEXT_COMPACT)
                .color(tone.ink)
                .wrap()
                .grow(1.0)
                .min_w(0.0),
        )
}

/// A choice that is on or off: a box that holds a tick when it is on, the words beside it,
/// and a quieter line under them when there is one. The whole row toggles it.
pub fn check_row(
    key: impl Into<String>,
    label: &str,
    caption: Option<&str>,
    checked: bool,
    press: impl Fn(&mut State) + 'static,
    look: Look,
) -> El {
    let c = look.c;
    let mut tick = el()
        .size(t::SPACE_18, t::SPACE_18)
        .radius(t::RADIUS_5)
        .center()
        .a11y_hidden();
    tick = if checked {
        tick.fill(c.accent)
            .child(icon(&icons::CHECK, t::SPACE_14).color(c.on_accent))
    } else {
        tick.fill(c.field).border(1.0, c.border_form)
    };
    // The box sits on the label's line; the caption goes under the label, not the box.
    let line = row()
        .w_full()
        .gap(t::SPACE_10)
        .items_center()
        .child(tick)
        .child(
            text(label)
                .font(t::TEXT_MEMBER)
                .color(c.ink)
                .wrap()
                .grow(1.0)
                .min_w(0.0),
        );
    let mut whole = col()
        .key(key)
        .w_full()
        .gap(t::SPACE_2)
        .px(t::SPACE_6)
        .py(t::SPACE_6)
        .radius(t::RADIUS_8)
        .hover(c.hover_member)
        .role("checkbox")
        .label(label)
        .checked(checked)
        .cursor(Cursor::Pointer)
        .focusable(true)
        .focus_ring(2.0, 0.0, c.focus)
        .on_press(press)
        .child(line);
    if let Some(caption) = caption {
        whole = whole.child(
            text(caption)
                .font(t::TEXT_META)
                .color(c.ink_secondary)
                .wrap()
                .w_full()
                .pl(t::SPACE_18 + t::SPACE_10),
        );
    }
    whole
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
    segments(key, label, options, selected, pick, Fit::Full, look)
}

/// [`segmented`] as wide as its words, at the end of a setting's row (the Preferences
/// boards): 30 tall, 12 to each side of a segment's words, or `width` across each.
pub fn segmented_compact<V: Copy + PartialEq + 'static>(
    key: &str,
    label: &str,
    options: &[(V, &str)],
    selected: V,
    pick: impl Fn(&mut State, V) + Clone + 'static,
    width: Option<f32>,
    look: Look,
) -> El {
    segments(key, label, options, selected, pick, Fit::Words(width), look)
}

/// How wide a segmented control is: its row's width shared out, or its words'.
#[derive(Clone, Copy)]
enum Fit {
    Full,
    Words(Option<f32>),
}

fn segments<V: Copy + PartialEq + 'static>(
    key: &str,
    label: &str,
    options: &[(V, &str)],
    selected: V,
    pick: impl Fn(&mut State, V) + Clone + 'static,
    fit: Fit,
    look: Look,
) -> El {
    let c = look.c;
    let mut track = row().key(key).fill(c.track).role("radiogroup").label(label);
    track = match fit {
        Fit::Full => track
            .w_full()
            .padding(t::SPACE_4)
            .gap(t::SPACE_4)
            .radius(t::RADIUS_12),
        Fit::Words(_) => track
            .padding(t::SPACE_2 + 1.0)
            .gap(t::SPACE_2)
            .radius(t::RADIUS_10),
    };
    for (i, (value, name)) in options.iter().enumerate() {
        let (value, on) = (*value, *value == selected);
        let pick = pick.clone();
        let mut segment = row()
            .key(format!("{key}-{i}"))
            .center()
            .role("radio")
            .label(*name)
            .checked(on)
            .cursor(Cursor::Pointer)
            .focusable(true)
            .focus_ring(2.0, 0.0, c.focus)
            .on_press(move |s: &mut State| pick(s, value));
        segment = match fit {
            Fit::Full => segment
                .grow(1.0)
                .h(t::SEGMENT_HEIGHT)
                .radius(t::RADIUS_9)
                .child(
                    text(*name)
                        .font(t::TEXT_TAB)
                        .color(if on { c.ink } else { c.ink_quiet }),
                ),
            Fit::Words(width) => {
                let segment = segment.h(t::SEGMENT_COMPACT).radius(t::RADIUS_8);
                let segment = match width {
                    Some(width) => segment.w(width),
                    None => segment.px(t::SPACE_12),
                };
                segment.hover_group().child(
                    text(*name)
                        .font(t::TEXT_LINK_SM)
                        .color(if on { c.ink } else { c.ink_muted })
                        .group_color(c.ink),
                )
            }
        };
        if on {
            segment = shadowed(segment.fill(c.tab_active), look.shadows.tab);
        }
        track = track.child(segment);
    }
    track
}

/// A choice that is on or off, as a switch: the accent's track with the knob at its end when
/// on, a grey one with the knob at its start when off. `label` names it for assistive
/// technology; the words beside it are the caller's.
pub fn switch(
    key: impl Into<String>,
    label: &str,
    on: bool,
    press: impl Fn(&mut State) + 'static,
    look: Look,
) -> El {
    switch_where(key, label, on, true, press, look)
}

/// [`switch`], or one that is shown and not this account's to turn (`enabled` false): at
/// half strength, taking no press.
pub fn switch_where(
    key: impl Into<String>,
    label: &str,
    on: bool,
    enabled: bool,
    press: impl Fn(&mut State) + 'static,
    look: Look,
) -> El {
    let c = look.c;
    // The knob is near white on either ground, as a system's switch's is.
    let knob = if look.dark { c.ink } else { c.surface };
    let track = row()
        .key(key)
        .size(t::SWITCH_WIDTH, t::SWITCH_HEIGHT)
        .padding((t::SWITCH_HEIGHT - t::SWITCH_KNOB) / 2.0)
        .radius(t::RADIUS_PILL)
        .fill(if on { c.accent } else { c.border_hover })
        .role("switch")
        .label(label)
        .checked(on);
    let track = if enabled {
        track
            .cursor(Cursor::Pointer)
            .focusable(true)
            .focus_ring(2.0, 2.0, c.focus)
            .on_press(press)
    } else {
        track.disabled(true).opacity(0.5)
    };
    let track = if on {
        track.child(el().grow(1.0))
    } else {
        track
    };
    track.child(shadowed(
        el().size(t::SWITCH_KNOB, t::SWITCH_KNOB)
            .radius(t::RADIUS_PILL)
            .fill(knob)
            .a11y_hidden(),
        look.shadows.tab,
    ))
}

/// One of a few choices drawn as pictures (the Preferences boards' themes and presets): its
/// `sketch` on a box 92 tall, ringed in the accent when it is the one chosen and outlined
/// otherwise, its name under it with a tick when chosen, and `caption` under that.
pub fn picture_choice(
    key: impl Into<String>,
    name: &str,
    caption: Option<&str>,
    sketch: El,
    on: bool,
    press: impl Fn(&mut State) + 'static,
    look: Look,
) -> El {
    let c = look.c;
    let inner = t::RADIUS_12;
    let mut picture = sketch
        .w_full()
        .h(t::CHOICE_SKETCH)
        .radius(inner)
        .clip_rounded(inner);
    if !on {
        picture = picture.border(1.0, c.line_strong);
    }
    // The ring stands off the picture by its own width: a frame as thick again, in the
    // panel's fill, between the two.
    let ring = el()
        .w_full()
        .padding(t::CHOICE_RING * 2.0)
        .radius(inner + t::CHOICE_RING * 2.0)
        .border(
            t::CHOICE_RING,
            if on {
                c.accent
            } else {
                rune::Color::TRANSPARENT
            },
        )
        .child(picture);
    let mut named =
        row()
            .gap(t::SPACE_6)
            .items_center()
            .child(
                text(name)
                    .font(t::TEXT_MEMBER)
                    .color(if on { c.ink } else { c.ink_body }),
            );
    if on {
        named = named.child(icon(&icons::CHECK, t::SPACE_14).color(c.accent_ink));
    }
    let mut words = col()
        .w_full()
        .gap(t::SPACE_2)
        .px(t::CHOICE_RING * 2.0)
        .child(named);
    if let Some(caption) = caption {
        words = words.child(text(caption).font(t::TEXT_META).color(c.ink_muted).wrap());
    }
    col()
        .key(key)
        .grow(1.0)
        .min_w(0.0)
        .gap(t::SPACE_6)
        .radius(inner + t::CHOICE_RING * 2.0)
        .role("radio")
        .label(name)
        .checked(on)
        .cursor(Cursor::Pointer)
        .focusable(true)
        .focus_ring(2.0, 2.0, c.focus)
        .on_press(press)
        .child(ring)
        .child(words)
}

/// Settings' rows in an outlined box, a rule between each two (the design system's
/// `SettingsGroup`).
pub fn settings_group(key: impl Into<String>, rows: Vec<El>, look: Look) -> El {
    let c = look.c;
    let mut group = col()
        .key(key)
        .w_full()
        .radius(t::RADIUS_14)
        .border(1.0, c.line_strong);
    for (i, setting) in rows.into_iter().enumerate() {
        group = group.child(if i == 0 {
            setting
        } else {
            setting.border_sides([1.0, 0.0, 0.0, 0.0], c.line)
        });
    }
    group
}

/// A setting's row: its name over a line on what it does, and its control at the end. The
/// control says its name to assistive technology itself.
pub fn setting_row(
    key: impl Into<String>,
    name: &str,
    caption: &str,
    control: El,
    look: Look,
) -> El {
    let c = look.c;
    let (strong, quiet) = (c.ink, c.ink_muted);
    row()
        .key(key)
        .w_full()
        .px(t::SPACE_16)
        .py(t::SPACE_14)
        .gap(t::SPACE_16)
        .items_center()
        .child(
            col()
                .grow(1.0)
                .min_w(0.0)
                .gap(t::SPACE_2)
                .child(text(name).font(t::TEXT_MEMBER).color(strong).a11y_hidden())
                .child(text(caption).font(t::TEXT_META).color(quiet).wrap()),
        )
        .child(control)
}

/// One of the nine tones to choose (a folder's colour, a role's): a disc of its ground,
/// ringed in the accent with a tick in its ink when it is the one chosen.
pub fn tone_swatch(
    key: impl Into<String>,
    tone: &'static str,
    on: bool,
    press: impl Fn(&mut State) + 'static,
    look: Look,
) -> El {
    let c = look.c;
    let (ground, ink) = t::tone_named(tone, c);
    let mut name = tone.to_string();
    if let Some(first) = name.get_mut(0..1) {
        first.make_ascii_uppercase();
    }
    let swatch = el()
        .key(key)
        .size(t::TONE_SWATCH, t::TONE_SWATCH)
        .radius(t::TONE_SWATCH / 2.0)
        .fill(ground)
        .center()
        .role("radio")
        .label(name)
        .checked(on)
        .cursor(Cursor::Pointer)
        .focusable(true)
        .focus_ring(2.0, 2.0, c.focus)
        .on_press(press);
    if on {
        shadowed(swatch, look.shadows.ring_current)
            .child(icon(&icons::CHECK, t::SPACE_14).color(ink))
    } else {
        swatch.border(1.0, c.line_strong)
    }
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
        look.shadows.floating,
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
