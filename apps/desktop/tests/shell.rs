//! The screens over a state no instance is behind, driven through a windowless `Runtime`:
//! what the instance would report is applied with `State::receive`, and the tree Rune
//! builds is read back. `tests/live.rs` is the same shell against a real instance.

#![allow(clippy::expect_used, clippy::unwrap_used)]

mod common;

use std::collections::HashMap;

use common::*;
use lumia_client::app::ChannelSummary;
use lumia_shell::app::State;
use lumia_shell::app::{HomeView, Place, Route};
use lumia_shell::session::{Failure, Pending, Update};
use rune::{Key, KeyEvent, Mods};

#[test]
fn the_first_screen_asks_for_an_instance_and_refuses_what_is_not_an_address() {
    let mut rt = runtime(State::new(data_dir()));
    assert!(rt.find("instance-card").is_some());
    press(&mut rt, "instance");
    type_text(&mut rt, "chat.example.com");
    press(&mut rt, "continue");
    assert_eq!(
        rt.state.sign_in.error,
        "Enter the instance address, starting with http:// or https://"
    );
    assert!(
        words(&rt).contains("starting with http:// or https://"),
        "the error is on screen"
    );
    assert!(!rt.state.session.is_open());
}

#[test]
fn an_instance_shows_who_runs_it_before_asking_for_an_account() {
    let mut rt = runtime(opened());
    let shown = words(&rt);
    assert!(rt.find("account-card").is_some());
    assert!(
        shown.contains("chat.example.com") && shown.contains("run by Example Co · Lumia 0.2.1"),
        "{shown}"
    );
    assert!(shown.contains("Welcome back") && shown.contains("Create an account"));

    press(&mut rt, "to-create");
    let shown = words(&rt);
    assert!(
        shown.contains("A new account on chat.example.com.")
            && shown.contains("Already have an account?"),
        "{shown}"
    );

    press(&mut rt, "change-instance");
    assert!(rt.find("instance-card").is_some() && !rt.state.session.is_open());
}

#[test]
fn refusals_are_worded_and_a_second_factor_is_asked_for_once() {
    let mut rt = runtime(opened());
    rt.update(|s, _| s.receive(&refusal("UNAUTHENTICATED", &[])));
    settle(&mut rt);
    assert!(words(&rt).contains("Wrong handle or password."));

    assert!(rt.find("totp").is_none());
    rt.update(|s, _| s.receive(&refusal("FAILED_PRECONDITION", &[("mfa", "totp")])));
    settle(&mut rt);
    assert!(rt.find("totp").is_some(), "the code field appears");
    assert_eq!(rt.state.sign_in.error, "");
    rt.update(|s, _| s.receive(&refusal("UNAUTHENTICATED", &[])));
    settle(&mut rt);
    assert!(words(&rt).contains("That code was not accepted."));

    rt.update(|s, _| s.receive(&refusal("FAILED_PRECONDITION", &[("mfa", "passkey")])));
    settle(&mut rt);
    assert!(words(&rt).contains("asks for its passkey"));
}

#[test]
fn the_shell_lands_on_the_first_server_and_its_first_channel() {
    let rt = runtime(signed_in());
    assert_eq!(rt.state.route, Route::Shell);
    assert_eq!(rt.state.server_id(), Some("acme"));
    assert_eq!(rt.state.session.channel(), Some("general"));
    for key in [
        "server-acme",
        "server-garden",
        "channel-general",
        "channel-design",
        "composer",
        "member-grace",
    ] {
        assert!(rt.find(key).is_some(), "{key} is on screen");
    }
    assert!(
        rt.find("channel-plots").is_none(),
        "another server's channel is not listed"
    );
    let shown = words(&rt);
    for expected in [
        "Messages in general",
        "Grace Hopper",
        "Ada Lovelace",
        "Members · 2",
        "Today",
        "Reading it now.",
    ] {
        assert!(shown.contains(expected), "{expected:?} in:\n{shown}");
    }
    // Someone the servers do not list reads as the start of their id.
    assert!(shown.contains("stranger"));
}

#[test]
fn the_rail_goes_home_to_adding_a_server_and_to_the_profile() {
    let mut rt = runtime(signed_in());
    press(&mut rt, "home");
    assert_eq!(rt.state.place, Place::Home);
    assert_eq!(rt.state.home_view, HomeView::Conversations);
    assert!(
        rt.state.session.conversation.is_none(),
        "no channel stays open behind Home"
    );
    assert!(rt.find("channel-general").is_none() && rt.find("members").is_none());

    press(&mut rt, "add-server");
    assert_eq!(rt.state.home_view, HomeView::AddServer);
    press(&mut rt, "self");
    assert_eq!(rt.state.home_view, HomeView::Profile);

    press(&mut rt, "server-acme");
    assert_eq!(rt.state.server_id(), Some("acme"));
    assert_eq!(rt.state.session.channel(), Some("general"));
}

