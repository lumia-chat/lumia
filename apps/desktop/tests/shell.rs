//! The screens over a state no instance is behind, driven through a windowless `Runtime`:
//! what the instance would report is applied with `State::receive`, and the tree Rune
//! builds is read back. `tests/live.rs` is the same shell against a real instance.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};

use lumia_client::app::{
    ChannelSummary, InstanceSummary, MemberSummary, MessageSummary, ServerSummary, SnapshotSummary,
};
use lumia_shell::app::{Route, State};
use lumia_shell::session::{Failure, Pending, Update};
use lumia_shell::ui;
use rune::{Env, Key, KeyEvent, Mods, Runtime};

fn data_dir() -> PathBuf {
    static NEXT: AtomicU32 = AtomicU32::new(0);
    let dir = std::env::temp_dir().join(format!(
        "lumia-shell-test-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

fn runtime(state: State) -> Runtime<State> {
    let mut rt = Runtime::new(state, ui::view, (1180.0, 760.0), Env::new(false, 2.0));
    settle(&mut rt);
    rt
}

fn settle(rt: &mut Runtime<State>) {
    rt.frame(0.0);
    for _ in 0..20 {
        rt.frame(1.0 / 60.0);
    }
}

fn press(rt: &mut Runtime<State>, key: &str) {
    let (x, y) = rt
        .find(key)
        .unwrap_or_else(|| panic!("{key} is not on screen"));
    rt.pointer_move(x, y);
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    settle(rt);
}

fn type_text(rt: &mut Runtime<State>, text: &str) {
    for c in text.chars() {
        rt.key(KeyEvent {
            key: Key::Text(c.to_string()),
            mods: Mods::default(),
        });
    }
    settle(rt);
}

/// Every text and label on screen, to look for wording in.
fn words(rt: &Runtime<State>) -> String {
    rt.elements()
        .into_iter()
        .flat_map(|e| [e.text, e.label])
        .flatten()
        .collect::<Vec<_>>()
        .join("\n")
}

fn info() -> InstanceSummary {
    InstanceSummary {
        host: "chat.example.com".into(),
        operator_name: "Example Co".into(),
        operator_url: String::new(),
        terms_url: String::new(),
        privacy_url: String::new(),
        software_version: "0.2.1".into(),
        identity_proof_sign_in_enabled: false,
        identity_proof_create_enabled: false,
        open_registration: true,
        web_push_public_key: String::new(),
        magic_link_enabled: false,
        oidc_name: String::new(),
    }
}

/// A state with an instance open that nothing answers for, its trust card as given.
fn opened() -> State {
    let mut state = State::new(data_dir());
    state.sign_in.instance.replace_text("http://127.0.0.1:9");
    state.connect();
    assert!(
        state.session.is_open(),
        "opening needs only the local stores: {}",
        state.sign_in.error
    );
    state.receive(&Update::Info(Ok(info())));
    state
}

fn refusal(code: &str, details: &[(&str, &str)]) -> Update {
    let details: HashMap<String, String> = details
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();
    Update::SignedIn(Err(Failure {
        code: code.into(),
        message: format!("{code} from the instance"),
        details,
    }))
}

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

fn snapshot() -> SnapshotSummary {
    let channel = |id: &str, server: &str, name: &str, kind: &str| ChannelSummary {
        channel_id: id.into(),
        server_id: server.into(),
        name: name.into(),
        kind: kind.into(),
        last_seq: 0,
        participant_account_ids: Vec::new(),
        host: String::new(),
    };
    SnapshotSummary {
        account_id: "me".into(),
        handle: "ada".into(),
        display_name: "Ada Lovelace".into(),
        servers: vec![
            ServerSummary {
                server_id: "acme".into(),
                name: "Acme Works".into(),
            },
            ServerSummary {
                server_id: "garden".into(),
                name: "Garden".into(),
            },
        ],
        channels: vec![
            channel("general", "acme", "general", "TEXT"),
            channel("design", "acme", "design", "TEXT"),
            channel("plots", "garden", "plots", "TEXT"),
            channel("dm", "", "", "PERSONAL_DM"),
        ],
        pending_requests: Vec::new(),
    }
}

fn member(id: &str, handle: &str, name: &str) -> MemberSummary {
    MemberSummary {
        account_id: id.into(),
        handle: handle.into(),
        display_name: name.into(),
        nickname: String::new(),
        pronouns: String::new(),
        bio: String::new(),
    }
}

fn message(id: &str, author: &str, body: &str, micros: i64) -> MessageSummary {
    MessageSummary {
        message_id: id.into(),
        channel_id: "general".into(),
        seq: 0,
        author_account_id: author.into(),
        body: body.into(),
        created_at_micros: micros,
        deleted: false,
    }
}

/// Signed in, with two servers and their members, and three messages in the open channel.
fn signed_in() -> State {
    let mut state = opened();
    state.receive(&Update::SignedIn(Ok(())));
    state.receive(&Update::Snapshot(Ok(snapshot())));
    state.receive(&Update::Members {
        server_id: "acme".into(),
        members: vec![
            member("me", "ada", "Ada Lovelace"),
            member("grace", "grace", "Grace Hopper"),
        ],
    });
    let now = chrono::Utc::now().timestamp_micros();
    let day = 86_400_000_000;
    state.session.messages = vec![
        message("m1", "grace", "Shipped on **Friday**.", now - 3 * day),
        message("m2", "me", "Reading it now.", now - 60_000_000),
        message("m3", "stranger", "Who am I?", now),
    ];
    state.session.revision += 1;
    state
}

#[test]
fn the_shell_lands_on_the_first_server_and_its_first_channel() {
    let rt = runtime(signed_in());
    assert_eq!(rt.state.route, Route::Shell);
    assert_eq!(rt.state.server.as_deref(), Some("acme"));
    assert_eq!(rt.state.session.channel.as_deref(), Some("general"));
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
    assert!(
        rt.find("channel-dm").is_none(),
        "a personal conversation is not a channel"
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
fn a_press_changes_channel_and_server() {
    let mut rt = runtime(signed_in());
    press(&mut rt, "channel-design");
    assert_eq!(rt.state.session.channel.as_deref(), Some("design"));
    assert!(words(&rt).contains("Nothing here yet"));

    press(&mut rt, "server-garden");
    assert_eq!(rt.state.server.as_deref(), Some("garden"));
    assert_eq!(rt.state.session.channel.as_deref(), Some("plots"));
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
