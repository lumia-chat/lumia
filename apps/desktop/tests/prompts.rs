//! Linking by a prompt (identity spec 4.2.1) over a state no instance is behind: this
//! computer as the new device (waiting, the digits, declined, expired, the other way, and
//! linked), and as the one that holds the identity (the prompt over the window at each
//! stage, its words, its calls and how it ends), with when each asks the instance. What the
//! instance would answer is applied with `State::receive`; `tests/live_setup.rs` links a
//! second computer this way against a real one.

#![allow(clippy::expect_used, clippy::unwrap_used)]

mod common;

use std::collections::HashMap;

use common::*;
use lumia_client::app::{AppEvent, LinkProgress, LinkPrompt, LinkRequestSummary, RosterSummary};
use lumia_shell::app::account::Prompted;
use lumia_shell::app::prompts::{self, ENDED_FOR, POLL, STOPPED};
use lumia_shell::app::{HomeView, Route, State};
use lumia_shell::session::identity::{IdentityUpdate, Setup};
use lumia_shell::session::{Failure, Update};
use lumia_shell::ui;
use rune::{Env, Runtime};

fn identity(update: IdentityUpdate) -> Update {
    Update::Identity(update)
}

fn receive(rt: &mut Runtime<State>, update: IdentityUpdate) {
    rt.update(|s, _| s.receive(&Update::Identity(update)));
    settle(rt);
}

fn progress(stage: &str, digits: &str) -> LinkProgress {
    LinkProgress {
        stage: stage.into(),
        digits: digits.into(),
    }
}

fn failure(code: &str, reason: Option<&str>, message: &str) -> Failure {
    let mut details = HashMap::new();
    if let Some(reason) = reason {
        details.insert("reason".to_string(), reason.to_string());
    }
    Failure {
        code: code.into(),
        message: message.into(),
        details,
    }
}

fn shows(rt: &Runtime<State>, expected: &[&str]) {
    let shown = words(rt);
    for expected in expected {
        assert!(shown.contains(expected), "{expected:?} in:\n{shown}");
    }
}

/// Signed in at Devices, the account's private messages on another device.
fn elsewhere() -> Runtime<State> {
    let mut state = signed_in();
    state.go_home(HomeView::Devices);
    state.receive(&identity(IdentityUpdate::SetUp {
        setup: Setup::Elsewhere,
        phrase_kept: false,
    }));
    let mut rt = Runtime::new(state, ui::view, (1180.0, 900.0), Env::new(false, 2.0));
    settle(&mut rt);
    rt
}

/// "Link from a device you use", and the instance opened the session.
fn waiting() -> Runtime<State> {
    let mut rt = elsewhere();
    press(&mut rt, "setup-link");
    receive(
        &mut rt,
        IdentityUpdate::LinkStarted(Ok(progress("waiting", ""))),
    );
    rt
}

// ---- This computer as the new device ----------------------------------------------------

#[test]
fn a_computer_signed_in_links_by_a_prompt_and_compares_the_digits() {
    let mut rt = elsewhere();
    press(&mut rt, "setup-link");
    assert_eq!(rt.state.route, Route::Link);
    assert_eq!(
        rt.state.account.link.prompted,
        Prompted::Starting,
        "the session is asked for at once"
    );
    receive(
        &mut rt,
        IdentityUpdate::LinkStarted(Ok(progress("waiting", ""))),
    );
    assert_eq!(rt.state.account.link.prompted, Prompted::Waiting);
    shows(
        &rt,
        &[
            "Link this computer",
            "Open Lumia on a device that has your private messages. It will ask about this computer.",
            "Waiting for your other device",
            "Cancel",
            "Link another way",
        ],
    );
    assert!(
        rt.find("start-link").is_none() && rt.find("link-request").is_none(),
        "nothing to copy"
    );

    receive(
        &mut rt,
        IdentityUpdate::LinkProgressed(Ok(progress("compare", "482 913"))),
    );
    assert_eq!(
        rt.state.account.link.prompted,
        Prompted::Compare("482 913".into())
    );
    shows(
        &rt,
        &[
            "482 913",
            "Number 482 913",
            "Check that your other device shows the same number, then approve it there.",
            "Cancel",
        ],
    );
    assert!(
        rt.find("link-another-way").is_none() && rt.find("waiting").is_none(),
        "once another device answered, there is only the number"
    );

    // Approved there: claimed here, signed in by it, and on to the shell as a claimed code.
    let roster = RosterSummary {
        identity_id: "9f2c".into(),
        version: 2,
        devices: Vec::new(),
    };
    receive(&mut rt, IdentityUpdate::Linked(Ok(roster)));
    assert_eq!(rt.state.route, Route::Shell);
    assert_eq!(rt.state.home_view, HomeView::Devices);
    assert_eq!(rt.state.account.link.prompted, Prompted::Idle);
    assert!(rt.state.session.identity.roster.is_some());
}

