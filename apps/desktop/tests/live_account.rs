//! The account flows against a real instance, driven through the window's state as the
//! screens drive it: linking this computer, then revoking the other device, setting a
//! profile and reading it back, and choosing a status that the instance then reports; and a
//! magic-link sign-in with the mail read from the dev instance's local mailbox.
//!
//! A CLI device plays the device that holds the identity. Runs only with
//! `LUMIA_DEV_INSTANCE` and `LUMIA_CLI` (the path of a built `lumia-cli`) set; without them
//! each test passes having checked nothing, and says so.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use lumia_shell::app::{HomeView, Route, State};
use lumia_shell::session::Connection;
use rune::TextEdit;

const PASSWORD: &str = "correct-horse-battery-staple";

fn environment() -> Option<(String, String)> {
    match (
        std::env::var("LUMIA_DEV_INSTANCE"),
        std::env::var("LUMIA_CLI"),
    ) {
        (Ok(instance), Ok(bin)) => Some((instance, bin)),
        _ => {
            eprintln!("skipped: no LUMIA_DEV_INSTANCE and LUMIA_CLI");
            None
        }
    }
}

/// A name no other run or test has used: the instance is shared.
fn unique(tag: &str) -> String {
    static NEXT: AtomicU32 = AtomicU32::new(0);
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("a clock")
        .as_millis();
    format!(
        "{tag}{millis}{}{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    )
}

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("lumia-shell-account-{name}"));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

/// Runs the CLI over `data`: whether it succeeded, and what it printed.
fn cli_try(bin: &str, data: &Path, args: &[&str]) -> (bool, String) {
    let out = Command::new(bin)
        .arg("--data-dir")
        .arg(data)
        .args(args)
        .output()
        .expect("the CLI runs");
    let mut said = String::from_utf8_lossy(&out.stdout).into_owned();
    said.push_str(&String::from_utf8_lossy(&out.stderr));
    (out.status.success(), said)
}

fn cli(bin: &str, data: &Path, args: &[&str]) -> String {
    let (ok, said) = cli_try(bin, data, args);
    assert!(ok, "lumia-cli {args:?}: {said}");
    said
}

/// Closes the shell and removes what a test that passed left on disk (`holder` is the CLI's
/// directory, beside the shell's). A test that fails leaves its stores to look at.
fn finished(state: State, holder: &Path) {
    drop(state);
    if let Some(dir) = holder.parent() {
        let _ = std::fs::remove_dir_all(dir);
    }
}

