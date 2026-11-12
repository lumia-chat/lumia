//! Making and managing servers, over a state no instance is behind: add a server (create
//! one, or join by invite with its preview first), a new channel, invite people. What the
//! instance would answer is applied with `State::receive`; `tests/live_servers.rs` is the
//! same flows against a real instance.

#![allow(clippy::expect_used, clippy::unwrap_used)]

mod common;

use std::collections::HashMap;

use chrono::{DateTime, Local};
use common::*;
use lumia_client::app::{
    ChannelSummary, InvitePreviewSummary, InviteSummary, ServerSummary, SnapshotSummary,
};
use lumia_shell::app::{HomeView, Place, Popover, State};
use lumia_shell::session::servers::ServerUpdate;
use lumia_shell::session::{Failure, Update};
use rune::{Key, KeyEvent, Mods, Runtime};

// ---- What these tests share ---------------------------------------------------------------

fn failure(code: &str, message: &str) -> Failure {
    Failure {
        code: code.into(),
        message: message.into(),
        details: HashMap::new(),
    }
}

/// Applies a server-management answer as if the instance had sent it.
fn answer(rt: &mut Runtime<State>, update: ServerUpdate) {
    rt.update(|s, _| s.receive(&Update::Server(update)));
    settle(rt);
}

fn snapshot_arrives(rt: &mut Runtime<State>, snapshot: SnapshotSummary) {
    rt.update(|s, _| s.receive(&Update::Snapshot(Ok(snapshot))));
    settle(rt);
}

fn key(rt: &mut Runtime<State>, key: Key) {
    rt.key(KeyEvent {
        key,
        mods: Mods::default(),
    });
    settle(rt);
}

fn focused(rt: &Runtime<State>) -> Option<String> {
    rt.elements()
        .into_iter()
        .find(|e| e.focused)
        .and_then(|e| e.key)
}

/// Whether the element keyed `key` is behind a modal surface.
fn inert(rt: &Runtime<State>, key: &str) -> bool {
    rt.elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some(key))
        .unwrap_or_else(|| panic!("{key} is not in the tree"))
        .inert
}

/// Scrolls the element keyed `key` into view, then presses it.
fn reveal_and_press(rt: &mut Runtime<State>, key: &str) {
    let target = key.to_string();
    rt.update(move |_, tx| tx.scroll_into_view(target));
    settle(rt);
    press(rt, key);
}

fn channel(id: &str, server: &str, name: &str) -> ChannelSummary {
    ChannelSummary {
        channel_id: id.into(),
        server_id: server.into(),
        name: name.into(),
        kind: "TEXT".into(),
        last_seq: 0,
        participant_account_ids: Vec::new(),
        host: String::new(),
    }
}

/// The usual snapshot with one more server and a channel in it.
fn snapshot_with_server(id: &str, name: &str) -> SnapshotSummary {
    let mut snapshot = snapshot();
    snapshot.servers.push(ServerSummary {
        server_id: id.into(),
        name: name.into(),
    });
    snapshot
        .channels
        .push(channel(&format!("{id}-general"), id, "general"));
    snapshot
}

fn preview() -> InvitePreviewSummary {
    InvitePreviewSummary {
        code: "k3v9x2".into(),
        server_id: "club".into(),
        server_name: "Chess Club".into(),
        channel_name: String::new(),
        member_count: 12,
        as_guest: false,
        host: "chat.example.com".into(),
        operator_name: "Example Co".into(),
        open_registration: true,
        identity_proof_create_enabled: false,
    }
}

fn invite(
    code: &str,
    max_uses: u32,
    uses: u32,
    expires: Option<i64>,
    email: &str,
) -> InviteSummary {
    InviteSummary {
        code: code.into(),
        url: format!("https://chat.example.com/invite/{code}"),
        server_id: "acme".into(),
        expires_at_micros: expires,
        max_uses,
        uses,
        email: email.into(),
    }
}

/// Signed in, on Home's "Add a server".
fn adding() -> Runtime<State> {
    let mut rt = runtime(signed_in());
    press(&mut rt, "add-server");
    assert_eq!(rt.state.place, Place::Home);
    assert_eq!(rt.state.home_view, HomeView::AddServer);
    rt
}

/// Signed in, in Acme Works, with "Invite people" open.
fn inviting() -> Runtime<State> {
    let mut rt = runtime(signed_in());
    open_invites(&mut rt);
    assert_eq!(rt.state.popover, Some(Popover::Invite));
    rt
}

// ---- Add a server -------------------------------------------------------------------------

