//! Roles, titles and tags (architecture 7.5), over a state no instance is behind: Acme as the
//! step 11 boards have it (`scenes::roles`), seen by its owner, by Devon the moderator and by
//! Sam, who manages nothing. Who is offered which section, the list and its locks, a role's
//! editor and what it asks first, a role moved by its menu and by its handle, a member's row,
//! the tag beside a name, and the profile card's forms. What the instance would answer is
//! applied with `State::receive`.

#![allow(clippy::expect_used, clippy::unwrap_used)]

mod common;

use std::collections::HashMap;

use common::*;
use lumia_client::app::{MemberSummary, ServerSummary, SnapshotSummary};
use lumia_client::roles::{RoleSummary, TagSummary};
use lumia_shell::app::roles::Section;
use lumia_shell::app::{Popover, State};
use lumia_shell::scenes::roles::{ACME, Acme, acme, person_id, role_id};
use lumia_shell::session::roles::RoleUpdate;
use lumia_shell::session::servers::{ServerUpdate, Setting};
use lumia_shell::session::{Failure, Update};
use lumia_shell::settings::Layout;
use rune::{Key, KeyEvent, Mods, Runtime};

/// Acme on screen, as `viewer` sees it.
fn seen_by(viewer: &'static str) -> Runtime<State> {
    let mut state = opened();
    acme(&mut state, &Acme::seen_by(viewer));
    runtime(state)
}

/// The server's settings, on `section`.
fn open(rt: &mut Runtime<State>, section: &str) {
    press(rt, "server-title");
    press(rt, "menu-server-settings");
    press(rt, &format!("section-{section}"));
}

fn refused(code: &str, field: Option<&str>) -> Failure {
    Failure {
        code: code.into(),
        message: code.to_lowercase(),
        details: field
            .map(|f| HashMap::from([("field".to_string(), f.to_string())]))
            .unwrap_or_default(),
    }
}

/// The snapshot as it is held, with Acme changed by `change`.
fn changed(rt: &Runtime<State>, change: impl Fn(&mut ServerSummary)) -> SnapshotSummary {
    let mut snapshot = rt.state.session.snapshot.clone().unwrap();
    change(&mut snapshot.servers[0]);
    snapshot
}

fn answer(rt: &mut Runtime<State>, update: RoleUpdate) {
    rt.update(move |s, _| s.receive(&Update::Role(update)));
    settle(rt);
}

/// The roles as the list draws them, by name.
fn listed(rt: &Runtime<State>) -> Vec<String> {
    rt.state
        .roles_listed()
        .into_iter()
        .map(|role| role.name)
        .collect()
}

fn member(rt: &Runtime<State>, who: &str) -> MemberSummary {
    rt.state
        .session
        .member_in(ACME, &person_id(who))
        .cloned()
        .unwrap()
}

fn label_of(rt: &Runtime<State>, key: &str) -> String {
    rt.elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some(key))
        .and_then(|e| e.label)
        .unwrap_or_else(|| panic!("{key} is not on screen"))
}

// ---- Sections -----------------------------------------------------------------------------

#[test]
fn the_settings_show_the_sections_an_account_may_use_and_open_on_the_first() {
    // The owner has all three, and lands on Overview.
    let mut rt = seen_by("ryan");
    press(&mut rt, "server-title");
    press(&mut rt, "menu-server-settings");
    assert_eq!(rt.state.server_forms.settings.section, Section::Overview);
    for key in ["section-overview", "section-roles", "section-members"] {
        assert!(rt.find(key).is_some(), "{key} is not listed");
    }
    assert!(words(&rt).contains("Tag beside names"));
    // The member list gives the page the pane.
    assert!(rt.find("member-search").is_none());

    // Devon holds Manage roles and Manage members, not Manage server: Roles first.
    let mut rt = seen_by("devon");
    press(&mut rt, "server-title");
    press(&mut rt, "menu-server-settings");
    assert_eq!(rt.state.server_forms.settings.section, Section::Roles);
    assert!(rt.find("section-overview").is_none());
    assert!(rt.find("section-roles").is_some() && rt.find("section-members").is_some());
    assert!(words(&rt).contains(
        "In order, highest first. Drag a role to move it. A new role starts last, just above Everyone."
    ));

    // Sam manages nothing there, and is offered no settings.
    let mut rt = seen_by("sam");
    press(&mut rt, "server-title");
    assert!(rt.find("menu-invite").is_some());
    assert!(rt.find("menu-server-settings").is_none());
}

#[test]
fn a_section_taken_away_gives_way_to_what_is_left() {
    let mut rt = seen_by("devon");
    open(&mut rt, "roles");
    press(&mut rt, &format!("role-open-{}", role_id("eng")));
    assert!(rt.state.roles.editor.is_some());
    // Someone took Manage roles from Moderator: Members is what Devon has left.
    let snapshot = changed(&rt, |server| {
        server.can_manage_roles = false;
        for role in &mut server.roles {
            role.can_manage = false;
        }
    });
    rt.update(move |s, _| s.receive(&Update::Snapshot(Ok(snapshot))));
    settle(&mut rt);
    assert_eq!(rt.state.server_forms.settings.section, Section::Members);
    assert!(rt.state.roles.editor.is_none());
    assert!(rt.find("section-roles").is_none());

    // And Manage members too: the settings close, back to the channel.
    let snapshot = changed(&rt, |server| server.can_manage_members = false);
    rt.update(move |s, _| s.receive(&Update::Snapshot(Ok(snapshot))));
    settle(&mut rt);
    assert!(!rt.state.server_settings_open());
}

