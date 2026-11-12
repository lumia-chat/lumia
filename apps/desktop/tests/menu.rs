//! The Account menu: which of its items can be chosen in each state, what they do, and the
//! items reaching the real binary. The last runs `lumia` headless with a script, as Rune's
//! tooling fires a menu item through the same lookup the menu bar uses.

#![allow(clippy::expect_used, clippy::unwrap_used)]

mod common;

use std::process::Command;

use common::*;
use lumia_shell::app::{Route, State, menu};
use lumia_shell::settings::Settings;

#[test]
fn the_items_follow_where_the_app_stands() {
    // (Sign out, Change instance)
    assert_eq!(menu::standing(&State::new(data_dir())), (false, false));
    assert_eq!(menu::standing(&opened()), (false, true));
    assert_eq!(menu::standing(&signed_in()), (true, true));
}

#[test]
fn change_instance_leaves_without_signing_out_and_asks_again_next_launch() {
    let dir = data_dir();
    let mut state = State::new(dir.clone());
    state.sign_in.instance.replace_text("http://127.0.0.1:9");
    state.connect();
    assert_eq!(
        Settings::load(&dir).instance.as_deref(),
        Some("http://127.0.0.1:9")
    );
    state.receive(&lumia_shell::session::Update::SignedIn(Ok(())));
    state.receive(&lumia_shell::session::Update::Snapshot(Ok(snapshot())));
    assert_eq!(state.route, Route::Shell);

    state.forget_instance();
    assert_eq!(state.route, Route::SignIn);
    assert!(!state.session.is_open() && state.session.snapshot.is_none());
    assert_eq!(Settings::load(&dir).instance, None, "forgotten on disk too");
    assert_eq!(menu::standing(&state), (false, false));

    let rt = runtime(state);
    assert!(rt.find("instance-card").is_some(), "back to asking for one");
}

/// Runs the built shell headless over `dir` with a script, and returns whether it
/// succeeded with what it printed to stderr.
fn run_script(dir: &std::path::Path, name: &str, events: &str) -> (bool, String) {
    let script = dir.join(format!("{name}.json"));
    std::fs::write(&script, events).unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_lumia"))
        .env("LUMIA_DATA_DIR", dir)
        .env_remove("LUMIA_DEV_INSTANCE")
        .arg("--script")
        .arg(&script)
        .arg("--tree")
        .arg(dir.join(format!("{name}-tree.json")))
        .args(["--at", "1.0"])
        .output()
        .expect("the shell runs");
    (
        out.status.success(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

#[test]
fn the_binary_has_the_items_and_enables_them_for_its_state() {
    // A saved instance nothing answers for: open, not signed in.
    let dir = data_dir();
    std::fs::create_dir_all(&dir).unwrap();
    Settings {
        instance: Some("http://127.0.0.1:9".into()),
    }
    .save(&dir);

    // Sign out has nobody to sign out, so the menu refuses it.
    let (ok, said) = run_script(
        &dir,
        "sign-out",
        r#"[{"t": 0.3, "menu": "sign-out"}, {"t": 0.6, "type": "mark", "name": "end"}]"#,
    );
    assert!(!ok && said.contains("disabled"), "{said}");
    assert_eq!(
        Settings::load(&dir).instance.as_deref(),
        Some("http://127.0.0.1:9")
    );

    // Change instance is there to choose, and a link handed over on the way does no harm.
    let (ok, said) = run_script(
        &dir,
        "change",
        r#"[{"t": 0.2, "open_url": "lumia://sign-in?state=abc&magic=token"},
            {"t": 0.3, "menu": "change-instance"},
            {"t": 0.6, "type": "mark", "name": "end"}]"#,
    );
    assert!(ok, "{said}");
    assert_eq!(Settings::load(&dir).instance, None);
    let tree = std::fs::read_to_string(dir.join("change-tree.json")).unwrap();
    assert!(tree.contains("Which instance do you want to sign in to?"));

    // With no instance open, neither item can be chosen.
    let (ok, said) = run_script(
        &dir,
        "nothing",
        r#"[{"t": 0.3, "menu": "change-instance"}, {"t": 0.6, "type": "mark", "name": "end"}]"#,
    );
    assert!(!ok && said.contains("disabled"), "{said}");
}
