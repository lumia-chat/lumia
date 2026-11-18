//! What is typed and not sent (#23) through a windowless `Runtime` over a real core on a
//! directory of its own: each conversation keeps its words and the reply they answer when
//! another comes on screen, a second launch on the same directory finds them, a send and an
//! emptied field keep nothing, and signing out forgets them. The channels here have ids
//! the core takes; the other suites' (`general`) keep nothing, as a store that refuses does.

#![allow(clippy::expect_used, clippy::unwrap_used)]

mod common;

use common::*;
use lumia_client::app::QuoteSummary;
use lumia_shell::app::State;
use lumia_shell::app::drafts::KEEP_AFTER_MICROS;
use lumia_shell::session::Update;
use rune::Runtime;

const GENERAL: &str = "01890000-0000-7000-8000-00000000a001";
const DESIGN: &str = "01890000-0000-7000-8000-00000000a002";

/// Signed in on `dir`, in #general, with #design beside it.
fn signed_in_at(dir: std::path::PathBuf) -> State {
    let mut state = State::new(dir);
    state.sign_in.instance.replace_text("http://127.0.0.1:9");
    state.connect();
    assert!(state.session.is_open(), "{}", state.sign_in.error);
    state.receive(&Update::Info(Ok(info())));
    state.receive(&Update::SignedIn(Ok(())));
    let mut snapshot = snapshot();
    snapshot.channels[0].channel_id = GENERAL.into();
    snapshot.channels[1].channel_id = DESIGN.into();
    state.receive(&Update::Snapshot(Ok(snapshot)));
    for channel in [GENERAL, DESIGN] {
        state
            .session
            .channel_members
            .insert(channel.into(), vec!["me".to_string(), "grace".to_string()]);
    }
    state
}

fn now() -> i64 {
    chrono::Utc::now().timestamp_micros()
}

/// The window looks at the composer, as it does before every frame.
fn look(rt: &mut Runtime<State>, at: i64) {
    rt.update(move |s, _| {
        s.tick(at);
    });
    settle(rt);
}

fn typed(rt: &mut Runtime<State>, words: &str) {
    press(rt, "composer");
    type_text(rt, words);
}

fn open(rt: &mut Runtime<State>, channel: &'static str) {
    rt.update(move |s, _| s.select_channel(channel));
    look(rt, now());
}

fn quote() -> QuoteSummary {
    QuoteSummary {
        message_id: "01890000-0000-7000-8000-00000000b001".into(),
        author_account_id: "grace".into(),
        text: "Shipped on Friday.".into(),
        ..QuoteSummary::default()
    }
}

#[test]
fn each_conversation_keeps_its_own_words_and_the_reply_they_answer() {
    let mut rt = runtime(signed_in_at(data_dir()));
    look(&mut rt, now());
    assert_eq!(rt.state.session.channel(), Some(GENERAL));
    typed(&mut rt, "Half a thought");
    rt.update(|s, _| s.replies.replying = Some(quote()));

    open(&mut rt, DESIGN);
    assert_eq!(
        rt.state.draft.text, "",
        "another conversation's field is its own"
    );
    assert_eq!(rt.state.replies.replying, None);
    typed(&mut rt, "Elsewhere");

    open(&mut rt, GENERAL);
    assert_eq!(rt.state.draft.text, "Half a thought");
    assert_eq!(rt.state.replies.replying, Some(quote()));
    // On screen, where it was typed, the caret after it.
    assert!(words(&rt).contains("Half a thought"), "{}", words(&rt));
    type_text(&mut rt, ".");
    assert_eq!(rt.state.draft.text, "Half a thought.");

    open(&mut rt, DESIGN);
    assert_eq!(rt.state.draft.text, "Elsewhere");
    assert_eq!(rt.state.replies.replying, None);
    // Home and back, and a channel chosen again while it is open, lose nothing either.
    rt.update(|s, _| s.go_home(lumia_shell::app::HomeView::Conversations));
    look(&mut rt, now());
    assert_eq!(rt.state.draft.text, "");
    rt.update(|s, _| s.select_server("acme"));
    look(&mut rt, now());
    open(&mut rt, DESIGN);
    open(&mut rt, DESIGN);
    assert_eq!(rt.state.draft.text, "Elsewhere");
}

#[test]
fn words_are_kept_once_they_rest_and_found_by_the_next_launch() {
    let dir = data_dir();
    let start = now();
    {
        let mut rt = runtime(signed_in_at(dir.clone()));
        look(&mut rt, start);
        typed(&mut rt, "Not sent yet");
        // Still being typed: nothing is written at every key.
        look(&mut rt, start + 1);
        assert!(rt.state.session.draft(GENERAL).is_none());
        assert!(
            rt.state.draft_wake(start + 1).is_some(),
            "the window asks to be woken to keep it"
        );
        // Rested: kept, with no switch and no quit. The process may now die as it likes.
        look(&mut rt, start + 1 + KEEP_AFTER_MICROS);
        assert_eq!(
            rt.state.session.draft(GENERAL).unwrap().body,
            "Not sent yet"
        );
        assert_eq!(rt.state.draft_wake(start + 2 + KEEP_AFTER_MICROS), None);
        // More typed and the window closed before it rested: kept as it closes.
        type_text(&mut rt, ", but soon");
        rt.update(|s, _| s.keep_draft());
    }
    let mut rt = runtime(signed_in_at(dir));
    look(&mut rt, now());
    assert_eq!(rt.state.draft.text, "Not sent yet, but soon");
    assert!(words(&rt).contains("Not sent yet, but soon"));
}

#[test]
fn an_emptied_field_and_signing_out_keep_nothing() {
    let dir = data_dir();
    let mut rt = runtime(signed_in_at(dir.clone()));
    look(&mut rt, now());
    typed(&mut rt, "Never mind");
    open(&mut rt, DESIGN);
    assert!(rt.state.session.draft(GENERAL).is_some());
    open(&mut rt, GENERAL);
    // Emptied by hand: the draft goes once the field rests.
    rt.update(|s, _| s.draft.replace_text(String::new()));
    let at = now();
    look(&mut rt, at);
    look(&mut rt, at + KEEP_AFTER_MICROS);
    assert!(rt.state.session.draft(GENERAL).is_none());

    // Signing out: the window empties its field without keeping it, and the core forgets
    // the rest with the account.
    typed(&mut rt, "For nobody else");
    open(&mut rt, DESIGN);
    typed(&mut rt, "Nor this");
    rt.update(|s, _| s.sign_out());
    settle(&mut rt);
    assert_eq!(rt.state.draft.text, "");
    look(&mut rt, now());
    assert_eq!(rt.state.draft.text, "");
}
