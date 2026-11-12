//! A notice for a personal message against a real instance (architecture 15.5): the shell
//! is a second device of an identity, someone else writes to that identity in an
//! end-to-end encrypted conversation, and the shell, not in front, owes the system a
//! notice with the sender's name and the message; a click on it opens the conversation;
//! in front, nothing is owed. Runs only with `LUMIA_DEV_INSTANCE` and `LUMIA_CLI` (the
//! path of a built `lumia-cli`) set; without them it passes having checked nothing, and
//! says so.
//!
//! Nothing is posted to the system here: only a bundled app can post.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use lumia_shell::app::{HomeView, Place, Route, State};
use lumia_shell::session::identity::IdentityUpdate;
use lumia_shell::session::{Connection, Conversation, Update};

const PASSWORD: &str = "correct-horse-battery-staple";

/// Runs the CLI over `data` and returns what it printed; it has half a minute.
fn cli(bin: &str, data: &Path, args: &[&str]) -> String {
    let mut child = Command::new(bin)
        .arg("--data-dir")
        .arg(data)
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the CLI runs");
    let deadline = Instant::now() + Duration::from_secs(30);
    while child.try_wait().expect("the CLI's status").is_none() {
        if Instant::now() > deadline {
            let _ = child.kill();
            panic!("lumia-cli {args:?} did not finish");
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    let out = child.wait_with_output().expect("the CLI's output");
    assert!(
        out.status.success(),
        "lumia-cli {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).into_owned()
}

/// The first UUID in `text`.
fn uuid_in(text: &str) -> String {
    text.split(|c: char| !(c.is_ascii_hexdigit() || c == '-'))
        .find(|word| word.len() == 36 && word.matches('-').count() == 4)
        .unwrap_or_else(|| panic!("no id in: {text}"))
        .to_string()
}

/// An account with an identity on a CLI device, its key packages published.
fn account(bin: &str, data: &Path, instance: &str, handle: &str) -> String {
    cli(bin, data, &["instance", "add", instance]);
    cli(
        bin,
        data,
        &[
            "account",
            "create",
            "--handle",
            handle,
            "--password",
            PASSWORD,
        ],
    );
    cli(bin, data, &["identity", "create"]);
    cli(
        bin,
        data,
        &["identity", "link-account", "--password", PASSWORD],
    );
    cli(bin, data, &["dm", "setup"]);
    cli(bin, data, &["whoami"]).trim().to_string()
}

fn wait(state: &mut State, what: &str, done: impl Fn(&State) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(20);
    while !done(state) {
        assert!(
            Instant::now() < deadline,
            "timed out waiting for {what}; error {:?}",
            state.session.last_error
        );
        state.drain();
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// Lets the session work for a while with nothing in particular to wait for.
fn idle(state: &mut State, time: Duration) {
    let until = Instant::now() + time;
    while Instant::now() < until {
        state.drain();
        std::thread::sleep(Duration::from_millis(20));
    }
}

#[test]
fn a_personal_message_is_owed_a_notice_and_its_click_opens_the_conversation() {
    let (Ok(instance), Ok(bin)) = (
        std::env::var("LUMIA_DEV_INSTANCE"),
        std::env::var("LUMIA_CLI"),
    ) else {
        eprintln!("skipped: no LUMIA_DEV_INSTANCE and LUMIA_CLI");
        return;
    };
    let dir = std::env::temp_dir().join(format!("lumia-shell-notice-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let (holder, other) = (dir.join("holder"), dir.join("other"));
    let stamp = format!(
        "{}{}",
        std::process::id(),
        chrono::Utc::now().timestamp_subsec_millis()
    );
    let (mine, theirs) = (format!("ntme{stamp}"), format!("ntyou{stamp}"));

    // The account's first device, a CLI one that holds the identity.
    account(&bin, &holder, &instance, &mine);

    // The shell becomes its second device (identity spec 4.2) and is signed in by proof.
    let mut state = State::new(dir.join("shell"));
    state.sign_in.instance.replace_text(&instance);
    state.connect();
    assert!(state.session.is_open(), "{}", state.sign_in.error);
    state.session.request_link("desktop");
    let request = loop {
        let found = state.session.poll().into_iter().find_map(|u| match u {
            Update::Identity(IdentityUpdate::LinkRequested(r)) => Some(r),
            _ => None,
        });
        if let Some(request) = found {
            break request.expect("a link request");
        }
        std::thread::sleep(Duration::from_millis(20));
    };
    let approved = cli(&bin, &holder, &["identity", "approve", request.trim()]);
    let code = approved
        .split_whitespace()
        .rev()
        .find(|word| word.len() == 8 && word.chars().all(|c| c.is_ascii_alphanumeric()))
        .unwrap_or_else(|| panic!("no claim code in: {approved}"));
    state.session.claim_link(code);
    wait(&mut state, "the link", |s| s.session.signed_in);
    // The linking screen is another part of the shell; it is what would go to the shell.
    state.route = Route::Shell;
    wait(&mut state, "the gateway", |s| {
        s.session.connection == Connection::Connected && s.session.snapshot.is_some()
    });
    // Its key packages are published on connect; the group below is made with a leaf for it.
    idle(&mut state, Duration::from_secs(2));

    // Someone else asks for a conversation, and the first device accepts, which makes it.
    let them = account(&bin, &other, &instance, &theirs);
    let me = state.session.me().to_string();
    cli(&bin, &other, &["dm", "request", &me]);
    let request = uuid_in(&cli(&bin, &holder, &["dm", "requests"]));
    let accepted = cli(&bin, &holder, &["dm", "respond", &request, "accept"]);
    let channel = uuid_in(
        accepted
            .lines()
            .find(|line| line.contains("personal DM"))
            .unwrap_or_else(|| panic!("no conversation in: {accepted}")),
    );
    wait(&mut state, "the conversation in the snapshot", |s| {
        s.session.channel_summary(&channel).is_some()
    });
    // Their device joins from its welcome by following the conversation for a moment.
    let mut tail = Command::new(&bin)
        .arg("--data-dir")
        .arg(&other)
        .args(["dm", "tail", &channel])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("the CLI runs");
    idle(&mut state, Duration::from_secs(3));
    let _ = tail.kill();
    let _ = tail.wait();

    // The window is not in front. They write.
    state.window_focused(false);
    cli(
        &bin,
        &other,
        &["dm", "send", &channel, "Are you **there**?"],
    );
    wait(&mut state, "the notice", |s| !s.notices.to_post.is_empty());
    let notice = state.notices.to_post.remove(0);
    assert_eq!(notice.channel_id, channel);
    assert_eq!(notice.title, theirs, "their handle; they set no name");
    assert_eq!(notice.body, "Are you **there**?");
    idle(&mut state, Duration::from_millis(500));
    assert!(state.notices.to_post.is_empty(), "told once");

    // A click on it opens the conversation at Home.
    state.open_notice(&channel);
    assert_eq!(state.place, Place::Home);
    assert_eq!(state.home_view, HomeView::Conversations);
    assert_eq!(
        state.session.conversation,
        Some(Conversation::Dm(channel.clone()))
    );

    // In front, the next message is theirs to read, not the system's to announce.
    state.window_focused(true);
    cli(&bin, &other, &["dm", "send", &channel, "Good."]);
    idle(&mut state, Duration::from_secs(2));
    assert!(state.notices.to_post.is_empty());

    // This account's own message from its other device is nobody's news.
    state.window_focused(false);
    cli(&bin, &holder, &["dm", "send", &channel, "Here."]);
    idle(&mut state, Duration::from_secs(2));
    assert!(
        state.notices.to_post.is_empty(),
        "{:?}",
        state.notices.to_post
    );

    let _ = them;
    let _ = std::fs::remove_dir_all(&dir);
}
