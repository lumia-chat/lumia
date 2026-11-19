//! Members in a server's settings (architecture 7.5, `app::role_settings`), as the "Members"
//! board draws them: a search over the list already held, each member's row (who they are,
//! Owner or Guest marked, their handle and title, every role they hold as a tag), a row
//! opening under itself to what this account may change of them (their roles, their title,
//! Kick); and the two small forms the profile card's Edit opens, in the card's place.
//!
//! The members stand in the order the server's list has them, a page at a time
//! (`role_settings::MEMBERS_PAGE`), with "Show more members" for the rest. A row with nothing
//! of its member to change does not open and has no chevron.
//!
//! In a window too narrow for the aside in the row (`State::narrow`) the list is under the
//! 520 points the board folds it at, and is folded as the board folds it: a member's roles
//! go under their name, and the parts of their editing state one under the other.

use lumia_client::app::{MemberSummary, ServerSummary};
use lumia_client::roles::{self as core, MemberOffers, RoleChoice};
use rune::{Align, Cursor, El, Side, col, el, icon, row, text, text_input};

use super::components::{
    Look, action_button, avatar_of, danger_outline, error_text, field_of_label, popover, role_dot,
    role_tag, secondary_button, small_button, tag,
};
use super::roles::{count, hint};
use crate::app::role_settings::{CardForm, MemberEdit};
use crate::app::roles::{self, TagTone};
use crate::app::{Popover, State};
use crate::session::display_name;
use crate::tokens::{self as t, icons};

/// The Members section: the search, the list, and the way to more of it.
pub fn section(s: &State, server: &ServerSummary, look: Look) -> El {
    let c = look.c;
    let all: &[MemberSummary] = s
        .session
        .members
        .get(&server.server_id)
        .map_or(&[], Vec::as_slice);
    let found = roles::searched(all, &s.roles.search.text);
    let head = row()
        .w_full()
        .gap(t::SPACE_8)
        .items_center()
        .child(
            text("Members")
                .font(t::read(t::TITLE_SERVER))
                .color(c.ink)
                .role("heading"),
        )
        .child(count(all.len(), "in this server", look));
    let search = row()
        .key("members-search-box")
        .w_full()
        .h(t::CONTROL_SMALL)
        .px(t::SPACE_12)
        .gap(t::SPACE_8)
        .items_center()
        .radius(t::RADIUS_10)
        .fill(c.field)
        .border(1.0, c.border)
        .focus_within_ring(1.0, -1.0, c.border_active)
        .child(icon(&icons::SEARCH, t::SPACE_16).color(c.ink_muted))
        .child(
            text_input(&s.roles.search, t::TEXT_MENU)
                .key("members-search")
                .label("Search members")
                .placeholder("Search members", c.ink_muted)
                .color(c.ink_field)
                .caret(c.accent, c.accent_soft)
                .grow(1.0)
                .min_w(0.0)
                .on_edit(|s: &mut State| &mut s.roles.search),
        );
    let mut list = col()
        .key("roster")
        .w_full()
        .radius(t::RADIUS_14)
        .border(1.0, c.line_strong)
        .role("list")
        .label("Members");
    for (i, member) in found.iter().take(s.roles.shown).enumerate() {
        let item = member_item(s, server, member, look);
        list = list.child(if i == 0 {
            item
        } else {
            // The rule is part of the row's height, as the board's is.
            item.border_sides([1.0, 0.0, 0.0, 0.0], c.line)
                .border_layout()
        });
    }
    let mut page = col()
        .key("members-section")
        .w_full()
        .max_w(t::SETTINGS_PAGE)
        .gap(t::SPACE_12)
        .child(head)
        .child(search);
    page = if found.is_empty() {
        page.child(
            text(if all.is_empty() {
                core::READING_MEMBERS
            } else {
                core::NO_MEMBER_MATCHES
            })
            .font(t::TEXT_META)
            .color(c.ink_quiet)
            .padding(t::SPACE_10),
        )
    } else {
        page.child(list)
    };
    if found.len() > s.roles.shown {
        page = page.child(
            small_button(
                "more-members",
                "Show more members",
                true,
                |s, _| s.show_more_members(),
                look,
            )
            .w_full(),
        );
    }
    page
}

