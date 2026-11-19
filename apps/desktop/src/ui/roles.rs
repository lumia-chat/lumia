//! Roles in a server's settings (architecture 7.5, `app::role_settings`), as the "Roles" and
//! "Role editor" boards draw them: the sections beside the page, the roles in order with how
//! many hold each, a role lifted by its handle, its menu, and its editor (name, colour, the
//! tag switch with its preview, the permissions under their headings with Administrator
//! apart, who holds it, Delete), with the two questions asked first.
//!
//! What is offered comes from the core through `app::roles`; nothing here decides it. Where
//! the boards are silent it does as the web's `routes/roles.tsx` does: a save says "Saved."
//! beside Save and a refusal is said under the head; a role nobody holds says so where its
//! holders would be listed; "Add members" lists those who can hold the role and do not, by
//! name; a move the instance refused is said over the list.
//!
//! While a role is dragged its place stays, dashed and empty; a copy follows the pointer and
//! a line in the accent says where it would land. Both are drawn at the root
//! ([`drag_layer`]) so the page's scrolling does not clip them.

use lumia_client::app::{App, MemberSummary, ServerSummary};
use lumia_client::roles::{PermissionChoice, RoleSummary};
use rune::{Align, Cursor, Drag, El, PointerPress, Side, Tx, col, el, icon, row, text};

use super::components::{
    Look, action_button, avatar_of, danger_action, error_text, field_of_label, icon_button,
    menu_rule, popover, primary_button, role_dot, role_tag, secondary_button, section_label,
    settings_group, shadowed, small_button_with, switch, switch_where, tone_swatch,
};
use super::server_popovers::{find, person_row, picker_list};
use crate::app::roles::{self, Section, Step, TagTone};
use crate::app::{Popover, State, context};
use crate::session::display_name;
use crate::tokens::{self as t, icons};

/// The key of a role's row, which its question hangs from when its menu asked.
pub fn role_key(id: &str) -> String {
    format!("role-{id}")
}

/// The key of the editor's "Add members", which its picker hangs from.
const ADD_HOLDERS: &str = "add-holders";
/// The key of the editor's "Delete role".
const DELETE_ROLE: &str = "delete-role";

/// A section's title in the display face, with whatever leads it (a role's dot).
fn title(words: &str, look: Look) -> El {
    text(words)
        .font(t::read(t::TITLE_SERVER))
        .color(look.c.ink)
        .truncate()
        .shrink()
        .min_w(0.0)
        .role("heading")
}

/// A row of an outlined group in the editor: a name over a line on what it is, and its
/// switch at the end, on the lines the boards read them at. `inks` are the name's and the
/// line's, for Administrator's row on its amber.
fn line_row(
    key: impl Into<String>,
    name: &str,
    caption: &str,
    control: El,
    inks: Option<(rune::Color, rune::Color)>,
    look: Look,
) -> El {
    let c = look.c;
    let (strong, quiet) = inks.unwrap_or((c.ink, c.ink_muted));
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
                .child(
                    text(name)
                        .font(t::read(t::TEXT_MEMBER))
                        .color(strong)
                        .a11y_hidden(),
                )
                .child(text(caption).font(t::TEXT_SET_CAPTION).color(quiet).wrap()),
        )
        .child(control)
}

/// A line under a field or a heading.
pub(super) fn hint(words: &str, look: Look) -> El {
    text(words)
        .font(t::TEXT_HINT)
        .color(look.c.ink_muted)
        .wrap()
        .w_full()
}

/// A count beside a heading: "128" after Members.
pub(super) fn count(n: usize, what: &str, look: Look) -> El {
    text(n.to_string())
        .font(t::TEXT_LINK_SM)
        .color(look.c.ink_muted)
        .label(format!("{n} {what}"))
}

/// A heading inside the editor: "Permissions", "Delete role".
fn heading(words: &str, look: Look) -> El {
    text(words)
        .font(t::read(t::TITLE_SECTION))
        .color(look.c.ink)
        .role("heading")
}

/// The small label over a field or a part: "Colour", "Roles".
pub(super) fn legend(words: &str, look: Look) -> El {
    text(words)
        .font(t::read(t::TEXT_LABEL))
        .color(look.c.ink_body)
}

// ---- The sections ---------------------------------------------------------------------------