#[test]
fn the_tag_beside_names_is_saved_as_it_is_chosen() {
    let mut rt = seen_by("ryan");
    open(&mut rt, "overview");
    let shown = words(&rt);
    for part in [
        "Each member’s highest role that is shown as a tag, in the role’s colour.",
        "Each member’s title, in no colour. A member without a title has no tag.",
        "A profile shows a member’s title and roles either way.",
    ] {
        assert!(shown.contains(part), "{part}");
    }
    press(&mut rt, "tag-titles");
    assert!(rt.state.server_forms.settings.tag.busy);
    // Drawn chosen at once.
    let server = rt.state.session.server(ACME).cloned().unwrap();
    assert!(rt.state.tag_titles_shown(&server) && !server.tag_titles);

    // Refused: the choice goes back, and the page says why.
    rt.update(|s, _| {
        s.receive(&Update::Server(ServerUpdate::SettingSaved {
            server_id: ACME.into(),
            setting: Setting::Tag,
            result: Err(Failure {
                code: "PERMISSION_DENIED".into(),
                ..Failure::default()
            }),
        }));
    });
    settle(&mut rt);
    assert!(!rt.state.tag_titles_shown(&server));
    assert!(words(&rt).contains("You cannot change this server's settings."));

    // Taken: the server shows titles.
    press(&mut rt, "tag-titles");
    let snapshot = changed(&rt, |server| server.tag_titles = true);
    rt.update(move |s, _| {
        s.receive(&Update::Server(ServerUpdate::SettingSaved {
            server_id: ACME.into(),
            setting: Setting::Tag,
            result: Ok(snapshot),
        }));
    });
    settle(&mut rt);
    let server = rt.state.session.server(ACME).cloned().unwrap();
    assert!(server.tag_titles && rt.state.tag_titles_shown(&server));
    assert!(!rt.state.server_forms.settings.tag.busy);
}

// ---- The list -----------------------------------------------------------------------------

#[test]
fn the_roles_are_listed_in_order_with_who_holds_them() {
    let mut rt = seen_by("ryan");
    open(&mut rt, "roles");
    assert_eq!(
        listed(&rt),
        [
            "Admin",
            "Moderator",
            "Design",
            "Engineering",
            "Can invite",
            "Everyone"
        ]
    );
    // Counted from the server's member list: the seven.
    assert_eq!(
        label_of(&rt, &format!("role-open-{}", role_id("eng"))),
        "Engineering, 2 members"
    );
    assert_eq!(
        label_of(&rt, &format!("role-open-{}", role_id("invite"))),
        "Can invite, 2 members, not shown as tag"
    );
    assert_eq!(
        label_of(&rt, &format!("role-open-{}", role_id("everyone"))),
        "Everyone, 7 members"
    );
    // Everyone has no handle; the others have one.
    assert!(
        rt.find(&format!("role-handle-{}", role_id("everyone")))
            .is_none()
    );
    assert!(
        rt.find(&format!("role-handle-{}", role_id("admin")))
            .is_some()
    );
}

#[test]
fn a_role_at_or_above_ones_own_is_locked_and_does_not_open() {
    let mut rt = seen_by("devon");
    open(&mut rt, "roles");
    for locked in ["admin", "mod"] {
        assert!(
            rt.find(&format!("role-open-{}", role_id(locked))).is_none(),
            "{locked} opens"
        );
        assert!(
            rt.find(&format!("role-handle-{}", role_id(locked)))
                .is_none()
        );
    }
    assert!(words(&rt).contains("Admin, 1 member, not yours to change"));
    rt.update(|s, _| s.open_role(&role_id("admin")));
    assert!(rt.state.roles.editor.is_none());
    // Nor does its menu come up.
    rt.update(|s, _| s.open_role_menu(&role_id("mod"), [0.0, 0.0]));
    assert_eq!(rt.state.popover, None);

    press(&mut rt, &format!("role-open-{}", role_id("design")));
    assert!(rt.state.roles.editor.is_some());
    press(&mut rt, "role-back");
    // Everyone always opens.
    press(&mut rt, &format!("role-open-{}", role_id("everyone")));
    let shown = words(&rt);
    assert!(shown.contains(
        "What every member may do before any role adds to it. Guests are not among them."
    ));
    assert!(rt.find("role-name").is_none() && rt.find("delete-role").is_none());
    assert!(!shown.contains("Members with this role"));
}

// ---- The editor ---------------------------------------------------------------------------

