//! Setting up private messages on this computer (#20) over a state no instance is behind:
//! sign-up's line and "Already use Lumia on another device?", what Home and Devices show
//! for each way setting up can come out, the reminder and "In a week", the phrase with its
//! warning before saving and its three-word check, restoring and starting over. What the
//! core would answer is applied with `State::receive`.

#![allow(clippy::expect_used, clippy::unwrap_used)]

mod common;

use std::collections::HashMap;

use common::*;
use lumia_shell::app::setup::phrase_file;
use lumia_shell::app::{HomeView, Route, State};
use lumia_shell::session::identity::{IdentityUpdate, Setup};
use lumia_shell::session::{Failure, Update};
use lumia_shell::ui;
use rune::{Env, Runtime};

const PHRASE: &str = "orbit cactus timber velvet harbor pencil meadow copper rescue glimpse fabric lunar anchor whisper salad ticket marble frozen clever sunny violin garden token mirror";

fn failure(code: &str) -> Failure {
    Failure {
        code: code.into(),
        message: format!("{code} from the instance"),
        details: HashMap::new(),
    }
}

/// A window tall enough that a whole card is on screen.
fn tall(state: State) -> Runtime<State> {
    let mut rt = Runtime::new(state, ui::view, (1180.0, 1600.0), Env::new(false, 2.0));
    settle(&mut rt);
    rt
}

fn receive(rt: &mut Runtime<State>, update: IdentityUpdate) {
    rt.update(|s, _| s.receive(&Update::Identity(update)));
    settle(rt);
}

fn set_up(rt: &mut Runtime<State>, setup: Setup, phrase_kept: bool) {
    receive(rt, IdentityUpdate::SetUp { setup, phrase_kept });
}

/// An instance open that lets identity proof sign in.
fn offering() -> State {
    let mut state = opened();
    let mut info = info();
    info.identity_proof_sign_in_enabled = true;
    state.receive(&Update::Info(Ok(info)));
    state
}

fn at(view: HomeView) -> Runtime<State> {
    let mut state = signed_in();
    state.go_home(view);
    tall(state)
}

// ---- Sign-up ----------------------------------------------------------------------------

#[test]
fn sign_up_says_private_messages_come_with_the_account_and_offers_link_and_restore() {
    let mut rt = tall(offering());
    press(&mut rt, "to-create");
    let shown = words(&rt);
    for expected in [
        "Private messages are set up on this computer with your account. You’ll be asked to save a recovery phrase afterwards.",
        "Already use Lumia on another device?",
        "Link from a device you use",
        "Restore with your recovery phrase",
    ] {
        assert!(shown.contains(expected), "{expected:?} in:\n{shown}");
    }
    assert!(
        !shown.contains("findable"),
        "the core has no flagship account yet"
    );
    press(&mut rt, "signup-link");
    assert_eq!(rt.state.route, Route::Link);
    press(&mut rt, "link-back");
    press(&mut rt, "signup-restore");
    assert_eq!(rt.state.route, Route::Restore);
    assert!(words(&rt).contains("Back to sign in"));
    press(&mut rt, "restore-back");
    assert_eq!(rt.state.route, Route::SignIn);
}

#[test]
fn an_instance_without_identity_proof_offers_neither_at_sign_up() {
    let mut rt = tall(opened());
    press(&mut rt, "to-create");
    assert!(rt.find("signup-link").is_none() && rt.find("signup-restore").is_none());
    assert!(words(&rt).contains("Private messages are set up on this computer"));
    press(&mut rt, "to-sign-in");
    assert!(rt.find("to-restore").is_none());
}

#[test]
fn sign_in_offers_restore_beside_linking() {
    let mut rt = tall(offering());
    assert!(rt.find("to-link").is_some());
    press(&mut rt, "to-restore");
    assert_eq!(rt.state.route, Route::Restore);
}

// ---- What Home and Devices show ---------------------------------------------------------

#[test]
fn signing_in_sets_up_and_the_panes_wait_for_it() {
    let mut rt = at(HomeView::NewMessage);
    assert_eq!(rt.state.session.identity.setup, Setup::Checking);
    assert!(words(&rt).contains("Setting up private messages…"));
    set_up(
        &mut rt,
        Setup::Failed("transport: connection refused".into()),
        false,
    );
    let shown = words(&rt);
    assert!(
        shown.contains("Private messages aren’t set up on this computer")
            && shown.contains("transport: connection refused"),
        "{shown}"
    );
    press(&mut rt, "setup-retry");
    assert_eq!(rt.state.session.identity.setup, Setup::Checking);
}