/// The sections of a server's settings, beside the page: the ones this account may use, the
/// one shown marked.
pub fn sections(s: &State, server: &ServerSummary, look: Look) -> El {
    let c = look.c;
    let shown = s.server_forms.settings.section;
    let mut list = col()
        .w_full()
        .gap(t::SPACE_2)
        .role("tablist")
        .label("Server settings");
    for section in roles::settings_sections(server) {
        let on = section == shown;
        let glyph = match section {
            Section::Overview => &icons::SETTINGS,
            Section::Roles => &icons::SHIELD,
            Section::Members => &icons::MEMBERS,
        };
        let mut item = row()
            .key(format!("section-{}", section.label().to_lowercase()))
            .w_full()
            .h(t::SECTION_ITEM)
            .px(t::SPACE_10)
            .gap(t::SPACE_10)
            .items_center()
            .radius(t::RADIUS_8)
            .role("tab")
            .label(section.label())
            .focus_selected(on)
            .cursor(Cursor::Pointer)
            .focusable(true)
            .focus_ring(2.0, 0.0, c.focus)
            .on_press(move |s: &mut State| s.choose_section(section))
            .child(icon(glyph, t::SECTION_GLYPH).color(if on { c.accent } else { c.ink_faint }))
            .child(
                text(section.label())
                    .font(if on { t::TEXT_NAV_ACTIVE } else { t::TEXT_NAV })
                    .color(if on { c.ink } else { c.ink_quiet })
                    .truncate()
                    .grow(1.0)
                    .min_w(0.0),
            );
        item = if on {
            item.fill(c.inset)
        } else {
            item.hover(c.hover_member)
        };
        list = list.child(item);
    }
    col()
        .key("settings-sections")
        .w(t::SECTIONS_WIDTH)
        .h_full()
        .pl(t::SPACE_16)
        .pr(t::SPACE_8)
        .py(t::SPACE_16)
        .border_sides([0.0, 1.0, 0.0, 0.0], c.line)
        .child(list)
}

// ---- The list -------------------------------------------------------------------------------

/// The Roles section: the list, or the editor in its place.
pub fn section(s: &State, server: &ServerSummary, look: Look) -> El {
    if s.roles.editor.is_some() {
        return editor(s, server, look);
    }
    let listed = s.roles_listed();
    let head = row()
        .w_full()
        .gap(t::SPACE_16)
        .items_center()
        .child(
            col()
                .grow(1.0)
                .min_w(0.0)
                .gap(t::SPACE_4)
                .child(
                    title("Roles", look).label(format!("Roles, {}", roles::roles_count(&listed))),
                )
                .child(
                    text(roles::ROLES_HINT)
                        .font(t::TEXT_CARD_LINE)
                        .color(look.c.ink_muted)
                        .wrap()
                        .w_full(),
                ),
        )
        .child(small_button_with(
            "new-role",
            &icons::PLUS,
            "New role",
            server.can_manage_roles,
            |s, _| s.new_role(),
            look,
        ));
    let mut rows = col()
        .key("roles")
        .w_full()
        .gap(t::SPACE_6)
        .role("list")
        .label("Roles");
    for role in &listed {
        rows = rows.child(role_row(s, role, look));
    }
    let mut page = col()
        .key("roles-section")
        .w_full()
        .max_w(t::SETTINGS_PAGE)
        .gap(t::SPACE_12)
        .child(head);
    if !s.roles.list_error.is_empty() {
        page = page.child(error_text(&s.roles.list_error, look));
    }
    page.child(rows)
}

/// What a role's row says: its colour, its name over how many hold it, and the quiet mark of
/// one that is nobody's tag.
fn role_words(s: &State, role: &RoleSummary, locked: bool, look: Look) -> El {
    let c = look.c;
    let mut meta = row().gap(t::SPACE_12).items_center();
    if let Some(held) = s.holders_of(role) {
        meta = meta.child(
            text(roles::members_count(held))
                .font(t::read(t::TEXT_META))
                .color(c.ink_muted),
        );
    }
    if role.hide_tag && !role.is_everyone {
        meta = meta.child(
            row()
                .gap(t::SPACE_4)
                .items_center()
                .child(icon(&icons::EYE_OFF, t::META_GLYPH).color(c.ink_muted))
                .child(
                    text("Not shown as tag")
                        .font(t::read(t::TEXT_META))
                        .color(c.ink_muted),
                ),
        );
    }
    row()
        .grow(1.0)
        .min_w(0.0)
        .gap(t::SPACE_12)
        .items_center()
        .child(role_dot(roles::tone_of(&role.color), look))
        .child(
            col()
                .grow(1.0)
                .min_w(0.0)
                .gap(t::SPACE_2 / 2.0)
                .child(
                    text(role.name.clone())
                        .font(t::TEXT_ROLE)
                        .color(if locked { c.ink_body } else { c.ink })
                        .truncate(),
                )
                .child(meta),
        )
}

/// What a row is called: its name, how many hold it, and what else is true of it.
fn role_label(s: &State, role: &RoleSummary, locked: bool) -> String {
    let mut label = role.name.clone();
    if let Some(held) = s.holders_of(role) {
        label = format!("{label}, {}", roles::members_count(held));
    }
    if role.hide_tag && !role.is_everyone {
        label.push_str(", not shown as tag");
    }
    if locked {
        label.push_str(", not yours to change");
    }
    label
}