#[test]
fn adding_a_server_is_one_page_with_a_card_to_create_and_a_card_to_join() {
    let rt = adding();
    let shown = words(&rt);
    for expected in [
        "Add a server",
        "Your own, or one you were invited to",
        "Create a server",
        "You own it; invite people once it exists.",
        "Join with an invite",
        "Paste the link someone sent you; what it leads to shows first.",
        "Invite link or code",
        "Look it up",
    ] {
        assert!(shown.contains(expected), "{expected:?} in:\n{shown}");
    }
    for key in [
        "create-server-card",
        "server-name",
        "create-server",
        "join-server-card",
        "invite-link",
        "look-up-invite",
    ] {
        assert!(rt.find(key).is_some(), "{key} is on screen");
    }
    assert!(
        rt.find("join-server").is_none(),
        "nothing to join before an invite is looked up"
    );
}

#[test]
fn a_server_is_created_by_name_and_opens_once_the_snapshot_lists_it() {
    let mut rt = adding();
    // Nothing is asked for without a name.
    press(&mut rt, "create-server");
    assert!(!rt.state.server_forms.add.creating);

    press(&mut rt, "server-name");
    type_text(&mut rt, "Our place");
    press(&mut rt, "create-server");
    assert!(rt.state.server_forms.add.creating);
    // A second press while the instance is asked does nothing more.
    press(&mut rt, "create-server");

    answer(
        &mut rt,
        ServerUpdate::ServerCreated(Ok(ServerSummary {
            server_id: "ours".into(),
            name: "Our place".into(),
        })),
    );
    assert!(!rt.state.server_forms.add.creating);
    assert_eq!(rt.state.server_forms.add.name.text, "");
    assert_eq!(
        rt.state.place,
        Place::Home,
        "the snapshot does not list it yet"
    );

    // A snapshot asked for before the server existed arrives first.
    snapshot_arrives(&mut rt, snapshot());
    assert_eq!(rt.state.place, Place::Home);
    assert_eq!(rt.state.home_view, HomeView::AddServer);

    snapshot_arrives(&mut rt, snapshot_with_server("ours", "Our place"));
    assert_eq!(rt.state.server_id(), Some("ours"));
    assert_eq!(rt.state.session.channel(), Some("ours-general"));
    assert!(rt.find("server-ours").is_some() && rt.find("channel-ours-general").is_some());

    // Landing happens once: a later snapshot leaves the person where they went.
    press(&mut rt, "server-acme");
    snapshot_arrives(&mut rt, snapshot_with_server("ours", "Our place"));
    assert_eq!(rt.state.server_id(), Some("acme"));
}

#[test]
fn return_in_the_name_field_creates_the_server() {
    let mut rt = adding();
    press(&mut rt, "server-name");
    type_text(&mut rt, "Our place");
    key(&mut rt, Key::Enter);
    assert!(rt.state.server_forms.add.creating);
}

#[test]
fn a_server_the_instance_refuses_says_why_and_keeps_the_name() {
    let mut rt = adding();
    press(&mut rt, "server-name");
    type_text(&mut rt, "Our place");
    press(&mut rt, "create-server");
    answer(
        &mut rt,
        ServerUpdate::ServerCreated(Err(failure(
            "PERMISSION_DENIED",
            "this instance does not let you create servers",
        ))),
    );
    assert!(words(&rt).contains("this instance does not let you create servers"));
    assert_eq!(rt.state.server_forms.add.name.text, "Our place");
    assert!(!rt.state.server_forms.add.creating, "it can be tried again");
    assert_eq!(rt.state.home_view, HomeView::AddServer);
}

#[test]
fn someone_who_went_elsewhere_is_not_pulled_into_the_server_they_made() {
    let mut rt = adding();
    press(&mut rt, "server-name");
    type_text(&mut rt, "Our place");
    press(&mut rt, "create-server");
    press(&mut rt, "server-garden");
    answer(
        &mut rt,
        ServerUpdate::ServerCreated(Ok(ServerSummary {
            server_id: "ours".into(),
            name: "Our place".into(),
        })),
    );
    snapshot_arrives(&mut rt, snapshot_with_server("ours", "Our place"));
    assert_eq!(rt.state.server_id(), Some("garden"));
    assert!(
        rt.find("server-ours").is_some(),
        "it is on the rail all the same"
    );
}