#[test]
fn the_session_is_asked_about_every_two_seconds_and_on_account_events_one_at_a_time() {
    let mut rt = waiting();
    let at = rt.state.account.link.asked_at;
    assert!(!rt.state.account.link.asking);
    assert!(!rt.state.tick(at + POLL - 1) && !rt.state.account.link.asking);
    rt.state.tick(at + POLL);
    assert!(rt.state.account.link.asking, "asked after two seconds");
    assert_eq!(rt.state.account.link.asked_at, at + POLL);
    assert_eq!(rt.state.prompt_wake(at + POLL), Some(POLL));

    // An account event while the question is on its way asks once more when it lands.
    rt.update(|s, _| {
        s.receive(&Update::Gateway(AppEvent::AccountApplied { count: 1 }));
    });
    assert!(rt.state.account.link.again);
    receive(
        &mut rt,
        IdentityUpdate::LinkProgressed(Ok(progress("waiting", ""))),
    );
    assert!(rt.state.account.link.asking && !rt.state.account.link.again);
    receive(
        &mut rt,
        IdentityUpdate::LinkProgressed(Ok(progress("waiting", ""))),
    );
    assert!(!rt.state.account.link.asking);

    // A failure is said, and asked again.
    receive(
        &mut rt,
        IdentityUpdate::LinkProgressed(Err(failure("", None, "connection refused"))),
    );
    shows(&rt, &["connection refused"]);
    assert_eq!(rt.state.account.link.prompted, Prompted::Waiting);
    receive(
        &mut rt,
        IdentityUpdate::LinkProgressed(Ok(progress("waiting", ""))),
    );
    assert!(!words(&rt).contains("connection refused"));

    // Ended: no more asking, and nothing to wake for.
    receive(
        &mut rt,
        IdentityUpdate::LinkProgressed(Ok(progress("expired", ""))),
    );
    let later = rt.state.account.link.asked_at + 10 * POLL;
    rt.state.tick(later);
    assert!(!rt.state.account.link.asking);
    assert_eq!(rt.state.prompt_wake(later), None);
}

#[test]
fn declined_and_expired_are_said_and_tried_again_or_cancelled() {
    let mut rt = waiting();
    receive(
        &mut rt,
        IdentityUpdate::LinkProgressed(Ok(progress("declined", ""))),
    );
    shows(
        &rt,
        &[
            "It wasn’t approved on your other device.",
            "Try again",
            "Cancel",
        ],
    );
    assert!(rt.find("link-another-way").is_none() && rt.find("waiting").is_none());
    press(&mut rt, "link-again");
    assert_eq!(rt.state.account.link.prompted, Prompted::Starting);
    receive(
        &mut rt,
        IdentityUpdate::LinkStarted(Ok(progress("waiting", ""))),
    );
    receive(
        &mut rt,
        IdentityUpdate::LinkProgressed(Ok(progress("expired", ""))),
    );
    shows(&rt, &["That took too long.", "Try again"]);

    // The core no longer holding the session is an end too.
    press(&mut rt, "link-again");
    receive(
        &mut rt,
        IdentityUpdate::LinkStarted(Ok(progress("waiting", ""))),
    );
    receive(
        &mut rt,
        IdentityUpdate::LinkProgressed(Err(failure("", None, "no link in progress"))),
    );
    assert_eq!(rt.state.account.link.prompted, Prompted::Expired);

    press(&mut rt, "link-cancel");
    assert_eq!(rt.state.route, Route::Shell);
    assert_eq!(rt.state.account.link.prompted, Prompted::Idle);
}

