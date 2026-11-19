//! The menus a right-click opens (#25), through a windowless `Runtime` over a state no
//! instance is behind: a message's actions where it was right-clicked, by the toolbar's
//! rules and with what the press was on first, a person's on their row, their message and
//! their conversation, and the Menu key and Shift+F10 opening the same.

#![allow(clippy::expect_used, clippy::unwrap_used)]

mod common;

use common::*;
use lumia_client::app::{ChannelSummary, ReactionSummary};
use lumia_shell::app::context::AT;
use lumia_shell::app::{HomeView, Person, Place, Popover, State};
use lumia_shell::session::channels::ChannelUpdate;
use lumia_shell::session::{Failure, Update};
use rune::{Key, KeyEvent, Mods, Runtime};

fn key(rt: &mut Runtime<State>, key: Key, shift: bool) {
    rt.key(KeyEvent {
        key,
        mods: Mods {
            shift,
            ..Mods::default()
        },
    });
    settle(rt);
}

fn rect(rt: &Runtime<State>, key: &str) -> [f32; 4] {
    rt.presented(key)
        .unwrap_or_else(|| panic!("{key} is not drawn"))
        .0
}

fn focus(rt: &mut Runtime<State>, key: &'static str) {
    rt.update(move |_, tx| tx.focus(key, true));
    settle(rt);
}

// ---- A message's menu ---------------------------------------------------------------------

#[test]
fn a_right_click_on_a_message_opens_its_actions_by_the_toolbars_rules() {
    let mut rt = runtime(signed_in());
    right_click(&mut rt, "message-m1");
    assert_eq!(rt.state.popover, Some(Popover::MessageContext));
    let menu = rt
        .elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some("message-context"))
        .expect("the menu");
    assert_eq!(menu.role, "menu");
    assert_eq!(menu.label.as_deref(), Some("Message actions"));
    // Another's message, and no moderator: Reply, Add reaction and Copy text.
    for there in ["context-reply", "context-react", "menu-copy"] {
        assert!(rt.find(there).is_some(), "{there}");
    }
    for absent in [
        "menu-edit",
        "menu-delete",
        "context-copy-selection",
        "context-quote",
        "context-open-link",
    ] {
        assert!(rt.find(absent).is_none(), "{absent}");
    }
    // The toolbar is not over the row while its menu is.
    assert!(rt.find("more-m1").is_none());
    key(&mut rt, Key::Escape, false);
    assert!(rt.state.popover.is_none() && rt.find("message-context").is_none());

    // This account's own: Edit and Delete too.
    right_click(&mut rt, "message-m2");
    for there in ["context-reply", "menu-edit", "menu-copy", "menu-delete"] {
        assert!(rt.find(there).is_some(), "{there}");
    }
    let shown = words(&rt);
    let order: Vec<usize> = [
        "Reply",
        "Add reaction",
        "Edit message",
        "Copy text",
        "Delete message",
    ]
    .iter()
    .map(|label| shown.find(label).unwrap_or_else(|| panic!("{label}")))
    .collect();
    assert!(order.is_sorted(), "in the toolbar's order: {shown}");
}

#[test]
fn the_menu_opens_where_the_pointer_is() {
    let mut rt = runtime(signed_in());
    let (x, y) = rt.find("message-m1").unwrap();
    right_click(&mut rt, "message-m1");
    assert_eq!(rt.state.context.at, [x, y]);
    let [left, top, ..] = rect(&rt, "message-context");
    assert!((left - x).abs() < 1.0, "{left} at {x}");
    assert!(top >= y && top - y < 12.0, "{top} under {y}");
}