#[test]
fn an_invite_shows_what_it_leads_to_before_anything_is_joined() {
    let mut rt = adding();
    press(&mut rt, "look-up-invite");
    assert!(
        !rt.state.server_forms.add.busy,
        "nothing is looked up without a link"
    );

    press(&mut rt, "invite-link");
    type_text(&mut rt, "https://chat.example.com/invite/k3v9x2");
    press(&mut rt, "look-up-invite");
    assert!(rt.state.server_forms.add.busy);
    assert!(rt.find("join-server").is_none());

    answer(&mut rt, ServerUpdate::InvitePreview(Ok(preview())));
    assert!(!rt.state.server_forms.add.busy);
    let shown = words(&rt);
    assert!(
        shown.contains(
            "Chess Club · 12 members on chat.example.com, run by Example Co. Your account on this instance joins it; nothing else is shared."
        ),
        "{shown}"
    );
    assert!(shown.contains("Join Chess Club"), "{shown}");
    assert_eq!(rt.state.place, Place::Home, "a preview joins nothing");

    // One member, a guest membership, and an operator who gave no name.
    answer(
        &mut rt,
        ServerUpdate::InvitePreview(Ok(InvitePreviewSummary {
            member_count: 1,
            as_guest: true,
            operator_name: String::new(),
            ..preview()
        })),
    );
    let shown = words(&rt);
    assert!(
        shown.contains(
            "Chess Club · 1 member on chat.example.com, run by an unnamed operator · you would join as a guest."
        ),
        "{shown}"
    );
}

#[test]
fn an_invite_that_leads_nowhere_says_so() {
    let mut rt = adding();
    press(&mut rt, "invite-link");
    type_text(&mut rt, "nothing");
    press(&mut rt, "look-up-invite");
    answer(
        &mut rt,
        ServerUpdate::InvitePreview(Err(failure("NOT_FOUND", "invite not found"))),
    );
    assert!(words(&rt).contains("That invite is unknown, expired or used up."));
    assert!(rt.find("join-server").is_none());
    assert!(!rt.state.server_forms.add.busy);

    // Looking again clears what the last look said, and any other failure reads as given.
    press(&mut rt, "look-up-invite");
    assert_eq!(rt.state.server_forms.add.error, "");
    answer(
        &mut rt,
        ServerUpdate::InvitePreview(Err(failure("", "connection refused"))),
    );
    assert!(words(&rt).contains("connection refused"));
}

#[test]
fn a_new_look_takes_the_old_preview_away() {
    let mut rt = adding();
    press(&mut rt, "invite-link");
    type_text(&mut rt, "k3v9x2");
    press(&mut rt, "look-up-invite");
    answer(&mut rt, ServerUpdate::InvitePreview(Ok(preview())));
    assert!(rt.find("join-server").is_some());
    press(&mut rt, "look-up-invite");
    assert!(rt.state.server_forms.add.preview.is_none());
    assert!(rt.find("join-server").is_none());
}

#[test]
fn joining_opens_the_server_once_the_snapshot_lists_it() {
    let mut rt = adding();
    press(&mut rt, "invite-link");
    type_text(&mut rt, "k3v9x2");
    key(&mut rt, Key::Enter);
    assert!(
        rt.state.server_forms.add.busy,
        "Return in the field looks it up"
    );
    answer(&mut rt, ServerUpdate::InvitePreview(Ok(preview())));

    press(&mut rt, "join-server");
    assert!(rt.state.server_forms.add.busy);
    answer(
        &mut rt,
        ServerUpdate::InviteAccepted(Err(failure("NOT_FOUND", "that invite was just used up"))),
    );
    assert!(words(&rt).contains("that invite was just used up"));
    assert_eq!(rt.state.place, Place::Home);
    assert!(
        rt.find("join-server").is_some(),
        "the preview stays, to try again"
    );

    press(&mut rt, "join-server");
    assert_eq!(rt.state.server_forms.add.error, "");
    answer(&mut rt, ServerUpdate::InviteAccepted(Ok("club".into())));
    assert_eq!(rt.state.place, Place::Home, "not listed yet");
    assert_eq!(rt.state.server_forms.add.invite.text, "");
    assert!(rt.state.server_forms.add.preview.is_none());

    snapshot_arrives(&mut rt, snapshot_with_server("club", "Chess Club"));
    assert_eq!(rt.state.server_id(), Some("club"));
    assert_eq!(rt.state.session.channel(), Some("club-general"));
}

#[test]
fn an_invite_to_a_server_already_joined_opens_it_at_once() {
    let mut rt = adding();
    press(&mut rt, "invite-link");
    type_text(&mut rt, "k3v9x2");
    press(&mut rt, "look-up-invite");
    answer(
        &mut rt,
        ServerUpdate::InvitePreview(Ok(InvitePreviewSummary {
            server_id: "garden".into(),
            server_name: "Garden".into(),
            ..preview()
        })),
    );
    press(&mut rt, "join-server");
    answer(&mut rt, ServerUpdate::InviteAccepted(Ok("garden".into())));
    assert_eq!(rt.state.server_id(), Some("garden"));
}

// ---- The server's menu --------------------------------------------------------------------