/// A member in the list: their row, and under it, while it is open, what may be changed of
/// them.
fn member_item(s: &State, server: &ServerSummary, member: &MemberSummary, look: Look) -> El {
    let c = look.c;
    let name = display_name(member);
    let me = member.account_id == s.session.me();
    let offers = roles::member_offers(server, member, me);
    let opens = core::offers_any(&offers);
    let open = s
        .roles
        .member
        .as_ref()
        .filter(|open| open.account_id == member.account_id);
    let mut named = row().w_full().gap(t::SPACE_6).items_center().child(
        text(name.clone())
            .font(t::read(t::TEXT_MEMBER))
            .color(c.ink)
            .truncate()
            .shrink()
            .min_w(0.0),
    );
    if let Some(mark) = roles::member_mark(member) {
        named = named.child(tag(mark, look));
    }
    let sub = if member.title.is_empty() {
        format!("@{}", member.handle)
    } else {
        format!("@{} · {}", member.handle, member.title)
    };
    let held = core::roles_held(&server.roles, &member.role_ids);
    let mut said = name.clone();
    if let Some(mark) = roles::member_mark(member) {
        said = format!("{said}, {mark}");
    }
    if !member.title.is_empty() {
        said = format!("{said}, {}", member.title);
    }
    if !held.is_empty() {
        let names: Vec<&str> = held.iter().map(|r| r.label.as_str()).collect();
        said = format!("{said}, {}", names.join(", "));
    }
    let folded = s.narrow();
    let tags = (!held.is_empty()).then(|| {
        let mut tags = row().gap(t::SPACE_6).flex_wrap().a11y_hidden();
        tags = if folded {
            tags.w_full().pt(t::SPACE_4)
        } else {
            tags.max_w(t::ROSTER_ROLES).justify_end()
        };
        for role in &held {
            tags = tags.child(role_tag(
                &role.label,
                TagTone::of(role.tone.as_deref()),
                false,
                look,
            ));
        }
        tags
    });
    let mut words = col().grow(1.0).min_w(0.0).child(named).child(
        text(sub)
            .font(t::read(t::TEXT_META))
            .color(c.ink_muted)
            .truncate(),
    );
    // Folded, the roles are under the name; otherwise at the row's end.
    let (under, beside) = if folded { (tags, None) } else { (None, tags) };
    words = words.children(under);
    let mut line = row()
        .key(format!("roster-{}", member.account_id))
        .w_full()
        .px(t::SPACE_14)
        .py(t::SPACE_10)
        .gap(t::SPACE_12)
        .items_center()
        .radius(t::RADIUS_14)
        .label(said)
        .child(avatar_of(s, &name, &member.account_id, t::AVATAR_MD, look))
        .child(words)
        .children(beside);
    if opens {
        let id = member.account_id.clone();
        line = line
            .role("button")
            .expanded(open.is_some())
            .hover(c.hover_member)
            .cursor(Cursor::Pointer)
            .focusable(true)
            .focus_ring(2.0, 0.0, c.focus)
            .on_press(move |s: &mut State| s.toggle_member(&id))
            .child(
                icon(
                    if open.is_some() {
                        &icons::CHEVRON_DOWN
                    } else {
                        &icons::CHEVRON_RIGHT
                    },
                    t::ROW_END_GLYPH,
                )
                .color(if open.is_some() {
                    c.ink_quiet
                } else {
                    c.ink_faint
                }),
            );
    } else {
        line = line.role("group");
    }
    let mut item = col()
        .key(format!("roster-item-{}", member.account_id))
        .w_full()
        .role("listitem")
        .child(line);
    if let Some(open) = open {
        item = item.child(
            el().w_full()
                .pl(if folded {
                    t::SPACE_14
                } else {
                    t::ROSTER_INDENT
                })
                .pr(t::SPACE_14)
                .pt(t::SPACE_6)
                .pb(t::SPACE_16)
                .child(member_edit(s, server, &name, offers, open, look)),
        );
    }
    item
}