#[test]
fn a_session_that_cannot_be_opened_says_why() {
    let mut rt = elsewhere();
    press(&mut rt, "setup-link");
    receive(
        &mut rt,
        IdentityUpdate::LinkStarted(Err(failure("RATE_LIMITED", None, "rate limited"))),
    );
    assert_eq!(rt.state.account.link.prompted, Prompted::Idle);
    shows(
        &rt,
        &[
            "Your other devices couldn’t be asked.",
            "Too many devices are waiting to be linked. Try again in a few minutes.",
            "Try again",
        ],
    );
}

#[test]
fn cancelling_while_it_opens_forgets_the_late_answer() {
    let mut rt = elsewhere();
    press(&mut rt, "setup-link");
    press(&mut rt, "link-cancel");
    assert_eq!(rt.state.route, Route::Shell);
    receive(
        &mut rt,
        IdentityUpdate::LinkStarted(Ok(progress("waiting", ""))),
    );
    assert_eq!(rt.state.route, Route::Shell);
    assert_eq!(rt.state.account.link.prompted, Prompted::Idle);
}

#[test]
fn link_another_way_is_the_copied_request_and_the_typed_code() {
    let mut rt = waiting();
    press(&mut rt, "link-another-way");
    assert!(rt.state.account.link.another_way);
    assert_eq!(rt.state.account.link.prompted, Prompted::Idle);
    shows(
        &rt,
        &[
            "This computer gets its own device key.",
            "Start linking",
            "Not now",
        ],
    );
    press(&mut rt, "start-link");
    receive(
        &mut rt,
        IdentityUpdate::LinkRequested(Ok("UkVRVUVTVA".into())),
    );
    shows(&rt, &["UkVRVUVTVA", "Copy request", "Claim and sign in"]);
    // A late answer about the session given up changes nothing here.
    receive(
        &mut rt,
        IdentityUpdate::LinkProgressed(Ok(progress("compare", "111 111"))),
    );
    assert!(rt.find("link-digits").is_none());
    press(&mut rt, "link-back");
    assert_eq!(rt.state.route, Route::Shell);
}

// ---- This computer, which holds the identity: the prompt --------------------------------

const SESSION: &str = "0196f1c2-5555-7000-8000-000000000005";

fn asking(id: &str, kind: &str, label: &str, origin: &str, stage: &str) -> LinkPrompt {
    LinkPrompt {
        session_id: id.into(),
        request: LinkRequestSummary {
            device_id: format!("device-{id}"),
            kind: kind.into(),
            label: label.into(),
            web_origin: origin.into(),
        },
        stage: stage.into(),
        digits: if stage == "compare" {
            "482 913".into()
        } else {
            String::new()
        },
    }
}

fn phone(stage: &str) -> LinkPrompt {
    asking(SESSION, "native", "Pixel 9", "", stage)
}

/// Signed in at Home with `listed` prompting.
fn prompted(listed: Vec<LinkPrompt>) -> Runtime<State> {
    let mut rt = runtime(signed_in());
    receive(&mut rt, IdentityUpdate::Prompts(Ok(listed)));
    rt
}