/// A role's row: the handle it is dragged by, what it says, and where it leads. Everyone has
/// no handle; a role at or above this account's highest has none either, is on the inset
/// ground with a lock, and does not open.
fn role_row(s: &State, role: &RoleSummary, look: Look) -> El {
    let c = look.c;
    let locked = roles::locked_role(role);
    let moves = !role.is_everyone && !locked;
    let lifted = s
        .roles
        .drag
        .as_ref()
        .is_some_and(|drag| drag.id == role.role_id);
    let mut handle = el().w(t::ROLE_HANDLE).h(t::ROLE_ROW).center().a11y_hidden();
    if moves {
        let dragged = role.role_id.clone();
        handle = handle
            .key(format!("role-handle-{}", role.role_id))
            .cursor(Cursor::Grab)
            .help("Drag to move")
            .on_drag(move |s: &mut State, drag: &mut Drag, tx: &mut Tx| {
                s.drag_role(&dragged, drag, tx);
            })
            .child(icon(&t::GRIP, t::SPACE_16).color(c.ink_faint));
    }
    let end = if locked {
        icon(&icons::LOCK, t::ROW_END_GLYPH).color(c.ink_quiet)
    } else {
        icon(&icons::CHEVRON_RIGHT, t::ROW_END_GLYPH).color(c.ink_faint)
    };
    let mut open = row()
        .grow(1.0)
        .min_w(0.0)
        .min_h(t::ROLE_ROW)
        .pl(t::SPACE_2)
        .pr(t::SPACE_14)
        .py(t::SPACE_8)
        .gap(t::SPACE_12)
        .items_center()
        .radius(t::RADIUS_14)
        .label(role_label(s, role, locked))
        .child(role_words(s, role, locked, look))
        .child(end);
    if locked {
        open = open
            .role("group")
            .help("At or above your own highest role, so not yours to change");
    } else {
        let id = role.role_id.clone();
        open = open
            .key(format!("role-open-{}", role.role_id))
            .role("button")
            .cursor(Cursor::Pointer)
            .focusable(true)
            .focus_ring(2.0, 0.0, c.focus)
            .on_press(move |s: &mut State| s.open_role(&id));
        if moves {
            let id = role.role_id.clone();
            open = open.on_secondary_press(move |s: &mut State, at: PointerPress, _: &mut Tx| {
                s.open_role_menu(&id, [at.x, at.y]);
            });
        }
    }
    let placed = role.role_id.clone();
    let mut frame = row()
        .key(role_key(&role.role_id))
        .w_full()
        .min_h(t::ROLE_ROW)
        .items_center()
        .radius(t::RADIUS_14)
        // The edge is part of the row's height, as the board's is.
        .border_layout()
        .role("listitem")
        .on_layout_tx(move |s: &mut State, rect: [f32; 4], tx: &mut Tx| {
            s.place_role(&placed, rect);
            // Only a drag reads where the rows are; nothing to draw again.
            tx.live_only();
        });
    if lifted {
        // Its place while it is held: dashed and empty. The handle stays where it is in
        // the tree, unseen, since it is what the drag is held by.
        return frame
            .border(1.5, c.border_dashed)
            .border_dash(t::ROLE_DASH[0], t::ROLE_DASH[1])
            .child(handle.opacity(0.0))
            .child(open.opacity(0.0));
    }
    frame = frame.border(1.0, c.line_strong);
    frame = if locked {
        frame.fill(c.inset)
    } else {
        frame
            .fill(c.surface)
            .hover(c.hover_member)
            .hover_group()
            .child(super::components::hover_layer(t::RADIUS_14).border(1.0, c.border_hover))
    };
    frame.child(handle).child(open)
}

