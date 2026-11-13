//! Home's screens over a state no instance is behind: the sidebar, the personal conversation
//! with what its log says, requests, a new message, and what a computer without an identity
//! is told. What the instance and the DM engine would report is applied with
//! `State::receive`. `tests/live_dm.rs` is the same half of the session against a real
//! instance.

#![allow(clippy::expect_used, clippy::unwrap_used)]

mod common;

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use common::*;
use lumia_client::app::{
    App, AppEvent, ChannelSummary, DmChannelSummary, DmLogSummary, DmMessageSummary,
    DmRequestSummary, Opener, Place as DevicePlace, SnapshotSummary, StoreName, host_of,
};
use lumia_client::sqlite::SqliteStore;
use lumia_shell::app::{HomeView, Place, State};
use lumia_shell::session::dms::{Answer, DmUpdate};
use lumia_shell::session::{Conversation, Failure, Update};
use rune::{Key, KeyEvent, Mods, Runtime};

const INSTANCE: &str = "http://127.0.0.1:9";

/// Makes an identity in `dir` with the core, as a device that was linked would hold one:
/// the shell's stores are the core's, laid out as `Session::open` lays them.
fn create_identity(dir: &Path) {
    let (host, identity) = (dir.join(host_of(INSTANCE)), dir.join("identity"));
    for d in [&host, &identity] {
        std::fs::create_dir_all(d).unwrap();
    }
    let opener: Opener = Arc::new(move |name| {
        SqliteStore::open(&match name {
            StoreName::Instance => host.join("store.sqlite"),
            StoreName::Keys => host.join("keys.sqlite"),
            StoreName::IdentityStore => identity.join("store.sqlite"),
            StoreName::IdentityKeys => identity.join("keys.sqlite"),
        })
    });
    let app = App::open(INSTANCE, DevicePlace::Native, opener).unwrap();
    tokio::runtime::Builder::new_current_thread()
        .build()
        .unwrap()
        .block_on(app.identity_create("test"))
        .expect("an identity is made locally");
}

fn dm(id: &str, host: &str, others: &[&str]) -> ChannelSummary {
    let mut participants = vec!["me".to_string()];
    participants.extend(others.iter().map(|o| o.to_string()));
    ChannelSummary {
        channel_id: id.into(),
        server_id: String::new(),
        name: String::new(),
        kind: if others.len() > 1 {
            "PERSONAL_GROUP_DM"
        } else {
            "PERSONAL_DM"
        }
        .into(),
        last_seq: 0,
        participant_account_ids: participants,
        host: host.into(),
        private: false,
        is_default: false,
        member_count: 0,
        topic: String::new(),
    }
}

fn request(id: &str, from: &str, shared: &[&str], friends: u32) -> DmRequestSummary {
    DmRequestSummary {
        request_id: id.into(),
        from_account_id: from.into(),
        to_account_id: "me".into(),
        state: "PENDING".into(),
        shared_server_ids: shared.iter().map(|s| s.to_string()).collect(),
        mutual_friend_count: friends,
    }
}

/// The account's snapshot with two personal conversations, one hosted elsewhere.
fn home_snapshot() -> SnapshotSummary {
    let mut snapshot = snapshot();
    snapshot.channels.push(dm("with-grace", "", &["grace"]));
    snapshot
        .channels
        .push(dm("with-alan", "other.example", &["alan"]));
    snapshot
}