#[test]
fn the_prompt_says_who_asks_at_each_stage() {
    let browser = asking(SESSION, "web", "", "https://chat.example.com", "new");
    assert_eq!(
        prompts::title(&browser),
        "A browser on https://chat.example.com wants your private messages"
    );
    assert_eq!(
        prompts::linked(&browser),
        "The browser on https://chat.example.com is linked."
    );
    let unnamed = asking(SESSION, "native", " ", "", "waiting");
    assert_eq!(prompts::title(&unnamed), "Waiting for the new device…");
    assert_eq!(prompts::linked(&unnamed), "The new device is linked.");
    assert_eq!(
        prompts::title(&asking(SESSION, "native", "", "", "new")),
        "A new device wants your private messages"
    );
    assert_eq!(
        prompts::title(&phone("new")),
        "Pixel 9 wants your private messages"
    );
    assert_eq!(prompts::title(&phone("waiting")), "Waiting for Pixel 9…");
    assert_eq!(
        prompts::title(&phone("compare")),
        "Does Pixel 9 show this number?"
    );
    assert_eq!(prompts::linked(&phone("compare")), "Pixel 9 is linked.");
    assert_eq!(
        prompts::line(&phone("new")),
        Some("Only continue if you’re linking a device of yours right now.")
    );
    assert_eq!(
        prompts::line(&phone("refused")),
        Some("This request can’t be trusted. Don’t approve it.")
    );
    assert_eq!(prompts::line(&phone("compare")), None);
}

#[test]
fn a_prompt_is_continued_compared_and_approved() {
    let mut rt = prompted(vec![phone("new")]);
    assert!(rt.find("link-prompt").is_some(), "over the window");
    shows(
        &rt,
        &[
            "Pixel 9 wants your private messages",
            "Only continue if you’re linking a device of yours right now.",
            "Continue",
            "Not me",
        ],
    );
    press(&mut rt, "prompt-continue");
    assert!(rt.state.prompts.busy);
    receive(&mut rt, IdentityUpdate::PromptAnswered(Ok(())));
    assert!(!rt.state.prompts.busy);

    receive(&mut rt, IdentityUpdate::Prompts(Ok(vec![phone("waiting")])));
    shows(&rt, &["Waiting for Pixel 9…", "Not me"]);
    assert!(rt.find("prompt-continue").is_none() && rt.find("waiting").is_some());

    receive(&mut rt, IdentityUpdate::Prompts(Ok(vec![phone("compare")])));
    shows(
        &rt,
        &[
            "Does Pixel 9 show this number?",
            "482 913",
            "They match",
            "They don’t match",
        ],
    );
    press(&mut rt, "prompt-match");
    assert!(rt.state.prompts.busy);
    shows(&rt, &["Approving…"]);
    receive(&mut rt, IdentityUpdate::PromptConfirmed(Ok(())));
    receive(&mut rt, IdentityUpdate::Prompts(Ok(Vec::new())));
    shows(&rt, &["Pixel 9 is linked.", "Done"]);
    let until = rt.state.prompts.ended.as_ref().unwrap().until;
    assert_eq!(
        rt.state.prompt_wake(until - ENDED_FOR),
        Some(POLL.min(ENDED_FOR))
    );

    // It goes by itself.
    assert!(!rt.state.tick(until - 1));
    rt.update(move |s, _| assert!(s.tick(until)));
    settle(&mut rt);
    assert!(rt.find("link-prompt").is_none());
    assert_eq!(rt.state.prompt_wake(until), None);
}

#[test]
fn they_dont_match_stops_it_and_not_me_just_closes_it() {
    let mut rt = prompted(vec![phone("compare")]);
    press(&mut rt, "prompt-mismatch");
    receive(&mut rt, IdentityUpdate::PromptDeclined(Ok(())));
    receive(&mut rt, IdentityUpdate::Prompts(Ok(Vec::new())));
    shows(&rt, &[STOPPED]);
    press(&mut rt, "prompt-done");
    assert!(rt.find("link-prompt").is_none());

    let mut rt = prompted(vec![phone("new")]);
    press(&mut rt, "prompt-not-me");
    receive(&mut rt, IdentityUpdate::PromptDeclined(Ok(())));
    receive(&mut rt, IdentityUpdate::Prompts(Ok(Vec::new())));
    assert!(rt.find("link-prompt").is_none() && rt.state.prompts.ended.is_none());
}