/// The role following the pointer and the line where it would land, over everything.
pub fn drag_layer(s: &State, look: Look) -> Option<El> {
    let drag = s.roles.drag.as_ref()?;
    let c = look.c;
    let listed = s.roles_listed();
    let role = listed.iter().find(|r| r.role_id == drag.id)?;
    let [x, y, w, h] = drag.origin;
    let mut layer = el()
        .key("role-drag")
        .pin(Some(0.0), Some(0.0), Some(0.0), Some(0.0))
        .a11y_hidden();
    if let Some(before) = &drag.before
        && let Some([_, top, _, _]) = s.roles.spots.get(before)
    {
        // In the middle of the gap over the row it would land before.
        layer = layer.child(
            el().key("role-line")
                .absolute(x, top - (t::SPACE_6 + t::ROLE_LINE) / 2.0)
                .size(w, t::ROLE_LINE)
                .radius(t::RADIUS_PILL)
                .fill(c.accent),
        );
    }
    let ghost = shadowed(
        row()
            .key("role-ghost")
            .absolute(x + drag.by[0], y + drag.by[1])
            .size(w, h)
            .items_center()
            .radius(t::RADIUS_14)
            .fill(c.surface)
            .border(1.0, c.line_strong)
            .child(
                el().w(t::ROLE_HANDLE)
                    .h(t::ROLE_ROW)
                    .center()
                    .child(icon(&t::GRIP, t::SPACE_16).color(c.ink_faint)),
            )
            .child(
                row()
                    .grow(1.0)
                    .min_w(0.0)
                    .pl(t::SPACE_2)
                    .pr(t::SPACE_14)
                    .gap(t::SPACE_12)
                    .items_center()
                    .child(role_words(s, role, false, look))
                    .child(icon(&icons::CHEVRON_RIGHT, t::ROW_END_GLYPH).color(c.ink_faint)),
            ),
        look.shadows.floating,
    );
    Some(layer.child(ghost))
}

/// What choosing a row of a role's menu does; none for one that leads nowhere.
type Chosen = Option<Box<dyn Fn(&mut State, &mut Tx)>>;

/// A row of a role's menu: a menu's item, in the danger ink or dimmed where it leads nowhere.
fn menu_row(
    key: &'static str,
    glyph: &'static rune::Icon,
    label: &str,
    danger: bool,
    press: Chosen,
    look: Look,
) -> El {
    let c = look.c;
    let item = row()
        .key(key)
        .w_full()
        .h(t::MENU_ITEM_HEIGHT)
        .px(t::SPACE_10)
        .gap(t::SPACE_10)
        .items_center()
        .radius(t::RADIUS_8)
        .role("menuitem")
        .label(label)
        .child(icon(glyph, t::SPACE_16).color(if danger { c.ink_quiet } else { c.ink_body }))
        .child(
            text(label)
                .font(t::TEXT_MENU)
                .color(if danger { c.danger } else { c.ink_body })
                .truncate()
                .grow(1.0)
                .min_w(0.0),
        );
    match press {
        Some(press) => item
            .hover(c.hover_member)
            .cursor(Cursor::Pointer)
            .focusable(true)
            .focus_ring(2.0, 0.0, c.focus)
            .on_press_tx(move |s: &mut State, tx| press(s, tx)),
        None => item.disabled(true).opacity(0.5),
    }
}

/// A role's menu where its row was right-clicked: the keyboard's way to do what a drag does.
/// The highest role this account may move has no way up, and the last above Everyone no way
/// down.
pub fn role_menu(s: &State, look: Look) -> Option<El> {
    let c = look.c;
    let id = s.roles.menu_for.clone()?;
    let listed = s.roles_listed();
    let role = listed.iter().find(|r| r.role_id == id)?;
    let step = |step: Step| -> Chosen {
        roles::step_before(&listed, &id, step)
            .map(|_| Box::new(move |s: &mut State, tx: &mut Tx| s.role_menu_step(step, tx)) as _)
    };
    let anchor = role_key(&id);
    Some(
        shadowed(
            col()
                .key("role-menu")
                .w(t::CONTEXT_MENU_WIDTH)
                .padding(t::SPACE_6)
                .gap(t::SPACE_2)
                .radius(t::RADIUS_14)
                .fill(c.surface)
                .border(1.0, c.line_strong),
            look.shadows.floating,
        )
        .anchored_fit(context::AT, Side::Bottom, Align::Start, t::SPACE_6)
        .modal(true, State::close_popover)
        .role("menu")
        .label(role.name.clone())
        .child(section_label(role.name.clone(), t::SPACE_6, look).pb(t::SPACE_2))
        .child(menu_row(
            "role-menu-edit",
            &icons::SETTINGS,
            "Edit role",
            false,
            Some(Box::new(|s, _| s.role_menu_edit())),
            look,
        ))
        .child(menu_rule(look))
        .child(menu_row(
            "role-menu-up",
            &icons::ARROW_UP,
            "Move up",
            false,
            step(Step::Up),
            look,
        ))
        .child(menu_row(
            "role-menu-down",
            &icons::ARROW_DOWN,
            "Move down",
            false,
            step(Step::Down),
            look,
        ))
        .child(menu_rule(look))
        .child(menu_row(
            "role-menu-delete",
            &icons::TRASH,
            "Delete role",
            true,
            Some(Box::new(move |s, _| s.role_menu_delete(&anchor))),
            look,
        )),
    )
}

// ---- The editor -----------------------------------------------------------------------------

