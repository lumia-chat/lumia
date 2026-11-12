//! Personal conversations against a real instance, with CLI devices playing the other
//! people: the shell is linked to an identity, accepts a request and creates the
//! conversation's group, sends and receives encrypted messages, then asks someone else,
//! is accepted, and does the same in a group the other side created. Runs only with
//! `LUMIA_DEV_INSTANCE` and `LUMIA_CLI` (the path of a built `lumia-cli`) set; without them
//! it passes having checked nothing, and says so.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use lumia_shell::app::{HomeView, Place, State};
use lumia_shell::session::dms::Answer;
use lumia_shell::session::identity::IdentityUpdate;
use lumia_shell::session::{Connection, Update};
use rune::TextEdit;

const PASSWORD: &str = "correct-horse-battery-staple";
const PATIENCE: Duration = Duration::from_secs(30);

/// A tag for this run's handles and directories: the instance is shared and keeps every
/// account ever made on it, and a process id comes round again.
fn unique() -> String {
    let seconds = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    format!("{}x{:x}", std::process::id(), seconds & 0xff_ffff)
}

/// Runs the CLI over `data` and returns what it printed. A run that does not end (a device
/// waiting for a welcome that never comes) fails the test instead of hanging it.
fn cli(bin: &str, data: &Path, args: &[&str]) -> String {
    let mut child = Command::new(bin)
        .arg("--data-dir")
        .arg(data)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the CLI runs");
    let deadline = Instant::now() + PATIENCE;
    loop {
        if child
            .try_wait()
            .expect("the CLI can be waited for")
            .is_some()
        {
            break;
        }
        if Instant::now() > deadline {
            let _ = child.kill();
            let out = child.wait_with_output().expect("the CLI's output");
            panic!(
                "lumia-cli {args:?} did not end:\n{}\n{}",
                String::from_utf8_lossy(&out.stdout),
                String::from_utf8_lossy(&out.stderr)
            );
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    let out = child.wait_with_output().expect("the CLI's output");
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

/// A person on a CLI device: an account, an identity it is bound to, and key packages
/// published so a group can add the device. Returns the account's id.
fn person(bin: &str, instance: &str, data: &Path, handle: &str) -> String {
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
    id_in(&cli(bin, data, &["whoami"]))
}

/// Drains the session, reacting as the window would, until `done`.
fn wait(state: &mut State, what: &str, done: impl Fn(&State) -> bool) {
    let deadline = Instant::now() + PATIENCE;
    while !done(state) {
        assert!(
            Instant::now() < deadline,
            "timed out waiting for {what}; error {:?}, connection {:?}, conversations {:?}, \
             logs {:?}, pending {:?}, note {:?}, request error {:?}",
            state.session.last_error,
            state.session.connection,
            state.session.dm.channels,
            state
                .session
                .dm
                .logs
                .iter()
                .map(|(id, log)| (id, log.messages.len(), log.before_join, log.undecryptable))
                .collect::<Vec<_>>(),
            state.session.pending,
            state.home.note,
            state.home.request_error,
        );
        state.drain();
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// Drains the session until `pick` finds its update, and returns what it made of it.
fn wait_for<T>(state: &mut State, what: &str, pick: impl Fn(&Update) -> Option<T>) -> T {
    let deadline = Instant::now() + PATIENCE;
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

/// Links the shell over `data` to the identity the CLI device `holder` holds, which signs
/// it in by identity proof.
fn linked_shell(bin: &str, instance: &str, holder: &Path, data: PathBuf) -> State {
    let mut state = State::new(data);
    state.sign_in.instance = TextEdit::new(instance);
    state.connect();
    assert!(state.session.is_open(), "{}", state.sign_in.error);
    assert!(!state.session.has_identity());
    state.session.request_link("desktop");
    let request = wait_for(&mut state, "the link request", |u| match u {
        Update::Identity(IdentityUpdate::LinkRequested(r)) => Some(r.clone()),
        _ => None,
    })
    .expect("a link request");
    let approved = cli(bin, holder, &["identity", "approve", request.trim()]);
    let code = approved
        .split_whitespace()
        .rev()
        .find(|word| word.len() == 8 && word.chars().all(|c| c.is_ascii_alphanumeric()))
        .unwrap_or_else(|| panic!("no claim code in: {approved}"));
    state.session.claim_link(code);
    wait(&mut state, "the claim and the first snapshot", |s| {
        s.session.has_identity() && s.session.signed_in && s.session.snapshot.is_some()
    });
    wait(&mut state, "the gateway", |s| {
        s.session.connection == Connection::Connected
    });
    state
}

/// Sends from the shell's open conversation and waits for the log to hold it.
fn say(state: &mut State, body: &str) {
    let before = state.session.entries.len();
    assert!(
        state
            .session
            .send(body, chrono::Utc::now().timestamp_micros()),
        "{:?}",
        state.session.last_error
    );
    let pending: Vec<_> = state.session.pending_here().collect();
    assert_eq!(pending.len(), 1, "it is pending at once");
    assert_eq!(pending[0].body, body);
    wait(state, "the send to be delivered", |s| {
        s.session.pending.is_empty()
    });
    let me = state.session.me().to_string();
    let entries = &state.session.entries;
    assert_eq!(entries.len(), before + 1, "{entries:?}");
    let last = entries.last().unwrap();
    assert_eq!(
        (last.body.as_str(), last.author_account_id.as_str()),
        (body, me.as_str())
    );
}

/// The CLI device in `data` says `body` in `channel`; the shell, with that conversation
/// open, reads it as coming from `author`.
fn hear(state: &mut State, bin: &str, data: &Path, channel: &str, body: &str, author: &str) {
    let sent = cli(bin, data, &["dm", "send", channel, body]);
    assert!(sent.contains("delivered"), "{sent}");
    wait(state, "the other side's message", |s| {
        s.session.entries.iter().any(|e| e.body == body)
    });
    let entry = state
        .session
        .entries
        .iter()
        .find(|e| e.body == body)
        .unwrap();
    assert_eq!(entry.author_account_id, author);
    assert!(
        state.session.pending.is_empty(),
        "someone else's message settles no send of ours"
    );
}

#[test]
fn the_shell_exchanges_requests_and_encrypted_messages_with_cli_devices() {
    let (Ok(instance), Ok(bin)) = (
        std::env::var("LUMIA_DEV_INSTANCE"),
        std::env::var("LUMIA_CLI"),
    ) else {
        eprintln!("skipped: no LUMIA_DEV_INSTANCE and LUMIA_CLI");
        return;
    };
    let pid = unique();
    let dir = std::env::temp_dir().join(format!("lumia-shell-dm-{pid}"));
    let _ = std::fs::remove_dir_all(&dir);
    let (holder, asker, asked, shell) = (
        dir.join("holder"),
        dir.join("asker"),
        dir.join("asked"),
        dir.join("shell"),
    );

    // This person's first device, which holds the identity; and two other people.
    let me = person(&bin, &instance, &holder, &format!("dmme{pid}"));
    let asker_id = person(&bin, &instance, &asker, &format!("dmasker{pid}"));
    let asked_id = person(&bin, &instance, &asked, &format!("dmasked{pid}"));

    let mut state = linked_shell(&bin, &instance, &holder, shell.clone());
    assert_eq!(state.session.me(), me);
    assert_eq!(state.place, Place::Home);
    assert!(state.session.personal_channels().next().is_none());

    // ---- Someone asks; the shell accepts, which makes it the group's creator -----------
    let request = id_in(&cli(&bin, &asker, &["dm", "request", &me]));
    wait(&mut state, "the request to show", |s| {
        s.session.snapshot.as_ref().is_some_and(|snap| {
            snap.pending_requests
                .iter()
                .any(|r| r.request_id == request)
        })
    });
    let waiting = &state.session.snapshot.as_ref().unwrap().pending_requests;
    assert_eq!(waiting[0].from_account_id, asker_id);
    wait(&mut state, "the asker's profile", |s| {
        s.session.person(&asker_id).is_some()
    });
    assert_eq!(
        state.session.person(&asker_id).unwrap().handle,
        format!("dmasker{pid}")
    );

    state.go_home(HomeView::Requests);
    state.answer_request(&request, Answer::Accept);
    wait(&mut state, "the accepted conversation to open", |s| {
        s.session.dm().is_some() && s.home.answering.is_none()
    });
    assert_eq!(state.home_view, HomeView::Conversations);
    assert_eq!(state.home.request_error, "");
    let with_asker = state.session.dm().unwrap().to_string();
    let listed = state
        .session
        .channel_summary(&with_asker)
        .expect("the snapshot that came with the answer lists the conversation");
    assert_eq!(listed.kind, "PERSONAL_DM");
    assert!(listed.participant_account_ids.contains(&asker_id));
    assert!(
        state
            .session
            .snapshot
            .as_ref()
            .unwrap()
            .pending_requests
            .is_empty()
    );
    wait(&mut state, "the group this device created", |s| {
        s.session.dm_member(&with_asker) == Some(true)
    });

    // The asker's device takes the welcome.
    let joined = cli(&bin, &asker, &["dm", "open", &me]);
    assert!(
        joined.contains("group ready") || joined.contains("already a member"),
        "{joined}"
    );

    say(&mut state, "From the shell, **encrypted**.");
    hear(
        &mut state,
        &bin,
        &asker,
        &with_asker,
        "From the CLI, to the shell.",
        &asker_id,
    );
    // The CLI device decrypted what the shell sent.
    let read = cli(&bin, &asker, &["dm", "messages", &with_asker]);
    assert!(read.contains("From the shell, **encrypted**."), "{read}");
    say(&mut state, "And again.");
    assert_eq!(state.session.entries.len(), 3);

    // ---- The shell asks someone; they accept, which makes their device the creator -----
    state.go_home(HomeView::NewMessage);
    state.start_dm(&asked_id);
    wait(&mut state, "the request to go out", |s| {
        s.home.starting.is_none()
    });
    assert!(
        state.home.note.starts_with("Request sent to ")
            && state
                .home
                .note
                .ends_with("The conversation opens when they accept."),
        "{}",
        state.home.note
    );
    assert_eq!(state.home_view, HomeView::NewMessage);
    // Asking twice makes no second request: the instance answers with the one that waits.
    state.start_dm(&asked_id);
    wait(&mut state, "the second ask to be answered", |s| {
        s.home.starting.is_none()
    });
    assert!(
        state.home.note.starts_with("Request sent to "),
        "{}",
        state.home.note
    );
    let waiting = cli(&bin, &asked, &["dm", "requests"]);
    assert_eq!(waiting.lines().count(), 1, "{waiting}");

    let theirs = id_in(&waiting);
    let accepted = cli(&bin, &asked, &["dm", "respond", &theirs, "accept"]);
    assert!(accepted.contains("group ready"), "{accepted}");
    // The conversation appears by itself, and this device is welcomed into its group.
    wait(&mut state, "the conversation the other side opened", |s| {
        s.session
            .personal_channels()
            .any(|c| c.participant_account_ids.contains(&asked_id))
    });
    let with_asked = state
        .session
        .personal_channels()
        .find(|c| c.participant_account_ids.contains(&asked_id))
        .unwrap()
        .channel_id
        .clone();
    wait(&mut state, "the welcome into their group", |s| {
        s.session.dm_member(&with_asked) == Some(true)
    });

    // New message on someone there is a conversation with opens it.
    state.start_dm(&asked_id);
    wait(&mut state, "the conversation to open", |s| {
        s.session.dm() == Some(with_asked.as_str())
    });
    assert_eq!(state.home_view, HomeView::Conversations);
    assert!(state.session.entries.is_empty(), "nothing was said yet");
    hear(
        &mut state,
        &bin,
        &asked,
        &with_asked,
        "You asked; here I am.",
        &asked_id,
    );
    say(&mut state, "Thank you for accepting.");
    let read = cli(&bin, &asked, &["dm", "send", &with_asked, "Received."]);
    assert!(read.contains("Thank you for accepting."), "{read}");
    wait(&mut state, "their answer", |s| s.session.entries.len() == 3);

    // Going back to the first conversation shows it as it was left, at once.
    state.open_dm(&with_asker);
    assert_eq!(
        state.session.entries.len(),
        3,
        "read from what was last read"
    );

    // ---- A second launch reads both conversations from the device's own store ----------
    drop(state);
    let mut again = State::new(shell);
    again.connect();
    assert!(again.session.signed_in && again.session.has_identity());
    again.session.load();
    wait(&mut again, "the second launch's conversations", |s| {
        s.session.personal_channels().count() == 2
            && s.session.dm_member(&with_asker) == Some(true)
            && s.session.dm_member(&with_asked) == Some(true)
    });
    assert_eq!(
        again.place,
        Place::Home,
        "an account with conversations lands on Home"
    );
    again.open_dm(&with_asked);
    wait(&mut again, "the stored conversation", |s| {
        s.session.entries.len() == 3
    });
    let bodies: Vec<&str> = again
        .session
        .entries
        .iter()
        .map(|e| e.body.as_str())
        .collect();
    assert_eq!(
        bodies,
        [
            "You asked; here I am.",
            "Thank you for accepting.",
            "Received."
        ]
    );
    let log = again.session.dm_log(&with_asked).unwrap();
    assert_eq!((log.before_join, log.undecryptable), (0, 0));
}

/// The order a person meets: signed in with a password on a computer that holds no
/// identity, told to link it, linked while the gateway is already connected, and then in a
/// conversation someone else's device creates: welcomed into its group, or, as things are,
/// left out of it and rejoining.
#[test]
fn a_shell_linked_after_signing_in_gets_into_a_conversation_someone_else_created() {
    let (Ok(instance), Ok(bin)) = (
        std::env::var("LUMIA_DEV_INSTANCE"),
        std::env::var("LUMIA_CLI"),
    ) else {
        eprintln!("skipped: no LUMIA_DEV_INSTANCE and LUMIA_CLI");
        return;
    };
    let pid = unique();
    let dir = std::env::temp_dir().join(format!("lumia-shell-dm-late-{pid}"));
    let _ = std::fs::remove_dir_all(&dir);
    let (holder, other, shell) = (dir.join("holder"), dir.join("other"), dir.join("shell"));
    let handle = format!("dmlate{pid}");
    let me = person(&bin, &instance, &holder, &handle);
    let other_id = person(&bin, &instance, &other, &format!("dmother{pid}"));

    let mut state = State::new(shell);
    state.sign_in.instance = TextEdit::new(instance.as_str());
    state.connect();
    wait(&mut state, "the trust card", |s| s.session.info.is_some());
    state.sign_in.handle = TextEdit::new(handle.as_str());
    state.sign_in.password.replace_text(PASSWORD);
    state.submit();
    wait(&mut state, "the gateway", |s| {
        s.session.connection == Connection::Connected && s.session.snapshot.is_some()
    });
    assert_eq!(state.session.me(), me);
    assert!(!state.session.has_identity(), "nothing is linked yet");

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
    // The claim's own answer, not `has_identity`: that is true from the moment the roster
    // is stored, while the claim still holds the identity's stores.
    wait_for(&mut state, "the claim", |u| match u {
        Update::Identity(IdentityUpdate::Linked(r)) => Some(r.clone()),
        _ => None,
    })
    .expect("linked");
    assert!(state.session.has_identity());

    // This person asks; the other accepts, so the other's device creates the group and
    // has to find a key package of this computer to add it.
    state.go_home(HomeView::NewMessage);
    state.start_dm(&other_id);
    wait(&mut state, "the request to go out", |s| {
        s.home.starting.is_none()
    });
    assert!(
        state.home.note.starts_with("Request sent to "),
        "{}",
        state.home.note
    );
    let theirs = id_in(&cli(&bin, &other, &["dm", "requests"]));
    let accepted = cli(&bin, &other, &["dm", "respond", &theirs, "accept"]);
    assert!(accepted.contains("group ready"), "{accepted}");
    wait(&mut state, "the conversation the other side opened", |s| {
        s.session
            .personal_channels()
            .any(|c| c.participant_account_ids.contains(&other_id))
    });
    let channel = state
        .session
        .personal_channels()
        .find(|c| c.participant_account_ids.contains(&other_id))
        .unwrap()
        .channel_id
        .clone();
    state.open_dm(&channel);
    wait(&mut state, "the engine to know the conversation", |s| {
        s.session.dm_member(&channel).is_some()
    });

    // A device linked while its gateway was already connected has published no key
    // packages (the core publishes them on a welcome, and the engine did not exist at the
    // last one), so the creator could not add it: `created group ... with 2 leaves`, the
    // creator's and the first device's. Until the core publishes them when the engine
    // starts, the way in is the one the screen offers: Rejoin.
    let deadline = Instant::now() + Duration::from_secs(3);
    while Instant::now() < deadline && state.session.dm_member(&channel) != Some(true) {
        state.drain();
        std::thread::sleep(Duration::from_millis(20));
    }
    if state.session.dm_member(&channel) == Some(true) {
        eprintln!("welcomed by the creator:\n{accepted}");
    } else {
        eprintln!("not welcomed, rejoining; the creator said:\n{accepted}");
        assert!(
            !state.session.send("Too early.", 0),
            "nothing is sent from outside the group"
        );
        state.rejoin_dm();
        assert!(state.home.rejoining);
        wait(&mut state, "the rejoin", |s| {
            !s.home.rejoining && s.session.dm_member(&channel) == Some(true)
        });
        assert_eq!(state.home.rejoin_error, "");
        state.session.last_error = None;

        // The CLI's `dm send` encrypts as it connects, before it has read the rejoin's
        // commit: this message is at the epoch this device was never in. The log counts
        // it as sent before the join, which the screen says above the messages.
        cli(
            &bin,
            &other,
            &["dm", "send", &channel, "Across the rejoin."],
        );
        wait(&mut state, "the message from before the join", |s| {
            s.session
                .dm_log(&channel)
                .is_some_and(|log| log.before_join == 1)
        });
        assert!(state.session.entries.is_empty());
    }

    say(&mut state, "Linked late, and here.");
    hear(&mut state, &bin, &other, &channel, "And heard.", &other_id);
    let read = cli(&bin, &other, &["dm", "messages", &channel]);
    assert!(read.contains("Linked late, and here."), "{read}");
}