#[test]
fn each_item_does_what_the_toolbars_does() {
    let mut rt = runtime(signed_in());
    right_click(&mut rt, "message-m1");
    press(&mut rt, "context-reply");
    assert!(rt.state.popover.is_none());
    let quote = rt.state.replies.replying.clone().expect("replying");
    assert_eq!(quote.message_id, "m1");
    assert!(rt.find("reply-bar").is_some());

    // Add reaction: the picker, for that message, where the menu was.
    right_click(&mut rt, "message-m1");
    press(&mut rt, "context-react");
    assert_eq!(rt.state.popover, Some(Popover::EmojiPicker));
    let picker = rt.state.replies.picker.as_ref().expect("the picker");
    assert_eq!(
        (picker.message_id.as_str(), picker.anchor.as_str()),
        ("m1", AT)
    );
    assert!(rt.find("emoji-search").is_some());
    key(&mut rt, Key::Escape, false);

    // Copy text.
    let clipboard = rune::clipboard::memory();
    rt.use_clipboard(&clipboard);
    right_click(&mut rt, "message-m1");
    press(&mut rt, "menu-copy");
    assert!(rt.state.popover.is_none());
    assert_eq!(
        clipboard.text().as_deref().map(String::as_str),
        Some("Shipped on **Friday**.")
    );

    // Edit message, in place.
    right_click(&mut rt, "message-m2");
    press(&mut rt, "menu-edit");
    assert!(rt.find("edit-m2").is_some());
    // While a message is being edited there is no menu, as there is no toolbar.
    right_click(&mut rt, "message-m1");
    assert!(rt.state.popover.is_none());
    key(&mut rt, Key::Escape, false);

    // Delete message asks first.
    right_click(&mut rt, "message-m2");
    press(&mut rt, "menu-delete");
    assert_eq!(rt.state.popover, Some(Popover::DeleteMessage));
    assert!(words(&rt).contains("Delete this message?"));
}

#[test]
fn a_selection_the_message_is_in_comes_first() {
    let mut rt = runtime(signed_in());
    let clipboard = rune::clipboard::memory();
    rt.use_clipboard(&clipboard);
    rt.update(|_, tx| tx.select_text("message-m1/body/b0", 0..7));
    settle(&mut rt);
    assert_eq!(
        rt.state.replies.selected,
        Some((vec!["m1".to_string()], "Shipped".to_string()))
    );
    // Another message's menu says nothing of it.
    right_click(&mut rt, "message-m2");
    assert!(rt.find("context-copy-selection").is_none());
    key(&mut rt, Key::Escape, false);

    right_click(&mut rt, "message-m1");
    assert!(rt.find("context-copy-selection").is_some());
    press(&mut rt, "context-quote");
    let quote = rt.state.replies.replying.clone().expect("replying");
    assert_eq!(
        (quote.message_id.as_str(), quote.text.as_str()),
        ("m1", "Shipped")
    );

    rt.update(|_, tx| tx.select_text("message-m1/body/b0", 0..7));
    settle(&mut rt);
    right_click(&mut rt, "message-m1");
    press(&mut rt, "context-copy-selection");
    assert!(rt.state.popover.is_none());
    assert_eq!(
        clipboard.text().as_deref().map(String::as_str),
        Some("Shipped")
    );

    // Across messages it can be copied from any of them, and quoted from none.
    rt.update(|_, tx| tx.select_all_text("rows"));
    settle(&mut rt);
    let (ids, _) = rt.state.replies.selected.clone().expect("a selection");
    assert_eq!(ids, ["m1", "m2", "m3"]);
    right_click(&mut rt, "message-m2");
    assert!(rt.find("context-copy-selection").is_some());
    assert!(rt.find("context-quote").is_none());
}

#[test]
fn a_link_right_clicked_has_its_own_items_first() {
    let mut state = signed_in();
    state.session.entries.truncate(1);
    state.session.entries[0].body = "https://lumia.chat/notes".into();
    state.session.revision += 1;
    let mut rt = runtime(state);
    let clipboard = rune::clipboard::memory();
    rt.use_clipboard(&clipboard);
    // The body is the link and nothing else: a point in its first words is on it.
    let [x, y, _, h] = rect(&rt, "b0");
    let (x, y) = (x + 20.0, y + h / 2.0);
    rt.pointer_move(x, y);
    rt.secondary_down(x, y);
    rt.secondary_up(x, y);
    settle(&mut rt);
    assert_eq!(rt.state.popover, Some(Popover::MessageContext));
    assert_eq!(
        rt.state
            .context
            .message
            .as_ref()
            .and_then(|m| m.link.as_deref()),
        Some("https://lumia.chat/notes")
    );
    let shown = words(&rt);
    let order: Vec<usize> = ["Open link", "Copy link", "Reply"]
        .iter()
        .map(|label| shown.find(label).unwrap_or_else(|| panic!("{label}")))
        .collect();
    assert!(order.is_sorted(), "{shown}");
    press(&mut rt, "context-copy-link");
    assert_eq!(
        clipboard.text().as_deref().map(String::as_str),
        Some("https://lumia.chat/notes")
    );
    assert!(rt.state.popover.is_none());
}