#[test]
fn a_roles_editor_saves_once_something_differs_and_says_so() {
    let mut rt = seen_by("ryan");
    open(&mut rt, "roles");
    press(&mut rt, &format!("role-open-{}", role_id("mod")));
    let shown = words(&rt);
    for part in [
        "1 to 32 characters. No other role in Acme can have the same name.",
        "This role can be the tag beside a member’s name.",
        "Permissions",
        "Create, change and delete roles below their own highest, and give them to members.",
        "Members with this role",
        "Its members lose what it allowed. This can’t be undone.",
    ] {
        assert!(shown.contains(part), "{part}");
    }
    assert!(!rt.state.role_savable());
    press(&mut rt, "role-tone-sky");
    press(&mut rt, "role-shown");
    assert!(words(&rt).contains("This role only carries permissions. It is nobody’s tag."));
    press(&mut rt, "switch-CREATE_INVITES");
    assert!(rt.state.role_savable());
    press(&mut rt, "save-role");
    let editor = rt.state.roles.editor.as_ref().unwrap();
    assert!(editor.saving.busy);
    assert_eq!(editor.color, "sky");

    // Refused: said where it was tried, and what was chosen stays.
    answer(
        &mut rt,
        RoleUpdate::Saved {
            server_id: ACME.into(),
            role_id: Some(role_id("mod")),
            result: Err(refused("ALREADY_EXISTS", None)),
        },
    );
    assert!(words(&rt).contains("Another role in Acme has that name."));
    assert_eq!(rt.state.roles.editor.as_ref().unwrap().color, "sky");

    // Taken: the list has it so, and the editor says "Saved." with nothing left to save.
    press(&mut rt, "save-role");
    let snapshot = changed(&rt, |server| {
        let role = &mut server.roles[1];
        role.color = "sky".into();
        role.hide_tag = true;
        role.permissions.push("CREATE_INVITES".into());
    });
    answer(
        &mut rt,
        RoleUpdate::Saved {
            server_id: ACME.into(),
            role_id: Some(role_id("mod")),
            result: Ok(snapshot),
        },
    );
    assert!(words(&rt).contains("Saved."));
    assert!(!rt.state.role_savable());
    assert_eq!(rt.state.session.server(ACME).unwrap().roles[1].color, "sky");
}

#[test]
fn administrator_asks_before_it_is_turned_on() {
    let mut rt = seen_by("ryan");
    open(&mut rt, "roles");
    press(&mut rt, &format!("role-open-{}", role_id("mod")));
    press(&mut rt, "switch-ADMINISTRATOR");
    assert_eq!(rt.state.popover, Some(Popover::AskAdministrator));
    let shown = words(&rt);
    assert!(shown.contains("Turn on Administrator?"));
    assert!(shown.contains(
        "Administrator allows everything in Acme, in every channel, whatever a channel’s own permissions say."
    ));
    assert!(shown.contains("1 member holds Moderator."));
    // Cancel leaves it off.
    press(&mut rt, "ask-administrator-cancel");
    let on = |rt: &Runtime<State>| {
        rt.state
            .roles
            .editor
            .as_ref()
            .unwrap()
            .permissions
            .contains(&"ADMINISTRATOR".to_string())
    };
    assert!(!on(&rt) && !rt.state.role_savable());
    press(&mut rt, "switch-ADMINISTRATOR");
    press(&mut rt, "ask-administrator-confirm");
    assert!(on(&rt) && rt.state.role_savable());
    assert_eq!(rt.state.popover, None);
    // Turning it off asks nothing.
    press(&mut rt, "switch-ADMINISTRATOR");
    assert!(!on(&rt) && rt.state.popover.is_none());
}

#[test]
fn a_permission_one_does_not_hold_is_shown_and_not_turned() {
    let mut rt = seen_by("devon");
    open(&mut rt, "roles");
    press(&mut rt, &format!("role-open-{}", role_id("eng")));
    let shown = words(&rt);
    assert!(shown.contains(
        "Change the server’s name, picture and settings. You don’t hold it, so it is not yours to give or take."
    ));
    assert!(shown.contains(
        "Everything below, in every channel. A channel’s own permissions do not limit it. You don’t hold it, so it is not yours to give or take."
    ));
    rt.update(|s, _| {
        s.toggle_permission("MANAGE_SERVER");
        s.toggle_permission("ADMINISTRATOR");
    });
    settle(&mut rt);
    assert!(
        rt.state
            .roles
            .editor
            .as_ref()
            .unwrap()
            .permissions
            .is_empty()
    );
    assert_eq!(rt.state.popover, None);
    // One Devon holds is his to give.
    press(&mut rt, "switch-MANAGE_MESSAGES");
    assert_eq!(
        rt.state.roles.editor.as_ref().unwrap().permissions,
        ["MANAGE_MESSAGES"]
    );
}

#[test]
fn a_new_role_is_made_once_it_has_a_name_and_its_editor_goes_on_as_its_own() {
    let mut rt = seen_by("ryan");
    open(&mut rt, "roles");
    press(&mut rt, "new-role");
    let shown = words(&rt);
    assert!(shown.contains("New role"));
    assert!(!shown.contains("Members with this role") && !shown.contains("Delete role"));
    assert!(!rt.state.role_savable());
    press(&mut rt, "role-name");
    type_text(&mut rt, "Support");
    assert!(rt.state.role_savable());
    // Thirty-two characters and no more.
    type_text(&mut rt, &"x".repeat(40));
    assert_eq!(
        rt.state
            .roles
            .editor
            .as_ref()
            .unwrap()
            .name
            .text
            .chars()
            .count(),
        32
    );
    rt.update(|s, _| s.roles.role_name().replace_text(" Support "));
    settle(&mut rt);
    press(&mut rt, "save-role");

    // The instance placed it last above Everyone.
    let snapshot = changed(&rt, |server| {
        let at = server.roles.len() - 1;
        server.roles.insert(
            at,
            RoleSummary {
                role_id: role_id("support"),
                name: "Support".into(),
                color: String::new(),
                position: 6,
                is_everyone: false,
                hide_tag: false,
                permissions: Vec::new(),
                can_manage: true,
            },
        );
    });
    answer(
        &mut rt,
        RoleUpdate::Saved {
            server_id: ACME.into(),
            role_id: None,
            result: Ok(snapshot),
        },
    );
    let editor = rt.state.roles.editor.as_ref().unwrap();
    assert_eq!(editor.role_id, Some(role_id("support")));
    assert_eq!(editor.name.text, "Support");
    let shown = words(&rt);
    assert!(shown.contains("Saved.") && shown.contains("Nobody holds this role yet."));
    press(&mut rt, "role-back");
    assert_eq!(listed(&rt)[4..], ["Can invite", "Support", "Everyone"]);
}