#[test]
fn the_servers_name_opens_a_menu_with_inviting_people_and_a_new_channel() {
    let mut rt = runtime(signed_in());
    let shown = words(&rt);
    assert!(
        !shown.contains("New channel") && !shown.contains("Invite people"),
        "the sidebar itself holds neither: {shown}"
    );
    assert!(rt.state.popover.is_none());

    press(&mut rt, "server-title");
    assert_eq!(rt.state.popover, Some(Popover::ServerMenu));
    let menu = rt
        .elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some("server-menu"))
        .unwrap();
    assert_eq!(menu.role, "menu");
    assert_eq!(menu.label.as_deref(), Some("Acme Works"));
    let shown = words(&rt);
    assert!(shown.contains("New channel") && shown.contains("Invite people"));
    for behind in ["composer", "channel-design", "server-garden"] {
        assert!(inert(&rt, behind), "{behind} is inert behind the menu");
    }

    key(&mut rt, Key::Escape);
    assert!(rt.state.popover.is_none() && rt.find("server-menu").is_none());
    assert_eq!(focused(&rt).as_deref(), Some("server-title"));
}

#[test]
fn the_pencil_beside_a_servers_name_goes_to_a_new_message() {
    let mut rt = runtime(signed_in());
    press(&mut rt, "new-message");
    assert_eq!(rt.state.place, Place::Home);
    assert_eq!(rt.state.home_view, HomeView::NewMessage);
}

#[test]
fn a_server_with_no_channel_says_where_to_add_one() {
    let mut state = signed_in();
    let mut snapshot = snapshot();
    snapshot.servers.push(ServerSummary {
        server_id: "bare".into(),
        name: "Bare".into(),
    });
    state.receive(&Update::Snapshot(Ok(snapshot)));
    let mut rt = runtime(state);
    press(&mut rt, "server-bare");
    let shown = words(&rt);
    assert!(
        shown.contains("No channels yet. Add one from the menu under the server’s name."),
        "{shown}"
    );
    assert!(
        shown.contains(
            "Add a channel from the menu under the server’s name, or a server with the rail’s plus."
        ),
        "{shown}"
    );
    assert!(rt.find("server-title").is_some());
}

// ---- New channel --------------------------------------------------------------------------

#[test]
fn the_new_channel_popover_opens_traps_focus_and_closes() {
    let mut rt = runtime(signed_in());
    open_new_channel(&mut rt);
    assert_eq!(rt.state.popover, Some(Popover::NewChannel));
    assert!(rt.find("new-channel-popover").is_some());
    let shown = words(&rt);
    assert!(
        shown.contains("A text channel everyone in Acme Works can see."),
        "{shown}"
    );
    let dialog = rt
        .elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some("new-channel-popover"))
        .unwrap();
    assert_eq!(dialog.role, "dialog");
    assert_eq!(dialog.label.as_deref(), Some("New channel"));

    // Focus starts in the name, and the shell behind is out of reach.
    assert_eq!(focused(&rt).as_deref(), Some("channel-name"));
    for behind in [
        "composer",
        "channel-design",
        "server-garden",
        "server-title",
    ] {
        assert!(inert(&rt, behind), "{behind} is inert behind the popover");
    }
    assert!(!inert(&rt, "channel-name"));

    // Tab goes round inside it and never out.
    type_text(&mut rt, "planning");
    assert_eq!(rt.state.server_forms.channel.name.text, "planning");
    let mut visited = Vec::new();
    for _ in 0..6 {
        key(&mut rt, Key::Tab);
        visited.push(focused(&rt).unwrap_or_default());
    }
    assert!(
        visited
            .iter()
            .all(|k| k == "channel-name" || k == "create-channel"),
        "{visited:?}"
    );
    assert!(visited.iter().any(|k| k == "create-channel"), "{visited:?}");

    // Escape closes it and gives focus back to the name it hangs from.
    key(&mut rt, Key::Escape);
    assert!(rt.state.popover.is_none());
    assert!(rt.find("new-channel-popover").is_none());
    assert_eq!(focused(&rt).as_deref(), Some("server-title"));
    assert!(!inert(&rt, "composer"));

    // A press outside closes it too, and what was typed is still there on the way back.
    open_new_channel(&mut rt);
    assert_eq!(rt.state.popover, Some(Popover::NewChannel));
    press(&mut rt, "composer");
    assert!(rt.state.popover.is_none());
    open_new_channel(&mut rt);
    assert_eq!(rt.state.server_forms.channel.name.text, "planning");
}