/// The roles a member may be given and those taken away: the server's in order, each with a
/// box to tick; one not this account's to give or take is shown with a lock and not ticked.
fn role_choices(
    key: &str,
    label: &str,
    choices: &[RoleChoice],
    tick: fn(&mut State, &str),
    look: Look,
) -> El {
    let c = look.c;
    let mut list = col()
        .key(key.to_string())
        .w_full()
        .gap(t::SPACE_2)
        .role("group")
        .label(label);
    for choice in choices {
        let mut tick_box = el()
            .size(t::SPACE_18, t::SPACE_18)
            .radius(t::RADIUS_5)
            .center()
            .a11y_hidden();
        tick_box = match (choice.checked, choice.locked) {
            (true, false) => tick_box.fill(c.accent),
            (true, true) => tick_box.fill(c.ink_faint),
            (false, false) => tick_box.fill(c.field).border(1.0, c.border_form),
            (false, true) => tick_box.fill(c.inset).border(1.0, c.line_strong),
        };
        if choice.checked {
            tick_box = tick_box.child(icon(&icons::CHECK, t::SPACE_12).color(c.on_accent));
        }
        let mut item = row()
            .key(format!("{key}-{}", choice.id))
            .w_full()
            .px(t::SPACE_8)
            .py(t::SPACE_6)
            .gap(t::SPACE_10)
            .items_center()
            .radius(t::RADIUS_8)
            .role("checkbox")
            .label(if choice.locked {
                format!("{}, not yours to give or take", choice.name)
            } else {
                choice.name.clone()
            })
            .checked(choice.checked)
            .child(tick_box)
            .child(role_dot(choice.tone.as_deref(), look))
            .child(
                text(choice.name.clone())
                    .font(t::read(t::TEXT_COMPACT))
                    .color(if choice.locked { c.ink_muted } else { c.ink })
                    .truncate()
                    .grow(1.0)
                    .min_w(0.0),
            );
        item = if choice.locked {
            item.disabled(true)
                .child(icon(&icons::LOCK, t::SPACE_14).color(c.ink_faint))
        } else {
            let id = choice.id.clone();
            item.hover(c.hover_member)
                .cursor(Cursor::Pointer)
                .focusable(true)
                .focus_ring(2.0, 0.0, c.focus)
                .on_press(move |s: &mut State| tick(s, &id))
        };
        list = list.child(item);
    }
    list
}