/// A role's editor, in the list's place: one Save for its name, colour, tag switch and
/// permissions. A new role has no holders and nothing to delete; Everyone has its
/// permissions and nothing else.
fn editor(s: &State, server: &ServerSummary, look: Look) -> El {
    let c = look.c;
    let Some(form) = &s.roles.editor else {
        return el();
    };
    let role = s.edited_role();
    let everyone = role.is_some_and(|r| r.is_everyone);
    let typed = form.name.text.trim();
    let named = match role {
        Some(role) if everyone || typed.is_empty() => role.name.as_str(),
        Some(_) => typed,
        None => "New role",
    };
    let mut titled = col().grow(1.0).min_w(0.0).gap(t::SPACE_4).child(
        row()
            .w_full()
            .gap(t::SPACE_8)
            .items_center()
            .child(role_dot(roles::tone_of(&form.color), look))
            .child(title(named, look)),
    );
    if everyone {
        titled = titled.child(
            text(roles::EVERYONE_SUBTITLE)
                .font(t::TEXT_CARD_LINE)
                .color(c.ink_muted)
                .wrap()
                .w_full(),
        );
    }
    let mut head = row()
        .w_full()
        .gap(t::SPACE_16)
        .items_center()
        .child(
            // The board draws it out into the page's edge by 6, and 8 nearer the title.
            el().ml(-t::SPACE_6).mr(-t::SPACE_8).child(icon_button(
                "role-back",
                &icons::BACK,
                "Back to roles",
                State::close_role,
                look,
            )),
        )
        .child(titled);
    if form.saving.saved {
        head = head.child(
            text("Saved.")
                .font(t::TEXT_META)
                .color(c.ink_secondary)
                .role("status"),
        );
    }
    head = head.child(action_button(
        "save-role",
        if form.saving.busy {
            "Saving…"
        } else {
            "Save"
        },
        s.role_savable(),
        State::save_role,
        look,
    ));
    let mut page = col()
        .key("role-editor")
        .w_full()
        .max_w(t::SETTINGS_PAGE)
        .gap(t::SPACE_24)
        .child(head);
    if !form.saving.error.is_empty() {
        page = page.child(error_text(&form.saving.error, look));
    }
    if !everyone {
        page = page.child(identity(s, server, look));
    }
    page = page.child(permissions(s, server, look));
    if let Some(role) = role.filter(|_| !everyone) {
        page = page.child(holders(s, role, look)).child(
            col()
                .w_full()
                .gap(t::SPACE_10)
                .child(heading("Delete role", look))
                .child(
                    row()
                        .w_full()
                        .gap(t::SPACE_16)
                        .items_center()
                        .child(hint(roles::DELETE_ROLE_HINT, look).grow(1.0).min_w(0.0))
                        .child(danger_action(
                            DELETE_ROLE,
                            "Delete role",
                            true,
                            {
                                let id = role.role_id.clone();
                                move |s: &mut State| s.ask_delete_role(&id, DELETE_ROLE)
                            },
                            look,
                        )),
                ),
        );
    }
    page
}

