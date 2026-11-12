//! Who is typing, against a real instance (protocol 6.7): two accounts in one channel,
//! each a shell of its own. What one types into its composer is named under the other's,
//! a message ends it at once, and silence ends it when its time runs out. Runs only with
//! `LUMIA_DEV_INSTANCE` set; without it it passes having checked nothing, and says so.
//! It makes its own accounts, so it needs an instance with open registration.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::path::Path;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use lumia_shell::app::{HomeView, Place, Route, State};
use lumia_shell::session::Connection;
use rune::TextEdit;

const PASSWORD: &str = "correct-horse-battery-staple";

/// What the window's poll does to a state between frames, without a window.
fn turn(state: &mut State) {
    state.drain();
    state.tick(chrono::Utc::now().timestamp_micros());
}

/// Turns both shells until `done`, or fails after `within`.
fn wait(
    a: &mut State,
    b: &mut State,
    within: Duration,
    what: &str,
    done: impl Fn(&State, &State) -> bool,
) {
    let deadline = Instant::now() + within;
    while !done(a, b) {
        assert!(
            Instant::now() < deadline,
            "timed out waiting for {what}; errors {:?} and {:?}, typing {:?} and {:?}",
            a.session.last_error,
            b.session.last_error,
            a.session.typing_here(),
            b.session.typing_here(),
        );
        turn(a);
        turn(b);
        std::thread::sleep(Duration::from_millis(20));
    }
}

fn wait_one(state: &mut State, what: &str, done: impl Fn(&State) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(15);
    while !done(state) {
        assert!(
            Instant::now() < deadline,
            "timed out waiting for {what}; session error {:?}, sign-in {:?}",
            state.session.last_error,
            state.sign_in.error,
        );
        turn(state);
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// A state over `dir`, signed in to a new account on `instance`.
fn new_account(instance: &str, dir: &Path, handle: &str) -> State {
    let mut state = State::new(dir.to_path_buf());
    state.sign_in.instance = TextEdit::new(instance);
    state.connect();
    wait_one(&mut state, "the trust card", |s| s.session.info.is_some());
    state.sign_in.creating = true;
    state.sign_in.handle = TextEdit::new(handle);
    state.sign_in.password.replace_text(PASSWORD);
    state.submit();
    wait_one(&mut state, "the shell", |s| s.route == Route::Shell);
    wait_one(&mut state, "the first snapshot", |s| {
        s.session.snapshot.is_some()
    });
    state
}

#[test]
fn what_one_types_is_named_under_the_others_composer() {
    let Ok(instance) = std::env::var("LUMIA_DEV_INSTANCE") else {
        eprintln!("skipped: no LUMIA_DEV_INSTANCE");
        return;
    };
    let dir = std::env::temp_dir().join(format!("lumia-shell-typing-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis()
        % 100_000_000;
    let (ada, grace) = (format!("typada{stamp}"), format!("typgrace{stamp}"));

    // ---- Two accounts in one channel ---------------------------------------------------
    let mut owner = new_account(&instance, &dir.join("owner"), &ada);
    owner.go_home(HomeView::AddServer);
    owner
        .server_forms
        .add
        .name
        .replace_text(format!("Shell typing {stamp}"));
    owner.create_server();
    wait_one(&mut owner, "the new server on screen", |s| {
        matches!(s.place, Place::Server(_))
    });
    owner.open_new_channel();
    owner.server_forms.channel.name.replace_text("general");
    owner.create_channel();
    wait_one(&mut owner, "the new channel on screen", |s| {
        s.session.channel().is_some()
    });
    let channel_id = owner.session.channel().unwrap().to_string();
    wait_one(&mut owner, "the gateway", |s| {
        s.session.connection == Connection::Connected
    });
    // A message, so each shell can tell when its subscription to the channel is live.
    let first = format!("Before anyone types, {stamp}");
    assert!(
        owner
            .session
            .send(&first, chrono::Utc::now().timestamp_micros())
    );
    wait_one(&mut owner, "the first message to land", |s| {
        s.session.entries.iter().any(|e| e.body == first)
    });
    owner.open_invites();
    owner.make_invite();
    wait_one(&mut owner, "the invite link", |s| {
        s.server_forms.invite.made.is_some()
    });
    let link = owner.server_forms.invite.made.clone().unwrap().url;
    owner.close_popover();

    let mut guest = new_account(&instance, &dir.join("guest"), &grace);
    guest.go_home(HomeView::AddServer);
    guest.server_forms.add.invite.replace_text(&link);
    guest.look_up_invite();
    wait_one(&mut guest, "the invite's preview", |s| {
        s.server_forms.add.preview.is_some()
    });
    guest.join_invite();
    wait_one(&mut guest, "the joined channel on screen", |s| {
        s.session.channel() == Some(channel_id.as_str())
    });
    wait_one(&mut guest, "the channel's history", |s| {
        s.session.entries.iter().any(|e| e.body == first)
    });
    // The owner knows the guest by name once the member list has them.
    let guest_id = guest.session.me().to_string();
    wait(
        &mut owner,
        &mut guest,
        Duration::from_secs(15),
        "the owner to know the guest",
        |owner, _| owner.session.people.contains_key(&guest_id),
    );
    assert!(owner.session.typing_here().is_empty() && guest.session.typing_here().is_empty());

    // ---- The guest types: the owner sees who, and the guest sees nothing of itself -----
    guest.draft.replace_text("Hel");
    wait(
        &mut owner,
        &mut guest,
        Duration::from_secs(10),
        "the guest's typing at the owner",
        |owner, _| owner.session.typing_here() == [grace.clone()],
    );
    assert!(guest.session.typing_here().is_empty());
    let ends = owner.session.typing_ends().unwrap() - chrono::Utc::now().timestamp_micros();
    assert!(
        (5_000_000..=8_500_000).contains(&ends),
        "about eight seconds are left: {ends}"
    );

    // ---- Its message ends that at once --------------------------------------------------
    let hello = format!("Hello, {stamp}");
    assert!(
        guest
            .session
            .send(&hello, chrono::Utc::now().timestamp_micros())
    );
    guest.draft.clear();
    wait(
        &mut owner,
        &mut guest,
        Duration::from_secs(10),
        "the guest's message at the owner",
        |owner, _| owner.session.entries.iter().any(|e| e.body == hello),
    );
    assert!(
        owner.session.typing_here().is_empty(),
        "the message came, so its author is not typing it"
    );

    // ---- The owner types and then says nothing: it runs out on its own ------------------
    owner.draft.replace_text("Hi");
    wait(
        &mut owner,
        &mut guest,
        Duration::from_secs(10),
        "the owner's typing at the guest",
        |_, guest| guest.session.typing_here() == [ada.clone()],
    );
    let shown = Instant::now();
    wait(
        &mut owner,
        &mut guest,
        Duration::from_secs(12),
        "the owner's typing to run out",
        |_, guest| guest.session.typing_here().is_empty(),
    );
    let lasted = shown.elapsed();
    assert!(
        lasted > Duration::from_secs(5) && lasted < Duration::from_secs(10),
        "shown for about eight seconds: {lasted:?}"
    );
}
