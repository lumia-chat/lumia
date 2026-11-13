//! The account menu, the profile card and the members' search, through a windowless
//! `Runtime` over a state no instance is behind. The server's menu is in `servers.rs`,
//! beside the popovers it opens.

#![allow(clippy::expect_used, clippy::unwrap_used)]

mod common;

use common::*;
use lumia_shell::app::{HomeView, Person, Place, Popover, Route};
use rune::{Key, KeyEvent, Mods, Runtime};

use lumia_shell::app::State;

fn key(rt: &mut Runtime<State>, key: Key) {
    rt.key(KeyEvent {
        key,
        mods: Mods::default(),
    });
    settle(rt);
}

fn element(rt: &Runtime<State>, key: &str) -> rune::ElementInfo {
    rt.elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some(key))
        .unwrap_or_else(|| panic!("{key} is not in the tree"))
}

// ---- The account menu ---------------------------------------------------------------------

#[test]
fn the_self_avatar_opens_the_account_menu() {
    let mut rt = runtime(signed_in());
    press(&mut rt, "self");
    assert_eq!(rt.state.popover, Some(Popover::Account));
    assert_eq!(
        rt.state.place,
        Place::Server("acme".into()),
        "nothing moved"
    );
    let menu = element(&rt, "account-menu");
    assert_eq!(menu.role, "menu");
    assert_eq!(menu.label.as_deref(), Some("Your account"));
    let shown = words(&rt);
    for expected in [
        "Ada Lovelace",
        "@ada",
        "Status",
        "Online",
        "Away",
        "Do not disturb",
        "View profile",
        "Devices",
        "Sign out",
    ] {
        assert!(shown.contains(expected), "{expected:?} in:\n{shown}");
    }
    // With no status chosen yet, Online is the one ticked.
    assert_eq!(element(&rt, "status-online").role, "menuitemradio");
    assert!(
        element(&rt, "composer").inert,
        "the shell behind is out of reach"
    );

    key(&mut rt, Key::Escape);
    assert!(rt.state.popover.is_none() && rt.find("account-menu").is_none());
}

#[test]
fn the_account_menu_leads_to_the_profile_the_devices_and_out() {
    let mut rt = runtime(signed_in());
    press(&mut rt, "self");
    press(&mut rt, "menu-profile");
    assert_eq!(rt.state.place, Place::Home);
    assert_eq!(rt.state.home_view, HomeView::Profile);
    assert!(rt.state.popover.is_none());

    press(&mut rt, "self");
    press(&mut rt, "menu-devices");
    assert_eq!(rt.state.home_view, HomeView::Devices);

    press(&mut rt, "self");
    press(&mut rt, "menu-sign-out");
    assert_eq!(rt.state.route, Route::SignIn);
    assert!(!rt.state.session.signed_in);
}

#[test]
fn choosing_a_status_closes_the_menu() {
    let mut rt = runtime(signed_in());
    press(&mut rt, "self");
    press(&mut rt, "status-away");
    assert!(rt.state.popover.is_none());
    assert_eq!(rt.state.place, Place::Server("acme".into()));
}

// ---- The profile card ---------------------------------------------------------------------

#[test]
fn a_members_row_opens_their_card() {
    let mut rt = runtime(signed_in());
    press(&mut rt, "member-grace");
    assert_eq!(rt.state.popover, Some(Popover::Person));
    assert_eq!(
        rt.state.person,
        Some(Person {
            account_id: "grace".into(),
            anchor: "member-grace".into()
        })
    );
    let card = element(&rt, "person-card");
    assert_eq!(card.role, "dialog");
    assert_eq!(card.label.as_deref(), Some("Grace Hopper, profile"));
    let shown = words(&rt);
    for expected in ["@grace", "Offline", "Message Grace", "Close profile"] {
        assert!(shown.contains(expected), "{expected:?} in:\n{shown}");
    }

    press(&mut rt, "card-close");
    assert!(rt.state.popover.is_none() && rt.find("person-card").is_none());
}

#[test]
fn a_messages_avatar_opens_its_authors_card() {
    let mut rt = runtime(signed_in());
    press(&mut rt, "avatar-m1");
    assert_eq!(rt.state.popover, Some(Popover::Person));
    assert_eq!(
        rt.state.person.as_ref().map(|p| p.account_id.as_str()),
        Some("grace")
    );
    assert!(words(&rt).contains("Message Grace"));
}

#[test]
fn ones_own_card_says_so_and_offers_no_message() {
    let mut rt = runtime(signed_in());
    press(&mut rt, "member-me");
    let shown = words(&rt);
    assert!(shown.contains("Ada Lovelace, profile"), "{shown}");
    assert!(rt.find("card-message").is_none());
}

#[test]
fn message_privately_on_a_card_goes_to_a_new_message_and_asks_for_the_conversation() {
    let mut rt = runtime(signed_in());
    press(&mut rt, "member-grace");
    press(&mut rt, "card-message-privately");
    assert_eq!(rt.state.place, Place::Home);
    assert_eq!(rt.state.home_view, HomeView::NewMessage);
    assert!(rt.state.popover.is_none() && rt.state.person.is_none());
    assert_eq!(rt.state.home.starting.as_deref(), Some("grace"));
}

// ---- Find a member ------------------------------------------------------------------------

#[test]
fn find_a_member_lists_who_it_matches_by_name_or_handle() {
    let mut rt = runtime(signed_in());
    assert!(rt.find("member-me").is_some() && rt.find("member-grace").is_some());

    press(&mut rt, "find-member");
    type_text(&mut rt, "hop");
    assert!(rt.find("member-grace").is_some());
    assert!(rt.find("member-me").is_none());
    assert!(
        words(&rt).contains("2 in this channel"),
        "the heading still counts everyone"
    );

    type_text(&mut rt, "zzz");
    assert!(rt.find("member-grace").is_none());
    assert!(words(&rt).contains("No member matches that."));

    // Another server starts with nothing typed.
    press(&mut rt, "server-garden");
    assert!(rt.state.member_filter.text.is_empty());
}

// ---- The conversation's foot --------------------------------------------------------------

#[test]
fn the_composer_says_what_return_does_and_a_short_conversation_sits_on_it() {
    let rt = runtime(signed_in());
    assert!(words(&rt).contains("Return to send"));
    // Three messages do not fill the pane: the last is just above the composer, not at
    // the top of the pane under the header.
    let (_, last) = rt.find("m3").unwrap();
    let (_, composer) = rt.find("composer").unwrap();
    let (_, header) = rt.find("header").unwrap();
    assert!(
        composer - last < last - header,
        "the last message ({last}) is nearer the composer ({composer}) than the header ({header})"
    );
}
