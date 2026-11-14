//! A server's settings (architecture 6.7), over a state no instance is behind: who is
//! offered them, the name and the retention saved, refused and said, and the way back to a
//! channel. What the instance would answer is applied with `State::receive`.

#![allow(clippy::expect_used, clippy::unwrap_used)]

mod common;

use std::collections::HashMap;

use common::*;
use lumia_client::app::SnapshotSummary;
use lumia_shell::app::State;
use lumia_shell::session::servers::{ServerUpdate, Setting};
use lumia_shell::session::{Failure, Update};
use rune::{Runtime, TextEdit};

fn answer(rt: &mut Runtime<State>, setting: Setting, result: Result<SnapshotSummary, Failure>) {
    rt.update(move |s, _| {
        s.receive(&Update::Server(ServerUpdate::SettingSaved {
            server_id: "acme".into(),
            setting,
            result,
        }));
    });
    settle(rt);
}

fn refused(code: &str, field: Option<&str>) -> Failure {
    Failure {
        code: code.into(),
        message: code.to_lowercase(),
        details: field
            .map(|f| HashMap::from([("field".to_string(), f.to_string())]))
            .unwrap_or_default(),
    }
}

/// The snapshot with Acme Works changed by `change`.
fn changed(change: impl Fn(&mut lumia_client::app::ServerSummary)) -> SnapshotSummary {
    let mut snapshot = snapshot();
    change(&mut snapshot.servers[0]);
    snapshot
}

fn open_settings(rt: &mut Runtime<State>) {
    press(rt, "server-title");
    press(rt, "menu-server-settings");
}

#[test]
fn the_settings_are_offered_to_who_may_change_them() {
    let mut rt = runtime(signed_in());
    press(&mut rt, "server-title");
    assert!(words(&rt).contains("Server settings"));
    press(&mut rt, "menu-server-settings");
    let shown = words(&rt);
    for part in [
        "Server settings",
        "Overview",
        "Message history",
        "Choose a picture…",
    ] {
        assert!(shown.contains(part), "{part} is not on the page");
    }
    assert_eq!(rt.state.server_forms.settings.name.text, "Acme Works");

    // Garden is one this account is a member of, with no say in it.
    rt.update(|s, _| s.select_server("garden"));
    settle(&mut rt);
    press(&mut rt, "server-title");
    assert!(rt.find("menu-invite").is_some());
    assert!(rt.find("menu-server-settings").is_none());
    assert!(!words(&rt).contains("Server settings"));
}

#[test]
fn the_name_is_saved_and_the_rail_follows() {
    let mut rt = runtime(signed_in());
    open_settings(&mut rt);
    rt.update(|s, _| s.server_forms.settings.name = TextEdit::new("  Acme Co "));
    settle(&mut rt);
    press(&mut rt, "save-server-name");
    assert!(rt.state.server_forms.settings.name_saving.busy);

    answer(
        &mut rt,
        Setting::Name,
        Ok(changed(|s| s.name = "Acme Co".into())),
    );
    assert!(words(&rt).contains("Saved."));
    assert_eq!(rt.state.server_forms.settings.name.text, "Acme Co");
    let rail = rt
        .elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some("server-acme"))
        .unwrap();
    assert_eq!(rail.label.as_deref(), Some("Acme Co"));
}

#[test]
fn a_refusal_is_said_in_words() {
    let mut rt = runtime(signed_in());
    open_settings(&mut rt);
    rt.update(|s, _| s.server_forms.settings.name = TextEdit::new("Ours"));
    settle(&mut rt);
    press(&mut rt, "save-server-name");
    answer(
        &mut rt,
        Setting::Name,
        Err(refused("PERMISSION_DENIED", None)),
    );
    assert!(words(&rt).contains("You cannot change this server's settings."));

    press(&mut rt, "save-server-name");
    answer(
        &mut rt,
        Setting::Name,
        Err(refused("INVALID_ARGUMENT", Some("name"))),
    );
    assert!(words(&rt).contains("A server name is 1 to 100 characters."));

    // A picture the instance will not take is worded as an account's photo is.
    answer(
        &mut rt,
        Setting::Picture,
        Err(refused("FAILED_PRECONDITION", None)),
    );
    assert!(words(&rt).contains("This instance does not keep pictures."));
}