#[test]
fn a_refused_request_offers_only_not_me() {
    let rt = prompted(vec![phone("refused")]);
    shows(
        &rt,
        &["This request can’t be trusted. Don’t approve it.", "Not me"],
    );
    assert!(rt.find("prompt-continue").is_none() && rt.find("prompt-match").is_none());
}

#[test]
fn prompts_come_one_at_a_time_oldest_first_and_close_when_their_session_goes() {
    let older = asking("older", "native", "Pixel 9", "", "new");
    let newer = asking("newer", "web", "", "https://chat.example.com", "new");
    let mut rt = prompted(vec![older.clone(), newer.clone()]);
    shows(&rt, &["Pixel 9 wants your private messages"]);
    assert!(!words(&rt).contains("A browser on"));

    // The one on screen stays while it is listed, whatever comes before it.
    let first = asking("first", "native", "iPad", "", "new");
    receive(
        &mut rt,
        IdentityUpdate::Prompts(Ok(vec![first, older, newer.clone()])),
    );
    shows(&rt, &["Pixel 9 wants your private messages"]);

    // Its session went (approved or declined elsewhere, or expired): the next one.
    receive(&mut rt, IdentityUpdate::Prompts(Ok(vec![newer])));
    shows(
        &rt,
        &["A browser on https://chat.example.com wants your private messages"],
    );
    receive(&mut rt, IdentityUpdate::Prompts(Ok(Vec::new())));
    assert!(rt.find("link-prompt").is_none());
}

#[test]
fn refusals_are_worded_and_a_failed_question_changes_nothing() {
    let mut rt = prompted(vec![phone("new")]);
    press(&mut rt, "prompt-continue");
    receive(
        &mut rt,
        IdentityUpdate::PromptAnswered(Err(failure(
            "FAILED_PRECONDITION",
            Some("answered"),
            "answered",
        ))),
    );
    shows(&rt, &["Another of your devices answered it first."]);
    assert!(!rt.state.prompts.busy);

    receive(&mut rt, IdentityUpdate::Prompts(Ok(vec![phone("compare")])));
    press(&mut rt, "prompt-match");
    receive(
        &mut rt,
        IdentityUpdate::PromptConfirmed(Err(failure(
            "INVALID_ARGUMENT",
            Some("too_many_devices"),
            "roster",
        ))),
    );
    shows(
        &rt,
        &["Your identity already has as many devices as it can hold. Revoke one first."],
    );
    assert!(rt.state.prompts.ended.is_none());

    // The sessions could not be asked about: the prompt stays as it was.
    receive(
        &mut rt,
        IdentityUpdate::Prompts(Err(failure("", None, "connection refused"))),
    );
    assert!(rt.find("link-prompt").is_some());
}

#[test]
fn a_prompt_takes_the_window_from_a_popover_and_goes_with_a_sign_out() {
    let mut rt = runtime(signed_in());
    rt.update(|s, _| s.open_account_menu());
    settle(&mut rt);
    assert!(rt.state.popover.is_some());
    receive(&mut rt, IdentityUpdate::Prompts(Ok(vec![phone("new")])));
    assert!(rt.state.popover.is_none());
    assert!(rt.find("link-prompt").is_some());

    // Polled every two seconds while it is open.
    assert_eq!(rt.state.prompt_wake(rt.state.prompts.asked_at), Some(POLL));

    rt.update(|s, _| s.sign_out());
    settle(&mut rt);
    assert_eq!(rt.state.route, Route::SignIn);
    assert!(rt.find("link-prompt").is_none() && !rt.state.prompts.open());
}

#[test]
fn polling_is_due_after_two_seconds() {
    assert!(!prompts::due(0, POLL - 1));
    assert!(prompts::due(0, POLL));
    assert!(prompts::due(5, 5 + 3 * POLL));
}
