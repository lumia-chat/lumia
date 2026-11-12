//! The session against a real instance: sign in, load, connect, send, and see the send move
//! from pending into the store. Runs only with `LUMIA_DEV_INSTANCE`, `LUMIA_DEV_HANDLE` and
//! `LUMIA_DEV_PASSWORD` set, as `scripts/smoke.sh` sets them; without them it passes having
//! checked nothing, and says so.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::time::{Duration, Instant};

use lumia_shell::app::{Route, State};
use lumia_shell::session::Connection;
use lumia_shell::ui;
use rune::{Env, Runtime, TextEdit};

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
        s.session.connection == Connection::Connected && s.session.channel().is_some()
    });
    wait(&mut state, "the channel's history", |s| {
        !s.session.entries.is_empty()
    });
    let before = state.session.entries.len();

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
        .entries
        .iter()
        .find(|e| e.id == id)
        .expect("the store holds it under the id the send minted");
    assert_eq!(stored.body, body);
    assert_eq!(stored.author_account_id, state.session.me());
    assert_eq!(state.session.entries.len(), before + 1);

    // A second launch over the same stores has the place, the channel, its messages and
    // the names at once, from what the core kept and from SQLite, before the instance has
    // been heard from: nothing is drained between opening and these assertions.
    let me = state.session.me().to_string();
    let name = state.session.self_name().to_string();
    let channel = state.session.channel().map(str::to_string);
    drop(state);
    let mut again = State::new(dir);
    assert_eq!(
        again.settings.instance.as_deref(),
        Some(instance.trim_end_matches('/'))
    );
    again.connect();
    assert!(again.session.signed_in, "the tokens were kept");
    again.resume();
    assert_eq!(again.route, Route::Shell);
    assert_eq!(again.session.channel().map(str::to_string), channel);
    assert!(
        again.session.entries.iter().any(|e| e.id == id),
        "read back from the local store"
    );
    assert_eq!(
        again.session.name_of(&me),
        name,
        "names come from what was kept"
    );
    assert!(
        again.session.people.len() >= 2,
        "the other member is known offline"
    );
    // And the instance still gets asked: the launch ends connected.
    wait(&mut again, "the second launch's gateway", |s| {
        s.session.connection == Connection::Connected
    });
}

/// History older than a device's store: the store of a device that signs in to a channel
/// with more than a replay's worth of events begins at the channel's snapshot, and the rest
/// comes from the instance a page at a time.
#[test]
fn older_history_pages_in_from_the_store_and_then_the_instance() {
    let (Ok(instance), Ok(handle), Ok(password)) = (
        std::env::var("LUMIA_DEV_INSTANCE"),
        std::env::var("LUMIA_DEV_HANDLE"),
        std::env::var("LUMIA_DEV_PASSWORD"),
    ) else {
        eprintln!("skipped: no LUMIA_DEV_INSTANCE, LUMIA_DEV_HANDLE and LUMIA_DEV_PASSWORD");
        return;
    };
    const SENT: usize = 1100;
    let sign_in = |name: &str| {
        let dir = std::env::temp_dir().join(format!("lumia-shell-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let mut state = State::new(dir);
        state.sign_in.instance = TextEdit::new(instance.as_str());
        state.connect();
        wait(&mut state, "the trust card", |s| s.session.info.is_some());
        state.sign_in.handle = TextEdit::new(handle.as_str());
        state.sign_in.password.replace_text(password.as_str());
        state.submit();
        wait(&mut state, "the gateway", |s| {
            s.route == Route::Shell && s.session.connection == Connection::Connected
        });
        state
    };
    // The channel the other test leaves alone.
    let design = |state: &State| {
        state
            .session
            .snapshot
            .as_ref()
            .and_then(|snap| snap.channels.iter().find(|c| c.name == "design"))
            .map(|c| c.channel_id.clone())
    };

    let mut writer = sign_in("writer");
    let Some(channel) = design(&writer) else {
        eprintln!("skipped: the account has no channel named design");
        return;
    };
    writer.select_channel(&channel);
    for i in 0..SENT {
        assert!(writer.session.send(&format!("history {i}"), 0), "send {i}");
        // In step with the gateway, so the queue never outruns its acknowledgements.
        if i % 50 == 49 {
            wait(&mut writer, "a batch to land", |s| {
                s.session.pending.is_empty()
            });
        }
    }
    wait(&mut writer, "the last batch to land", |s| {
        s.session.pending.is_empty()
    });
    drop(writer);

    // A device that was not there for any of it.
    let mut reader = sign_in("reader");
    reader.select_channel(&channel);
    wait(&mut reader, "the channel's newest messages", |s| {
        !s.session.entries.is_empty()
    });
    let first_page = reader.session.entries.len();
    assert!(
        first_page < SENT,
        "the store begins at the snapshot, not at the first message"
    );
    assert_eq!(
        reader.session.entries.last().map(|e| e.body.as_str()),
        Some(format!("history {}", SENT - 1).as_str())
    );

    let mut steps = 0;
    while !reader.session.at_beginning() {
        let before = reader.session.entries.len();
        reader.load_older();
        wait(&mut reader, "a page of older messages", |s| {
            !s.session.loading_older()
                && (s.session.entries.len() > before || s.session.at_beginning())
        });
        steps += 1;
        assert!(
            steps < 40,
            "paging does not end: {} entries",
            reader.session.entries.len()
        );
    }
    let bodies: Vec<&str> = reader
        .session
        .entries
        .iter()
        .map(|e| e.body.as_str())
        .collect();
    assert_eq!(bodies.len(), SENT, "every message, once");
    for (i, body) in bodies.iter().enumerate() {
        assert_eq!(*body, format!("history {i}"), "in order, oldest first");
    }
    assert!(steps >= 2, "it took more than one page: {steps}");
    println!("{SENT} messages: {first_page} at first, the rest in {steps} steps");
    drop(reader);

    // The same from the window: scrolling up through the list asks for each page as the
    // reader nears the oldest row on screen, until the channel's first message is there.
    let mut scroller = sign_in("scroller");
    scroller.select_channel(&channel);
    wait(&mut scroller, "the channel's newest messages", |s| {
        !s.session.entries.is_empty()
    });
    let mut rt = Runtime::new(scroller, ui::view, (1180.0, 760.0), Env::new(false, 2.0));
    rt.frame(0.0);
    let (x, y) = rt.find("messages").expect("the list is on screen");
    let deadline = Instant::now() + Duration::from_secs(60);
    while !rt.state.session.at_beginning() {
        assert!(
            Instant::now() < deadline,
            "scrolling does not reach the beginning: {} entries",
            rt.state.session.entries.len()
        );
        rt.wheel(x, y, 3000.0);
        rt.frame(1.0 / 60.0);
        rt.update(|s, _| {
            s.drain();
        });
        std::thread::sleep(Duration::from_millis(2));
    }
    assert_eq!(rt.state.session.entries.len(), SENT);
    assert_eq!(rt.state.session.entries[0].body, "history 0");
}
