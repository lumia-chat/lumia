//! The session against a real instance: sign in, load, connect, send, and see the send move
//! from pending into the store. Runs only with `LUMIA_DEV_INSTANCE`, `LUMIA_DEV_HANDLE` and
//! `LUMIA_DEV_PASSWORD` set, as `scripts/smoke.sh` sets them; without them it passes having
//! checked nothing, and says so.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::time::{Duration, Instant};

use lumia_shell::app::{Route, State};
use lumia_shell::session::Connection;
use rune::TextEdit;

fn wait(state: &mut State, what: &str, done: impl Fn(&State) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(15);
    while !done(state) {
        assert!(
            Instant::now() < deadline,
            "timed out waiting for {what}; error {:?}, sign-in {:?}",
            state.session.last_error,
            state.sign_in.error
        );
        state.drain();
        std::thread::sleep(Duration::from_millis(20));
    }
}

#[test]
fn a_message_sent_from_the_shell_goes_from_pending_into_the_store() {
    let (Ok(instance), Ok(handle), Ok(password)) = (
        std::env::var("LUMIA_DEV_INSTANCE"),
        std::env::var("LUMIA_DEV_HANDLE"),
        std::env::var("LUMIA_DEV_PASSWORD"),
    ) else {
        eprintln!("skipped: no LUMIA_DEV_INSTANCE, LUMIA_DEV_HANDLE and LUMIA_DEV_PASSWORD");
        return;
    };
    let dir = std::env::temp_dir().join(format!("lumia-shell-live-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let mut state = State::new(dir.clone());
    state.sign_in.instance = TextEdit::new(instance.as_str());
    state.connect();
    wait(&mut state, "the trust card", |s| s.session.info.is_some());
    state.sign_in.handle = TextEdit::new(handle.as_str());
    state.sign_in.password.replace_text(password.as_str());
    state.submit();
    wait(&mut state, "the shell", |s| s.route == Route::Shell);
    wait(&mut state, "the gateway and a channel", |s| {
        s.session.connection == Connection::Connected && s.session.channel.is_some()
    });
    wait(&mut state, "the channel's history", |s| {
        !s.session.messages.is_empty()
    });
    let before = state.session.messages.len();

    let body = format!("Sent from the shell's live test at {:?}", Instant::now());
    assert!(
        state
            .session
            .send(&body, chrono::Utc::now().timestamp_micros())
    );
    assert_eq!(
        state.session.pending.len(),
        1,
        "it is pending at once: {:?}",
        state.session.last_error
    );
    let id = state.session.pending[0].id.clone();
    wait(&mut state, "the send to land", |s| {
        s.session.pending.is_empty()
    });
    let stored = state
        .session
        .messages
        .iter()
        .find(|m| m.message_id == id)
        .expect("the store holds it under the id the send minted");
    assert_eq!(stored.body, body);
    assert_eq!(stored.author_account_id, state.session.me());
    assert_eq!(state.session.messages.len(), before + 1);

    // A second launch over the same stores is signed in and reads the channel from SQLite.
    drop(state);
    let mut again = State::new(dir);
    assert_eq!(
        again.settings.instance.as_deref(),
        Some(instance.trim_end_matches('/'))
    );
    again.connect();
    assert!(again.session.signed_in, "the tokens were kept");
    again.session.load();
    wait(&mut again, "the second launch's channel", |s| {
        s.session.channel.is_some()
    });
    assert!(
        again.session.messages.iter().any(|m| m.message_id == id),
        "read back from the local store"
    );
}
