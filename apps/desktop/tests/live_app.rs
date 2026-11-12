//! The bundled app, launched for real: what only a real launch shows. One running copy
//! however it is started again, a `lumia:` link handed over by the system (an Apple event)
//! and by a second launch (its argument, over the socket) signing the app in, the right to
//! post notifications that only a bundle has, and the window's frame saved.
//!
//! It opens a window on this Mac for a few seconds, twice, behind whatever is in front.
//! Runs only on macOS with `LUMIA_DEV_APP` (a **debug** `Lumia.app` from
//! `scripts/bundle-macos.sh --debug`; a debug build honours the `LUMIA_DEV_BROWSER` file
//! this drives it with) and `LUMIA_DEV_INSTANCE` set; without them it passes having
//! checked nothing, and says so. No browser is driven: the link is the one the instance's
//! page would send the browser back on.

#![cfg(target_os = "macos")]
#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use lumia_shell::app::{Route, State};
use lumia_shell::settings::Settings;

const PASSWORD: &str = "correct-horse-battery-staple";

fn wait(state: &mut State, what: &str, done: impl Fn(&State) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(15);
    while !done(state) {
        assert!(
            Instant::now() < deadline,
            "timed out waiting for {what}; error {:?}, sign-in {:?}",
            state.session.last_error,
            state.sign_in.error
        );
        state.drain();
        std::thread::sleep(Duration::from_millis(20));
    }
}