#[test]
fn a_notice_and_a_deleted_message_have_no_menu() {
    let mut state = signed_in();
    state.session.entries[0].deleted = true;
    state.session.entries[2].notice = "JOINED".into();
    state.session.revision += 1;
    let mut rt = runtime(state);
    right_click(&mut rt, "message-m1");
    assert!(rt.state.popover.is_none(), "deleted");
    right_click(&mut rt, "m3");
    assert!(rt.state.popover.is_none(), "a notice");
}

// ---- A person's menu ----------------------------------------------------------------------

#[test]
fn a_right_click_on_a_member_opens_their_menu() {
    let mut rt = runtime(signed_in());
    right_click(&mut rt, "member-grace");
    assert_eq!(rt.state.popover, Some(Popover::PersonMenu));
    let menu = rt
        .elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some("person-menu"))
        .expect("the menu");
    assert_eq!(menu.role, "menu");
    assert_eq!(menu.label.as_deref(), Some("Grace Hopper"));
    let shown = words(&rt);
    let order: Vec<usize> = ["View profile", "Message Grace", "Message privately"]
        .iter()
        .map(|label| shown.find(label).unwrap_or_else(|| panic!("{label}")))
        .collect();
    assert!(order.is_sorted(), "{shown}");

    // View profile: their card, beside the row as a press opens it.
    press(&mut rt, "person-view");
    assert_eq!(rt.state.popover, Some(Popover::Person));
    assert_eq!(
        rt.state.person,
        Some(Person {
            account_id: "grace".into(),
            anchor: "member-grace".into()
        })
    );
    assert!(rt.find("person-card").is_some() && rt.find("person-menu").is_none());
}

#[test]
fn ones_own_menu_is_view_profile_alone() {
    let mut rt = runtime(signed_in());
    right_click(&mut rt, "member-me");
    assert!(rt.find("person-view").is_some());
    assert!(rt.find("person-message").is_none());
    assert!(rt.find("person-message-privately").is_none());
}

#[test]
fn the_menu_offers_what_the_card_would() {
    // Message privately: a new message at Home, asking for the conversation.
    let mut rt = runtime(signed_in());
    right_click(&mut rt, "member-grace");
    press(&mut rt, "person-message-privately");
    assert_eq!(rt.state.place, Place::Home);
    assert_eq!(rt.state.home_view, HomeView::NewMessage);
    assert_eq!(rt.state.home.starting.as_deref(), Some("grace"));

    // Message Grace: the server DM, and a refusal opens her card to say why.
    let mut rt = runtime(signed_in());
    right_click(&mut rt, "member-grace");
    press(&mut rt, "person-message");
    assert!(rt.state.popover.is_none() && rt.state.channel_forms.dm_busy);
    rt.update(|s, _| {
        s.receive(&Update::Channel(ChannelUpdate::DmOpened {
            server_id: "acme".into(),
            account_id: "grace".into(),
            result: Err(Failure {
                code: "FAILED_PRECONDITION".into(),
                ..Default::default()
            }),
        }));
    });
    settle(&mut rt);
    assert_eq!(rt.state.popover, Some(Popover::Person));
    assert!(words(&rt).contains("They don’t take messages here."));

    // Someone who takes no server DMs here: the personal way alone.
    let mut rt = runtime(signed_in());
    rt.update(|s, _| {
        let mut grace = member("grace", "grace", "Grace Hopper");
        grace.server_dms_off = true;
        s.receive(&Update::Members {
            server_id: "acme".into(),
            members: vec![member("me", "ada", "Ada Lovelace"), grace],
        });
    });
    settle(&mut rt);
    right_click(&mut rt, "member-grace");
    assert!(rt.find("person-message").is_none());
    assert!(rt.find("person-message-privately").is_some());
}