#[test]
fn deleting_a_role_asks_first_and_says_how_many_hold_it() {
    let mut rt = seen_by("ryan");
    open(&mut rt, "roles");
    press(&mut rt, &format!("role-open-{}", role_id("eng")));
    press(&mut rt, "delete-role");
    assert_eq!(rt.state.popover, Some(Popover::DeleteRole));
    let shown = words(&rt);
    assert!(shown.contains("Delete Engineering?"));
    assert!(shown.contains("2 members hold this role. They lose what it allowed."));
    assert!(shown.contains("This can’t be undone."));
    press(&mut rt, "delete-role-question-cancel");
    assert!(rt.state.roles.editor.is_some() && rt.state.popover.is_none());

    press(&mut rt, "delete-role");
    press(&mut rt, "delete-role-confirm");
    assert!(rt.state.roles.deleting.as_ref().unwrap().busy);
    answer(
        &mut rt,
        RoleUpdate::Deleted {
            server_id: ACME.into(),
            role_id: role_id("eng"),
            result: Err(refused("PERMISSION_DENIED", None)),
        },
    );
    assert!(words(&rt).contains("That isn’t yours to change."));
    press(&mut rt, "delete-role-confirm");
    let snapshot = changed(&rt, |server| {
        server.roles.retain(|r| r.role_id != role_id("eng"));
    });
    answer(
        &mut rt,
        RoleUpdate::Deleted {
            server_id: ACME.into(),
            role_id: role_id("eng"),
            result: Ok(snapshot),
        },
    );
    assert!(rt.state.roles.editor.is_none() && rt.state.popover.is_none());
    assert!(!listed(&rt).contains(&"Engineering".to_string()));
}

#[test]
fn an_editor_closes_when_someone_else_deletes_its_role() {
    let mut rt = seen_by("ryan");
    open(&mut rt, "roles");
    press(&mut rt, &format!("role-open-{}", role_id("eng")));
    let snapshot = changed(&rt, |server| {
        server.roles.retain(|r| r.role_id != role_id("eng"));
    });
    rt.update(move |s, _| s.receive(&Update::Snapshot(Ok(snapshot))));
    settle(&mut rt);
    assert!(rt.state.roles.editor.is_none());
    assert!(words(&rt).contains("That role is gone. Someone deleted it."));
}

// ---- Moving -------------------------------------------------------------------------------

#[test]
fn a_roles_menu_moves_it_and_a_refusal_puts_it_back() {
    let mut rt = seen_by("ryan");
    open(&mut rt, "roles");
    right_click(&mut rt, &format!("role-open-{}", role_id("eng")));
    assert_eq!(rt.state.popover, Some(Popover::RoleMenu));
    for key in [
        "role-menu-edit",
        "role-menu-up",
        "role-menu-down",
        "role-menu-delete",
    ] {
        assert!(rt.find(key).is_some(), "{key}");
    }
    press(&mut rt, "role-menu-up");
    // Drawn moved at once, and the whole order sent.
    assert_eq!(
        listed(&rt)[..4],
        ["Admin", "Moderator", "Engineering", "Design"]
    );
    assert_eq!(
        rt.state.session.roles.pending,
        Some((
            ACME.to_string(),
            ["admin", "mod", "eng", "design", "invite"]
                .map(role_id)
                .to_vec()
        ))
    );
    assert_eq!(
        announced(&rt).as_deref(),
        Some("Engineering moved to position 3")
    );
    let sent = rt.state.session.roles.sent();
    answer(
        &mut rt,
        RoleUpdate::Ordered {
            server_id: ACME.into(),
            sent,
            result: Err(refused("INVALID_ARGUMENT", Some("role_ids"))),
        },
    );
    assert_eq!(
        listed(&rt)[..4],
        ["Admin", "Moderator", "Design", "Engineering"]
    );
    assert!(words(&rt).contains("The roles changed while you were moving one. Try again."));

    // The highest has no way up, and the last above Everyone no way down.
    right_click(&mut rt, &format!("role-open-{}", role_id("admin")));
    let up = rt
        .elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some("role-menu-up"))
        .unwrap();
    assert!(!up.focusable, "Admin has a way up");
    rt.update(|s, _| s.close_popover());
    settle(&mut rt);
    right_click(&mut rt, &format!("role-open-{}", role_id("invite")));
    let down = rt
        .elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some("role-menu-down"))
        .unwrap();
    assert!(!down.focusable, "Can invite has a way down");

    // "Delete role" from the menu asks first too.
    press(&mut rt, "role-menu-delete");
    assert_eq!(rt.state.popover, Some(Popover::DeleteRole));
    assert!(words(&rt).contains("Delete Can invite?"));
}

/// The middle of the element keyed `key`.
fn middle(rt: &Runtime<State>, key: &str) -> (f32, f32) {
    let ([x, y, w, h], _) = rt
        .presented(key)
        .unwrap_or_else(|| panic!("{key} is not drawn"));
    (x + w / 2.0, y + h / 2.0)
}

/// Presses the handle of `role` and moves the pointer to `to` in steps, without letting go.
fn drag_to(rt: &mut Runtime<State>, role: &str, to: (f32, f32)) {
    let (x, y) = middle(rt, &format!("role-handle-{}", role_id(role)));
    rt.pointer_move(x, y);
    rt.pointer_down(x, y);
    settle(rt);
    for step in 1..=8 {
        let f = step as f32 / 8.0;
        rt.pointer_move(x + (to.0 - x) * f, y + (to.1 - y) * f);
        rt.frame(1.0 / 60.0);
    }
    settle(rt);
}

