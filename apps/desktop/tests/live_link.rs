//! Linking this computer to an identity against a real instance (identity spec 4.2): a CLI
//! device holds the identity, the shell asks to be linked, the CLI approves, and the shell
//! claims the code and is signed in by identity proof. Runs only with `LUMIA_DEV_INSTANCE`
//! and `LUMIA_CLI` (the path of a built `lumia-cli`) set; without them it passes having
//! checked nothing, and says so.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::path::Path;
use std::process::Command;
use std::time::{Duration, Instant};

use lumia_shell::app::State;
use lumia_shell::session::Update;
use lumia_shell::session::identity::IdentityUpdate;
use rune::TextEdit;

const PASSWORD: &str = "correct-horse-battery-staple";

/// Runs the CLI over `data` and returns what it printed.
fn cli(bin: &str, data: &Path, args: &[&str]) -> String {
    let out = Command::new(bin)
        .arg("--data-dir")
        .arg(data)
        .args(args)
        .output()
        .expect("the CLI runs");
    assert!(
        out.status.success(),
        "lumia-cli {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).into_owned()
}

/// Drains the session until `pick` finds its update, and returns what it made of it.
fn wait_for<T>(state: &mut State, what: &str, pick: impl Fn(&Update) -> Option<T>) -> T {
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        for update in state.session.poll() {
            if let Some(found) = pick(&update) {
                return found;
            }
        }
        assert!(Instant::now() < deadline, "timed out waiting for {what}");
        std::thread::sleep(Duration::from_millis(20));
    }
}

#[test]
fn this_computer_links_to_an_identity_a_cli_device_holds() {
    let (Ok(instance), Ok(bin)) = (
        std::env::var("LUMIA_DEV_INSTANCE"),
        std::env::var("LUMIA_CLI"),
    ) else {
        eprintln!("skipped: no LUMIA_DEV_INSTANCE and LUMIA_CLI");
        return;
    };
    let dir = std::env::temp_dir().join(format!("lumia-shell-link-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let (holder, shell) = (dir.join("cli"), dir.join("shell"));
    let handle = format!("link{}", std::process::id());

    // A device that holds the identity, with the account bound to it.
    cli(&bin, &holder, &["instance", "add", &instance]);
    cli(
        &bin,
        &holder,
        &[
            "account",
            "create",
            "--handle",
            &handle,
            "--password",
            PASSWORD,
        ],
    );
    cli(&bin, &holder, &["identity", "create"]);
    cli(
        &bin,
        &holder,
        &["identity", "link-account", "--password", PASSWORD],
    );

    let mut state = State::new(shell);
    state.sign_in.instance = TextEdit::new(instance.as_str());
    state.connect();
    assert!(state.session.is_open(), "{}", state.sign_in.error);
    assert!(!state.session.has_identity());

    state.session.request_link("desktop");
    let request = wait_for(&mut state, "the link request", |u| match u {
        Update::Identity(IdentityUpdate::LinkRequested(r)) => Some(r.clone()),
        _ => None,
    })
    .expect("a link request");
    let approved = cli(&bin, &holder, &["identity", "approve", request.trim()]);
    let code = approved
        .split_whitespace()
        .rev()
        .find(|word| word.len() == 8 && word.chars().all(|c| c.is_ascii_alphanumeric()))
        .unwrap_or_else(|| panic!("no claim code in: {approved}"));

    state.session.claim_link(code);
    let roster = wait_for(&mut state, "the claim", |u| match u {
        Update::Identity(IdentityUpdate::Linked(r)) => Some(r.clone()),
        _ => None,
    })
    .expect("linked");
    assert!(state.session.has_identity() && state.session.signed_in);
    assert_eq!(roster.devices.len(), 2, "the CLI's device and this one");
    assert!(
        roster
            .devices
            .iter()
            .any(|d| d.this_device && d.label == "desktop"),
        "this computer is in the roster under the name it gave: {:?}",
        roster.devices
    );
}