#[test]
fn an_account_with_its_identity_elsewhere_gets_link_restore_and_start_over() {
    let mut rt = at(HomeView::Requests);
    set_up(&mut rt, Setup::Elsewhere, false);
    let shown = words(&rt);
    for expected in [
        "Private messages need this computer",
        "Your account has private messages on another device. Bring this computer in to read and send them here.",
        "Link from a device you use",
        "Restore with your recovery phrase",
        "Lost every device and the phrase? Start over",
    ] {
        assert!(shown.contains(expected), "{expected:?} in:\n{shown}");
    }
    press(&mut rt, "setup-restore");
    assert_eq!(rt.state.route, Route::Restore);
    assert!(
        words(&rt).contains("Not now"),
        "signed in, back to the shell"
    );
    press(&mut rt, "restore-back");
    assert_eq!(rt.state.route, Route::Shell);

    press(&mut rt, "setup-start-over");
    assert_eq!(rt.state.route, Route::StartOver);
    press(&mut rt, "start-over-keep");
    assert_eq!(rt.state.route, Route::Shell);
}

#[test]
fn binding_that_needs_the_password_asks_for_it() {
    let mut rt = at(HomeView::Devices);
    set_up(&mut rt, Setup::NeedsReauth, false);
    assert!(words(&rt).contains("Set up private messages on this computer"));
    assert!(rt.find("devices-list").is_none());
    press(&mut rt, "setup-password");
    type_text(&mut rt, "hunter2hunter2");
    press(&mut rt, "setup-confirm");
    assert!(rt.state.setup.busy);
    assert_eq!(rt.state.session.identity.setup, Setup::Checking);

    receive(
        &mut rt,
        IdentityUpdate::SetupRefused(failure("UNAUTHENTICATED")),
    );
    assert!(!rt.state.setup.busy);
    assert!(words(&rt).contains("That password was not accepted."));
    assert_eq!(rt.state.session.identity.setup, Setup::NeedsReauth);

    set_up(&mut rt, Setup::Ready, true);
    assert!(rt.state.setup.password.text.is_empty() && rt.state.setup.error.is_empty());
    assert!(rt.find("setup-password").is_none());
}

// ---- The reminders ----------------------------------------------------------------------

#[test]
fn the_reminder_shows_until_put_away_for_a_week_and_devices_keeps_its_card() {
    let mut rt = at(HomeView::Conversations);
    assert!(rt.find("phrase-reminder").is_none());
    set_up(&mut rt, Setup::Ready, true);
    let shown = words(&rt);
    assert!(
        shown.contains("Save your recovery phrase")
            && shown.contains(
                "It’s the only way back to your private messages if you lose this computer."
            ),
        "{shown}"
    );

    press(&mut rt, "reminder-later");
    assert!(rt.find("phrase-reminder").is_none());
    let until = rt.state.settings.phrase_reminder_until.unwrap();
    let week = until - chrono::Utc::now().timestamp();
    assert!((7 * 86_400 - 60..=7 * 86_400).contains(&week), "{week}");
    // Remembered on this computer.
    let kept = lumia_shell::settings::Settings::load(rt.state.data_dir());
    assert_eq!(kept.phrase_reminder_until, Some(until));
    // A week on, it is back.
    assert!(rt.state.phrase_reminder(until));

    press(&mut rt, "tile-devices");
    let shown = words(&rt);
    assert!(
        shown.contains("Recovery phrase not saved")
            && shown.contains("If you lose this computer before you save it, your private messages can’t be recovered."),
        "{shown}"
    );
    press(&mut rt, "save-phrase-now");
    assert_eq!(rt.state.route, Route::Phrase);
}

// ---- The phrase -------------------------------------------------------------------------

fn at_phrase() -> Runtime<State> {
    let mut rt = at(HomeView::Devices);
    set_up(&mut rt, Setup::Ready, true);
    press(&mut rt, "save-phrase-now");
    receive(&mut rt, IdentityUpdate::Phrase(Ok(Some(PHRASE.into()))));
    rt
}

#[test]
fn the_phrase_is_numbered_in_two_columns_with_no_copy() {
    let rt = at_phrase();
    let shown = words(&rt);
    assert!(shown.contains(
        "Whoever has these 24 words has your identity and your private messages. Lumia can’t recover them for you."
    ));
    for (n, word) in PHRASE.split(' ').enumerate() {
        assert!(shown.contains(&format!("{}, {word}", n + 1)), "{word}");
    }
    // 13 starts the second column, level with 1.
    let (x1, y1) = rt.find("phrase-word-1").unwrap();
    let (x13, y13) = rt.find("phrase-word-13").unwrap();
    assert!(x13 > x1 && (y13 - y1).abs() < 1.0);
    assert!(!shown.contains("Copy"), "no copy button:\n{shown}");
    assert!(
        rt.find("print-phrase").is_none(),
        "Rune has no print dialog"
    );
}