#[test]
fn a_channel_is_created_by_name_and_opens_once_the_snapshot_lists_it() {
    let mut rt = runtime(signed_in());
    open_new_channel(&mut rt);
    // Nothing is asked for without a name.
    rt.update(|s, _| s.create_channel());
    assert!(!rt.state.server_forms.channel.busy);

    type_text(&mut rt, "planning");
    key(&mut rt, Key::Enter);
    assert!(
        rt.state.server_forms.channel.busy,
        "Return in the name creates it"
    );
    assert_eq!(
        rt.state.popover,
        Some(Popover::NewChannel),
        "open until the instance answers"
    );

    answer(
        &mut rt,
        ServerUpdate::ChannelCreated {
            server_id: "acme".into(),
            channel: Ok(channel("planning", "acme", "planning")),
        },
    );
    assert!(rt.state.popover.is_none(), "made: the popover closes");
    assert!(!rt.state.server_forms.channel.busy);
    assert_eq!(rt.state.server_forms.channel.name.text, "");
    assert_eq!(
        rt.state.session.channel(),
        Some("general"),
        "the snapshot does not list it yet"
    );

    let mut listed = snapshot();
    listed
        .channels
        .push(channel("planning", "acme", "planning"));
    snapshot_arrives(&mut rt, listed);
    assert_eq!(rt.state.session.channel(), Some("planning"));
    assert!(rt.find("channel-planning").is_some());
    assert!(words(&rt).contains("Message #planning"));
}

#[test]
fn the_create_button_makes_the_channel_too() {
    let mut rt = runtime(signed_in());
    open_new_channel(&mut rt);
    type_text(&mut rt, "planning");
    press(&mut rt, "create-channel");
    assert!(rt.state.server_forms.channel.busy);
}

#[test]
fn a_channel_the_instance_refuses_says_why_in_the_popover() {
    let mut rt = runtime(signed_in());
    open_new_channel(&mut rt);
    type_text(&mut rt, "planning");
    press(&mut rt, "create-channel");
    answer(
        &mut rt,
        ServerUpdate::ChannelCreated {
            server_id: "acme".into(),
            channel: Err(failure("PERMISSION_DENIED", "missing manage_channels")),
        },
    );
    assert_eq!(rt.state.popover, Some(Popover::NewChannel));
    assert!(words(&rt).contains("You cannot add channels to this server."));
    assert_eq!(rt.state.server_forms.channel.name.text, "planning");

    press(&mut rt, "create-channel");
    assert_eq!(rt.state.server_forms.channel.error, "");
    answer(
        &mut rt,
        ServerUpdate::ChannelCreated {
            server_id: "acme".into(),
            channel: Err(failure("INVALID_ARGUMENT", "a channel name has no spaces")),
        },
    );
    assert!(words(&rt).contains("a channel name has no spaces"));

    // The refusal is gone the next time the popover opens.
    key(&mut rt, Key::Escape);
    open_new_channel(&mut rt);
    assert!(!words(&rt).contains("a channel name has no spaces"));
}

#[test]
fn a_channel_made_in_a_server_the_person_left_does_not_follow_them() {
    let mut rt = runtime(signed_in());
    open_new_channel(&mut rt);
    type_text(&mut rt, "planning");
    press(&mut rt, "create-channel");
    key(&mut rt, Key::Escape);
    press(&mut rt, "server-garden");
    answer(
        &mut rt,
        ServerUpdate::ChannelCreated {
            server_id: "acme".into(),
            channel: Ok(channel("planning", "acme", "planning")),
        },
    );
    let mut listed = snapshot();
    listed
        .channels
        .push(channel("planning", "acme", "planning"));
    snapshot_arrives(&mut rt, listed);
    assert_eq!(rt.state.server_id(), Some("garden"));
    assert_eq!(rt.state.session.channel(), Some("plots"));
}

// ---- Invite people ------------------------------------------------------------------------

#[test]
fn the_invite_popover_opens_traps_focus_and_closes() {
    let mut rt = inviting();
    assert!(rt.find("invite-popover").is_some());
    let shown = words(&rt);
    for expected in [
        "Invite people",
        "A link to join Acme Works.",
        "Expires",
        "Never",
        "In a day",
        "In a week",
        "Uses",
        "Unlimited",
        "Once",
        "Ten",
        "Make a link",
        "Email",
        "Send by email",
    ] {
        assert!(shown.contains(expected), "{expected:?} in:\n{shown}");
    }
    let dialog = rt
        .elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some("invite-popover"))
        .unwrap();
    assert_eq!(dialog.role, "dialog");
    // Focus starts at the top of the form, and the shell behind is out of reach.
    assert_eq!(focused(&rt).as_deref(), Some("invite-expiry-0"));
    for behind in ["composer", "channel-general", "server-title", "home"] {
        assert!(inert(&rt, behind), "{behind} is inert behind the popover");
    }

    let inside = |k: &str| {
        k == "invite-email"
            || k == "make-invite"
            || k == "email-invite"
            || k.starts_with("invite-expiry-")
            || k.starts_with("invite-uses-")
    };
    let mut visited = Vec::new();
    for _ in 0..12 {
        key(&mut rt, Key::Tab);
        visited.push(focused(&rt).unwrap_or_default());
    }
    assert!(visited.iter().all(|k| inside(k)), "{visited:?}");
    assert!(visited.iter().any(|k| k == "make-invite"), "{visited:?}");

    key(&mut rt, Key::Escape);
    assert!(rt.state.popover.is_none() && rt.find("invite-popover").is_none());
    assert_eq!(focused(&rt).as_deref(), Some("server-title"));

    open_invites(&mut rt);
    press(&mut rt, "member-grace");
    assert!(rt.state.popover.is_none(), "a press outside closes it");
}