#[test]
fn a_role_is_dragged_by_its_handle_to_where_the_line_says() {
    let mut rt = seen_by("ryan");
    open(&mut rt, "roles");
    // Onto Moderator's upper half: before Moderator.
    let ([_, top, _, height], _) = rt.presented(&format!("role-{}", role_id("mod"))).unwrap();
    let (x, _) = middle(&rt, &format!("role-handle-{}", role_id("eng")));
    let to = (x + 30.0, top + height * 0.25);
    drag_to(&mut rt, "eng", to);
    let drag = rt.state.roles.drag.clone().expect("a drag is held");
    assert_eq!(drag.before, Some(role_id("mod")));
    assert!(rt.find("role-ghost").is_some() && rt.find("role-line").is_some());
    // Nothing is sent until it is let go.
    assert!(rt.state.session.roles.pending.is_none());
    rt.pointer_up(to.0, to.1);
    settle(&mut rt);
    assert!(rt.state.roles.drag.is_none() && rt.find("role-ghost").is_none());
    assert_eq!(
        listed(&rt)[..4],
        ["Admin", "Engineering", "Moderator", "Design"]
    );
    assert_eq!(
        announced(&rt).as_deref(),
        Some("Engineering moved to position 2")
    );
}

#[test]
fn escape_puts_a_dragged_role_back_and_a_drop_where_it_was_moves_nothing() {
    let mut rt = seen_by("ryan");
    open(&mut rt, "roles");
    let before = listed(&rt);
    let to = middle(&rt, &format!("role-{}", role_id("admin")));
    drag_to(&mut rt, "eng", (to.0, to.1 - 20.0));
    assert!(rt.state.roles.drag.is_some());
    rt.key(KeyEvent {
        key: Key::Escape,
        mods: Mods::default(),
    });
    settle(&mut rt);
    rt.pointer_up(to.0, to.1);
    settle(&mut rt);
    assert!(rt.state.roles.drag.is_none());
    assert_eq!(listed(&rt), before);
    assert!(rt.state.session.roles.pending.is_none());

    // Over its own row there is no line, and letting go sends nothing.
    let own = middle(&rt, &format!("role-{}", role_id("eng")));
    drag_to(&mut rt, "eng", (own.0 + 40.0, own.1 + 8.0));
    assert!(rt.find("role-ghost").is_some() && rt.find("role-line").is_none());
    rt.pointer_up(own.0 + 40.0, own.1 + 8.0);
    settle(&mut rt);
    assert_eq!(listed(&rt), before);
    assert!(rt.state.session.roles.pending.is_none());
}

#[test]
fn a_role_is_not_dragged_above_ones_own_highest() {
    let mut rt = seen_by("devon");
    open(&mut rt, "roles");
    let to = middle(&rt, &format!("role-{}", role_id("admin")));
    drag_to(&mut rt, "eng", (to.0, to.1 - 20.0));
    assert!(rt.state.roles.drag.as_ref().unwrap().before.is_none());
    assert!(rt.find("role-line").is_none());
    rt.pointer_up(to.0, to.1 - 20.0);
    settle(&mut rt);
    assert!(rt.state.session.roles.pending.is_none());
    // Its menu has no way up either: Design is the highest Devon manages.
    right_click(&mut rt, &format!("role-open-{}", role_id("design")));
    let up = rt
        .elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some("role-menu-up"))
        .unwrap();
    assert!(!up.focusable);
}

// ---- Who holds a role ---------------------------------------------------------------------

#[test]
fn a_roles_holders_are_removed_and_added_from_its_editor() {
    let mut rt = seen_by("ryan");
    open(&mut rt, "roles");
    press(&mut rt, &format!("role-open-{}", role_id("eng")));
    let (devon, sam, priya) = (person_id("devon"), person_id("sam"), person_id("priya"));
    assert!(rt.find(&format!("holder-{devon}")).is_some());
    assert!(rt.find(&format!("holder-{sam}")).is_some());
    assert!(rt.find(&format!("holder-{priya}")).is_none());

    press(&mut rt, &format!("remove-holder-{sam}"));
    assert_eq!(
        rt.state.roles.editor.as_ref().unwrap().removing.as_deref(),
        Some(sam.as_str())
    );
    let mut now = member(&rt, "sam");
    now.role_ids.retain(|id| *id != role_id("eng"));
    answer(
        &mut rt,
        RoleUpdate::Member {
            server_id: ACME.into(),
            account_id: sam.clone(),
            result: Ok(now),
        },
    );
    assert!(rt.find(&format!("holder-{sam}")).is_none());
    assert!(rt.state.roles.editor.as_ref().unwrap().removing.is_none());

    // "Add members": those who do not hold it, and no guests.
    press(&mut rt, "add-holders");
    assert_eq!(rt.state.popover, Some(Popover::AddHolders));
    assert!(rt.find(&format!("add-holder-{sam}")).is_some());
    assert!(rt.find(&format!("add-holder-{devon}")).is_none());
    assert!(
        rt.find(&format!("add-holder-{}", person_id("jordan")))
            .is_none()
    );
    press(&mut rt, &format!("add-holder-{sam}"));
    press(&mut rt, &format!("add-holder-{priya}"));
    assert!(words(&rt).contains("Add 2 members"));
    press(&mut rt, "add-holders-confirm");
    assert!(rt.state.roles.adding.busy);
    answer(
        &mut rt,
        RoleUpdate::Given {
            server_id: ACME.into(),
            role_id: role_id("eng"),
            result: Err(refused("FAILED_PRECONDITION", None)),
        },
    );
    assert!(words(&rt).contains("A member can’t hold that many roles."));
    press(&mut rt, "add-holders-confirm");
    answer(
        &mut rt,
        RoleUpdate::Given {
            server_id: ACME.into(),
            role_id: role_id("eng"),
            result: Ok(()),
        },
    );
    assert_eq!(rt.state.popover, None);
}

