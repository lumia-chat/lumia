//! Setting up private messages against a real instance (#20; architecture 5.4, identity spec
//! 4.1, 4.3 and 4.7), through the shell's forms: a new account is set up silently and its
//! phrase confirmed by three words (one wrong check first); a second computer signing in
//! is told the account has its identity elsewhere and restores it from the phrase; a third
//! starts over with a new identity under the password; and, with `LUMIA_CLI`, an account
//! the CLI made with no identity is set up at its next sign-in here. Runs only with
//! `LUMIA_DEV_INSTANCE` set; without it it passes having checked nothing, and says so.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::path::Path;
use std::process::Command;
use std::time::{Duration, Instant};

use lumia_shell::app::{HomeView, Route, State};
use lumia_shell::session::identity::Setup;
use rune::TextEdit;

const PASSWORD: &str = "correct-horse-battery-staple";

/// Drains the window's updates until `done` holds of the state.
fn wait_until(state: &mut State, what: &str, done: impl Fn(&State) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(20);
    while !done(state) {
        state.drain();
        assert!(
            Instant::now() < deadline,
            "timed out waiting for {what}: setup {:?}, route {:?}, errors {:?} {:?} {:?}",
            state.session.identity.setup,
            state.route,
            state.sign_in.error,
            state.setup.phrase.error,
            state.setup.restore.error,
        );
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// A computer of its own, at the instance's sign-in card.
fn computer(dir: &Path, instance: &str) -> State {
    let mut state = State::new(dir.to_path_buf());
    state.sign_in.instance = TextEdit::new(instance);
    state.connect();
    assert!(state.session.is_open(), "{}", state.sign_in.error);
    wait_until(&mut state, "the trust card", |s| s.session.info.is_some());
    state
}

fn sign_in(state: &mut State, handle: &str, creating: bool) {
    state.sign_in.creating = creating;
    state.sign_in.handle.replace_text(handle);
    state.sign_in.password.replace_text(PASSWORD);
    state.submit();
    wait_until(state, "the sign-in", |s| s.route == Route::Shell);
}

/// The phrase on screen, read from the phrase screen.
fn shown_phrase(state: &mut State) -> Vec<String> {
    state.open_phrase();
    wait_until(state, "the phrase", |s| !s.setup.phrase.words.is_empty());
    state.setup.phrase.words.clone()
}

/// Step 2 with the quiz's words taken from `words`, each moved one place on when `wrong`.
fn check(state: &mut State, words: &[String], wrong: bool) {
    state.phrase_written();
    wait_until(state, "the quiz", |s| !s.setup.phrase.answers.is_empty());
    let quiz = state.setup.phrase.quiz.clone();
    assert_eq!(quiz.len(), 3);
    for (n, position) in quiz.iter().enumerate() {
        let at = usize::try_from(*position).unwrap() - 1;
        let word = if wrong {
            &words[(at + 1) % words.len()]
        } else {
            &words[at]
        };
        state.setup.phrase.answer_mut(n).replace_text(word);
    }
    state.confirm_phrase();
    wait_until(state, "the check", |s| !s.setup.phrase.busy);
}

#[test]
fn a_new_account_is_set_up_here_restored_on_another_computer_and_started_over_on_a_third() {
    let Ok(instance) = std::env::var("LUMIA_DEV_INSTANCE") else {
        eprintln!("skipped: no LUMIA_DEV_INSTANCE");
        return;
    };
    let dir = std::env::temp_dir().join(format!("lumia-shell-setup-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let handle = format!("setup{}", std::process::id());

    // Sign-up: the identity is made silently, and the phrase waits to be saved.
    let mut first = computer(&dir.join("first"), &instance);
    sign_in(&mut first, &handle, true);
    wait_until(&mut first, "the silent setup", |s| {
        s.session.identity.setup == Setup::Ready
    });
    assert!(first.session.has_identity() && first.session.private_ready());
    assert!(first.session.identity.phrase_kept);
    assert!(first.phrase_reminder(chrono::Utc::now().timestamp()));

    // The phrase, a wrong check that shows it again, then three right words.
    let words = shown_phrase(&mut first);
    assert_eq!(words.len(), 24);
    check(&mut first, &words, true);
    assert!(first.setup.phrase.again && !first.setup.phrase.checking);
    assert!(first.session.identity.phrase_kept);
    check(&mut first, &words, false);
    assert_eq!(first.route, Route::Shell);
    assert!(!first.session.identity.phrase_kept, "the reminders end");

    // Another computer: the account's identity is on the first.
    let mut second = computer(&dir.join("second"), &instance);
    sign_in(&mut second, &handle, false);
    wait_until(&mut second, "the setup check", |s| {
        s.session.identity.setup != Setup::Checking
    });
    assert_eq!(second.session.identity.setup, Setup::Elsewhere);
    assert!(!second.session.has_identity() && !second.session.private_ready());

    // Restored from the phrase.
    second.start_restore();
    second.setup.restore.words.replace_text(words.join(" "));
    second.restore_identity();
    wait_until(&mut second, "the restore", |s| {
        s.route == Route::Shell && s.session.identity.setup == Setup::Ready
    });
    assert!(second.session.private_ready());
    assert!(
        !second.session.identity.phrase_kept,
        "a restored phrase was written down already"
    );

    // A third computer with neither: it starts over.
    let mut third = computer(&dir.join("third"), &instance);
    sign_in(&mut third, &handle, false);
    wait_until(&mut third, "the setup check", |s| {
        s.session.identity.setup == Setup::Elsewhere
    });
    third.open_start_over();
    third
        .setup
        .start_over
        .password
        .replace_text("not the password");
    third.start_over();
    wait_until(&mut third, "the refusal", |s| !s.setup.start_over.busy);
    assert_eq!(
        third.setup.start_over.error,
        "That password was not accepted."
    );
    third.setup.start_over.password.replace_text(PASSWORD);
    third.start_over();
    wait_until(&mut third, "starting over", |s| s.route == Route::Shell);
    assert_eq!(third.home_view, HomeView::Devices);
    assert_eq!(third.session.identity.setup, Setup::Ready);
    assert!(third.session.private_ready() && third.session.identity.phrase_kept);

    // The second computer's identity is no longer the account's: at its next launch it is
    // not set up (its session went with the old identity, or the account is another's).
    drop(second);
    let mut again = computer(&dir.join("second"), &instance);
    wait_until(&mut again, "the setup check", |s| {
        s.session.identity.setup != Setup::Checking
    });
    assert_ne!(again.session.identity.setup, Setup::Ready);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn an_account_made_without_an_identity_is_set_up_at_its_next_sign_in() {
    let (Ok(instance), Ok(bin)) = (
        std::env::var("LUMIA_DEV_INSTANCE"),
        std::env::var("LUMIA_CLI"),
    ) else {
        eprintln!("skipped: no LUMIA_DEV_INSTANCE and LUMIA_CLI");
        return;
    };
    let dir = std::env::temp_dir().join(format!("lumia-shell-unbound-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let handle = format!("unbound{}", std::process::id());
    let cli = |args: &[&str]| {
        let out = Command::new(&bin)
            .arg("--data-dir")
            .arg(dir.join("cli"))
            .args(args)
            .output()
            .expect("the CLI runs");
        assert!(
            out.status.success(),
            "lumia-cli {args:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    };
    cli(&["instance", "add", &instance]);
    cli(&[
        "account",
        "create",
        "--handle",
        &handle,
        "--password",
        PASSWORD,
    ]);

    let mut state = computer(&dir.join("shell"), &instance);
    sign_in(&mut state, &handle, false);
    wait_until(&mut state, "the setup", |s| {
        s.session.identity.setup != Setup::Checking
    });
    assert_eq!(state.session.identity.setup, Setup::Ready);
    assert!(state.session.private_ready() && state.session.identity.phrase_kept);
    let _ = std::fs::remove_dir_all(&dir);
}

/// Linking another device from this one (identity spec 4.2, this computer approving): the
/// second computer of the account shows its request, the first reads what it would add,
/// approves it from Devices and gives the code, and the second claims it and is a device
/// of the identity, with private messages ready.
#[test]
fn a_computer_that_holds_the_identity_approves_another_from_devices() {
    let Ok(instance) = std::env::var("LUMIA_DEV_INSTANCE") else {
        eprintln!("skipped: no LUMIA_DEV_INSTANCE");
        return;
    };
    let dir = std::env::temp_dir().join(format!("lumia-shell-approve-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let handle = format!("approve{}", std::process::id());

    let mut first = computer(&dir.join("first"), &instance);
    sign_in(&mut first, &handle, true);
    wait_until(&mut first, "the silent setup", |s| {
        s.session.identity.setup == Setup::Ready
    });

    // The second computer of the account is told its private messages are elsewhere, and
    // starts linking.
    let mut second = computer(&dir.join("second"), &instance);
    sign_in(&mut second, &handle, false);
    wait_until(&mut second, "the identity elsewhere", |s| {
        s.session.identity.setup == Setup::Elsewhere
    });
    second.start_linking();
    second.start_link();
    wait_until(&mut second, "the request", |s| {
        !s.account.link.request.is_empty()
    });
    let request = second.account.link.request.clone();

    // Devices on the first: what is not a request is refused before anything is sent;
    // the request says what it adds; approving gives the code.
    first.go_home(HomeView::Devices);
    first.open_approve();
    first
        .account
        .devices
        .approve
        .request
        .replace_text("not a request");
    first.approve_device();
    assert!(
        !first.account.devices.approve.busy,
        "nothing is sent for text that is not one"
    );
    first.account.devices.approve.request.replace_text(&request);
    let adds = lumia_client::app::App::identity_link_request_summary(&request).unwrap();
    assert_eq!(adds.kind, "native");
    first.approve_device();
    wait_until(&mut first, "the code", |s| {
        !s.account.devices.approve.code.is_empty() || !s.account.devices.approve.error.is_empty()
    });
    assert_eq!(first.account.devices.approve.error, "");
    let code = first.account.devices.approve.code.clone();
    assert_eq!(code.len(), 8);
    wait_until(&mut first, "the roster with two devices", |s| {
        s.session
            .identity
            .roster
            .as_ref()
            .is_some_and(|r| r.devices.iter().filter(|d| !d.revoked).count() == 2)
    });

    // The second claims it and holds the identity.
    second.account.link.code.replace_text(&code);
    second.claim_link();
    wait_until(&mut second, "the claim", |s| {
        s.session.has_identity() || !s.account.link.error.is_empty()
    });
    assert_eq!(second.account.link.error, "");
    wait_until(&mut second, "private messages ready", |s| {
        s.session.private_ready()
    });

    first.close_approve();
    assert!(!first.account.devices.approve.open && first.account.devices.approve.code.is_empty());
    let _ = std::fs::remove_dir_all(&dir);
}