/// A member's editing state: their roles beside their title, then Kick apart from Cancel and
/// Save. Each part is there only when it is this account's to change.
fn member_edit(
    s: &State,
    server: &ServerSummary,
    name: &str,
    offers: MemberOffers,
    open: &MemberEdit,
    look: Look,
) -> El {
    // Beside each other where there is room, one under the other in a narrow window.
    let folded = s.narrow();
    let part = || {
        let part = col().min_w(0.0).gap(t::SPACE_6);
        if folded {
            part.w_full()
        } else {
            part.grow(1.0)
        }
    };
    let mut fields = if folded {
        col().w_full().gap(t::SPACE_16)
    } else {
        row().w_full().gap(t::SPACE_24).items_start()
    };
    if offers.roles {
        fields = fields.child(
            part()
                .child(
                    text("Roles")
                        .font(t::TEXT_LABEL)
                        .color(look.c.ink_body)
                        .a11y_hidden(),
                )
                .child(role_choices(
                    "member-role",
                    &format!("{name}’s roles"),
                    &core::role_choices(&s.session.roles_of(&server.server_id), &open.roles),
                    State::tick_member_role,
                    look,
                )),
        );
    } else if let Some(note) = &offers.roles_note {
        fields = fields.child(
            part()
                .child(text("Roles").font(t::TEXT_LABEL).color(look.c.ink_body))
                .child(hint(note, look)),
        );
    }
    if offers.title {
        fields = fields.child(
            part()
                .child(field_of_label(
                    "member-title",
                    "Title",
                    &open.title,
                    "No title",
                    |s| s.roles.member_title(),
                    |s, _| s.save_member(),
                    look,
                ))
                .child(hint(core::TITLE_HINT, look)),
        );
    }
    let mut actions = row().w_full().gap(t::SPACE_8).items_center();
    if offers.kick {
        actions = actions.child(danger_outline(
            "member-kick",
            "Kick",
            &format!("Kick {name}"),
            !open.busy,
            State::kick_member,
            look,
        ));
    }
    actions = actions
        .child(el().grow(1.0))
        .child(secondary_button(
            "member-cancel",
            "Cancel",
            true,
            State::cancel_member,
            look,
        ))
        .child(action_button(
            "member-save",
            "Save",
            !open.busy,
            State::save_member,
            look,
        ));
    let mut form = col()
        .key("member-edit")
        .w_full()
        .gap(t::SPACE_16)
        .child(fields);
    if !open.error.is_empty() {
        form = form.child(error_text(&open.error, look));
    }
    form.child(actions)
}

// ---- The profile card's forms -------------------------------------------------------------

/// The two buttons a card's form ends with.
fn card_actions(busy: bool, look: Look) -> El {
    row()
        .w_full()
        .gap(t::SPACE_8)
        .justify_end()
        .child(secondary_button(
            "card-form-cancel",
            "Cancel",
            true,
            State::close_card_form,
            look,
        ))
        .child(action_button(
            "card-form-save",
            "Save",
            !busy,
            State::save_card_form,
            look,
        ))
}

/// The form Edit on a profile card opened, hung where the card hangs: the member's roles to
/// tick, or their title. Escape and a press outside go back to the card.
pub fn card_form(s: &State, look: Look) -> Option<El> {
    let (server, member) = s.card_member()?;
    let card = s.roles.card.as_ref()?;
    let person = s.person.as_ref()?;
    let name = display_name(member);
    let (key, title) = match card.form {
        CardForm::Roles => ("card-roles", "Roles"),
        CardForm::Title => ("card-title-form", "Title"),
    };
    let mut surface = popover(
        key,
        &person.anchor,
        title,
        &format!("{name} in {}", server.name),
        t::POP_WIDTH,
        look,
    )
    .anchored_fit(person.anchor.clone(), Side::Left, Align::Start, t::SPACE_8)
    .modal(true, State::close_card_form);
    surface = match card.form {
        CardForm::Roles => surface.child(role_choices(
            "card-role",
            &format!("{name}’s roles"),
            &core::role_choices(&s.session.roles_of(&server.server_id), &card.member.roles),
            State::tick_card_role,
            look,
        )),
        CardForm::Title => surface
            .child(field_of_label(
                "card-title",
                "Title",
                &card.member.title,
                "No title",
                |s| s.roles.card_title(),
                |s, _| s.save_card_form(),
                look,
            ))
            .child(hint(core::TITLE_HINT, look)),
    };
    if !card.member.error.is_empty() {
        surface = surface.child(error_text(&card.member.error, look));
    }
    Some(surface.child(card_actions(card.member.busy, look)))
}

/// Which popover of this module `popover` is, drawn.
pub fn overlay(s: &State, popover: Popover, look: Look) -> Option<El> {
    match popover {
        Popover::CardRoles | Popover::CardTitle => card_form(s, look),
        _ => None,
    }
}