#[test]
fn a_shorter_retention_says_what_it_deletes_before_it_is_saved() {
    let mut rt = runtime(signed_in());
    open_settings(&mut rt);
    assert!(!words(&rt).contains("will be deleted"));

    // Forever, 1 year, 90 days, 30 days.
    press(&mut rt, "retention-3");
    assert!(words(&rt).contains(
        "Messages older than 30 days will be deleted for everyone. This cannot be undone."
    ));
    press(&mut rt, "save-retention");
    assert!(rt.state.server_forms.settings.retention_saving.busy);
    answer(
        &mut rt,
        Setting::Retention,
        Ok(changed(|s| s.retention_days = 30)),
    );
    // Saved: 30 days is what the server keeps now, so nothing more is shorter.
    let shown = words(&rt);
    assert!(shown.contains("Saved."));
    assert!(!shown.contains("will be deleted"));

    // Longer is no loss; a time set elsewhere is offered as it is.
    press(&mut rt, "retention-0");
    assert!(!words(&rt).contains("will be deleted"));
    rt.update(|s, _| {
        s.receive(&Update::Snapshot(Ok(changed(|server| {
            server.retention_days = 14;
        }))));
    });
    settle(&mut rt);
    assert!(words(&rt).contains("14 days"));
}

#[test]
fn a_channel_goes_back_from_the_settings() {
    let mut rt = runtime(signed_in());
    open_settings(&mut rt);
    assert!(words(&rt).contains("Message history"));
    press(&mut rt, "channel-design");
    assert!(!words(&rt).contains("Message history"));
    assert!(!rt.state.server_settings_open());
}

/// A server's picture is drawn on the rail and on the page, and once it has decoded its
/// initials are gone from under it: a picture with transparency would show them through.
#[test]
fn a_servers_picture_stands_alone_once_drawn() {
    use lumia_shell::session::avatars::AvatarUpdate;

    let mut rt = runtime(signed_in());
    let url = format!("/api/v1/avatars/{}", "ac".repeat(32));
    let picture = std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../assets/brand/lumia-original-icon.png"
    ))
    .unwrap();
    let with_picture = changed(|s| s.icon_url.clone_from(&url));
    rt.update(move |s, _| {
        s.receive(&Update::Snapshot(Ok(with_picture)));
        s.receive(&Update::Avatar(AvatarUpdate::Fetched {
            url,
            bytes: Ok(std::sync::Arc::from(picture)),
        }));
    });
    // What a window's poll does before every frame: the pictures held are decoded for it.
    rt.update(|s, _| {
        s.tick(chrono::Utc::now().timestamp_micros());
    });
    settle(&mut rt);
    rt.state.pictures.wait();
    open_settings(&mut rt);

    let initials: Vec<_> = rt
        .elements()
        .into_iter()
        .filter(|e| e.text.as_deref() == Some("AW"))
        .collect();
    assert!(
        initials.is_empty(),
        "the initials are still drawn: {initials:?}"
    );
    // Garden has none: its initials are its tile.
    assert!(words(&rt).lines().any(|w| w == "G"));
    let pictures = rt
        .elements()
        .into_iter()
        .filter(|e| e.key.as_deref() == Some("picture"))
        .count();
    assert_eq!(pictures, 2, "on the rail and on the page");
}

#[test]
fn deleted_messages_are_shown_or_not_as_the_switch_says_saved_as_it_is_turned() {
    let mut rt = runtime(signed_in());
    open_settings(&mut rt);
    let shown = words(&rt);
    assert!(shown.contains("Show deleted messages"));
    assert!(shown.contains("Off, a deleted message disappears for everyone."));
    assert!(
        !rt.state.session.servers()[0].show_deleted,
        "off by default"
    );

    press(&mut rt, "show-deleted");
    assert!(
        rt.state.server_forms.settings.show_deleted.busy,
        "sent at once, with no Save"
    );
    answer(
        &mut rt,
        Setting::ShowDeleted,
        Ok(changed(|s| s.show_deleted = true)),
    );
    assert!(rt.state.session.servers()[0].show_deleted);
    assert!(!rt.state.server_forms.settings.show_deleted.busy);

    // A refusal is said under it, and the server stays as it was.
    press(&mut rt, "show-deleted");
    answer(
        &mut rt,
        Setting::ShowDeleted,
        Err(refused("PERMISSION_DENIED", None)),
    );
    assert!(rt.state.session.servers()[0].show_deleted);
    assert!(!rt.state.server_forms.settings.show_deleted.error.is_empty());
}