fn until(what: &str, time: Duration, mut done: impl FnMut() -> bool) {
    let deadline = Instant::now() + time;
    while !done() {
        assert!(Instant::now() < deadline, "timed out waiting for {what}");
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// The processes running the bundle's binary.
fn running(exe: &Path) -> Vec<String> {
    let out = Command::new("pgrep")
        .arg("-f")
        .arg(exe)
        .output()
        .expect("pgrep runs");
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .map(str::to_string)
        .collect()
}

fn quit(exe: &Path) {
    for pid in running(exe) {
        let _ = Command::new("kill").arg(pid).status();
    }
    until("the app to go", Duration::from_secs(10), || {
        running(exe).is_empty()
    });
}

/// Opens the bundle as Finder would, behind the app in front, as a new copy, with
/// `environment` set for it.
fn open(app: &Path, environment: &[(&str, &Path)]) {
    let mut open = Command::new("open");
    open.args(["-n", "-g"]);
    for (name, value) in environment {
        open.arg("--env").arg(format!("{name}={}", value.display()));
    }
    assert!(open.arg(app).status().expect("open runs").success());
}

/// A data directory that remembers `instance`, as one the app was used on.
fn data_dir(dir: &Path, instance: &str) -> PathBuf {
    std::fs::create_dir_all(dir).unwrap();
    Settings {
        instance: Some(instance.into()),
        ..Settings::default()
    }
    .save(dir);
    dir.to_path_buf()
}

/// What a query field of `url` holds, percent-decoded.
fn field(url: &str, name: &str) -> String {
    let (_, query) = url.split_once('?').expect("a query");
    let raw = query
        .split('&')
        .find_map(|pair| pair.strip_prefix(&format!("{name}=")))
        .unwrap_or_else(|| panic!("no {name} in {url}"));
    let mut out = Vec::new();
    let mut bytes = raw.bytes();
    while let Some(byte) = bytes.next() {
        if byte == b'%' {
            let hex = [bytes.next().unwrap(), bytes.next().unwrap()];
            out.push(u8::from_str_radix(std::str::from_utf8(&hex).unwrap(), 16).unwrap());
        } else {
            out.push(byte);
        }
    }
    String::from_utf8(out).unwrap()
}

/// A one-time sign-in token for `signed_in`'s account, from the instance.
fn token(signed_in: &mut State) -> String {
    signed_in.open_web_signed_in("devices");
    wait(signed_in, "the handoff page", |s| !s.links.handing_off);
    let page = signed_in.links.to_open.pop().expect("a page to open");
    field(&page, "magic")
}

/// Starts the app over `data` with a browser sign-in under way, and returns the link the
/// browser would come back on with `token`.
fn started(app: &Path, exe: &Path, data: &Path, scratch: &Path, token: &str) -> String {
    let (page, trace) = (scratch.join("page.txt"), scratch.join("trace.txt"));
    let _ = std::fs::remove_file(&page);
    open(
        app,
        &[
            ("LUMIA_DATA_DIR", data),
            ("LUMIA_DEV_BROWSER", &page),
            ("LUMIA_DEV_TRACE", &trace),
        ],
    );
    until("the app to start", Duration::from_secs(30), || {
        std::fs::read_to_string(&page).is_ok_and(|text| !text.is_empty())
    });
    assert_eq!(running(exe).len(), 1);
    let page = std::fs::read_to_string(&page).unwrap();
    format!("{}&magic={token}", field(&page, "return"))
}

/// Whether the stores under `data` hold a session for `account`, once the app is gone.
fn signed_in_as(data: &Path, instance: &str, account: &str) -> bool {
    let mut state = State::new(data.to_path_buf());
    state.sign_in.instance.replace_text(instance);
    state.connect();
    if !state.session.signed_in {
        return false;
    }
    state.session.load();
    wait(&mut state, "the app's account", |s| {
        s.session.snapshot.is_some()
    });
    state.session.me() == account
}

#[test]
fn the_bundled_app_is_one_copy_and_takes_its_links_from_the_system() {
    let (Ok(app), Ok(instance)) = (
        std::env::var("LUMIA_DEV_APP"),
        std::env::var("LUMIA_DEV_INSTANCE"),
    ) else {
        eprintln!("skipped: no LUMIA_DEV_APP and LUMIA_DEV_INSTANCE");
        return;
    };
    let app = PathBuf::from(app).canonicalize().expect("the bundle");
    let exe = app.join("Contents/MacOS/lumia");
    let instance = instance.trim_end_matches('/').to_string();
    assert!(
        running(&exe).is_empty(),
        "this bundle is already running; quit it first"
    );
    let scratch = std::env::temp_dir().join(format!("lumia-app-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&scratch);
    std::fs::create_dir_all(&scratch).unwrap();

    // An account to sign the app in to, held by a shell in this process.
    let mut one = State::new(scratch.join("one"));
    one.sign_in.instance.replace_text(&instance);
    one.connect();
    wait(&mut one, "the trust card", |s| s.session.info.is_some());
    one.sign_in.creating = true;
    one.sign_in.handle.replace_text(format!(
        "app{}{}",
        std::process::id(),
        chrono::Utc::now().timestamp_subsec_millis()
    ));
    one.sign_in.password.replace_text(PASSWORD);
    one.submit();
    wait(&mut one, "the new account", |s| {
        s.route == Route::Shell && s.session.snapshot.is_some()
    });
    let account = one.session.me().to_string();

    // ---- The system hands the running app a link (an Apple event) ----------------------
    let data = data_dir(&scratch.join("first"), &instance);
    let link = started(&app, &exe, &data, &scratch, &token(&mut one));

    // Only a bundle may post notifications, and this is one.
    let trace = std::fs::read_to_string(scratch.join("trace.txt")).unwrap_or_default();
    assert!(trace.contains("notifications can_post=true"), "{trace}");

    // Opened again as a new copy, run by hand, and run by hand with nothing to say: still
    // one process on this data directory.
    open(&app, &[("LUMIA_DATA_DIR", &data)]);
    let by_hand = Command::new(&exe)
        .env("LUMIA_DATA_DIR", &data)
        .output()
        .expect("the binary runs");
    assert!(
        by_hand.status.success(),
        "a second launch hands over and goes: {}",
        String::from_utf8_lossy(&by_hand.stderr)
    );
    std::thread::sleep(Duration::from_secs(2));
    assert_eq!(running(&exe).len(), 1, "one copy");

    // A headless run is refused the directory, in words.
    let headless = Command::new(&exe)
        .env("LUMIA_DATA_DIR", &data)
        .arg("--snap")
        .arg(scratch.join("refused.png"))
        .output()
        .expect("the binary runs");
    assert!(!headless.status.success());
    assert!(
        String::from_utf8_lossy(&headless.stderr).contains("LUMIA_DATA_DIR"),
        "{}",
        String::from_utf8_lossy(&headless.stderr)
    );

    // The link, opened with the app as a browser's redirect opens it.
    let opened = Command::new("open")
        .arg("-g")
        .arg("-a")
        .arg(&app)
        .arg(&link)
        .status()
        .expect("open runs");
    assert!(opened.success());
    std::thread::sleep(Duration::from_secs(4));
    quit(&exe);
    assert!(
        signed_in_as(&data, &instance, &account),
        "the link the system handed over signed the app in"
    );

    // ---- A second launch hands the running app its link (the argument, over the socket) -
    let data = data_dir(&scratch.join("second"), &instance);
    let link = started(&app, &exe, &data, &scratch, &token(&mut one));
    let second = Command::new(&exe)
        .env("LUMIA_DATA_DIR", &data)
        .arg(&link)
        .output()
        .expect("the binary runs");
    assert!(second.status.success());
    std::thread::sleep(Duration::from_secs(4));
    assert_eq!(running(&exe).len(), 1, "one copy");
    quit(&exe);
    assert!(
        signed_in_as(&data, &instance, &account),
        "the link a second launch handed over signed the app in"
    );

    // ---- The window's frame was saved for the next launch -------------------------------
    let id = Command::new("plutil")
        .args(["-extract", "CFBundleIdentifier", "raw"])
        .arg(app.join("Contents/Info.plist"))
        .output()
        .expect("plutil runs");
    let id = String::from_utf8_lossy(&id.stdout).trim().to_string();
    let frame = Command::new("defaults")
        .args(["read", &id, "NSWindow Frame Lumia"])
        .output()
        .expect("defaults runs");
    let frame = String::from_utf8_lossy(&frame.stdout).trim().to_string();
    assert_eq!(
        frame.split_whitespace().count(),
        8,
        "AppKit's frame string for {id}: {frame:?}"
    );

    one.sign_out();
    let _ = std::fs::remove_dir_all(&scratch);
}