/// Signed in at Home on a computer that holds an identity (or does not), with `snapshot`.
fn at_home(identity: bool, snapshot: SnapshotSummary) -> State {
    let dir = data_dir();
    if identity {
        create_identity(&dir);
    }
    let mut state = State::new(dir);
    state.sign_in.instance.replace_text(INSTANCE);
    state.connect();
    assert!(state.session.is_open(), "{}", state.sign_in.error);
    assert_eq!(state.session.has_identity(), identity);
    state.receive(&Update::Info(Ok(info())));
    state.receive(&Update::SignedIn(Ok(())));
    state.receive(&Update::Snapshot(Ok(snapshot)));
    state.receive(&Update::Members {
        server_id: "acme".into(),
        members: vec![
            member("me", "ada", "Ada Lovelace"),
            member("grace", "grace", "Grace Hopper"),
            member("edsger", "edsger", "Edsger Dijkstra"),
        ],
    });
    // Alan shares no server: the instance showed his profile when asked.
    let mut alan = member("alan", "alan", "Alan Turing");
    alan.pronouns = "he/him".into();
    alan.bio = "Machines and morphogenesis.".into();
    state.receive(&Update::Profile {
        account_id: "alan".into(),
        profile: Some(alan),
    });
    state.go_home(HomeView::Conversations);
    state
}

fn channels(list: &[(&str, bool)]) -> Update {
    Update::Dm(DmUpdate::Channels(Ok(list
        .iter()
        .map(|(id, member)| DmChannelSummary {
            channel_id: id.to_string(),
            participant_account_ids: Vec::new(),
            member: *member,
        })
        .collect())))
}

fn said(id: &str, mine: bool, account: &str, body: &str, micros: i64) -> DmMessageSummary {
    DmMessageSummary {
        message_id: id.into(),
        seq: 0,
        sender_identity_id: if mine { "aa" } else { "bb" }.into(),
        sender_device_id: "device".into(),
        sender_account_id: account.into(),
        body: body.into(),
        sent_at_micros: micros,
        edited: false,
        deleted: false,
        mine,
    }
}

fn log(channel: &str, messages: Vec<DmMessageSummary>, before_join: u32, lost: u32) -> Update {
    Update::Dm(DmUpdate::Log {
        channel_id: channel.into(),
        log: Ok(DmLogSummary {
            messages,
            before_join,
            undecryptable: lost,
        }),
    })
}

fn failure(code: &str, message: &str) -> Failure {
    Failure {
        code: code.into(),
        message: message.into(),
        details: HashMap::new(),
    }
}

fn receive(rt: &mut Runtime<State>, update: Update) {
    rt.update(|s, _| s.receive(&update));
    settle(rt);
}

fn enter(rt: &mut Runtime<State>) {
    rt.key(KeyEvent {
        key: Key::Enter,
        mods: Mods::default(),
    });
    settle(rt);
}

fn now() -> i64 {
    chrono::Utc::now().timestamp_micros()
}

#[test]
fn a_computer_without_an_identity_is_told_so_and_sent_to_devices() {
    let mut rt = runtime(at_home(false, home_snapshot()));
    for view in [
        HomeView::Conversations,
        HomeView::Requests,
        HomeView::NewMessage,
    ] {
        rt.update(|s, _| s.go_home(view));
        settle(&mut rt);
        let shown = words(&rt);
        assert!(
            shown.contains("Link this computer to your identity")
                && shown.contains("This computer has no device key yet"),
            "{view:?}:\n{shown}"
        );
        assert!(
            rt.find("composer").is_none() && rt.find("find-person").is_none(),
            "{view:?} offers nothing that needs an identity"
        );
    }
    // The conversations are still listed: the account is in them, this computer is not.
    assert!(rt.find("dm-with-grace").is_some());
    press(&mut rt, "dm-with-grace");
    assert!(words(&rt).contains("Link this computer to your identity"));
    assert!(rt.find("participants").is_none(), "and no aside");

    press(&mut rt, "link-this-computer");
    assert_eq!(rt.state.home_view, HomeView::Devices);
}

