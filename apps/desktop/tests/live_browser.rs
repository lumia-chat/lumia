//! The browser handoff against a real instance (architecture 6.3), in both directions:
//! a signed-in shell asks the instance for a one-time token and gets the page that opens
//! the web app as itself, and a fresh shell that started a sign-in through the browser is
//! handed the `lumia:` link a browser would come back on, with that token, and ends up
//! signed in as the same account. The token works once.
//!
//! No browser is driven: the link is built as the instance's page builds it (the return
//! address the shell gave, with `magic={token}` appended). Runs only with
//! `LUMIA_DEV_INSTANCE` set; without it it passes having checked nothing, and says so.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::path::PathBuf;
use std::time::{Duration, Instant};

use lumia_shell::app::{Route, State};

const PASSWORD: &str = "correct-horse-battery-staple";

fn wait(state: &mut State, what: &str, done: impl Fn(&State) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(15);
    while !done(state) {
        assert!(
            Instant::now() < deadline,
            "timed out waiting for {what}; error {:?}, sign-in {:?}, handoff {:?}",
            state.session.last_error,
            state.sign_in.error,
            state.links.handoff_error
        );
        state.drain();
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// A shell over a directory of its own with the instance open and its trust card in.
fn opened(dir: PathBuf, instance: &str) -> State {
    let mut state = State::new(dir);
    state.sign_in.instance.replace_text(instance);
    state.connect();
    wait(&mut state, "the trust card", |s| s.session.info.is_some());
    state
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

/// The link the instance's page sends the browser back on: the return address it was
/// given, with the token appended (`apps/web/src/routes/sign-in.tsx`, `land`).
fn link_back(sign_in_page: &str, token: &str) -> String {
    let back = field(sign_in_page, "return");
    let joint = if back.contains('?') { '&' } else { '?' };
    format!("{back}{joint}magic={token}")
}

#[test]
fn a_token_minted_by_one_shell_signs_another_in_through_its_link() {
    let Ok(instance) = std::env::var("LUMIA_DEV_INSTANCE") else {
        eprintln!("skipped: no LUMIA_DEV_INSTANCE");
        return;
    };
    let instance = instance.trim_end_matches('/').to_string();
    let dir = std::env::temp_dir().join(format!("lumia-shell-browser-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let handle = format!(
        "brow{}{}",
        std::process::id(),
        chrono::Utc::now().timestamp_subsec_millis()
    );

    // One shell, signed in to an account of its own.
    let mut one = opened(dir.join("one"), &instance);
    one.sign_in.creating = true;
    one.sign_in.handle.replace_text(&handle);
    one.sign_in.password.replace_text(PASSWORD);
    one.submit();
    wait(&mut one, "the new account", |s| s.route == Route::Shell);
    wait(&mut one, "its snapshot", |s| s.session.snapshot.is_some());
    let account = one.session.me().to_string();

    // It opens the web app as itself: the page carries a one-time token and where to land.
    one.open_web_signed_in("devices");
    assert!(one.links.handing_off);
    wait(&mut one, "the handoff page", |s| !s.links.handing_off);
    assert_eq!(one.links.handoff_error, "");
    let page = one.links.to_open.pop().expect("a page to open");
    assert!(
        page.starts_with(&format!("{instance}/sign-in?magic=")),
        "{page}"
    );
    assert_eq!(field(&page, "next"), "devices");
    let token = field(&page, "magic");
    assert!(token.len() >= 20, "a real token: {token}");

    // A fresh shell starts a sign-in through the browser and is handed the link the
    // browser would come back on.
    let mut two = opened(dir.join("two"), &instance);
    let sign_in_page = two
        .begin_browser_sign_in(false)
        .expect("a page to send the browser to");
    assert!(
        sign_in_page.starts_with(&format!("{instance}/sign-in?return=")),
        "{sign_in_page}"
    );
    let link = link_back(&sign_in_page, &token);
    assert!(
        link.starts_with("lumia://sign-in?state=") && link.contains("&magic="),
        "{link}"
    );
    assert!(two.open_url(&link), "the link is the app's");
    wait(&mut two, "the sign-in the link carries", |s| {
        s.route == Route::Shell
    });
    wait(&mut two, "the second shell's snapshot", |s| {
        s.session.snapshot.is_some()
    });
    assert!(two.session.signed_in);
    assert_eq!(two.session.me(), account, "signed in as the same account");
    assert_eq!(
        two.session.snapshot.as_ref().map(|s| s.handle.as_str()),
        Some(handle.as_str())
    );

    // The token is spent: a third shell that is handed it is refused in words.
    let mut three = opened(dir.join("three"), &instance);
    let sign_in_page = three.begin_browser_sign_in(false).unwrap();
    assert!(three.open_url(&link_back(&sign_in_page, &token)));
    wait(&mut three, "the refusal", |s| !s.sign_in.busy);
    assert_eq!(three.route, Route::SignIn);
    assert_eq!(three.sign_in.error, "That sign-in was not accepted.");

    // And a link nobody here asked for is not sent to the instance at all.
    let mut four = opened(dir.join("four"), &instance);
    let forged = format!("lumia://sign-in?state={}&magic={token}", "0".repeat(32));
    assert!(four.open_url(&forged));
    assert!(!four.sign_in.busy && four.route == Route::SignIn);

    one.sign_out();
    two.sign_out();
    wait(&mut one, "the first sign-out", |s| !s.session.signed_in);
    let _ = std::fs::remove_dir_all(&dir);
}