// ---- Members ------------------------------------------------------------------------------

#[test]
fn members_are_searched_by_name_handle_and_title_in_the_list_already_held() {
    let mut rt = seen_by("ryan");
    open(&mut rt, "members");
    let row = |who: &str| format!("roster-{}", person_id(who));
    for who in ["ryan", "maya", "devon", "sam", "priya", "leo", "jordan"] {
        assert!(rt.find(&row(who)).is_some(), "{who} is not listed");
    }
    assert_eq!(
        label_of(&rt, &row("ryan")),
        "Ryan Winchester, Owner, Founder"
    );
    assert_eq!(
        label_of(&rt, &row("maya")),
        "Maya Chen, Head of Design, Admin, Design"
    );
    assert_eq!(
        label_of(&rt, &row("jordan")),
        "Jordan Lee, Guest, Contractor"
    );

    press(&mut rt, "members-search");
    type_text(&mut rt, "ENGINEER");
    assert!(rt.find(&row("devon")).is_some() && rt.find(&row("sam")).is_some());
    assert!(rt.find(&row("maya")).is_none());
    rt.update(|s, _| s.roles.search.replace_text("@leo"));
    settle(&mut rt);
    assert!(rt.find(&row("leo")).is_some() && rt.find(&row("devon")).is_none());
    rt.update(|s, _| s.roles.search.replace_text("nobody"));
    settle(&mut rt);
    assert!(words(&rt).contains("No member matches that."));
}

#[test]
fn a_members_row_opens_to_their_roles_title_and_kick_and_saves_what_changed() {
    let mut rt = seen_by("ryan");
    open(&mut rt, "members");
    let devon = person_id("devon");
    press(&mut rt, &format!("roster-{devon}"));
    let open = rt.state.roles.member.as_ref().expect("the row opened");
    assert_eq!(open.title.text, "Staff Engineer");
    assert!(words(&rt).contains("Up to 32 characters. A title grants nothing."));
    assert_eq!(label_of(&rt, "member-kick"), "Kick Devon Park");

    // Design ticked, Moderator unticked, and a new title.
    press(&mut rt, &format!("member-role-{}", role_id("design")));
    press(&mut rt, &format!("member-role-{}", role_id("mod")));
    rt.update(|s, _| s.roles.member_title().replace_text(" Principal Engineer "));
    settle(&mut rt);
    press(&mut rt, "member-save");
    assert!(rt.state.roles.member.as_ref().unwrap().busy);

    answer(
        &mut rt,
        RoleUpdate::Member {
            server_id: ACME.into(),
            account_id: devon.clone(),
            result: Err(refused("INVALID_ARGUMENT", Some("title"))),
        },
    );
    assert!(words(&rt).contains("A title is up to 32 characters, on one line."));
    assert!(
        rt.state.roles.member.is_some(),
        "a refusal leaves the row open"
    );

    press(&mut rt, "member-save");
    let mut now = member(&rt, "devon");
    now.role_ids = vec![role_id("design"), role_id("eng")];
    now.title = "Principal Engineer".into();
    now.tag = Some(TagSummary {
        label: "Design".into(),
        color: "peach".into(),
    });
    answer(
        &mut rt,
        RoleUpdate::Member {
            server_id: ACME.into(),
            account_id: devon.clone(),
            result: Ok(now),
        },
    );
    // Saved: the row closes and shows it.
    assert!(rt.state.roles.member.is_none());
    assert_eq!(
        label_of(&rt, &format!("roster-{devon}")),
        "Devon Park, Principal Engineer, Design, Engineering"
    );

    // Cancel keeps nothing; a Save with nothing changed closes the row and sends nothing.
    press(&mut rt, &format!("roster-{devon}"));
    press(&mut rt, &format!("member-role-{}", role_id("admin")));
    press(&mut rt, "member-cancel");
    assert!(rt.state.roles.member.is_none());
    press(&mut rt, &format!("roster-{devon}"));
    press(&mut rt, "member-save");
    assert!(rt.state.roles.member.is_none());
}

#[test]
fn kick_asks_nothing_first_and_the_member_is_gone() {
    let mut rt = seen_by("ryan");
    open(&mut rt, "members");
    let leo = person_id("leo");
    press(&mut rt, &format!("roster-{leo}"));
    press(&mut rt, "member-kick");
    assert_eq!(rt.state.popover, None, "Kick asks nothing first");
    assert!(rt.state.roles.member.as_ref().unwrap().busy);
    answer(
        &mut rt,
        RoleUpdate::Kicked {
            server_id: ACME.into(),
            account_id: leo.clone(),
            result: Err(refused("PERMISSION_DENIED", None)),
        },
    );
    assert!(
        words(&rt)
            .contains("That isn’t yours to change. They may be at or above your own highest role.")
    );
    press(&mut rt, "member-kick");
    answer(
        &mut rt,
        RoleUpdate::Kicked {
            server_id: ACME.into(),
            account_id: leo.clone(),
            result: Ok(()),
        },
    );
    assert!(rt.find(&format!("roster-{leo}")).is_none());
    assert!(rt.state.session.member_in(ACME, &leo).is_none());
    assert!(
        !rt.state.session.channel_members["general"].contains(&leo),
        "out of the server's channels too"
    );
}