#[test]
fn the_sidebar_has_the_tiles_and_the_conversations_grouped_by_host() {
    let mut snapshot = home_snapshot();
    snapshot
        .pending_requests
        .push(request("r1", "edsger", &["acme"], 0));
    let mut rt = runtime(at_home(true, snapshot));
    let shown = words(&rt);
    for expected in [
        "Home",
        "Requests, 1 waiting",
        "Devices",
        "New",
        "this instance",
        "other.example",
        "Conversation with Grace Hopper",
        "Conversation with Alan Turing",
        "Add a server",
        // Nothing is chosen yet.
        "Your conversations",
        "Pick one on the left",
    ] {
        assert!(shown.contains(expected), "{expected:?} in:\n{shown}");
    }
    assert!(
        rt.find("channel-with-grace").is_none() && rt.state.session.conversation.is_none(),
        "a personal conversation is not a server's channel, and none opens by itself"
    );

    press(&mut rt, "tile-requests");
    assert_eq!(rt.state.home_view, HomeView::Requests);
    press(&mut rt, "tile-devices");
    assert_eq!(rt.state.home_view, HomeView::Devices);
    press(&mut rt, "tile-new");
    assert_eq!(rt.state.home_view, HomeView::NewMessage);
    press(&mut rt, "home-add-server");
    assert_eq!(rt.state.home_view, HomeView::AddServer);
    press(&mut rt, "new-message");
    assert_eq!(rt.state.home_view, HomeView::NewMessage);

    press(&mut rt, "home");
    press(&mut rt, "start-new-message");
    assert_eq!(rt.state.home_view, HomeView::NewMessage);
}

#[test]
fn home_without_conversations_says_so() {
    let rt = runtime(at_home(true, snapshot()));
    let shown = words(&rt);
    assert!(
        shown.contains("No conversations yet.") && shown.contains("Your conversations"),
        "{shown}"
    );
    assert!(
        !shown.contains("waiting"),
        "no count on Requests with none waiting"
    );
}

#[test]
fn a_conversation_shows_its_messages_and_what_its_log_could_not_read() {
    let mut rt = runtime(at_home(true, home_snapshot()));
    press(&mut rt, "dm-with-alan");
    assert_eq!(
        rt.state.session.conversation,
        Some(Conversation::Dm("with-alan".into()))
    );
    assert_eq!(rt.state.home_view, HomeView::Conversations);
    // Before the engine has answered: nothing to read, nothing to write with.
    let shown = words(&rt);
    assert!(
        shown.contains("Setting up the encrypted group…")
            && shown.contains("Only your devices and those of Alan Turing can read them."),
        "{shown}"
    );

    receive(&mut rt, channels(&[("with-alan", true)]));
    let t = now();
    receive(
        &mut rt,
        log(
            "with-alan",
            vec![
                // The engine has not tied this identity to an account yet: in a
                // conversation of two it can only be the other side.
                said("m1", false, "", "Is the machine thinking?", t - 120_000_000),
                said("m2", true, "", "It is **computing**.", t - 60_000_000),
            ],
            2,
            1,
        ),
    );
    let shown = words(&rt);
    for expected in [
        "Alan Turing",
        "he/him · on other.example",
        "Messages with Alan Turing",
        "Is the machine thinking?",
        "Ada Lovelace",
        "Message Alan Turing",
        "2 earlier messages were sent before this device joined and cannot be read here.",
        "1 could not be decrypted.",
        // The aside.
        "Details",
        "In this conversation",
        "@alan · he/him",
        "Machines and morphogenesis.",
        "@ada",
        "You",
    ] {
        assert!(shown.contains(expected), "{expected:?} in:\n{shown}");
    }
    assert!(rt.find("m1").is_some() && rt.find("m2").is_some());
    assert!(
        rt.find("rejoin").is_none(),
        "a member has nothing to rejoin"
    );
    assert_eq!(rt.state.session.entries[0].author_account_id, "alan");
    assert_eq!(rt.state.session.entries[1].author_account_id, "me");

    // One message before the join reads as one.
    receive(&mut rt, log("with-alan", Vec::new(), 1, 0));
    let shown = words(&rt);
    assert!(
        shown.contains("1 earlier message was sent before this device joined")
            && !shown.contains("could not be decrypted")
            && shown.contains("Nothing here yet"),
        "{shown}"
    );

    // Another conversation's log does not land in this one.
    receive(
        &mut rt,
        log(
            "with-grace",
            vec![said("g1", false, "grace", "Elsewhere.", t)],
            0,
            0,
        ),
    );
    assert!(rt.state.session.entries.is_empty());
    // It is what that conversation shows at once when it is opened.
    press(&mut rt, "dm-with-grace");
    assert!(words(&rt).contains("Elsewhere."));
    assert_eq!(rt.state.session.dm(), Some("with-grace"));
}