/// The name, the colour, and whether it is shown as a tag, with what that looks like.
fn identity(s: &State, server: &ServerSummary, look: Look) -> El {
    let c = look.c;
    let Some(form) = &s.roles.editor else {
        return el();
    };
    let name = col()
        .w_full()
        .gap(t::SPACE_6)
        .child(field_of_label(
            "role-name",
            "Name",
            &form.name,
            "Moderator",
            |s| s.roles.role_name(),
            |s, _| s.save_role(),
            look,
        ))
        .child(hint(&roles::role_name_hint(&server.name), look));
    let mut swatches = row()
        .key("role-tones")
        .gap(t::SPACE_6)
        .items_center()
        .flex_wrap()
        .role("radiogroup")
        .label("Colour");
    for tone in roles::TONES {
        swatches = swatches.child(tone_swatch(
            format!("role-tone-{tone}"),
            tone,
            form.color == tone,
            move |s: &mut State| s.choose_role_color(tone),
            look,
        ));
    }
    let none = form.color.is_empty();
    let mut no_colour = row()
        .key("role-tone-none")
        .h(t::TONE_SWATCH)
        .px(t::SPACE_10)
        .center()
        .radius(t::RADIUS_PILL)
        .fill(c.chip)
        .role("radio")
        .label("None")
        .checked(none)
        .cursor(Cursor::Pointer)
        .focusable(true)
        .focus_ring(2.0, 2.0, c.focus)
        .on_press(|s: &mut State| s.choose_role_color(""))
        .child(
            text("None")
                .font(t::TEXT_SWATCH_NONE)
                .color(c.ink_secondary),
        );
    no_colour = if none {
        shadowed(no_colour, look.shadows.ring_current)
    } else {
        no_colour.border(1.0, c.line_strong)
    };
    let colour = col()
        .w_full()
        .gap(t::SPACE_6)
        .child(legend("Colour", look).a11y_hidden())
        .child(swatches.child(no_colour));
    let shown = !form.hide_tag;
    let mut preview = row()
        .key("role-preview")
        .w_full()
        .px(t::SPACE_16)
        .py(t::SPACE_12)
        .gap(t::SPACE_8)
        .items_center()
        .role("img")
        .label(if shown && !form.name.text.trim().is_empty() {
            format!("Preview: Devon Park, {}, 9:11 AM", form.name.text.trim())
        } else {
            "Preview: Devon Park, no tag, 9:11 AM".to_string()
        })
        .child(
            text("Preview")
                .font(t::TEXT_SECTION)
                .color(c.ink_muted)
                .mr(t::SPACE_4),
        )
        .child({
            // The boards' Devon, on purple, whatever an id would hash to.
            let (ground, ink) = t::tone_named("purple", c);
            el().size(t::AVATAR_PREVIEW, t::AVATAR_PREVIEW)
                .radius(t::AVATAR_PREVIEW / 2.0)
                .fill(ground)
                .center()
                .child(text("DP").font(t::initials(t::AVATAR_PREVIEW)).color(ink))
        })
        .child(
            text("Devon Park")
                .font(t::read(t::TEXT_AUTHOR))
                .color(c.ink),
        );
    if shown && !form.name.text.trim().is_empty() {
        let tone = roles::tone_of(&form.color).map_or(TagTone::Neutral, TagTone::Tone);
        preview = preview.child(role_tag(form.name.text.trim(), tone, false, look).shrink());
    }
    preview = preview.child(text("9:11 AM").font(t::TEXT_META).color(c.ink_muted));
    let tag = settings_group(
        "role-tag",
        vec![
            line_row(
                "role-tag-row",
                "Show as tag",
                roles::tag_switch_caption(shown),
                switch(
                    "role-shown",
                    "Show as tag",
                    shown,
                    State::toggle_role_tag,
                    look,
                ),
                None,
                look,
            ),
            preview,
        ],
        look,
    );
    col()
        .w_full()
        .gap(t::SPACE_10)
        .child(name)
        .child(colour)
        .child(tag)
}

/// The permissions under their headings, as the core offers them. Administrator stands
/// apart, and asks before it is turned on; one this account does not hold is shown, dimmed,
/// and says why it is not its to give or take.
fn permissions(s: &State, server: &ServerSummary, look: Look) -> El {
    let c = look.c;
    let Some(form) = &s.roles.editor else {
        return el();
    };
    let apart = t::apart(c);
    let line = |choice: &PermissionChoice, inks: Option<(rune::Color, rune::Color)>| {
        let on = form.permissions.contains(&choice.name);
        let withheld = roles::withheld(server, &choice.name);
        let name = choice.name.clone();
        line_row(
            format!("permission-{}", choice.name),
            &choice.label,
            &if withheld {
                roles::withheld_caption(&choice.detail)
            } else {
                choice.detail.clone()
            },
            switch_where(
                format!("switch-{}", choice.name),
                &choice.label,
                on,
                !withheld,
                move |s: &mut State| s.toggle_permission(&name),
                look,
            ),
            inks,
            look,
        )
    };
    let mut part = col()
        .key("role-permissions")
        .w_full()
        .gap(t::SPACE_10)
        .child(heading("Permissions", look));
    for group in App::role_permissions() {
        part = part.child(
            text(group.title.clone())
                .font(t::read(t::TEXT_SECTION))
                .color(c.ink_muted)
                .role("heading"),
        );
        let (first, rest): (Vec<_>, Vec<_>) = group
            .permissions
            .iter()
            .partition(|choice| choice.name == roles::ADMINISTRATOR);
        for choice in first {
            part = part.child(
                col()
                    .key("permission-apart")
                    .w_full()
                    .radius(t::RADIUS_14)
                    .fill(apart.fill)
                    .border(1.0, apart.edge)
                    .child(line(choice, Some((apart.ink, apart.ink)))),
            );
        }
        part = part.child(settings_group(
            format!("permissions-{}", group.title.to_lowercase()),
            rest.into_iter().map(|choice| line(choice, None)).collect(),
            look,
        ));
    }
    part
}