#[test]
fn a_row_offers_what_the_account_may_change_of_that_member() {
    let mut rt = seen_by("devon");
    open(&mut rt, "members");
    // Sam is below Devon: roles (Admin and Moderator locked), the title and Kick.
    press(&mut rt, &format!("roster-{}", person_id("sam")));
    assert!(rt.find("member-title").is_some() && rt.find("member-kick").is_some());
    assert_eq!(
        label_of(&rt, &format!("member-role-{}", role_id("admin"))),
        "Admin, not yours to give or take"
    );
    rt.update(|s, _| s.tick_member_role(&role_id("admin")));
    assert!(
        !rt.state
            .roles
            .member
            .as_ref()
            .unwrap()
            .roles
            .contains(&role_id("admin"))
    );
    press(&mut rt, "member-cancel");

    // Maya is above him: her roles below his own, and nothing else.
    press(&mut rt, &format!("roster-{}", person_id("maya")));
    assert!(
        rt.find(&format!("member-role-{}", role_id("eng")))
            .is_some()
    );
    assert!(rt.find("member-title").is_none() && rt.find("member-kick").is_none());
    press(&mut rt, "member-cancel");

    // His own row: his title is his own, and he is never offered Kick.
    press(&mut rt, &format!("roster-{}", person_id("devon")));
    assert!(rt.find("member-title").is_some() && rt.find("member-kick").is_none());
    press(&mut rt, "member-cancel");

    // A guest: a title and Kick, and why no roles.
    press(&mut rt, &format!("roster-{}", person_id("jordan")));
    assert!(words(&rt).contains("Guests aren’t given roles."));
    assert!(
        rt.find(&format!("member-role-{}", role_id("eng")))
            .is_none()
    );
    assert!(rt.find("member-title").is_some() && rt.find("member-kick").is_some());
}

#[test]
fn a_narrow_window_folds_a_members_editing_state() {
    let mut rt = seen_by("ryan");
    open(&mut rt, "members");
    press(&mut rt, &format!("roster-{}", person_id("devon")));
    let top_of = |rt: &Runtime<State>, key: &str| rt.presented(key).unwrap().0[1];
    let last_role = format!("member-role-{}", role_id("invite"));
    // With room, the title stands beside the roles.
    assert!(top_of(&rt, "member-title") < top_of(&rt, &last_role));
    // Too narrow for the aside in the row, it goes under them.
    rt.update(|s, _| s.set_window_width(900.0));
    settle(&mut rt);
    let target = "member-title".to_string();
    rt.update(move |_, tx| tx.scroll_into_view(target));
    settle(&mut rt);
    assert!(top_of(&rt, "member-title") > top_of(&rt, &last_role));
}

// ---- The tag beside a name ----------------------------------------------------------------

/// The words of every tag drawn beside a message's author.
fn message_tags(rt: &Runtime<State>) -> Vec<String> {
    rt.elements()
        .into_iter()
        .filter(|e| e.path.contains("/messages") && e.path.contains("/tag"))
        .filter_map(|e| e.text)
        .collect()
}

fn says(state: &mut State, who: &[&str]) {
    let now = chrono::Utc::now().timestamp_micros();
    state.session.entries = who
        .iter()
        .enumerate()
        .map(|(i, who)| {
            message(
                &format!("t{i}"),
                &person_id(who),
                "Morning.",
                now - (10 - i as i64) * 600_000_000,
            )
        })
        .collect();
    state.session.revision += 1;
}

#[test]
fn a_role_is_the_tag_beside_a_name_in_messages_and_the_member_list() {
    let mut state = opened();
    acme(&mut state, &Acme::seen_by("ryan"));
    says(&mut state, &["maya", "devon", "leo", "ryan", "jordan"]);
    let mut rt = runtime(state);
    // Maya's highest role and Devon's; Leo's only role is not shown as a tag, the owner has
    // none for being the owner, and a guest holds no role.
    assert_eq!(message_tags(&rt), ["Admin", "Moderator"]);
    assert_eq!(
        label_of(&rt, &format!("member-{}", person_id("maya"))),
        "Maya Chen, Admin, online"
    );
    assert_eq!(
        label_of(&rt, &format!("member-{}", person_id("leo"))),
        "Leo Martins, offline"
    );

    // One line draws no tag.
    rt.update(|s, _| s.set_layout(Layout::OneLine));
    settle(&mut rt);
    assert!(message_tags(&rt).is_empty());
}

#[test]
fn a_title_is_the_tag_where_the_server_shows_titles_and_nothing_stands_in_for_one() {
    let mut state = opened();
    acme(
        &mut state,
        &Acme {
            titles: true,
            ..Acme::seen_by("ryan")
        },
    );
    says(&mut state, &["maya", "priya", "ryan"]);
    let rt = runtime(state);
    // Priya has no title: no tag, and her Design role does not stand in for it.
    assert_eq!(message_tags(&rt), ["Head of Design", "Founder"]);
    assert_eq!(
        label_of(&rt, &format!("member-{}", person_id("priya"))),
        "Priya Natarajan, offline"
    );
    assert_eq!(
        label_of(&rt, &format!("member-{}", person_id("devon"))),
        "Devon Park, Staff Engineer, away"
    );
}

/// The words drawn in someone's row of the member list.
fn row_words(rt: &Runtime<State>, who: &str) -> Vec<String> {
    let row = format!("/member-{}/", person_id(who));
    rt.elements()
        .into_iter()
        .filter(|e| e.path.contains(&row))
        .filter_map(|e| e.text)
        .collect()
}