#[test]
fn the_servers_invites_are_listed_with_their_uses_and_expiry() {
    let mut rt = inviting();
    assert!(rt.find("invites").is_none(), "no list before there is one");
    let expires = chrono::Utc::now().timestamp_micros() + 7 * 86_400_000_000;
    answer(
        &mut rt,
        ServerUpdate::Invites {
            server_id: "acme".into(),
            invites: Ok(vec![
                invite("p8m2q7", 1, 0, Some(expires), "pat@example.com"),
                invite("h4t6w1", 0, 3, None, ""),
                invite("z1z1z1", 10, 4, None, ""),
            ]),
        },
    );
    let shown = words(&rt);
    let at = DateTime::from_timestamp_micros(expires)
        .unwrap()
        .with_timezone(&Local)
        .format("%b %-d, %-I:%M %p");
    for expected in [
        "Invites".to_string(),
        "p8m2q7".to_string(),
        format!("to pat@example.com · 0/1 uses · expires {at}"),
        "3 uses · no expiry".to_string(),
        "4/10 uses · no expiry".to_string(),
        "Revoke invite h4t6w1".to_string(),
    ] {
        assert!(shown.contains(&expected), "{expected:?} in:\n{shown}");
    }
    let list = rt
        .elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some("invites"))
        .unwrap();
    assert_eq!(list.role, "list");
    let row = rt
        .elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some("invite-h4t6w1"))
        .unwrap();
    assert_eq!(row.role, "listitem");
}

#[test]
fn an_account_that_may_not_invite_is_told_so() {
    let mut rt = inviting();
    answer(
        &mut rt,
        ServerUpdate::Invites {
            server_id: "acme".into(),
            invites: Err(failure("PERMISSION_DENIED", "missing create_invites")),
        },
    );
    assert!(words(&rt).contains("You cannot invite people to this server."));

    // Opening it again asks again, with the refusal cleared.
    key(&mut rt, Key::Escape);
    open_invites(&mut rt);
    assert_eq!(rt.state.server_forms.invite.error, "");
    answer(
        &mut rt,
        ServerUpdate::Invites {
            server_id: "acme".into(),
            invites: Err(failure("", "connection refused")),
        },
    );
    assert!(words(&rt).contains("connection refused"));
}

#[test]
fn a_link_is_made_with_the_limits_chosen_and_copied() {
    let mut rt = inviting();
    assert_eq!(rt.state.server_forms.invite.expires_in_hours, 0);
    assert_eq!(rt.state.server_forms.invite.max_uses, 0);
    press(&mut rt, "invite-expiry-1");
    assert_eq!(rt.state.server_forms.invite.expires_in_hours, 24);
    press(&mut rt, "invite-expiry-2");
    assert_eq!(rt.state.server_forms.invite.expires_in_hours, 168);
    press(&mut rt, "invite-uses-1");
    assert_eq!(rt.state.server_forms.invite.max_uses, 1);
    press(&mut rt, "invite-uses-2");
    assert_eq!(rt.state.server_forms.invite.max_uses, 10);
    press(&mut rt, "invite-expiry-0");
    assert_eq!(rt.state.server_forms.invite.expires_in_hours, 0);

    assert!(rt.find("copy-invite").is_none(), "no link to copy yet");
    press(&mut rt, "make-invite");
    assert!(rt.state.server_forms.invite.busy);
    answer(
        &mut rt,
        ServerUpdate::InviteCreated {
            server_id: "acme".into(),
            invite: Ok(invite("k3v9x2", 10, 0, None, "")),
        },
    );
    assert!(!rt.state.server_forms.invite.busy);
    let shown = words(&rt);
    assert!(
        shown.contains("https://chat.example.com/invite/k3v9x2") && shown.contains("Copy link"),
        "{shown}"
    );
    assert!(
        shown.contains("0/10 uses · no expiry"),
        "the new link heads the list:\n{shown}"
    );
    assert_eq!(rt.state.server_forms.invite.list[0].code, "k3v9x2");

    press(&mut rt, "copy-invite");
    assert_eq!(
        rt.copy_request.take().as_deref(),
        Some("https://chat.example.com/invite/k3v9x2"),
        "the link goes to the clipboard"
    );
    assert!(rt.state.server_forms.invite.copied);
    let shown = words(&rt);
    assert!(
        shown.contains("Copied") && !shown.contains("Copy link"),
        "{shown}"
    );

    // Another link is not the one that was copied.
    press(&mut rt, "make-invite");
    assert!(!rt.state.server_forms.invite.copied);
    answer(
        &mut rt,
        ServerUpdate::InviteCreated {
            server_id: "acme".into(),
            invite: Ok(invite("n5n5n5", 10, 0, None, "")),
        },
    );
    let shown = words(&rt);
    assert!(
        shown.contains("https://chat.example.com/invite/n5n5n5") && shown.contains("Copy link"),
        "{shown}"
    );
    assert_eq!(rt.state.server_forms.invite.list.len(), 2);
}

