//! The on-screen updater (`app::updates`), which the Linux AppImage has, through a windowless
//! `Runtime`: what Preferences and the sidebar's foot show at each step, what each button
//! asks of the platform, and the daily check's schedule. The platform's half (the network,
//! the file put in place, the restart) is `platform::appimage`, which needs a real AppImage
//! and is not run here; what it reports is applied with `State::react_update`.

#![allow(clippy::expect_used, clippy::unwrap_used)]

mod common;

use common::*;
use lumia_shell::app::updates::{Ask, DAY, Phase, Report, Updater};
use lumia_shell::app::{HomeView, State};
use lumia_shell::settings::{Settings, UpdateChecks};
use rune::Runtime;

const CURRENT: &str = env!("CARGO_PKG_VERSION");

/// A signed-in state whose platform has an updater, on Preferences.
fn with_updater() -> Runtime<State> {
    let mut state = signed_in();
    state.updater = Some(Updater::default());
    state.go_home(HomeView::Preferences);
    runtime(state)
}

fn owed(rt: &Runtime<State>) -> Option<Ask> {
    rt.state.updater.as_ref().unwrap().owed
}

fn phase(rt: &Runtime<State>) -> Phase {
    rt.state.updater.as_ref().unwrap().phase.clone()
}

/// The platform taking what the window asked for, as its poll does.
fn take(rt: &mut Runtime<State>) -> Option<Ask> {
    rt.state.updater.as_mut().unwrap().owed.take()
}

fn report(rt: &mut Runtime<State>, report: Report) {
    rt.update(move |state, _| state.react_update(report));
    settle(rt);
}

#[test]
fn without_an_updater_there_is_no_card_and_no_notice() {
    let mut state = signed_in();
    state.go_home(HomeView::Preferences);
    let rt = runtime(state);
    assert!(rt.find("updates-card").is_none());
    assert!(rt.find("update-waiting").is_none());
}

#[test]
fn a_check_asked_for_answers_either_way() {
    let mut rt = with_updater();
    assert!(words(&rt).contains(&format!("This is Lumia {CURRENT}.")));
    assert!(words(&rt).contains("Check for updates automatically"));
    press(&mut rt, "check-for-updates");
    assert_eq!(owed(&rt), Some(Ask::Check));
    assert_eq!(phase(&rt), Phase::Checking);
    assert!(words(&rt).contains("Checking for updates…"));
    // A second press while one is under way asks for nothing more.
    assert_eq!(take(&mut rt), Some(Ask::Check));
    rt.update(|state, _| state.check_for_updates());
    assert_eq!(owed(&rt), None);

    report(&mut rt, Report::Checked(None));
    assert!(words(&rt).contains(&format!("Lumia {CURRENT} is the newest release.")));
    assert!(rt.find("update-waiting").is_none());

    press(&mut rt, "check-for-updates");
    take(&mut rt);
    report(
        &mut rt,
        Report::Failed("could not reach the release".into()),
    );
    assert!(words(&rt).contains("Could not update: could not reach the release"));
}

#[test]
fn a_release_found_is_downloaded_only_when_asked_then_waits_for_a_restart() {
    let mut rt = with_updater();
    press(&mut rt, "check-for-updates");
    take(&mut rt);
    report(&mut rt, Report::Checked(Some("99.0.0".into())));
    assert!(words(&rt).contains("Lumia 99.0.0 is available."));
    assert!(
        rt.find("update-waiting").is_some(),
        "the sidebar's foot says so too"
    );
    assert_eq!(owed(&rt), None, "nothing is downloaded without a press");

    press(&mut rt, "install-update");
    assert_eq!(owed(&rt), Some(Ask::Install));
    assert_eq!(phase(&rt), Phase::Downloading("99.0.0".into()));
    assert!(words(&rt).contains("Downloading Lumia 99.0.0…"));
    take(&mut rt);
    report(&mut rt, Report::Installed("99.0.0".into()));
    assert!(words(&rt).contains("Lumia 99.0.0 is ready. Restart Lumia to start using it."));

    press(&mut rt, "update-waiting-action");
    assert_eq!(owed(&rt), Some(Ask::Restart));
}

#[test]
fn the_sidebar_notice_starts_the_download_from_anywhere() {
    let mut rt = with_updater();
    rt.update(|state, _| {
        state.go_home(HomeView::Conversations);
        state.react_update(Report::Checked(Some("99.0.0".into())));
    });
    settle(&mut rt);
    assert!(rt.find("updates-card").is_none());
    assert!(words(&rt).contains("Lumia 99.0.0 is available."));
    press(&mut rt, "update-waiting-action");
    assert_eq!(owed(&rt), Some(Ask::Install));
}

#[test]
fn a_failed_download_says_why() {
    let mut rt = with_updater();
    rt.update(|state, _| state.react_update(Report::Checked(Some("99.0.0".into()))));
    press(&mut rt, "install-update");
    take(&mut rt);
    report(
        &mut rt,
        Report::Failed("the signature does not verify with the release key".into()),
    );
    assert!(
        words(&rt).contains("Could not update: the signature does not verify with the release key")
    );
    assert!(rt.find("update-waiting").is_none());
}

#[test]
fn the_daily_check_runs_once_a_day_and_quietly() {
    let mut rt = with_updater();
    let start = 1_800_000_000;
    rt.update(move |state, _| state.check_if_due(start));
    assert_eq!(take(&mut rt), Some(Ask::Check));
    assert_eq!(rt.state.settings.last_update_check, Some(start));
    // Nothing found, and the window says nothing it did not say before.
    report(&mut rt, Report::Checked(None));
    assert_eq!(phase(&rt), Phase::Idle);

    rt.update(move |state, _| state.check_if_due(start + DAY - 1));
    assert_eq!(owed(&rt), None, "not yet a day");
    rt.update(move |state, _| state.check_if_due(start + DAY));
    assert_eq!(take(&mut rt), Some(Ask::Check));
    // A failure it alone met is not shown.
    report(&mut rt, Report::Failed("offline".into()));
    assert_eq!(phase(&rt), Phase::Idle);
    // The time is kept for the next launch.
    let kept = Settings::load(rt.state.data_dir());
    assert_eq!(kept.last_update_check, Some(start + DAY));
}

#[test]
fn turning_the_daily_check_off_stops_it_and_is_kept() {
    let mut rt = with_updater();
    press(&mut rt, "check-updates-automatically");
    assert_eq!(rt.state.settings.update_checks, UpdateChecks::Off);
    assert_eq!(
        Settings::load(rt.state.data_dir()).update_checks,
        UpdateChecks::Off
    );
    rt.update(|state, _| state.check_if_due(1_800_000_000));
    assert_eq!(owed(&rt), None);
    // A check asked for still runs.
    press(&mut rt, "check-for-updates");
    assert_eq!(owed(&rt), Some(Ask::Check));
    press(&mut rt, "check-updates-automatically");
    assert_eq!(rt.state.settings.update_checks, UpdateChecks::Daily);
}

#[test]
fn a_release_waiting_is_not_checked_over() {
    let mut rt = with_updater();
    rt.update(|state, _| state.react_update(Report::Checked(Some("99.0.0".into()))));
    rt.update(|state, _| state.check_if_due(1_800_000_000));
    assert_eq!(owed(&rt), None);
    assert_eq!(phase(&rt), Phase::Available("99.0.0".into()));
}