#[test]
fn a_device_outside_the_group_cannot_write_and_can_rejoin() {
    let mut rt = runtime(at_home(true, home_snapshot()));
    press(&mut rt, "dm-with-grace");
    receive(&mut rt, channels(&[("with-grace", false)]));
    let shown = words(&rt);
    assert!(
        shown.contains("This device is not in the conversation's group yet.")
            && shown.contains("Setting up the encrypted group…")
            && shown.contains("Nothing to read yet"),
        "{shown}"
    );
    // The composer takes nothing.
    press(&mut rt, "composer");
    type_text(&mut rt, "hello?");
    enter(&mut rt);
    assert_eq!(rt.state.draft.text, "");
    assert!(rt.state.session.pending.is_empty());
    // Nor does the session, asked directly.
    rt.update(|s, _| assert!(!s.session.send("hello?", 0)));
    assert!(rt.state.session.pending.is_empty());

    press(&mut rt, "rejoin");
    assert!(rt.state.home.rejoining);
    assert!(words(&rt).contains("Rejoining…"));
    receive(
        &mut rt,
        Update::Dm(DmUpdate::Rejoined {
            channel_id: "with-grace".into(),
            result: Err(failure("NOT_FOUND", "no group info is published")),
        }),
    );
    assert!(!rt.state.home.rejoining);
    let shown = words(&rt);
    assert!(
        shown.contains("no group info is published") && shown.contains("Rejoin"),
        "{shown}"
    );

    press(&mut rt, "rejoin");
    receive(&mut rt, channels(&[("with-grace", true)]));
    receive(
        &mut rt,
        Update::Dm(DmUpdate::Rejoined {
            channel_id: "with-grace".into(),
            result: Ok(()),
        }),
    );
    let shown = words(&rt);
    assert!(
        rt.find("rejoin").is_none()
            && !shown.contains("no group info is published")
            && shown.contains("Message Grace Hopper"),
        "{shown}"
    );
}