/// Who holds the role, each with the way to take it from them, and "Add members". While the
/// server's members are not known (`State::members_known`) it is the heading alone, with no
/// count and nobody to add: "Nobody holds this role yet." would be a guess.
fn holders(s: &State, role: &RoleSummary, look: Look) -> El {
    let c = look.c;
    let known = s.members_known();
    let held: Vec<&MemberSummary> = known
        .unwrap_or_default()
        .iter()
        .filter(|m| m.role_ids.contains(&role.role_id))
        .collect();
    let removing = s.roles.editor.as_ref().and_then(|e| e.removing.as_deref());
    let mut titled = row()
        .grow(1.0)
        .min_w(0.0)
        .gap(t::SPACE_6)
        .items_center()
        .child(heading("Members with this role", look));
    if known.is_some() {
        titled = titled.child(count(held.len(), "hold it", look));
    }
    let head = row()
        .w_full()
        .gap(t::SPACE_16)
        .items_center()
        .child(titled)
        .child(small_button_with(
            ADD_HOLDERS,
            &icons::PLUS,
            "Add members",
            known.is_some(),
            |s, _| s.open_add_holders(),
            look,
        ));
    if known.is_none() {
        return col().w_full().gap(t::SPACE_10).child(head);
    }
    let mut list = col()
        .key("role-holders")
        .w_full()
        .padding(t::SPACE_4)
        .gap(t::SPACE_2 / 2.0)
        .radius(t::RADIUS_14)
        .border(1.0, c.line_strong)
        .role("list")
        .label(format!("Members with {}", role.name));
    if held.is_empty() {
        list = list.child(
            text("Nobody holds this role yet.")
                .font(t::TEXT_META)
                .color(c.ink_quiet)
                .padding(t::SPACE_10),
        );
    }
    for member in held {
        list = list.child(holder_row(s, member, role, removing.is_none(), look));
    }
    let mut part = col().w_full().gap(t::SPACE_10).child(head).child(list);
    if let Some(error) = s
        .roles
        .editor
        .as_ref()
        .map(|e| e.holders_error.as_str())
        .filter(|e| !e.is_empty())
    {
        part = part.child(error_text(error, look));
    }
    part
}

/// A holder's row: who they are, and Remove at its end, as a channel's member row has it.
fn holder_row(
    s: &State,
    member: &MemberSummary,
    role: &RoleSummary,
    enabled: bool,
    look: Look,
) -> El {
    let c = look.c;
    let name = display_name(member);
    let id = member.account_id.clone();
    let label = format!("Remove {name} from {}", role.name);
    let mut remove = el()
        .key(format!("remove-holder-{}", member.account_id))
        .size(t::MEMBER_ACTION, t::MEMBER_ACTION)
        .radius(t::RADIUS_8)
        .center()
        .role("button")
        .label(&label)
        .help("Remove from role")
        .disabled(!enabled)
        .child(
            el().pin(Some(0.0), Some(0.0), Some(0.0), Some(0.0))
                .radius(t::RADIUS_8)
                .fill(c.danger_soft)
                .border(1.0, c.danger_line)
                .group_opacity(0.0, 1.0),
        )
        .child(
            icon(&icons::MEMBER_REMOVE, t::SPACE_16)
                .color(c.ink_faint)
                .group_color(c.danger),
        );
    remove = if enabled {
        remove
            .hover_group_focus()
            .cursor(Cursor::Pointer)
            .focusable(true)
            .focus_ring(1.0, -1.0, c.danger)
            .on_press(move |s: &mut State| s.remove_holder(&id))
    } else {
        remove.opacity(0.5)
    };
    let mut words = col().grow(1.0).min_w(0.0).child(
        text(name.clone())
            .font(t::read(t::TEXT_MEMBER))
            .color(c.ink)
            .truncate(),
    );
    if !member.handle.is_empty() {
        words = words.child(
            text(format!("@{}", member.handle))
                .font(t::read(t::TEXT_META))
                .color(c.ink_muted)
                .truncate(),
        );
    }
    row()
        .key(format!("holder-{}", member.account_id))
        .w_full()
        .px(t::SPACE_10)
        .py(t::SPACE_6)
        .gap(t::SPACE_12)
        .items_center()
        .radius(t::RADIUS_10)
        .hover(c.hover_member)
        .role("listitem")
        .label(name.clone())
        .child(avatar_of(s, &name, &member.account_id, t::AVATAR_MD, look))
        .child(words)
        .child(remove)
}

// ---- The questions and the picker ---------------------------------------------------------

/// A question asked before something with weight, hung under the control keyed `anchor`: its
/// title, what it costs in a line or two, then Cancel, which has the keyboard first, and the
/// button that goes on.
fn question(
    key: &'static str,
    anchor: &str,
    title: &str,
    lines: &[String],
    error: &str,
    confirm: El,
    look: Look,
) -> El {
    let c = look.c;
    let mut words = col().w_full().gap(t::SPACE_8);
    for line in lines {
        words = words.child(
            text(line.clone())
                .font(t::TEXT_CARD_LINE)
                .color(c.ink_secondary)
                .wrap()
                .w_full(),
        );
    }
    let mut surface = shadowed(
        col()
            .key(key)
            .w(t::ASK_WIDTH)
            .padding(t::SPACE_18)
            .gap(t::SPACE_14)
            .radius(t::RADIUS_18)
            .fill(c.surface)
            .border(1.0, c.line_strong),
        look.shadows.floating,
    )
    .anchored_fit(anchor, Side::Bottom, Align::End, t::SPACE_6)
    .modal(true, State::close_popover)
    .role("dialog")
    .label(title)
    .child(
        text(title)
            .font(t::TITLE_SERVER)
            .color(c.ink)
            .wrap()
            .role("heading"),
    )
    .child(words);
    if !error.is_empty() {
        surface = surface.child(error_text(error, look));
    }
    surface.child(
        row()
            .w_full()
            .gap(t::SPACE_8)
            .justify_end()
            .child(secondary_button(
                format!("{key}-cancel"),
                "Cancel",
                true,
                State::close_popover,
                look,
            ))
            .child(confirm),
    )
}