#[test]
fn this_accounts_own_row_says_you_only_where_it_has_no_tag() {
    // Devon's tag stands where "You" would.
    let rt = seen_by("devon");
    let own = row_words(&rt, "devon");
    assert!(own.contains(&"Moderator".to_string()), "{own:?}");
    assert!(!own.contains(&"You".to_string()), "{own:?}");
    // Ryan owns Acme and holds no role: no tag, so "You".
    let rt = seen_by("ryan");
    let own = row_words(&rt, "ryan");
    assert!(own.contains(&"You".to_string()), "{own:?}");
}

// ---- The profile card ---------------------------------------------------------------------

fn card(rt: &mut Runtime<State>, who: &str) {
    press(rt, &format!("member-{}", person_id(who)));
    assert_eq!(rt.state.popover, Some(Popover::Person));
}

#[test]
fn the_card_shows_the_title_every_role_and_who_the_owner_is() {
    let mut rt = seen_by("sam");
    card(&mut rt, "devon");
    let shown = words(&rt);
    for part in ["Staff Engineer", "Roles", "Moderator", "Engineering"] {
        assert!(shown.contains(part), "{part}");
    }
    // Sam may change nothing of Devon.
    assert!(rt.find("card-edit-title").is_none() && rt.find("card-edit-roles").is_none());
    press(&mut rt, "card-close");

    // A role that is not shown as a tag is still listed.
    card(&mut rt, "leo");
    assert!(words(&rt).contains("Can invite"));
    press(&mut rt, "card-close");

    // The owner is marked, and holds no roles to list.
    card(&mut rt, "ryan");
    let shown = words(&rt);
    assert!(shown.contains("Owner") && shown.contains("Founder"));
    assert!(rt.find("card-roles-held").is_none());
    press(&mut rt, "card-close");

    // Sam's own: the title is Sam's to edit, the roles are not.
    card(&mut rt, "sam");
    assert_eq!(label_of(&rt, "card-edit-title"), "Edit title");
    assert!(rt.find("card-edit-roles").is_none());
}

#[test]
fn a_member_with_no_title_is_offered_one_on_their_own_card() {
    let mut rt = seen_by("priya");
    card(&mut rt, "priya");
    assert!(words(&rt).contains("Add a title"));
    press(&mut rt, "card-edit-title");
    assert_eq!(rt.state.popover, Some(Popover::CardTitle));
    let shown = words(&rt);
    assert!(shown.contains("Priya Natarajan in Acme"));
    assert!(shown.contains("Up to 32 characters. A title grants nothing."));
    press(&mut rt, "card-title");
    type_text(&mut rt, "Designer");
    press(&mut rt, "card-form-save");
    assert!(rt.state.roles.card.as_ref().unwrap().member.busy);
    let mut now = member(&rt, "priya");
    now.title = "Designer".into();
    answer(
        &mut rt,
        RoleUpdate::Member {
            server_id: ACME.into(),
            account_id: person_id("priya"),
            result: Ok(now),
        },
    );
    // Back to the card, which has it.
    assert_eq!(rt.state.popover, Some(Popover::Person));
    let title = rt
        .elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some("card-title"))
        .and_then(|e| e.text);
    assert_eq!(title.as_deref(), Some("Designer"));
    assert_eq!(label_of(&rt, "card-edit-title"), "Edit title");
}

#[test]
fn the_cards_edit_opens_the_roles_to_tick_and_cancel_goes_back_to_the_card() {
    let mut rt = seen_by("ryan");
    card(&mut rt, "devon");
    assert_eq!(label_of(&rt, "card-edit-roles"), "Edit roles");
    press(&mut rt, "card-edit-roles");
    assert_eq!(rt.state.popover, Some(Popover::CardRoles));
    assert!(words(&rt).contains("Devon Park in Acme"));
    press(&mut rt, &format!("card-role-{}", role_id("design")));
    press(&mut rt, "card-form-cancel");
    assert_eq!(rt.state.popover, Some(Popover::Person));
    assert!(rt.state.roles.card.is_none());

    press(&mut rt, "card-edit-roles");
    press(&mut rt, &format!("card-role-{}", role_id("design")));
    press(&mut rt, "card-form-save");
    answer(
        &mut rt,
        RoleUpdate::Member {
            server_id: ACME.into(),
            account_id: person_id("devon"),
            result: Err(refused("PERMISSION_DENIED", None)),
        },
    );
    assert_eq!(rt.state.popover, Some(Popover::CardRoles));
    assert!(words(&rt).contains("That isn’t yours to change."));
    press(&mut rt, "card-form-save");
    let mut now = member(&rt, "devon");
    now.role_ids.push(role_id("design"));
    answer(
        &mut rt,
        RoleUpdate::Member {
            server_id: ACME.into(),
            account_id: person_id("devon"),
            result: Ok(now),
        },
    );
    assert_eq!(rt.state.popover, Some(Popover::Person));
    let roles: Vec<String> = rt
        .elements()
        .into_iter()
        .filter(|e| e.path.contains("/card-roles-held/"))
        .filter_map(|e| e.text)
        .collect();
    assert_eq!(roles, ["Moderator", "Design", "Engineering"]);

    // A guest has a title to edit and no roles to give.
    press(&mut rt, "card-close");
    card(&mut rt, "jordan");
    assert!(words(&rt).contains("Guest"));
    assert!(rt.find("card-edit-title").is_some() && rt.find("card-edit-roles").is_none());
}