#[test]
fn a_send_is_pending_until_the_log_holds_it_and_one_that_cannot_be_queued_says_why() {
    let mut rt = runtime(at_home(true, home_snapshot()));
    press(&mut rt, "dm-with-grace");
    receive(&mut rt, channels(&[("with-grace", true)]));
    let before = now();
    receive(
        &mut rt,
        log(
            "with-grace",
            // The same words, said before: not the send that is about to be made.
            vec![said("old", true, "me", "On my way.", before - 1_000_000)],
            0,
            0,
        ),
    );

    press(&mut rt, "composer");
    type_text(&mut rt, "On my way.");
    enter(&mut rt);
    assert_eq!(rt.state.draft.text, "", "the draft is taken");
    assert_eq!(rt.state.session.pending.len(), 1);
    let placeholder = rt.state.session.pending[0].id.clone();
    assert!(words(&rt).contains("Sending…"));

    // The same words in the log under another id are not this send.
    let old = said("old", true, "me", "On my way.", before - 1_000_000);
    let other = said("other", true, "me", "On my way.", now());
    receive(
        &mut rt,
        log("with-grace", vec![old.clone(), other.clone()], 0, 0),
    );
    assert_eq!(rt.state.session.pending.len(), 1);

    // The engine queued it and says which message it is; the log then settles it.
    receive(
        &mut rt,
        Update::Dm(DmUpdate::Sent {
            pending_id: placeholder,
            result: Ok("new1".into()),
        }),
    );
    assert_eq!(rt.state.session.pending[0].id, "new1");
    assert!(words(&rt).contains("Sending…"));
    let mut held = vec![old, other, said("new1", true, "me", "On my way.", now())];
    receive(&mut rt, log("with-grace", held.clone(), 0, 0));
    assert!(rt.state.session.pending.is_empty());
    assert_eq!(rt.state.session.entries.len(), 3);
    assert!(!words(&rt).contains("Sending…"));

    // A log read before the engine's answer settles the send when the answer names it.
    type_text(&mut rt, "Nearly there.");
    enter(&mut rt);
    let placeholder = rt.state.session.pending[0].id.clone();
    held.push(said("new2", true, "me", "Nearly there.", now()));
    receive(&mut rt, log("with-grace", held.clone(), 0, 0));
    assert_eq!(
        rt.state.session.pending.len(),
        1,
        "not yet known to be new2"
    );
    receive(
        &mut rt,
        Update::Dm(DmUpdate::Sent {
            pending_id: placeholder,
            result: Ok("new2".into()),
        }),
    );
    assert!(rt.state.session.pending.is_empty());

    // A send the instance refuses after it was queued is marked by its id.
    type_text(&mut rt, "Let me in.");
    enter(&mut rt);
    let placeholder = rt.state.session.pending[0].id.clone();
    receive(
        &mut rt,
        Update::Dm(DmUpdate::Sent {
            pending_id: placeholder,
            result: Ok("new3".into()),
        }),
    );
    receive(
        &mut rt,
        Update::Gateway(AppEvent::Error {
            code: "PERMISSION_DENIED".into(),
            message: "you are not in this conversation".into(),
            command_id: "new3".into(),
        }),
    );
    assert!(words(&rt).contains("Not sent: you are not in this conversation"));
    press(&mut rt, "dismiss");
    assert!(rt.state.session.pending.is_empty());

    // A send the engine cannot queue is marked with the reason and can be dismissed.
    press(&mut rt, "composer");
    type_text(&mut rt, "Are you there?");
    enter(&mut rt);
    let id = rt.state.session.pending[0].id.clone();
    receive(
        &mut rt,
        Update::Dm(DmUpdate::Sent {
            pending_id: id,
            result: Err(failure("", "a commit is pending")),
        }),
    );
    assert!(words(&rt).contains("Not sent: a commit is pending"));
    // A failed send is not settled by someone saying the same words.
    receive(
        &mut rt,
        log(
            "with-grace",
            vec![said("x", true, "me", "Are you there?", now())],
            0,
            0,
        ),
    );
    assert_eq!(rt.state.session.pending.len(), 1);
    press(&mut rt, "dismiss");
    assert!(rt.state.session.pending.is_empty());
}

#[test]
fn the_engine_s_error_is_shown_and_a_log_that_cannot_be_read_says_why() {
    let mut rt = runtime(at_home(true, home_snapshot()));
    press(&mut rt, "dm-with-grace");
    receive(
        &mut rt,
        Update::Gateway(AppEvent::DmError {
            message: "a leaf failed roster validation".into(),
        }),
    );
    assert!(words(&rt).contains("a leaf failed roster validation"));
    receive(
        &mut rt,
        Update::Dm(DmUpdate::Log {
            channel_id: "with-grace".into(),
            log: Err(failure("", "the store is locked")),
        }),
    );
    assert!(words(&rt).contains("the store is locked"));
}