#[test]
fn an_account_with_a_personal_conversation_lands_on_home() {
    let mut state = opened();
    state.receive(&Update::SignedIn(Ok(())));
    let mut snapshot = snapshot();
    snapshot.channels.push(ChannelSummary {
        channel_id: "dm".into(),
        server_id: String::new(),
        name: String::new(),
        kind: "PERSONAL_DM".into(),
        last_seq: 0,
        participant_account_ids: vec!["me".into(), "grace".into()],
        host: String::new(),
    });
    state.receive(&Update::Snapshot(Ok(snapshot)));
    let rt = runtime(state);
    assert_eq!(rt.state.place, Place::Home);
    assert!(
        rt.find("channel-dm").is_none(),
        "a personal conversation is not a server's channel"
    );
    assert!(
        rt.find("server-acme").is_some(),
        "the servers are still on the rail"
    );
}

#[test]
fn a_launch_returns_to_the_channel_last_on_screen() {
    let dir = data_dir();
    let mut first = State::new(dir.clone());
    first.sign_in.instance.replace_text("http://127.0.0.1:9");
    first.connect();
    first.receive(&Update::SignedIn(Ok(())));
    first.receive(&Update::Snapshot(Ok(snapshot())));
    first.select_server("garden");
    first.select_server("acme");
    first.select_channel("design");
    drop(first);

    // Nothing but the settings file carries it over.
    let mut again = State::new(dir);
    again.connect();
    again.receive(&Update::SignedIn(Ok(())));
    again.receive(&Update::Snapshot(Ok(snapshot())));
    assert_eq!(again.server_id(), Some("acme"));
    assert_eq!(again.session.channel(), Some("design"));

    // A channel the account no longer has is not returned to.
    let mut without = snapshot();
    without.channels.retain(|c| c.channel_id != "design");
    let mut gone = State::new(again.data_dir().to_path_buf());
    drop(again);
    gone.connect();
    gone.receive(&Update::SignedIn(Ok(())));
    gone.receive(&Update::Snapshot(Ok(without)));
    assert_eq!(gone.session.channel(), Some("general"));
}

#[test]
fn a_press_changes_channel_and_server() {
    let mut rt = runtime(signed_in());
    press(&mut rt, "channel-design");
    assert_eq!(rt.state.session.channel(), Some("design"));
    assert!(words(&rt).contains("Nothing here yet"));

    press(&mut rt, "server-garden");
    assert_eq!(rt.state.server_id(), Some("garden"));
    assert_eq!(rt.state.session.channel(), Some("plots"));
    assert!(rt.find("channel-plots").is_some() && rt.find("channel-general").is_none());
}

#[test]
fn a_send_the_store_does_not_hold_yet_reads_as_sending_and_a_refused_one_says_why() {
    let mut rt = runtime(signed_in());
    rt.update(|s, _| {
        s.session.pending.push(Pending {
            id: "p1".into(),
            channel_id: "general".into(),
            body: "On my way.".into(),
            sent_at_micros: 0,
            failed: None,
        });
        s.session.pending.push(Pending {
            id: "elsewhere".into(),
            channel_id: "design".into(),
            body: "Not here.".into(),
            sent_at_micros: 0,
            failed: None,
        });
        s.session.revision += 1;
    });
    settle(&mut rt);
    let shown = words(&rt);
    assert!(
        rt.find("p1").is_some() && shown.contains("Sending…") && shown.contains("On my way."),
        "{shown}"
    );
    assert!(
        rt.find("elsewhere").is_none(),
        "another channel's send stays there"
    );

    rt.update(|s, _| {
        s.receive(&Update::Gateway(lumia_client::app::AppEvent::Error {
            code: "PERMISSION_DENIED".into(),
            message: "you cannot post here".into(),
            command_id: "p1".into(),
        }));
    });
    settle(&mut rt);
    assert!(words(&rt).contains("Not sent: you cannot post here"));
    press(&mut rt, "dismiss");
    assert!(rt.state.session.pending.iter().all(|p| p.id != "p1"));
}

#[test]
fn return_in_the_composer_sends_and_a_send_that_cannot_be_queued_keeps_the_draft() {
    let mut rt = runtime(signed_in());
    press(&mut rt, "composer");
    type_text(&mut rt, "Is anyone there?");
    assert_eq!(rt.state.draft.text, "Is anyone there?");
    // No gateway loop is running behind this state, so the core refuses the send.
    rt.key(KeyEvent {
        key: Key::Enter,
        mods: Mods::default(),
    });
    settle(&mut rt);
    assert_eq!(
        rt.state.draft.text, "Is anyone there?",
        "the words are not lost"
    );
    assert!(rt.state.session.pending.is_empty());
    assert!(words(&rt).contains("Not sent:"), "and the screen says so");
}

#[test]
fn a_session_the_instance_no_longer_knows_goes_back_to_sign_in() {
    let mut rt = runtime(signed_in());
    rt.update(|s, _| {
        s.receive(&Update::Snapshot(Err(Failure {
            code: "UNAUTHENTICATED".into(),
            message: "session revoked".into(),
            details: HashMap::new(),
        })))
    });
    settle(&mut rt);
    assert_eq!(rt.state.route, Route::SignIn);
}