#[test]
fn saving_the_phrase_warns_first_and_writes_it_readable_by_its_owner() {
    let mut rt = at_phrase();
    press(&mut rt, "save-phrase");
    let shown = words(&rt);
    for expected in [
        "Save your phrase as a file?",
        "This file is your identity: anyone who opens it can read your private messages.",
        "A folder that syncs to iCloud, Google Drive or OneDrive puts a copy in that cloud. Paper, or a drive you keep offline, is safer.",
        "Save anyway",
        "Write it down instead",
    ] {
        assert!(shown.contains(expected), "{expected:?} in:\n{shown}");
    }
    press(&mut rt, "write-instead");
    assert!(rt.find("save-warning").is_none() && rt.find("save-phrase").is_some());

    // Where "Save anyway"'s dialog would have put it.
    let path = data_dir().with_extension("txt");
    rt.update(|s, _| s.write_phrase(&path));
    settle(&mut rt);
    let written = std::fs::read_to_string(&path).unwrap();
    assert_eq!(written, phrase_file(&rt.state.setup.phrase.words));
    assert!(written.contains(" 1. orbit\n") && written.contains("24. mirror\n"));
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(&path).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
    }
    assert!(words(&rt).contains(&format!("Saved to {}.", path.display())));
    let _ = std::fs::remove_file(path);
}

#[test]
fn the_check_completes_words_and_a_wrong_one_shows_the_phrase_again() {
    let mut rt = at_phrase();
    press(&mut rt, "phrase-written");
    assert!(rt.state.setup.phrase.checking);
    receive(&mut rt, IdentityUpdate::Quiz(Ok(vec![4, 11, 19])));
    let shown = words(&rt);
    for expected in [
        "Check your phrase",
        "Step 2 of 2",
        "Type these three words from what you wrote down, to make sure it’s right.",
        "Word 4",
        "Word 11",
        "Word 19",
    ] {
        assert!(shown.contains(expected), "{expected:?} in:\n{shown}");
    }
    assert!(rt.find("phrase-word-1").is_none(), "the words are put away");

    // The wordlist completes a word as it is typed, and a whole word needs none.
    press(&mut rt, "phrase-answer-1");
    type_text(&mut rt, "fa");
    assert!(rt.find("completion-1-fabric").is_some());
    assert!(rt.find("completion-1-face").is_some());
    press(&mut rt, "completion-1-fabric");
    assert_eq!(rt.state.setup.phrase.answers[1].text, "fabric");
    assert!(rt.find("completions-1").is_none());

    assert!(!rt.state.setup.phrase.busy);
    press(&mut rt, "phrase-confirm");
    assert!(!rt.state.setup.phrase.busy, "not until all three are in");
    press(&mut rt, "phrase-answer-0");
    type_text(&mut rt, "velvet");
    press(&mut rt, "phrase-answer-2");
    type_text(&mut rt, "salad");
    press(&mut rt, "phrase-confirm");
    assert!(rt.state.setup.phrase.busy);

    receive(&mut rt, IdentityUpdate::Confirmed(Ok(false)));
    assert!(!rt.state.setup.phrase.checking);
    assert!(rt.find("phrase-word-1").is_some());
    assert!(words(&rt).contains("One of those words wasn’t right."));
    assert!(rt.state.session.identity.phrase_kept);
}

#[test]
fn three_right_words_end_the_reminders() {
    let mut rt = at_phrase();
    press(&mut rt, "phrase-written");
    receive(&mut rt, IdentityUpdate::Quiz(Ok(vec![1, 2, 3])));
    press(&mut rt, "phrase-show-again");
    assert!(rt.find("phrase-word-1").is_some());
    press(&mut rt, "phrase-written");
    receive(&mut rt, IdentityUpdate::Quiz(Ok(vec![1, 2, 3])));
    receive(&mut rt, IdentityUpdate::Confirmed(Ok(true)));
    assert_eq!(rt.state.route, Route::Shell);
    assert!(!rt.state.session.identity.phrase_kept);
    assert!(
        rt.state.setup.phrase.words.is_empty(),
        "the words are forgotten"
    );
    assert!(rt.find("phrase-not-saved").is_none());
    rt.update(|s, _| s.go_home(HomeView::Conversations));
    settle(&mut rt);
    assert!(rt.find("phrase-reminder").is_none());
}

