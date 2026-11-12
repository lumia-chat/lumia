//! System notifications (architecture 15.5) without a system: when the shell asks whether
//! a message is worth a notice, what it then owes, and what a click on one opens. Nothing
//! is posted here; `rune-notify` posts only from a bundled app, and `platform::settle`
//! drains what is owed either way.

#![allow(clippy::expect_used, clippy::unwrap_used)]

mod common;

use common::*;
use lumia_client::app::{AppEvent, ChannelSummary};
use lumia_shell::app::{HomeView, Place, Route, State};
use lumia_shell::platform;
use lumia_shell::session::notices::{Notice, NoticeUpdate, shorten};
use lumia_shell::session::{Conversation, Update};

const DM: &str = "dm-grace";

/// Signed in, with a personal conversation beside the two servers.
fn with_a_conversation() -> State {
    let mut state = signed_in();
    let mut snap = snapshot();
    snap.channels.push(ChannelSummary {
        channel_id: DM.into(),
        server_id: String::new(),
        name: String::new(),
        kind: "PERSONAL_DM".into(),
        last_seq: 0,
        participant_account_ids: vec!["me".into(), "grace".into()],
        host: String::new(),
    });
    state.receive(&Update::Snapshot(Ok(snap)));
    state
}

/// The gateway saying an operation landed in the conversation.
fn landed(seq: u64) -> Update {
    Update::Gateway(AppEvent::DmOperation {
        channel_id: DM.into(),
        seq,
        sender_identity_id: "aa".into(),
        sender_device_id: "device".into(),
    })
}

fn notice(seq: u64, body: &str) -> Notice {
    Notice {
        channel_id: DM.into(),
        seq,
        title: "Grace Hopper".into(),
        body: body.into(),
    }
}

/// The store's answer about the operation at `asked`.
fn read_back(asked: u64, found: Option<Notice>) -> Update {
    Update::Notice(NoticeUpdate {
        channel_id: DM.into(),
        seq: asked,
        notice: found,
    })
}

#[test]
fn a_message_from_someone_else_is_owed_a_notice_while_the_window_is_not_in_front() {
    let mut state = with_a_conversation();
    assert!(
        !state.notices.in_front,
        "a window that never had the focus is not in front"
    );
    state.receive(&landed(5));
    state.receive(&read_back(5, Some(notice(5, "Are you there?"))));
    assert_eq!(state.notices.to_post, [notice(5, "Are you there?")]);

    // The same message is told once, however often the store is asked about it.
    state.notices.to_post.clear();
    state.receive(&landed(5));
    state.receive(&read_back(5, Some(notice(5, "Are you there?"))));
    assert!(state.notices.to_post.is_empty());

    // An edit, a reaction, this account's own message from another device: nothing.
    state.receive(&landed(6));
    state.receive(&read_back(6, None));
    assert!(state.notices.to_post.is_empty());

    // Two land while the store is being read: the answer to the first is told, and the
    // second is asked about and told in its turn.
    state.receive(&landed(7));
    state.receive(&landed(8));
    state.receive(&read_back(7, Some(notice(7, "One."))));
    state.receive(&read_back(8, Some(notice(8, "Two."))));
    assert_eq!(
        state.notices.to_post,
        [notice(7, "One."), notice(8, "Two.")]
    );
}

#[test]
fn nothing_is_owed_to_someone_looking_at_the_window_or_signed_out() {
    let mut state = with_a_conversation();
    state.window_focused(true);
    state.receive(&landed(5));
    state.receive(&read_back(5, Some(notice(5, "Hello"))));
    assert!(state.notices.to_post.is_empty(), "the window is in front");

    // The window loses the focus: the next message is owed one.
    state.window_focused(false);
    state.receive(&landed(6));
    state.receive(&read_back(6, Some(notice(6, "Still there?"))));
    assert_eq!(state.notices.to_post.len(), 1);

    // Coming back in front before it was posted takes it back.
    state.window_focused(true);
    assert!(state.notices.to_post.is_empty());

    // An answer that arrives after the account signed out is dropped.
    state.window_focused(false);
    state.receive(&landed(7));
    state.sign_out();
    state.receive(&read_back(7, Some(notice(7, "Too late"))));
    assert!(state.notices.to_post.is_empty());
    assert_eq!(state.route, Route::SignIn);
}

#[test]
fn the_window_poll_takes_what_is_owed() {
    assert!(
        !rune_notify::available(),
        "only a bundled app may post; a test binary is not one"
    );
    let mut rt = runtime(with_a_conversation());
    rt.update(|s, tx| {
        s.receive(&landed(5));
        s.receive(&read_back(5, Some(notice(5, "Hello"))));
        assert_eq!(s.notices.to_post.len(), 1);
        // A test is no bundled app, so nothing reaches the system; the debt is still paid.
        platform::settle(s, tx);
    });
    assert!(rt.state.notices.to_post.is_empty());
}

#[test]
fn a_clicked_notice_opens_its_conversation() {
    let mut state = with_a_conversation();
    assert_eq!(state.place, Place::Server("acme".into()));
    assert_eq!(state.session.channel(), Some("general"));

    // A channel of the server on screen.
    state.open_notice("design");
    assert_eq!(state.session.channel(), Some("design"));

    // A channel of another server.
    state.open_notice("plots");
    assert_eq!(state.place, Place::Server("garden".into()));
    assert_eq!(state.session.channel(), Some("plots"));

    // A personal conversation: Home, with it on screen.
    state.open_notice(DM);
    assert_eq!(state.place, Place::Home);
    assert_eq!(state.home_view, HomeView::Conversations);
    assert_eq!(
        state.session.conversation,
        Some(Conversation::Dm(DM.into()))
    );

    // A conversation this account cannot see (the id comes back from outside the app).
    state.open_notice("somebody-elses");
    assert_eq!(state.place, Place::Home);
    assert_eq!(
        state.session.conversation,
        Some(Conversation::Dm(DM.into()))
    );
    assert!(!state.settle_notice(), "nothing is left waiting");
}

#[test]
fn a_click_that_starts_the_app_waits_for_the_snapshot() {
    let mut rt = runtime({
        let mut state = opened();
        state.receive(&Update::SignedIn(Ok(())));
        state
    });
    assert_eq!(rt.state.route, Route::Shell);
    rt.state.open_notice("design");
    assert_eq!(
        rt.state.session.channel(),
        None,
        "nothing to open it in yet"
    );

    rt.update(|s, tx| {
        s.receive(&Update::Snapshot(Ok(snapshot())));
        assert!(
            platform::settle(s, tx),
            "the poll after the snapshot opens it"
        );
    });
    settle(&mut rt);
    assert_eq!(rt.state.place, Place::Server("acme".into()));
    assert_eq!(rt.state.session.channel(), Some("design"));

    // Signed out, a click has nothing to open and is forgotten.
    let mut state = opened();
    state.open_notice("design");
    state.receive(&Update::SignedIn(Ok(())));
    state.receive(&Update::Snapshot(Ok(snapshot())));
    assert!(!state.settle_notice());
    assert_eq!(state.session.channel(), Some("general"));
}

#[test]
fn a_long_message_is_cut_for_the_notice() {
    assert_eq!(shorten("  Short.  "), "Short.");
    let long = "é".repeat(500);
    let cut = shorten(&long);
    assert_eq!(cut.chars().count(), 241, "240 characters and an ellipsis");
    assert!(cut.ends_with('…'));
}