#[test]
fn requests_say_who_asks_and_what_is_shared_and_accepting_opens_the_conversation() {
    let mut snapshot = snapshot();
    snapshot.pending_requests = vec![
        request("r1", "edsger", &["acme"], 2),
        request("r2", "alan", &[], 0),
        request("r3", "unknown-account", &["acme", "garden"], 1),
    ];
    let mut rt = runtime(at_home(true, snapshot));
    press(&mut rt, "tile-requests");
    let shown = words(&rt);
    for expected in [
        "People who want to message you",
        "the sender cannot see that you looked",
        "Request from Edsger Dijkstra",
        "@edsger",
        "Shares a server with you · 2 mutual friends",
        "Alan Turing",
        "Nothing in common yet",
        // Someone nothing is known of reads as the start of their id.
        "unknown-",
        "Shares 2 servers with you · 1 mutual friend",
    ] {
        assert!(shown.contains(expected), "{expected:?} in:\n{shown}");
    }

    // Declining: one answer at a time, and the request leaves with the next snapshot.
    press(&mut rt, "decline-r2");
    assert_eq!(rt.state.home.answering.as_deref(), Some("r2"));
    press(&mut rt, "accept-r1");
    assert_eq!(
        rt.state.home.answering.as_deref(),
        Some("r2"),
        "the others wait"
    );
    let mut after = common::snapshot();
    after.pending_requests = vec![
        request("r1", "edsger", &["acme"], 2),
        request("r3", "unknown-account", &["acme", "garden"], 1),
    ];
    receive(&mut rt, Update::Snapshot(Ok(after.clone())));
    receive(
        &mut rt,
        Update::Dm(DmUpdate::Answered {
            request_id: "r2".into(),
            answer: Answer::Decline,
            result: Ok(None),
        }),
    );
    assert_eq!(rt.state.home_view, HomeView::Requests);
    assert!(rt.state.home.answering.is_none());
    assert!(rt.find("request-r2").is_none() && rt.find("request-r1").is_some());

    // An answer the instance refuses says why.
    press(&mut rt, "block-r3");
    receive(
        &mut rt,
        Update::Dm(DmUpdate::Answered {
            request_id: "r3".into(),
            answer: Answer::Block,
            result: Err(failure("NOT_FOUND", "that request is gone")),
        }),
    );
    assert!(words(&rt).contains("that request is gone"));

    // Accepting: the snapshot with the conversation, then the answer, which opens it.
    press(&mut rt, "accept-r1");
    assert!(
        !words(&rt).contains("that request is gone"),
        "a new answer clears the old error"
    );
    after.pending_requests.remove(0);
    after.channels.push(dm("with-edsger", "", &["edsger"]));
    receive(&mut rt, Update::Snapshot(Ok(after)));
    receive(
        &mut rt,
        Update::Dm(DmUpdate::Answered {
            request_id: "r1".into(),
            answer: Answer::Accept,
            result: Ok(Some("with-edsger".into())),
        }),
    );
    assert_eq!(rt.state.home_view, HomeView::Conversations);
    assert_eq!(rt.state.session.dm(), Some("with-edsger"));
    let shown = words(&rt);
    assert!(
        shown.contains("Only your devices and those of Edsger Dijkstra can read them."),
        "{shown}"
    );
}

#[test]
fn no_requests_reads_as_nothing_waiting() {
    let mut rt = runtime(at_home(true, home_snapshot()));
    press(&mut rt, "tile-requests");
    let shown = words(&rt);
    assert!(
        shown.contains("Nothing waiting")
            && shown.contains("Declined requests disappear; blocked people cannot ask again."),
        "{shown}"
    );
}