#[test]
fn whether_someone_takes_server_dms_is_read_from_the_server_on_screen() {
    // Grace turned server DMs off in Garden alone, and Garden's list is the one read last:
    // what is held of her across servers says off.
    let mut rt = runtime(signed_in());
    rt.update(|s, _| {
        let mut grace = member("grace", "grace", "Grace Hopper");
        grace.server_dms_off = true;
        s.receive(&Update::Members {
            server_id: "garden".into(),
            members: vec![member("me", "ada", "Ada Lovelace"), grace],
        });
        s.session
            .channel_members
            .insert("plots".into(), vec!["me".into(), "grace".into()]);
    });
    settle(&mut rt);

    // In Acme she takes them: her menu and her card offer one.
    right_click(&mut rt, "member-grace");
    assert!(rt.find("person-message").is_some());
    press(&mut rt, "person-view");
    assert!(rt.find("card-message").is_some());
    assert!(!words(&rt).contains("Doesn’t take messages here"));
    press(&mut rt, "card-close");

    // In Garden she does not.
    rt.update(|s, _| s.select_server("garden"));
    settle(&mut rt);
    right_click(&mut rt, "member-grace");
    assert!(rt.find("person-message").is_none());
    assert!(rt.find("person-message-privately").is_some());
    press(&mut rt, "person-view");
    assert!(rt.find("card-message").is_none());
    assert!(words(&rt).contains("Doesn’t take messages here"));
    press(&mut rt, "card-close");

    // And the other way about: Acme's list read again after Garden's, she still takes none
    // in Garden, where the instance would refuse one.
    rt.update(|s, _| {
        s.receive(&Update::Members {
            server_id: "acme".into(),
            members: vec![
                member("me", "ada", "Ada Lovelace"),
                member("grace", "grace", "Grace Hopper"),
            ],
        });
    });
    settle(&mut rt);
    right_click(&mut rt, "member-grace");
    assert!(rt.find("person-message").is_none());
}

#[test]
fn a_messages_author_has_the_persons_menu_and_not_the_messages() {
    let mut rt = runtime(signed_in());
    right_click(&mut rt, "avatar-m1");
    assert_eq!(rt.state.popover, Some(Popover::PersonMenu));
    assert_eq!(
        rt.state.person,
        Some(Person {
            account_id: "grace".into(),
            anchor: "avatar-m1".into()
        })
    );
    key(&mut rt, Key::Escape, false);

    // The name beside the avatar too, its card hung from the avatar.
    // (It starts a little after the avatar's end, level with its top.)
    let [x, y, w, _] = rect(&rt, "avatar-m1");
    let (x, y) = (x + w + 24.0, y + 10.0);
    rt.pointer_move(x, y);
    rt.secondary_down(x, y);
    rt.secondary_up(x, y);
    settle(&mut rt);
    assert_eq!(rt.state.popover, Some(Popover::PersonMenu));
    assert_eq!(
        rt.state.person.as_ref().map(|p| p.anchor.as_str()),
        Some("avatar-m1")
    );
}

#[test]
fn a_server_dms_row_has_the_menu_of_the_person_it_is_with() {
    let mut rt = runtime(signed_in());
    rt.update(|s, _| {
        let mut snapshot = snapshot();
        let general = snapshot.channels[0].clone();
        snapshot.channels.push(ChannelSummary {
            channel_id: "dm1".into(),
            name: String::new(),
            kind: "SERVER_DM".into(),
            participant_account_ids: vec!["me".into(), "grace".into()],
            ..general
        });
        s.receive(&Update::Snapshot(Ok(snapshot)));
    });
    settle(&mut rt);
    right_click(&mut rt, "dm-dm1");
    assert_eq!(rt.state.popover, Some(Popover::PersonMenu));
    assert_eq!(
        rt.state.person,
        Some(Person {
            account_id: "grace".into(),
            anchor: "dm-dm1".into()
        })
    );
}

// ---- The keyboard -------------------------------------------------------------------------

#[test]
fn the_menu_key_and_shift_f10_open_the_same_menus() {
    let mut state = signed_in();
    state.session.entries[0].reactions = vec![ReactionSummary {
        reaction: "👍".into(),
        count: 1,
        me: false,
        account_ids: vec!["grace".into()],
    }];
    state.session.revision += 1;
    let mut rt = runtime(state);

    // On a member's row: theirs, under the row.
    focus(&mut rt, "member-grace");
    key(&mut rt, Key::Function(10), true);
    assert_eq!(rt.state.popover, Some(Popover::PersonMenu));
    let [x, y, _, h] = rect(&rt, "member-grace");
    assert_eq!(rt.state.context.at, [x, y + h]);
    key(&mut rt, Key::Escape, false);

    // On a control inside a message that is not its author: the message's.
    focus(&mut rt, "reaction-0");
    key(&mut rt, Key::ContextMenu, false);
    assert_eq!(rt.state.popover, Some(Popover::MessageContext));
    assert_eq!(
        rt.state
            .context
            .message
            .as_ref()
            .map(|m| m.message_id.as_str()),
        Some("m1")
    );
    key(&mut rt, Key::Escape, false);

    // On its author's avatar: the person's.
    focus(&mut rt, "avatar-m1");
    key(&mut rt, Key::Function(10), true);
    assert_eq!(rt.state.popover, Some(Popover::PersonMenu));
}