/// Drains the session until `done`, reacting as the window does.
fn wait(state: &mut State, what: &str, done: impl Fn(&State) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(20);
    while !done(state) {
        assert!(
            Instant::now() < deadline,
            "timed out waiting for {what}; session error {:?}, sign-in {:?}, link {:?}, devices {:?}, profile {:?}",
            state.session.last_error,
            state.sign_in.error,
            state.account.link.error,
            state.account.devices.error,
            state.account.profile.error,
        );
        state.drain();
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// The shell with the instance open and its trust card in hand.
fn opened(instance: &str, dir: PathBuf) -> State {
    let mut state = State::new(dir);
    state.sign_in.instance = TextEdit::new(instance);
    state.connect();
    assert!(state.session.is_open(), "{}", state.sign_in.error);
    wait(&mut state, "the trust card", |s| s.session.info.is_some());
    state
}

/// Where the link screen is opened from.
#[derive(Clone, Copy, PartialEq)]
enum From {
    /// The sign-in card, signed out.
    SignIn,
    /// The profile page of the account signed in with its password, which has no identity
    /// on this computer yet.
    Profile,
}

/// A CLI device with a new account bound to a new identity; `before` runs over it before
/// the shell arrives. Then the shell, linked to that identity through the link screen's
/// own steps and signed in by identity proof.
fn linked(
    instance: &str,
    bin: &str,
    tag: &str,
    from: From,
    before: impl FnOnce(&Path, &str),
) -> (State, PathBuf, String) {
    let dir = scratch(&unique(tag));
    let (holder, shell) = (dir.join("cli"), dir.join("shell"));
    let handle = unique(tag);
    cli(bin, &holder, &["instance", "add", instance]);
    cli(
        bin,
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
    cli(bin, &holder, &["identity", "create"]);
    cli(
        bin,
        &holder,
        &["identity", "link-account", "--password", PASSWORD],
    );
    before(&holder, &handle);

    let mut state = opened(instance, shell);
    assert!(!state.session.has_identity());
    assert!(
        state
            .session
            .info
            .as_ref()
            .is_some_and(|info| info.identity_proof_sign_in_enabled),
        "the dev instance lets identity proof sign in, which is what offers the link screen"
    );
    if from == From::Profile {
        state.sign_in.handle.replace_text(&handle);
        state.sign_in.password.replace_text(PASSWORD);
        state.submit();
        wait(&mut state, "the password sign-in", |s| {
            s.route == Route::Shell || !s.sign_in.error.is_empty()
        });
        assert_eq!(state.sign_in.error, "");
        state.go_home(HomeView::Profile);
        assert!(
            !state.session.has_identity() && !state.account.profile.loaded,
            "signed in, and the page has no identity to show a profile for"
        );
    }
    state.start_linking();
    assert_eq!(state.route, Route::Link);
    state.start_link();
    wait(&mut state, "the link request", |s| {
        !s.account.link.request.is_empty() || !s.account.link.error.is_empty()
    });
    assert_eq!(state.account.link.error, "");
    let request = state.account.link.request.clone();
    let approved = cli(bin, &holder, &["identity", "approve", request.trim()]);
    let code = approved
        .split_whitespace()
        .rev()
        .find(|word| word.len() == 8 && word.chars().all(|c| c.is_ascii_alphanumeric()))
        .unwrap_or_else(|| panic!("no claim code in: {approved}"))
        .to_string();

    // A code nobody was given is refused, and the screen stays to take the right one.
    state.account.link.code.replace_text("ZZZZ9999");
    state.claim_link();
    wait(&mut state, "the refusal of a wrong code", |s| {
        !s.account.link.busy
    });
    assert!(
        !state.account.link.error.is_empty() && state.route == Route::Link,
        "a wrong code is refused"
    );

    state.account.link.code.replace_text(&code);
    state.claim_link();
    wait(&mut state, "the claim", |s| {
        s.route == Route::Shell || (!s.account.link.busy && !s.account.link.error.is_empty())
    });
    assert_eq!(state.account.link.error, "");
    assert_eq!(state.route, Route::Shell);
    assert!(state.session.has_identity() && state.session.signed_in);
    wait(&mut state, "the snapshot", |s| s.session.snapshot.is_some());
    assert_eq!(
        state.session.snapshot.as_ref().map(|s| s.handle.as_str()),
        Some(handle.as_str()),
        "signed in by identity proof to the account the identity is bound to"
    );
    (state, holder, handle)
}

#[test]
fn another_device_is_revoked_with_the_account_password() {
    let Some((instance, bin)) = environment() else {
        return;
    };
    let (mut state, holder, _) =
        linked(&instance, &bin, "revoke", From::SignIn, |holder, handle| {
            // The session `account create` makes with a password is not bound to the device
            // (the request carries no device id without an identity proof, so the instance
            // mints one), and a revocation does not reach it. Signing in again binds one.
            cli(
                &bin,
                holder,
                &["login", "--handle", handle, "--password", PASSWORD],
            );
        });
    // The holder's session works until it is revoked.
    assert!(cli_try(&bin, &holder, &["servers"]).0);

    state.go_home(HomeView::Devices);
    wait(&mut state, "the roster", |s| {
        s.session.identity.roster.is_some()
    });
    let roster = state.session.identity.roster.clone().expect("a roster");
    assert_eq!(roster.devices.len(), 2, "{:?}", roster.devices);
    let here = roster
        .devices
        .iter()
        .find(|d| d.this_device)
        .expect("this device");
    let other = roster
        .devices
        .iter()
        .find(|d| !d.this_device)
        .expect("the CLI's device")
        .clone();
    assert_eq!(here.kind, "native");
    assert!(!here.revoked && !other.revoked);

    state.ask_revoke(&other.device_id);
    state
        .account
        .devices
        .password
        .replace_text("not-the-password-at-all");
    state.revoke();
    wait(&mut state, "the refusal of a wrong password", |s| {
        !s.account.devices.busy
    });
    assert_eq!(
        state.account.devices.error,
        "That password was not accepted."
    );
    assert!(state.account.devices.revoking.is_some(), "still asking");

    state.account.devices.password.replace_text(PASSWORD);
    state.revoke();
    wait(&mut state, "the revocation", |s| !s.account.devices.busy);
    assert_eq!(state.account.devices.error, "");
    assert!(state.account.devices.revoking.is_none());
    let after = state.session.identity.roster.clone().expect("a roster");
    assert!(after.version > roster.version);
    assert!(
        after
            .devices
            .iter()
            .any(|d| d.device_id == other.device_id && d.revoked),
        "{:?}",
        after.devices
    );
    assert!(
        after.devices.iter().any(|d| d.this_device && !d.revoked),
        "this device stays"
    );

    // The instance killed the revoked device's session: it can no longer ask for anything.
    let (ok, said) = cli_try(&bin, &holder, &["servers"]);
    assert!(!ok, "the revoked device still reads its servers: {said}");
    // This one's still works.
    state.session.snapshot = None;
    state.session.load();
    wait(&mut state, "a snapshot after the revocation", |s| {
        s.session.snapshot.is_some()
    });
    finished(state, &holder);
}

#[test]
fn a_profile_is_signed_sent_and_read_back() {
    let Some((instance, bin)) = environment() else {
        return;
    };
    // Linked from the profile page's notice: the page is there again afterwards, and asks
    // for the profile now that there is an identity to carry one.
    let (mut state, holder, handle) = linked(&instance, &bin, "profile", From::Profile, |_, _| {});
    assert_eq!(state.home_view, HomeView::Profile);
    wait(&mut state, "the profile held", |s| s.account.profile.loaded);
    assert_eq!(state.account.profile.error, "");
    assert!(
        state.session.identity.profile.is_none(),
        "nothing was ever set"
    );
    assert_eq!(state.account.profile.display_name.text, "");

    let name = format!("Ada of {handle}");
    state.account.profile.display_name.replace_text(&name);
    state.account.profile.pronouns.replace_text("she/her");
    state
        .account
        .profile
        .bio
        .replace_text("Notes on the Analytical Engine.");
    state.save_profile();
    assert!(state.account.profile.busy);
    wait(&mut state, "the save", |s| !s.account.profile.busy);
    assert_eq!(state.account.profile.error, "");
    assert!(state.account.profile.saved);
    let first = state
        .session
        .identity
        .profile
        .clone()
        .expect("the saved profile");
    assert_eq!(first.display_name, name);
    assert!(first.version >= 1);

    // The instance shows it as the account's name now.
    wait(&mut state, "the name in the snapshot", |s| {
        s.session
            .snapshot
            .as_ref()
            .is_some_and(|snap| snap.display_name == name)
    });

    // Coming back to the page reads it back into the form.
    state.go_home(HomeView::Devices);
    state.go_home(HomeView::Profile);
    assert!(!state.account.profile.loaded);
    wait(&mut state, "the profile read back", |s| {
        s.account.profile.loaded
    });
    let form = &state.account.profile;
    assert_eq!(
        (
            form.display_name.text.as_str(),
            form.pronouns.text.as_str(),
            form.bio.text.as_str()
        ),
        (name.as_str(), "she/her", "Notes on the Analytical Engine.")
    );

    // A second save is a newer version.
    state.account.profile.pronouns.replace_text("she/they");
    state.save_profile();
    wait(&mut state, "the second save", |s| !s.account.profile.busy);
    assert_eq!(state.account.profile.error, "");
    let second = state
        .session
        .identity
        .profile
        .clone()
        .expect("the saved profile");
    assert!(second.version > first.version);
    assert_eq!(second.pronouns, "she/they");
    finished(state, &holder);
}

#[test]
fn a_status_is_kept_and_reported_by_the_instance() {
    let Some((instance, bin)) = environment() else {
        return;
    };
    let mut server = String::new();
    let (mut state, holder, _) = linked(&instance, &bin, "status", From::SignIn, |holder, _| {
        let made = cli(&bin, holder, &["server-create", "Status check"]);
        server = made
            .split(|c: char| !(c.is_ascii_hexdigit() || c == '-'))
            .find(|word| word.len() == 36)
            .unwrap_or_else(|| panic!("no server id in: {made}"))
            .to_string();
    });
    wait(&mut state, "the gateway and the server", |s| {
        s.session.connection == Connection::Connected
            && s.session.servers().iter().any(|x| x.server_id == server)
    });
    let me = state.session.me().to_string();

    // In the server, this account is present as it is by default.
    state.select_server(&server);
    wait(&mut state, "this account's presence", |s| {
        s.session.presence_of(&server, &me).is_some()
    });
    assert_eq!(
        state
            .session
            .presence_of(&server, &me)
            .map(|(status, _)| status.as_str()),
        Some("ONLINE")
    );

    // On the profile page: do not disturb, with a line.
    state.go_home(HomeView::Profile);
    assert_eq!(state.account.profile.status, "ONLINE");
    state.pick_status("DO_NOT_DISTURB");
    state
        .account
        .profile
        .custom_status
        .replace_text("Heads down");
    state.save_status();
    assert_eq!(state.account.profile.status_error, "");
    assert!(state.account.profile.status_saved);

    // Back in the server, the instance reports it.
    state.select_server(&server);
    wait(&mut state, "the chosen status from the instance", |s| {
        s.session
            .presence_of(&server, &me)
            .is_some_and(|(status, custom)| status == "DO_NOT_DISTURB" && custom == "Heads down")
    });

    // And the page shows it again, from the store.
    state.go_home(HomeView::Profile);
    assert_eq!(state.account.profile.status, "DO_NOT_DISTURB");
    assert_eq!(state.account.profile.custom_status.text, "Heads down");

    // So does a second launch over the same stores, still signed in.
    drop(state);
    let shell = holder.parent().expect("the test's directory").join("shell");
    let mut again = opened(&instance, shell);
    assert!(again.session.signed_in && again.session.has_identity());
    again.go_home(HomeView::Profile);
    assert_eq!(again.account.profile.status, "DO_NOT_DISTURB");
    assert_eq!(again.account.profile.custom_status.text, "Heads down");
    finished(again, &holder);
}

/// An account made with a password on this computer, which holds no identity: its handle
/// and display name are its own to change (`PATCH /account`), a taken handle is refused in
/// words, the old handle no longer signs in and nobody else can take it.
#[test]
fn a_password_account_changes_its_handle_and_display_name() {
    let Some((instance, bin)) = environment() else {
        return;
    };
    let (before, after) = (unique("was"), unique("now"));
    let mut state = opened(&instance, scratch("rename"));
    state.sign_in.creating = true;
    state.sign_in.handle = TextEdit::new(before.as_str());
    state.sign_in.password.replace_text(PASSWORD);
    state.submit();
    wait(&mut state, "the shell", |s| s.route == Route::Shell);
    wait(&mut state, "the first snapshot", |s| {
        s.session.snapshot.is_some()
    });
    assert!(!state.session.has_identity());

    // Someone else, whose handle is theirs.
    let other = scratch("rename-other");
    let taken = unique("held");
    cli(&bin, &other, &["instance", "add", &instance]);
    cli(
        &bin,
        &other,
        &[
            "account",
            "create",
            "--handle",
            &taken,
            "--password",
            PASSWORD,
        ],
    );

    state.go_home(HomeView::Profile);
    assert!(state.names_account());
    assert_eq!(state.account.profile.account.handle.text, before);
    assert_eq!(state.account.profile.account.display_name.text, "");

    state.account.profile.account.handle.replace_text(&taken);
    state.save_account();
    wait(&mut state, "the refusal", |s| {
        !s.account.profile.account.busy
    });
    assert_eq!(state.account.profile.account.error, "That handle is taken.");

    state.account.profile.account.handle.replace_text(&after);
    state
        .account
        .profile
        .account
        .display_name
        .replace_text("  Ada Lovelace ");
    state.save_account();
    wait(&mut state, "the save", |s| !s.account.profile.account.busy);
    let form = &state.account.profile.account;
    assert!(form.saved, "{}", form.error);
    assert_eq!(
        form.display_name.text, "Ada Lovelace",
        "as the instance keeps it"
    );
    assert_eq!(state.session.self_name(), "Ada Lovelace");
    assert_eq!(
        state.session.snapshot.as_ref().map(|s| s.handle.as_str()),
        Some(after.as_str())
    );

    // The handle left is held for this account: nobody else makes an account with it.
    let squatter = scratch("rename-squatter");
    cli(&bin, &squatter, &["instance", "add", &instance]);
    let (made, said) = cli_try(
        &bin,
        &squatter,
        &[
            "account",
            "create",
            "--handle",
            &before,
            "--password",
            PASSWORD,
        ],
    );
    assert!(!made, "the old handle was given away: {said}");

    // A second launch signs in with the new handle, not the old one.
    let me = state.session.me().to_string();
    state.sign_out();
    wait(&mut state, "signing out", |s| !s.session.signed_in);
    state.sign_in.creating = false;
    state.sign_in.handle = TextEdit::new(before.as_str());
    state.sign_in.password.replace_text(PASSWORD);
    state.submit();
    wait(&mut state, "the old handle's refusal", |s| {
        !s.sign_in.busy && !s.sign_in.error.is_empty()
    });
    assert_eq!(state.sign_in.error, "Wrong handle or password.");
    state.sign_in.handle = TextEdit::new(after.as_str());
    state.submit();
    wait(&mut state, "the shell again", |s| {
        s.route == Route::Shell && s.session.snapshot.is_some()
    });
    assert_eq!(state.session.me(), me);
    assert_eq!(state.session.self_name(), "Ada Lovelace");
}

/// A photo chosen on this computer: the instance makes its own picture of the file, the
/// account names it, its bytes come back and are kept on disk for the next launch, someone
/// else in a server with this account sees it, and removing it leaves none.
#[test]
fn a_photo_is_set_seen_by_someone_else_kept_for_the_next_launch_and_removed() {
    let Some((instance, _cli)) = environment() else {
        return;
    };
    let fixtures =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../instance/test/support/fixtures/media");
    let dir = scratch("photo");
    let mut state = opened(&instance, dir.clone());
    state.sign_in.creating = true;
    state.sign_in.handle = TextEdit::new(unique("pic").as_str());
    state.sign_in.password.replace_text(PASSWORD);
    state.submit();
    wait(&mut state, "the shell", |s| s.route == Route::Shell);
    wait(&mut state, "the first snapshot", |s| {
        s.session.snapshot.is_some()
    });
    let me = state.session.me().to_string();
    assert_eq!(state.session.avatar_url_of(&me), "");

    // What is not a picture is refused in words, and nothing changes.
    state.go_home(HomeView::Profile);
    state.set_photo(&fixtures.join("not-a-picture.txt"));
    wait(&mut state, "the refusal", |s| !s.account.profile.photo.busy);
    assert_eq!(
        state.account.profile.photo.error,
        "That file is not a picture Lumia can read. Choose a PNG, JPEG, WebP or GIF."
    );
    assert_eq!(state.session.avatar_url_of(&me), "");

    // A photograph: saved, named by the account, and its bytes here.
    let chosen = fixtures.join("sideways.jpg");
    state.set_photo(&chosen);
    wait(&mut state, "the photo to be saved", |s| {
        !s.account.profile.photo.busy
    });
    let form = &state.account.profile.photo;
    assert!(form.saved, "{}", form.error);
    let url = state.session.avatar_url_of(&me).to_string();
    assert!(url.starts_with("/api/v1/avatars/"), "{url}");
    wait(&mut state, "the picture's bytes", |s| {
        s.session.picture_of(&me).is_some()
    });
    let held = state
        .session
        .avatars
        .held()
        .find(|(held, _)| *held == url)
        .map(|(_, bytes)| bytes.to_vec())
        .expect("the bytes are held by URL");
    let uploaded = std::fs::read(&chosen).unwrap();
    assert!(
        held.starts_with(&[0xFF, 0xD8]),
        "a JPEG of the instance's own"
    );
    assert_ne!(held, uploaded);
    let needle = b"LumiaTestCam";
    assert!(uploaded.windows(needle.len()).any(|w| w == needle));
    assert!(
        !held.windows(needle.len()).any(|w| w == needle),
        "the camera's name did not come back"
    );
    let kept = dir
        .join(lumia_client::app::host_of(&instance))
        .join("avatars")
        .join(url.rsplit('/').next().unwrap());
    assert_eq!(
        std::fs::read(&kept).unwrap(),
        held,
        "kept for the next launch"
    );

    // Someone who shares a server with this account sees the picture on it.
    let made = unique("Photo");
    state.go_home(HomeView::AddServer);
    state.server_forms.add.name.replace_text(&made);
    state.create_server();
    wait(&mut state, "the new server", |s| s.server_id().is_some());
    state.open_invites();
    state.make_invite();
    wait(&mut state, "the invite", |s| {
        s.server_forms.invite.made.is_some()
    });
    let link = state.server_forms.invite.made.clone().unwrap().url;
    state.close_popover();
    let mut other = opened(&instance, scratch("photo-other"));
    other.sign_in.creating = true;
    other.sign_in.handle = TextEdit::new(unique("see").as_str());
    other.sign_in.password.replace_text(PASSWORD);
    other.submit();
    wait(&mut other, "the other's shell", |s| s.route == Route::Shell);
    wait(&mut other, "the other's snapshot", |s| {
        s.session.snapshot.is_some()
    });
    other.go_home(HomeView::AddServer);
    other.server_forms.add.invite.replace_text(&link);
    other.look_up_invite();
    wait(&mut other, "the invite's preview", |s| {
        s.server_forms.add.preview.is_some()
    });
    other.join_invite();
    wait(&mut other, "the picture at the other account", |s| {
        s.session.picture_of(&me).is_some()
    });
    assert_eq!(other.session.avatar_url_of(&me), url);

    // A second launch has the picture from disk before the instance is heard from.
    drop(state);
    let mut again = opened(&instance, dir);
    assert!(again.session.signed_in);
    assert_eq!(
        again.session.picture_of(&me),
        Some(url.as_str()),
        "from the directory it was kept in, with nothing drained"
    );

    // Removed: the account names none.
    wait(&mut again, "the second launch's snapshot", |s| {
        s.session.connection == Connection::Connected
    });
    again.go_home(HomeView::Profile);
    again.remove_photo();
    wait(&mut again, "the removal", |s| !s.account.profile.photo.busy);
    assert!(again.account.profile.photo.saved);
    assert_eq!(again.session.avatar_url_of(&me), "");
    assert!(again.session.picture_of(&me).is_none());
}

/// `GET path` on the instance, for the dev mailbox (plain HTTP on a dev instance).
fn http_get(instance: &str, path: &str) -> String {
    let authority = instance
        .strip_prefix("http://")
        .unwrap_or_else(|| panic!("the mailbox is read over plain http, not {instance}"))
        .trim_end_matches('/');
    let address = if authority.contains(':') {
        authority.to_string()
    } else {
        format!("{authority}:80")
    };
    let mut stream = TcpStream::connect(&address).expect("the instance answers");
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .expect("a timeout");
    write!(
        stream,
        "GET {path} HTTP/1.0\r\nHost: {authority}\r\nAccept: application/json\r\nConnection: close\r\n\r\n"
    )
    .expect("the request is sent");
    let mut answer = Vec::new();
    stream.read_to_end(&mut answer).expect("the answer is read");
    let answer = String::from_utf8_lossy(&answer).into_owned();
    let (head, body) = answer.split_once("\r\n\r\n").expect("a head and a body");
    assert!(
        head.lines().next().is_some_and(|l| l.contains(" 200")),
        "{head}"
    );
    body.to_string()
}

/// The token of the newest magic-link mail to `email` in the dev instance's local mailbox,
/// which keeps mail instead of sending it: the line of the body that is the bare code.
fn mailed_token(instance: &str, email: &str) -> Option<String> {
    let body = http_get(instance, "/dev/mailbox/json");
    let mailbox: serde_json::Value = serde_json::from_str(&body).expect("the mailbox is JSON");
    let mails = mailbox.get("data")?.as_array()?;
    let mail = mails.iter().find(|mail| {
        mail.get("to")
            .is_some_and(|to| to.to_string().contains(email))
    })?;
    mail.get("text_body")?
        .as_str()?
        .lines()
        .map(str::trim)
        .find(|line| {
            line.len() == 32
                && line
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
        })
        .map(str::to_string)
}

#[test]
fn a_magic_link_signs_in_with_the_code_from_the_mail() {
    let Some((instance, bin)) = environment() else {
        return;
    };
    let dir = scratch(&unique("magic"));
    let handle = unique("magic");
    let email = format!("{handle}@example.com");
    let holder = dir.join("cli");
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
            "--email",
            &email,
        ],
    );

    let mut state = opened(&instance, dir.join("shell"));
    assert!(
        state
            .session
            .info
            .as_ref()
            .is_some_and(|info| info.magic_link_enabled),
        "the dev instance mails sign-in links"
    );
    state.sign_in.handle.replace_text(&email);
    state.request_magic_link();
    assert!(state.sign_in.busy);
    wait(&mut state, "the instance's answer to the request", |s| {
        !s.sign_in.busy
    });
    assert_eq!(state.sign_in.error, "");
    assert!(state.sign_in.magic_sent);

    // A code that was never mailed is refused in the screen's words.
    state
        .sign_in
        .magic_code
        .replace_text("0000000000000000000000000000000_");
    state.complete_magic_link();
    wait(&mut state, "the refusal of a wrong code", |s| {
        !s.sign_in.busy
    });
    assert_eq!(
        state.sign_in.error,
        "That code is unknown, used or expired. Ask for a new link."
    );
    assert_eq!(state.route, Route::SignIn);

    let deadline = Instant::now() + Duration::from_secs(10);
    let token = loop {
        if let Some(token) = mailed_token(&instance, &email) {
            break token;
        }
        assert!(
            Instant::now() < deadline,
            "no magic-link mail to {email} in the dev mailbox"
        );
        std::thread::sleep(Duration::from_millis(250));
    };
    // Pasted as the whole link, as a person copying it from the mail might.
    state
        .sign_in
        .magic_code
        .replace_text(format!("{instance}/sign-in?magic={token}"));
    state.complete_magic_link();
    wait(&mut state, "the sign-in", |s| {
        s.route == Route::Shell || (!s.sign_in.busy && !s.sign_in.error.is_empty())
    });
    assert_eq!(state.sign_in.error, "");
    assert_eq!(state.route, Route::Shell);
    assert!(state.session.signed_in && !state.sign_in.magic_sent);
    wait(&mut state, "the snapshot", |s| s.session.snapshot.is_some());
    assert_eq!(
        state.session.snapshot.as_ref().map(|s| s.handle.as_str()),
        Some(handle.as_str())
    );

    // The token is spent: a second shell cannot use it.
    let mut second = opened(&instance, dir.join("second"));
    second.sign_in.handle.replace_text(&email);
    second.sign_in.magic_sent = true;
    second.sign_in.magic_code.replace_text(&token);
    second.complete_magic_link();
    wait(&mut second, "the refusal of a used code", |s| {
        !s.sign_in.busy
    });
    assert_eq!(
        second.sign_in.error,
        "That code is unknown, used or expired. Ask for a new link."
    );
    drop(second);
    finished(state, &holder);
}