#[test]
fn a_link_the_instance_refuses_says_why() {
    let mut rt = inviting();
    for (code, message, wording) in [
        (
            "PERMISSION_DENIED",
            "missing create_invites",
            "You cannot invite people to this server.",
        ),
        (
            "RATE_LIMITED",
            "slow down",
            "Too many links in the last hour; try later.",
        ),
        (
            "INTERNAL",
            "the instance fell over",
            "the instance fell over",
        ),
    ] {
        press(&mut rt, "make-invite");
        assert_eq!(rt.state.server_forms.invite.error, "");
        answer(
            &mut rt,
            ServerUpdate::InviteCreated {
                server_id: "acme".into(),
                invite: Err(failure(code, message)),
            },
        );
        assert!(words(&rt).contains(wording), "{code}: {}", words(&rt));
        assert!(!rt.state.server_forms.invite.busy);
        assert!(rt.find("copy-invite").is_none());
    }
}

#[test]
fn an_invite_is_mailed_to_an_address_and_says_where_it_went() {
    let mut rt = inviting();
    // Nothing is sent without an address.
    rt.update(|s, _| s.email_invite());
    assert!(!rt.state.server_forms.invite.busy);

    press(&mut rt, "invite-email");
    type_text(&mut rt, "pat@example.com");
    assert_eq!(rt.state.server_forms.invite.email.text, "pat@example.com");
    press(&mut rt, "email-invite");
    assert!(rt.state.server_forms.invite.busy);
    let expires = chrono::Utc::now().timestamp_micros() + 7 * 86_400_000_000;
    answer(
        &mut rt,
        ServerUpdate::InviteEmailed {
            server_id: "acme".into(),
            invite: Ok(invite("p8m2q7", 1, 0, Some(expires), "pat@example.com")),
        },
    );
    assert!(!rt.state.server_forms.invite.busy);
    assert_eq!(rt.state.server_forms.invite.email.text, "");
    let shown = words(&rt);
    assert!(
        shown.contains("Sent to pat@example.com; it works once, within a week."),
        "{shown}"
    );
    assert!(
        shown.contains("to pat@example.com · 0/1 uses · expires"),
        "{shown}"
    );
    assert!(
        rt.find("copy-invite").is_none(),
        "a mailed invite is not a link to copy"
    );
}

#[test]
fn return_in_the_email_field_sends_the_invite() {
    let mut rt = inviting();
    press(&mut rt, "invite-email");
    type_text(&mut rt, "pat@example.com");
    key(&mut rt, Key::Enter);
    assert!(rt.state.server_forms.invite.busy);
}

#[test]
fn a_mailed_invite_the_instance_refuses_says_why() {
    let mut rt = inviting();
    press(&mut rt, "invite-email");
    type_text(&mut rt, "pat");
    for (code, message, wording) in [
        (
            "FAILED_PRECONDITION",
            "no mailer",
            "This instance sends no mail; make a link instead.",
        ),
        (
            "INVALID_ARGUMENT",
            "bad address",
            "That is not an email address.",
        ),
        (
            "RATE_LIMITED",
            "slow down",
            "Too many invites in the last hour; try later.",
        ),
        (
            "PERMISSION_DENIED",
            "missing create_invites",
            "You cannot invite people to this server.",
        ),
        (
            "INTERNAL",
            "the instance fell over",
            "the instance fell over",
        ),
    ] {
        press(&mut rt, "email-invite");
        assert_eq!(rt.state.server_forms.invite.error, "");
        answer(
            &mut rt,
            ServerUpdate::InviteEmailed {
                server_id: "acme".into(),
                invite: Err(failure(code, message)),
            },
        );
        let shown = words(&rt);
        assert!(shown.contains(wording), "{code}: {shown}");
        assert!(!shown.contains("Sent to"), "{shown}");
        assert_eq!(
            rt.state.server_forms.invite.email.text, "pat",
            "the address stays to be corrected"
        );
    }
}

