//! A changed name and picture reaching someone else (#15) against a real instance: a CLI
//! account makes a server and an invite, the shell joins it, and the CLI account then
//! changes its display name and picture (`account update`). The shell, told through its
//! account log, names it anew everywhere it holds it and fetches the new picture, without a
//! restart. Runs only with `LUMIA_DEV_INSTANCE` and `LUMIA_CLI` (the path of a built
//! `lumia-cli`) set; without them it passes having checked nothing, and says so. It makes
//! its own accounts, so it needs an instance with open registration.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use lumia_shell::app::{HomeView, Route, State};
use lumia_shell::session::Connection;
use rune::TextEdit;

const PASSWORD: &str = "correct-horse-battery-staple";
const PATIENCE: Duration = Duration::from_secs(30);

/// Runs the CLI over `data` and returns what it printed.
fn cli(bin: &str, data: &Path, args: &[&str]) -> String {
    let out = Command::new(bin)
        .arg("--data-dir")
        .arg(data)
        .args(args)
        .stdin(Stdio::null())
        .output()
        .expect("the CLI runs");
    assert!(
        out.status.success(),
        "lumia-cli {args:?}: {}\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).into_owned()
}

/// The first thing shaped like an id (a UUID) in what the CLI printed.
fn id_in(text: &str) -> String {
    text.split(|c: char| !(c.is_ascii_hexdigit() || c == '-'))
        .find(|word| word.len() == 36 && word.matches('-').count() == 4)
        .unwrap_or_else(|| panic!("no id in: {text}"))
        .to_string()
}

/// Drains the session, reacting as the window would, until `done`.
fn wait(state: &mut State, what: &str, done: impl Fn(&State) -> bool) {
    let deadline = Instant::now() + PATIENCE;
    while !done(state) {
        assert!(
            Instant::now() < deadline,
            "timed out waiting for {what}; session error {:?}, connection {:?}, add {:?}",
            state.session.last_error,
            state.session.connection,
            state.server_forms.add.error,
        );
        state.drain();
        std::thread::sleep(Duration::from_millis(20));
    }
}

#[test]
fn a_new_name_and_picture_reach_someone_who_shares_a_server() {
    let (Ok(instance), Ok(bin)) = (
        std::env::var("LUMIA_DEV_INSTANCE"),
        std::env::var("LUMIA_CLI"),
    ) else {
        eprintln!("skipped: no LUMIA_DEV_INSTANCE and LUMIA_CLI");
        return;
    };
    let dir = std::env::temp_dir().join(format!("lumia-shell-profiles-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis()
        % 100_000_000;

    // ---- Someone on the CLI makes a server and an invite to it -------------------------
    let other = dir.join("other");
    cli(&bin, &other, &["instance", "add", &instance]);
    cli(
        &bin,
        &other,
        &[
            "account",
            "create",
            "--handle",
            &format!("prfoth{stamp}"),
            "--password",
            PASSWORD,
        ],
    );
    let them = id_in(&cli(&bin, &other, &["whoami"]));
    let server = id_in(&cli(
        &bin,
        &other,
        &["server-create", &format!("Names {stamp}")],
    ));
    let made = cli(&bin, &other, &["invite-create", &server]);
    let link = made
        .split_whitespace()
        .nth(1)
        .unwrap_or_else(|| panic!("no link in: {made}"))
        .to_string();

    // ---- The shell's account joins, and lists them ------------------------------------
    let mut state = State::new(dir.join("shell"));
    state.sign_in.instance = TextEdit::new(&instance);
    state.connect();
    wait(&mut state, "the trust card", |s| s.session.info.is_some());
    state.sign_in.creating = true;
    state.sign_in.handle = TextEdit::new(format!("prfme{stamp}"));
    state.sign_in.password.replace_text(PASSWORD);
    state.submit();
    wait(&mut state, "the shell", |s| s.route == Route::Shell);
    state.go_home(HomeView::AddServer);
    state.server_forms.add.invite.replace_text(&link);
    state.look_up_invite();
    wait(&mut state, "the invite's preview", |s| {
        s.server_forms.add.preview.is_some()
    });
    state.join_invite();
    wait(
        &mut state,
        "the joined server, its members and the gateway",
        |s| {
            s.server_id() == Some(server.as_str())
                && s.session.connection == Connection::Connected
                && s.session
                    .members
                    .get(&server)
                    .is_some_and(|all| all.iter().any(|m| m.account_id == them))
        },
    );
    assert_eq!(state.session.avatar_url_of(&them), "", "no picture yet");

    // ---- They change their name and picture; the shell shows both -------------------------
    let name = format!("Renamed {stamp}");
    let photo = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../instance/test/support/fixtures/media/sideways.jpg");
    cli(
        &bin,
        &other,
        &[
            "account",
            "update",
            "--name",
            &name,
            "--picture",
            photo.to_str().unwrap(),
        ],
    );
    wait(&mut state, "the new name and picture", |s| {
        s.session.name_of(&them) == name
            && s.session
                .avatar_url_of(&them)
                .starts_with("/api/v1/avatars/")
    });
    let listed = state.session.members[&server]
        .iter()
        .find(|m| m.account_id == them)
        .unwrap();
    assert_eq!(listed.display_name, name, "in the member list");
    let url = state.session.avatar_url_of(&them).to_string();
    wait(&mut state, "the picture's bytes", |s| {
        s.session.picture_of(&them) == Some(url.as_str())
    });
}
