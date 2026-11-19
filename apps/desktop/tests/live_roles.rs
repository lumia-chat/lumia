//! Roles, titles and tags against a real instance (architecture 7.5), through the session
//! and the state the screens call, on two shells: a server's owner and a member who joined by
//! invite. The owner makes a role, is refused a second of the same name, and gives the first
//! to the member from Members; the member's own shell tags them from its account log, with
//! nothing asked. The member sets a title on their own card and the owner's shell has it; the
//! owner has the server show titles, and back; a second role is moved above the first; the
//! first is deleted, asked first, and the tag goes on both; and the member is kicked. Runs
//! only with `LUMIA_DEV_INSTANCE` set; without it it passes having checked nothing, and says
//! so. It makes its own accounts, so it needs an instance with open registration.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::path::Path;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use lumia_shell::app::role_settings::CardForm;
use lumia_shell::app::roles::{self, Section};
use lumia_shell::app::{HomeView, Place, Popover, Route, State};
use lumia_shell::session::Connection;
use rune::TextEdit;

const PASSWORD: &str = "correct-horse-battery-staple";

fn wait(state: &mut State, what: &str, done: impl Fn(&State) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(20);
    while !done(state) {
        assert!(
            Instant::now() < deadline,
            "timed out waiting for {what}; session error {:?}, list {:?}, editor {:?}, member {:?}, card {:?}, tag {:?}",
            state.session.last_error,
            state.roles.list_error,
            state.roles.editor.as_ref().map(|e| &e.saving),
            state.roles.member.as_ref().map(|m| &m.error),
            state.roles.card.as_ref().map(|c| &c.member.error),
            state.server_forms.settings.tag,
        );
        state.drain();
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// Lets both shells take what has arrived, until `done` of the two.
fn wait_both(a: &mut State, b: &mut State, what: &str, done: impl Fn(&State, &State) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(20);
    while !done(a, b) {
        assert!(
            Instant::now() < deadline,
            "timed out waiting for {what}; session errors {:?} and {:?}",
            a.session.last_error,
            b.session.last_error,
        );
        a.drain();
        b.drain();
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// A state over `dir`, signed in to a new account on `instance`.
fn new_account(instance: &str, dir: &Path, handle: &str) -> State {
    let mut state = State::new(dir.to_path_buf());
    state.sign_in.instance = TextEdit::new(instance);
    state.connect();
    wait(&mut state, "the trust card", |s| s.session.info.is_some());
    state.sign_in.creating = true;
    state.sign_in.handle = TextEdit::new(handle);
    state.sign_in.password.replace_text(PASSWORD);
    state.submit();
    wait(&mut state, "the shell", |s| s.route == Route::Shell);
    wait(&mut state, "the first snapshot", |s| {
        s.session.snapshot.is_some()
    });
    state
}

/// The label and colour of someone's tag in a server, as a shell holds it.
fn tag(state: &State, server_id: &str, account_id: &str) -> Option<(String, String)> {
    state
        .session
        .tag_in(server_id, account_id)
        .map(|tag| (tag.label.clone(), tag.color.clone()))
}

/// The server's roles by name, as a shell's snapshot has them.
fn role_names(state: &State, server_id: &str) -> Vec<String> {
    state
        .session
        .server(server_id)
        .map(|server| server.roles.iter().map(|r| r.name.clone()).collect())
        .unwrap_or_default()
}

/// Makes a role in the editor of the server whose settings are open, and answers its id.
fn make_role(owner: &mut State, name: &str, color: &str) -> String {
    owner.choose_section(Section::Roles);
    owner.new_role();
    owner.roles.role_name().replace_text(name);
    owner.choose_role_color(color);
    assert!(owner.role_savable());
    owner.save_role();
    wait(owner, "the new role's editor", |s| {
        s.roles
            .editor
            .as_ref()
            .is_some_and(|e| e.saving.saved && e.role_id.is_some())
    });
    let id = owner
        .roles
        .editor
        .as_ref()
        .and_then(|e| e.role_id.clone())
        .unwrap();
    owner.close_role();
    id
}

#[test]
fn roles_titles_and_tags_are_managed_and_reach_the_other_shell() {
    let Ok(instance) = std::env::var("LUMIA_DEV_INSTANCE") else {
        eprintln!("skipped: no LUMIA_DEV_INSTANCE");
        return;
    };
    let dir = std::env::temp_dir().join(format!("lumia-shell-roles-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis()
        % 100_000_000;
    let server_name = format!("Shell roles {stamp}");

    // ---- A server, a channel in it, and a member who joins by invite ------------------
    let mut owner = new_account(&instance, &dir.join("owner"), &format!("rolown{stamp}"));
    owner.go_home(HomeView::AddServer);
    owner.server_forms.add.name.replace_text(&server_name);
    owner.create_server();
    wait(&mut owner, "the new server on screen", |s| {
        matches!(s.place, Place::Server(_))
    });
    let server_id = owner.server_id().unwrap().to_string();
    owner.open_new_channel();
    owner.server_forms.channel.name.replace_text("general");
    owner.create_channel();
    wait(&mut owner, "the new channel on screen", |s| {
        s.session.channel().is_some()
    });
    owner.open_invites();
    owner.make_invite();
    wait(&mut owner, "the invite link", |s| {
        s.server_forms.invite.made.is_some()
    });
    let invite = owner.server_forms.invite.made.clone().unwrap();
    owner.close_popover();

    let mut member = new_account(&instance, &dir.join("member"), &format!("rolmem{stamp}"));
    member.go_home(HomeView::AddServer);
    member.server_forms.add.invite.replace_text(&invite.url);
    member.look_up_invite();
    wait(&mut member, "the invite's preview", |s| {
        s.server_forms.add.preview.is_some()
    });
    member.join_invite();
    wait(&mut member, "the joined server on screen", |s| {
        s.server_id() == Some(server_id.as_str())
    });
    let (owner_id, member_id) = (
        owner.session.me().to_string(),
        member.session.me().to_string(),
    );
    wait_both(&mut owner, &mut member, "both gateways", |a, b| {
        a.session.connection == Connection::Connected
            && b.session.connection == Connection::Connected
    });

    // ---- What each is offered ----------------------------------------------------------
    // A new server has Everyone alone, and its owner has every section.
    assert_eq!(role_names(&owner, &server_id), ["Everyone"]);
    let mine = owner.session.server(&server_id).cloned().unwrap();
    assert!(mine.is_owner && mine.can_manage_roles && mine.can_manage_members);
    assert_eq!(
        roles::settings_sections(&mine),
        [Section::Overview, Section::Roles, Section::Members]
    );
    assert!(owner.settings_offered());
    assert!(
        !member.settings_offered(),
        "someone who joined by invite manages nothing"
    );
    owner.open_server_settings();
    assert_eq!(owner.server_forms.settings.section, Section::Overview);
    // Opening the settings asks for the members again: the one who joined is there.
    let listed = (server_id.clone(), member_id.clone());
    wait(&mut owner, "the member in the owner's list", move |s| {
        s.session.member_in(&listed.0, &listed.1).is_some()
    });
    let owned = owner.session.member_in(&server_id, &owner_id).unwrap();
    assert!(owned.owner && owned.tag.is_none(), "the owner has no tag");
    let theirs = owner.session.member_in(&server_id, &member_id).unwrap();
    assert!(theirs.can_manage && !theirs.owner && !theirs.guest);

    // ---- A role is made; a second of its name is refused ------------------------------
    let support = make_role(&mut owner, "Support", "sky");
    assert_eq!(role_names(&owner, &server_id), ["Support", "Everyone"]);
    owner.new_role();
    owner.roles.role_name().replace_text("SUPPORT");
    owner.save_role();
    wait(&mut owner, "the refusal", |s| {
        s.roles
            .editor
            .as_ref()
            .is_some_and(|e| !e.saving.busy && !e.saving.error.is_empty())
    });
    assert_eq!(
        owner.roles.editor.as_ref().unwrap().saving.error,
        format!("Another role in {server_name} has that name.")
    );
    owner.close_role();
    // The member's shell has the role from its account log.
    let named = server_id.clone();
    wait(&mut member, "the role on the member's shell", move |s| {
        role_names(s, &named) == ["Support", "Everyone"]
    });

    // ---- The role is given from Members, and tags the member on both shells -----------
    owner.choose_section(Section::Members);
    owner.toggle_member(&member_id);
    assert!(owner.roles.member.is_some(), "the member's row opens");
    owner.tick_member_role(&support);
    owner.save_member();
    wait(&mut owner, "the member's roles saved", |s| {
        s.roles.member.is_none()
    });
    let sky = Some(("Support".to_string(), "sky".to_string()));
    assert_eq!(tag(&owner, &server_id, &member_id), sky);
    let (at, who, wanted) = (server_id.clone(), member_id.clone(), sky.clone());
    wait(&mut member, "the tag on the member's own shell", move |s| {
        tag(s, &at, &who) == wanted
    });
    // And the snapshot it took again says which roles are its own.
    let (at, role) = (server_id.clone(), support.clone());
    wait(&mut member, "the member's own roles", move |s| {
        s.session
            .server(&at)
            .is_some_and(|server| server.role_ids.contains(&role))
    });

    // ---- A title, set by the member on their own card ---------------------------------
    member.select_server(&server_id);
    member.open_person(&member_id, "member-self");
    let (_, me) = member.card_member().expect("the member's own card");
    let offers = roles::member_offers(member.session.server(&server_id).unwrap(), me, true);
    assert!(offers.title && !offers.roles && !offers.kick);
    member.edit_on_card(CardForm::Title);
    assert_eq!(member.popover, Some(Popover::CardTitle));
    member.roles.card_title().replace_text("Staff Engineer");
    member.save_card_form();
    wait(&mut member, "the title saved", |s| {
        s.popover == Some(Popover::Person) && s.roles.card.is_none()
    });
    let (at, who) = (server_id.clone(), member_id.clone());
    wait(&mut owner, "the title on the owner's shell", move |s| {
        s.session
            .member_in(&at, &who)
            .is_some_and(|m| m.title == "Staff Engineer")
    });
    // The server shows roles, so the tag is still the role.
    assert_eq!(tag(&owner, &server_id, &member_id), sky);

    // ---- The server shows titles, and back ---------------------------------------------
    owner.choose_section(Section::Overview);
    owner.choose_tag_titles(true);
    assert!(owner.server_forms.settings.tag.busy);
    let titled = Some(("Staff Engineer".to_string(), String::new()));
    let (at, who, wanted) = (server_id.clone(), member_id.clone(), titled.clone());
    wait_both(&mut owner, &mut member, "titles on both shells", |a, b| {
        tag(a, &at, &who) == wanted && tag(b, &at, &who) == wanted
    });
    assert!(owner.session.server(&server_id).unwrap().tag_titles);
    assert!(
        tag(&owner, &server_id, &owner_id).is_none(),
        "no title, no tag: nothing stands in for one"
    );
    owner.choose_tag_titles(false);
    let (at, who, wanted) = (server_id.clone(), member_id.clone(), sky.clone());
    wait_both(
        &mut owner,
        &mut member,
        "roles again on both shells",
        |a, b| tag(a, &at, &who) == wanted && tag(b, &at, &who) == wanted,
    );

    // ---- A second role, moved above the first -----------------------------------------
    let helpers = make_role(&mut owner, "Helpers", "");
    assert_eq!(
        role_names(&owner, &server_id),
        ["Support", "Helpers", "Everyone"]
    );
    let listed = owner.roles_listed();
    assert!(roles::lands_before(&listed, &helpers, &support));
    owner
        .session
        .order_roles(&server_id, roles::moved_before(&listed, &helpers, &support));
    // Drawn moved at once, before the instance answers.
    assert_eq!(
        owner
            .roles_listed()
            .iter()
            .map(|r| r.name.as_str())
            .collect::<Vec<_>>(),
        ["Helpers", "Support", "Everyone"]
    );
    let at = server_id.clone();
    wait_both(
        &mut owner,
        &mut member,
        "the order on both shells",
        |a, b| {
            a.session.roles.pending.is_none()
                && role_names(a, &at) == ["Helpers", "Support", "Everyone"]
                && role_names(b, &at) == ["Helpers", "Support", "Everyone"]
        },
    );
    assert_eq!(owner.roles.list_error, "");

    // ---- A role deleted, asked first: its tag goes everywhere -------------------------
    owner.choose_section(Section::Roles);
    owner.open_role(&support);
    assert_eq!(
        owner.edited_role().and_then(|r| owner.holders_of(r)),
        Some(1),
        "one member holds it"
    );
    owner.ask_delete_role(&support, "delete-role");
    assert_eq!(owner.popover, Some(Popover::DeleteRole));
    owner.confirm_delete_role();
    wait(&mut owner, "the role deleted", |s| {
        s.roles.editor.is_none() && s.popover.is_none()
    });
    assert_eq!(role_names(&owner, &server_id), ["Helpers", "Everyone"]);
    let (at, who) = (server_id.clone(), member_id.clone());
    wait_both(
        &mut owner,
        &mut member,
        "the tag gone on both shells",
        |a, b| {
            tag(a, &at, &who).is_none()
                && tag(b, &at, &who).is_none()
                && role_names(b, &at) == ["Helpers", "Everyone"]
        },
    );

    // ---- Kick: nothing is asked first, and the member's shell leaves the server -------
    owner.choose_section(Section::Members);
    owner.toggle_member(&member_id);
    owner.kick_member();
    assert_eq!(owner.popover, None);
    let (at, who) = (server_id.clone(), member_id.clone());
    wait(
        &mut owner,
        "the member gone from the owner's list",
        move |s| s.roles.member.is_none() && s.session.member_in(&at, &who).is_none(),
    );
    let at = server_id.clone();
    wait(
        &mut member,
        "the server gone from the member's shell",
        move |s| s.session.server(&at).is_none(),
    );

    let _ = std::fs::remove_dir_all(&dir);
}