#[test]
fn a_new_message_finds_a_person_and_opens_or_asks() {
    let mut rt = runtime(at_home(true, home_snapshot()));
    press(&mut rt, "tile-new");
    let shown = words(&rt);
    assert!(
        shown.contains("Someone you share a server with")
            && shown.contains("Find a person")
            && shown.contains("Edsger Dijkstra")
            && shown.contains("@grace"),
        "{shown}"
    );
    assert!(
        rt.find("person-me").is_none(),
        "this account is not offered to itself"
    );
    assert!(
        rt.find("person-alan").is_none(),
        "someone no server lists is not offered"
    );

    press(&mut rt, "find-person");
    type_text(&mut rt, "EDS");
    assert!(rt.find("person-edsger").is_some() && rt.find("person-grace").is_none());

    // The instance wants a request first, and one went out.
    press(&mut rt, "message-edsger");
    assert_eq!(rt.state.home.starting.as_deref(), Some("edsger"));
    receive(
        &mut rt,
        Update::Dm(DmUpdate::Started {
            account_id: "edsger".into(),
            result: Ok(None),
        }),
    );
    assert!(rt.state.home.starting.is_none());
    assert!(
        words(&rt)
            .contains("Request sent to Edsger Dijkstra. The conversation opens when they accept.")
    );
    assert_eq!(rt.state.home_view, HomeView::NewMessage);

    // Asked already.
    press(&mut rt, "message-edsger");
    receive(
        &mut rt,
        Update::Dm(DmUpdate::Started {
            account_id: "edsger".into(),
            result: Err(failure("ALREADY_EXISTS", "a request is already pending")),
        }),
    );
    assert!(words(&rt).contains("You already asked; waiting for their answer."));

    // Anything else is said as the instance said it.
    press(&mut rt, "message-edsger");
    receive(
        &mut rt,
        Update::Dm(DmUpdate::Started {
            account_id: "edsger".into(),
            result: Err(failure("PERMISSION_DENIED", "they have blocked you")),
        }),
    );
    assert!(words(&rt).contains("they have blocked you"));

    // Nobody matches.
    press(&mut rt, "find-person");
    type_text(&mut rt, "zzz");
    assert!(words(&rt).contains("No one to show"));
    rt.update(|s, _| s.home.query.clear());
    settle(&mut rt);

    // A conversation that can be opened is: the snapshot that lists it, then the answer.
    press(&mut rt, "message-grace");
    receive(
        &mut rt,
        Update::Dm(DmUpdate::Started {
            account_id: "grace".into(),
            result: Ok(Some("with-grace".into())),
        }),
    );
    assert_eq!(rt.state.home_view, HomeView::Conversations);
    assert_eq!(rt.state.session.dm(), Some("with-grace"));
    assert!(
        rt.state.home.note.is_empty() && rt.state.home.query.text.is_empty(),
        "the form is left clean for the next time"
    );
}

#[test]
fn an_answer_that_comes_after_leaving_home_does_not_pull_the_shell_back() {
    let mut rt = runtime(at_home(true, home_snapshot()));
    press(&mut rt, "tile-new");
    press(&mut rt, "message-grace");
    press(&mut rt, "server-acme");
    receive(
        &mut rt,
        Update::Dm(DmUpdate::Started {
            account_id: "grace".into(),
            result: Ok(Some("with-grace".into())),
        }),
    );
    assert_eq!(rt.state.place, Place::Server("acme".into()));
    assert_eq!(rt.state.session.channel(), Some("general"));
    assert!(rt.state.home.starting.is_none());
}

#[test]
fn going_to_a_server_and_back_keeps_home_where_it_was_without_a_channel_behind_it() {
    let mut rt = runtime(at_home(true, home_snapshot()));
    press(&mut rt, "dm-with-grace");
    receive(&mut rt, channels(&[("with-grace", true)]));
    press(&mut rt, "composer");
    type_text(&mut rt, "half a thought");
    press(&mut rt, "dm-with-alan");
    assert_eq!(
        rt.state.draft.text, "",
        "a draft does not follow into another conversation"
    );
    press(&mut rt, "server-acme");
    assert_eq!(rt.state.session.channel(), Some("general"));
    press(&mut rt, "home");
    assert!(
        rt.state.session.conversation.is_none(),
        "the channel is not open behind Home"
    );
    assert!(words(&rt).contains("Your conversations"));
}