#[test]
fn an_invite_is_revoked_from_the_list() {
    let mut rt = inviting();
    answer(
        &mut rt,
        ServerUpdate::Invites {
            server_id: "acme".into(),
            invites: Ok(vec![invite("h4t6w1", 0, 3, None, "")]),
        },
    );
    press(&mut rt, "make-invite");
    answer(
        &mut rt,
        ServerUpdate::InviteCreated {
            server_id: "acme".into(),
            invite: Ok(invite("k3v9x2", 0, 0, None, "")),
        },
    );
    assert!(rt.find("copy-invite").is_some());

    // A refusal leaves the invite listed and says why.
    reveal_and_press(&mut rt, "revoke-h4t6w1");
    assert!(rt.state.server_forms.invite.busy);
    answer(
        &mut rt,
        ServerUpdate::InviteRevoked {
            server_id: "acme".into(),
            code: "h4t6w1".into(),
            result: Err(failure(
                "PERMISSION_DENIED",
                "that invite is not yours to revoke",
            )),
        },
    );
    assert!(words(&rt).contains("that invite is not yours to revoke"));
    assert_eq!(rt.state.server_forms.invite.list.len(), 2);

    reveal_and_press(&mut rt, "revoke-h4t6w1");
    assert_eq!(rt.state.server_forms.invite.error, "");
    answer(
        &mut rt,
        ServerUpdate::InviteRevoked {
            server_id: "acme".into(),
            code: "h4t6w1".into(),
            result: Ok(()),
        },
    );
    assert!(rt.find("invite-h4t6w1").is_none());
    assert!(
        rt.find("invite-k3v9x2").is_some() && rt.find("copy-invite").is_some(),
        "the other invite and the link just made stay"
    );

    // Revoking the link just made takes it away from the top as well.
    reveal_and_press(&mut rt, "revoke-k3v9x2");
    answer(
        &mut rt,
        ServerUpdate::InviteRevoked {
            server_id: "acme".into(),
            code: "k3v9x2".into(),
            result: Ok(()),
        },
    );
    assert!(rt.find("copy-invite").is_none() && rt.find("invites").is_none());
    assert!(!words(&rt).contains("https://chat.example.com/invite/k3v9x2"));
}

#[test]
fn one_servers_invites_are_not_shown_for_another() {
    let mut rt = inviting();
    answer(
        &mut rt,
        ServerUpdate::Invites {
            server_id: "acme".into(),
            invites: Ok(vec![invite("h4t6w1", 0, 3, None, "")]),
        },
    );
    press(&mut rt, "invite-expiry-1");
    press(&mut rt, "make-invite");
    answer(
        &mut rt,
        ServerUpdate::InviteCreated {
            server_id: "acme".into(),
            invite: Ok(invite("k3v9x2", 0, 0, None, "")),
        },
    );
    key(&mut rt, Key::Escape);

    press(&mut rt, "server-garden");
    open_invites(&mut rt);
    assert!(words(&rt).contains("A link to join Garden."));
    assert!(rt.find("invites").is_none() && rt.find("copy-invite").is_none());
    assert_eq!(
        rt.state.server_forms.invite.expires_in_hours, 24,
        "the limits chosen stay"
    );

    // An answer about the server left behind changes nothing here.
    answer(
        &mut rt,
        ServerUpdate::Invites {
            server_id: "acme".into(),
            invites: Ok(vec![invite("h4t6w1", 0, 3, None, "")]),
        },
    );
    answer(
        &mut rt,
        ServerUpdate::InviteCreated {
            server_id: "acme".into(),
            invite: Ok(invite("late00", 0, 0, None, "")),
        },
    );
    assert!(rt.find("invites").is_none() && rt.find("copy-invite").is_none());
    assert!(rt.state.server_forms.invite.list.is_empty());

    answer(
        &mut rt,
        ServerUpdate::Invites {
            server_id: "garden".into(),
            invites: Ok(vec![invite("g7g7g7", 0, 1, None, "")]),
        },
    );
    assert!(rt.find("invite-g7g7g7").is_some());
}

#[test]
fn signing_out_forgets_what_the_forms_held() {
    let mut rt = inviting();
    press(&mut rt, "invite-email");
    type_text(&mut rt, "pat@example.com");
    answer(
        &mut rt,
        ServerUpdate::InviteCreated {
            server_id: "acme".into(),
            invite: Ok(invite("k3v9x2", 0, 0, None, "")),
        },
    );
    rt.update(|s, _| s.sign_out());
    settle(&mut rt);
    assert!(rt.state.popover.is_none());
    assert!(rt.state.server_forms.invite.made.is_none());
    assert_eq!(rt.state.server_forms.invite.email.text, "");
}