/// "Turn on Administrator?", against its switch.
pub fn ask_administrator(s: &State, look: Look) -> Option<El> {
    let server = s.settings_summary()?;
    let form = s.roles.editor.as_ref()?;
    let role = s.edited_role();
    let name = role.map_or(form.name.text.trim(), |r| r.name.as_str());
    let held = role.and_then(|r| s.holders_of(r));
    Some(question(
        "ask-administrator",
        "switch-ADMINISTRATOR",
        "Turn on Administrator?",
        &roles::administrator_question(&server.name, name, held),
        "",
        action_button(
            "ask-administrator-confirm",
            "Turn on",
            true,
            State::confirm_administrator,
            look,
        ),
        look,
    ))
}

/// "Delete Moderator?", against Delete role or the row whose menu asked.
pub fn delete_role(s: &State, look: Look) -> Option<El> {
    let deleting = s.roles.deleting.as_ref()?;
    let listed = s.roles_listed();
    let role = listed.iter().find(|r| r.role_id == deleting.role_id)?;
    Some(question(
        "delete-role-question",
        &deleting.anchor,
        &format!("Delete {}?", role.name),
        &roles::delete_question(s.holders_of(role)),
        &deleting.error,
        danger_action(
            "delete-role-confirm",
            "Delete role",
            !deleting.busy,
            State::confirm_delete_role,
            look,
        ),
        look,
    ))
}

/// "Add members": the server's members who do not hold the role in the editor, ticked and
/// given it together. Guests are not among them, being given no roles.
pub fn add_holders(s: &State, look: Look) -> Option<El> {
    let role = s.edited_role()?;
    let form = &s.roles.adding;
    let all = s.members_known()?;
    let mut candidates: Vec<&MemberSummary> = roles::searched(all, &form.filter.text)
        .into_iter()
        .filter(|m| !m.guest && !m.role_ids.contains(&role.role_id))
        .collect();
    candidates.sort_by_cached_key(|m| display_name(m).to_lowercase());
    let rows: Vec<El> = candidates
        .into_iter()
        .map(|m| {
            let id = m.account_id.clone();
            let chosen = form.chosen.contains(&id);
            person_row(
                s,
                format!("add-holder-{id}"),
                m,
                None,
                Some(chosen),
                Some(Box::new(move |s: &mut State| s.toggle_holder(&id))),
                look,
            )
        })
        .collect();
    let mut surface = popover(
        "add-holders-popover",
        ADD_HOLDERS,
        "Add members",
        &format!("To {}.", role.name),
        t::ASK_WIDTH,
        look,
    )
    .anchored_fit(ADD_HOLDERS, Side::Bottom, Align::End, t::SPACE_6)
    .child(find(
        "add-holders-find",
        &form.filter,
        |s| &mut s.roles.adding.filter,
        look,
    ));
    surface = if rows.is_empty() {
        surface.child(
            text(if form.filter.text.trim().is_empty() {
                "Everyone who can hold this role already does."
            } else {
                "No member matches that."
            })
            .font(t::TEXT_META)
            .color(look.c.ink_quiet)
            .wrap()
            .w_full(),
        )
    } else {
        surface.child(picker_list(
            "add-holders-list",
            "Members to add",
            rows,
            look,
        ))
    };
    if !form.error.is_empty() {
        surface = surface.child(error_text(&form.error, look));
    }
    let chosen = form.chosen.len();
    Some(surface.child(primary_button(
        "add-holders-confirm",
        &match chosen {
            0 | 1 => "Add".to_string(),
            n => format!("Add {n} members"),
        },
        !form.busy && chosen > 0,
        State::add_holders,
        look,
    )))
}

/// Which popover of this module `popover` is, drawn.
pub fn overlay(s: &State, popover: Popover, look: Look) -> Option<El> {
    match popover {
        Popover::RoleMenu => role_menu(s, look),
        Popover::AskAdministrator => ask_administrator(s, look),
        Popover::DeleteRole => delete_role(s, look),
        Popover::AddHolders => add_holders(s, look),
        _ => None,
    }
}