#[test]
fn a_phrase_already_confirmed_closes_the_screen() {
    let mut rt = at(HomeView::Devices);
    set_up(&mut rt, Setup::Ready, true);
    press(&mut rt, "save-phrase-now");
    receive(&mut rt, IdentityUpdate::Phrase(Ok(None)));
    assert_eq!(rt.state.route, Route::Shell);
    assert!(!rt.state.session.identity.phrase_kept);
}

// ---- Restoring --------------------------------------------------------------------------

#[test]
fn restore_turns_on_at_24_words() {
    let mut rt = tall(offering());
    press(&mut rt, "to-restore");
    let shown = words(&rt);
    for expected in [
        "With your recovery phrase",
        "0 of 24",
        "Restoring makes this computer your identity’s only device.",
        "Messages from before can’t be read here unless history backup was on.",
        "Restore turns on once all 24 words are in.",
    ] {
        assert!(shown.contains(expected), "{expected:?} in:\n{shown}");
    }
    press(&mut rt, "restore-words");
    let twelve: Vec<&str> = PHRASE.split(' ').take(12).collect();
    type_text(&mut rt, &twelve.join(" "));
    assert!(words(&rt).contains("12 of 24"));
    press(&mut rt, "restore");
    assert!(!rt.state.setup.restore.busy, "not at 12");
    let rest: Vec<&str> = PHRASE.split(' ').skip(12).collect();
    press(&mut rt, "restore-words");
    rt.key(rune::KeyEvent {
        key: rune::Key::End,
        mods: rune::Mods::default(),
    });
    type_text(&mut rt, &format!(" {}", rest.join(" ")));
    assert!(words(&rt).contains("24 of 24"));
    press(&mut rt, "restore");
    assert!(rt.state.setup.restore.busy);

    receive(
        &mut rt,
        IdentityUpdate::Restored(Err(Failure {
            message: "that phrase is not this identity's root".into(),
            ..Failure::default()
        })),
    );
    assert!(!rt.state.setup.restore.busy);
    assert!(words(&rt).contains("that phrase is not this identity's root"));
    assert_eq!(rt.state.route, Route::Restore);

    press(&mut rt, "restore");
    receive(
        &mut rt,
        IdentityUpdate::Restored(Ok(lumia_client::app::RosterSummary {
            identity_id: "9f2c".into(),
            version: 2,
            devices: Vec::new(),
        })),
    );
    assert_eq!(rt.state.route, Route::Shell);
    assert!(rt.state.session.signed_in);
}

// ---- Starting over ----------------------------------------------------------------------

#[test]
fn starting_over_says_what_it_does_and_takes_the_password() {
    let mut rt = at(HomeView::Conversations);
    set_up(&mut rt, Setup::Elsewhere, false);
    press(&mut rt, "setup-start-over");
    let shown = words(&rt);
    for expected in [
        "With a new identity",
        "Without a device or your recovery phrase, your old identity can’t come back.",
        "Your account, servers and channels stay as they are.",
        "Private messages from before stay unreadable.",
        // The sample account has no personal conversation.
        "Nobody has talked with you privately yet, so nobody will see a key change.",
        "You’ll get a new recovery phrase to save.",
        "Only do this if the old phrase is truly gone.",
        "Confirm with your password",
        "Keep looking for the phrase",
    ] {
        assert!(shown.contains(expected), "{expected:?} in:\n{shown}");
    }
    press(&mut rt, "start-over-confirm");
    assert!(!rt.state.setup.start_over.busy, "not without the password");
    press(&mut rt, "start-over-password");
    type_text(&mut rt, "hunter2hunter2");
    press(&mut rt, "start-over-confirm");
    assert!(rt.state.setup.start_over.busy);

    receive(
        &mut rt,
        IdentityUpdate::StartedOver(Err(failure("UNAUTHENTICATED"))),
    );
    assert!(words(&rt).contains("That password was not accepted."));
    assert_eq!(rt.state.route, Route::StartOver);

    press(&mut rt, "start-over-confirm");
    receive(&mut rt, IdentityUpdate::StartedOver(Ok(())));
    assert_eq!(rt.state.route, Route::Shell);
    assert_eq!(rt.state.home_view, HomeView::Devices);
    assert_eq!(rt.state.session.identity.setup, Setup::Ready);
    assert!(rt.state.session.identity.phrase_kept);
    assert!(
        rt.find("phrase-not-saved").is_some(),
        "the new phrase to save"
    );
}
