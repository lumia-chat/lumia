//! The system browser and the `lumia:` links it comes back on (architecture 6.3): the
//! sign-in card's buttons per instance capability, what a link opened with the app does in
//! each state, and the page a signed-in app opens as itself. No instance and no browser:
//! Rune's windowless runtime keeps the pages a handler asks to open
//! (`Runtime::take_url_requests`), and `State::receive` stands in for the instance.

#![allow(clippy::expect_used, clippy::unwrap_used)]

mod common;

use std::time::{Duration, Instant};

use common::*;
use lumia_client::app::InstanceSummary;
use lumia_shell::app::links::{self, Link};
use lumia_shell::app::{Route, State};
use lumia_shell::platform;
use lumia_shell::session::browser::BrowserUpdate;
use lumia_shell::session::{Failure, Update};
use rune::Runtime;

/// The `state` a sign-in page was opened with, read out of its return address.
fn state_of(page: &str) -> String {
    let (_, rest) = page
        .split_once("state%3D")
        .unwrap_or_else(|| panic!("no state in {page}"));
    rest.chars().take_while(char::is_ascii_hexdigit).collect()
}

/// Presses a browser sign-in button and returns the page it sent the browser to.
fn ask(rt: &mut Runtime<State>, button: &str) -> String {
    press(rt, button);
    let mut pages = rt.take_url_requests();
    assert_eq!(pages.len(), 1, "one page opens: {pages:?}");
    pages.remove(0)
}

fn came_back(token: Result<(), Failure>) -> Update {
    Update::Browser(BrowserUpdate::SignedIn(token))
}

#[test]
fn a_link_is_read_strictly() {
    let sign_in = |state: &str, token: &str| {
        Some(Link::SignIn {
            state: state.into(),
            token: token.into(),
        })
    };
    assert_eq!(
        links::parse("lumia://sign-in?state=ab12&magic=tok_en-1"),
        sign_in("ab12", "tok_en-1")
    );
    // The page appends its token to whatever return address it was given.
    assert_eq!(
        links::parse("LUMIA://sign-in/?magic=t&state=s#frag"),
        sign_in("s", "t")
    );
    assert_eq!(
        links::parse("lumia://sign-in?state=s&error=oidc"),
        Some(Link::Refused { state: "s".into() })
    );
    assert_eq!(links::parse("lumia://done"), Some(Link::Done));

    for foreign in [
        "",
        "lumia",
        "lumia:sign-in?state=s&magic=t",
        "https://chat.example.com/sign-in?state=s&magic=t",
        "file:///etc/passwd",
        "lumia://settings",
        "lumia://sign-in",
        "lumia://sign-in?state=s",
        // No state: not a sign-in this app can have started.
        "lumia://sign-in?magic=t",
        // A token is URL-safe as the instance mints it; anything else is refused whole.
        "lumia://sign-in?state=s&magic=a%20b",
        "lumia://sign-in?state=s&magic=a/b",
        "lumia://sign-in?state=s&magic=",
        "lumia://sign-in?state=s&magic=t\nu",
        "lumia://sign-in?state=s s&magic=t",
        "lumia://sign-in.evil?state=s&magic=t",
        "lumia://evil/sign-in?state=s&magic=t",
    ] {
        assert_eq!(links::parse(foreign), None, "{foreign:?}");
    }
    let long = format!("lumia://sign-in?state=s&magic={}", "a".repeat(4000));
    assert_eq!(
        links::parse(&long),
        None,
        "a link far longer than any real one"
    );
}

#[test]
fn the_card_offers_a_passkey_and_the_provider_the_instance_names() {
    let mut rt = runtime(opened());
    assert!(rt.find("passkey-sign-in").is_some());
    assert!(
        rt.find("oidc-sign-in").is_none(),
        "this instance names no provider"
    );
    assert!(words(&rt).contains("Sign in with a passkey"));

    // An instance with a provider names it on a button of its own, first.
    rt.update(|s, _| {
        s.receive(&Update::Info(Ok(InstanceSummary {
            oidc_name: "Example SSO".into(),
            ..info()
        })));
    });
    settle(&mut rt);
    let (_, provider) = rt.find("oidc-sign-in").expect("the provider's button");
    let (_, passkey) = rt.find("passkey-sign-in").expect("the passkey button");
    assert!(provider < passkey, "the provider comes first");
    assert!(words(&rt).contains("Example SSO"));

    // Neither makes a new account.
    press(&mut rt, "to-create");
    assert!(rt.find("passkey-sign-in").is_none() && rt.find("oidc-sign-in").is_none());
}

#[test]
fn the_buttons_send_the_browser_to_the_instance_with_a_way_back() {
    let mut state = opened();
    state.receive(&Update::Info(Ok(InstanceSummary {
        oidc_name: "Example SSO".into(),
        ..info()
    })));
    let mut rt = runtime(state);
    assert!(rt.find("browser-waiting").is_none());

    let page = ask(&mut rt, "passkey-sign-in");
    assert!(
        page.starts_with("http://127.0.0.1:9/sign-in?return=lumia%3A%2F%2Fsign-in%3Fstate%3D"),
        "{page}"
    );
    let first = state_of(&page);
    assert_eq!(first.len(), 32, "128 bits of state: {first}");
    assert!(
        rt.find("browser-waiting").is_some() && words(&rt).contains("Finish the sign-in"),
        "the card says where the sign-in went"
    );

    let page = ask(&mut rt, "oidc-sign-in");
    assert!(
        page.starts_with(
            "http://127.0.0.1:9/api/v1/auth/oidc/start?return=lumia%3A%2F%2Fsign-in%3Fstate%3D"
        ),
        "{page}"
    );
    assert_ne!(
        state_of(&page),
        first,
        "each sign-in has a state of its own"
    );
}

#[test]
fn a_sign_in_comes_back_only_to_the_app_that_started_it() {
    let mut rt = runtime(opened());

    // Nothing asked for: a page, or anything that can run `open`, tried to sign the app in.
    let forged = "lumia://sign-in?state=00000000000000000000000000000000&magic=theirs";
    assert!(rt.state.open_url(forged));
    assert!(!rt.state.sign_in.busy, "nothing was sent to the instance");
    assert!(rt.state.sign_in.error.contains("did not start in this app"));

    let state = state_of(&ask(&mut rt, "passkey-sign-in"));
    assert_eq!(
        rt.state.sign_in.error, "",
        "starting again clears the refusal"
    );

    // The wrong state is refused and does not spend the one that is out.
    assert!(rt.state.open_url(forged));
    assert!(!rt.state.sign_in.busy);
    assert!(rt.state.sign_in.error.contains("did not start in this app"));

    // The right one is taken, once.
    let back = format!("lumia://sign-in?state={state}&magic=one-time-token");
    assert!(rt.state.open_url(&back));
    assert!(
        rt.state.sign_in.busy,
        "the token is on its way to the instance"
    );
    assert_eq!(rt.state.sign_in.error, "");
    rt.update(|s, _| s.receive(&came_back(Ok(()))));
    settle(&mut rt);
    assert_eq!(rt.state.route, Route::Shell);
    assert!(rt.state.session.signed_in);

    // Signed in, a sign-in link does nothing at all, whoever sends it.
    assert!(!rt.state.open_url(&back));
    assert!(!rt.state.open_url(forged));
    assert_eq!(rt.state.route, Route::Shell);
    assert_eq!(rt.state.sign_in.error, "");
}

#[test]
fn a_state_is_spent_by_its_use_and_by_time_and_belongs_to_its_instance() {
    // Used once.
    let mut rt = runtime(opened());
    let state = state_of(&ask(&mut rt, "passkey-sign-in"));
    let back = format!("lumia://sign-in?state={state}&magic=token");
    assert!(rt.state.open_url(&back));
    rt.update(|s, _| {
        s.receive(&came_back(Err(Failure {
            code: "UNAUTHENTICATED".into(),
            message: "unknown, used or expired".into(),
            ..Failure::default()
        })));
    });
    settle(&mut rt);
    assert!(words(&rt).contains("That sign-in was not accepted."));
    assert!(!rt.state.sign_in.busy);
    rt.state.open_url(&back);
    assert!(
        !rt.state.sign_in.busy && rt.state.sign_in.error.contains("did not start in this app"),
        "the same link again is not taken"
    );

    // Too late.
    let mut rt = runtime(opened());
    let state = state_of(&ask(&mut rt, "passkey-sign-in"));
    let back = format!("lumia://sign-in?state={state}&magic=token");
    let later = Instant::now() + Duration::from_secs(11 * 60);
    assert!(rt.state.open_url_at(&back, later));
    assert!(!rt.state.sign_in.busy);
    assert!(rt.state.sign_in.error.contains("took too long"));

    // Started for one instance, not taken by another.
    let mut rt = runtime(opened());
    let state = state_of(&ask(&mut rt, "passkey-sign-in"));
    let back = format!("lumia://sign-in?state={state}&magic=token");
    rt.update(|s, _| {
        s.change_instance();
        s.sign_in.instance.replace_text("http://127.0.0.1:10");
        s.connect();
        s.receive(&Update::Info(Ok(info())));
    });
    settle(&mut rt);
    assert!(
        rt.find("browser-waiting").is_none(),
        "nothing is out for this instance"
    );
    assert!(rt.state.open_url(&back));
    assert!(!rt.state.sign_in.busy);
    assert!(rt.state.sign_in.error.contains("did not start in this app"));
}

#[test]
fn a_provider_that_refused_says_so() {
    let mut rt = runtime(opened());
    let state = state_of(&ask(&mut rt, "passkey-sign-in"));
    // As the system hands it over: through the app's link hook.
    rt.update(|s, tx| {
        platform::open_url(s, &format!("lumia://sign-in?state={state}&error=oidc"), tx);
    });
    settle(&mut rt);
    assert!(!rt.state.sign_in.busy);
    assert!(words(&rt).contains("That sign-in was not accepted."));
    assert!(rt.find("browser-waiting").is_none(), "that sign-in is over");
}

#[test]
fn a_link_that_is_not_the_apps_changes_nothing() {
    // With no instance open there is nothing to sign in to, and nothing to say.
    let mut rt = runtime(State::new(data_dir()));
    for link in [
        "lumia://sign-in?state=abc&magic=token",
        "https://chat.example.com/sign-in?magic=token",
        "lumia://nowhere",
        "not a link",
    ] {
        rt.update(|s, tx| platform::open_url(s, link, tx));
        settle(&mut rt);
        assert_eq!(rt.state.route, Route::SignIn);
        assert_eq!(rt.state.sign_in.error, "", "{link}");
        assert!(!rt.state.sign_in.busy && !rt.state.session.is_open());
    }
    assert!(
        rt.take_url_requests().is_empty(),
        "a link never opens a page"
    );

    // Signed in, the same links leave the shell where it is.
    let mut rt = runtime(signed_in());
    let channel = rt.state.session.channel().map(str::to_string);
    for link in [
        "lumia://sign-in?state=abc&magic=token",
        "lumia://nowhere",
        "lumia://done",
        "javascript:alert(1)",
    ] {
        rt.update(|s, tx| platform::open_url(s, link, tx));
        settle(&mut rt);
        assert_eq!(rt.state.route, Route::Shell, "{link}");
        assert_eq!(
            rt.state.session.channel().map(str::to_string),
            channel,
            "{link}"
        );
    }
}

#[test]
fn a_signed_in_app_opens_the_web_page_as_itself() {
    // The answers are given here; what the unreachable instance says is never drained.
    let mut rt = runtime(signed_in());
    rt.state.open_web_signed_in("devices");
    assert!(rt.state.links.handing_off);

    let page = "http://127.0.0.1:9/sign-in?magic=one-time&next=devices";
    rt.update(|s, tx| {
        s.receive(&Update::Browser(BrowserUpdate::Handoff(Ok(page.into()))));
        // What the window's poll does once the session's answer is in.
        platform::settle(s, tx);
    });
    settle(&mut rt);
    assert!(!rt.state.links.handing_off);
    assert_eq!(
        rt.take_url_requests(),
        [page],
        "the window's poll opens the page"
    );
    assert!(rt.state.links.to_open.is_empty());

    rt.state.open_web_signed_in("devices");
    rt.update(|s, tx| {
        s.receive(&Update::Browser(BrowserUpdate::Handoff(Err(Failure {
            message: "the instance is away".into(),
            ..Failure::default()
        }))));
        platform::settle(s, tx);
    });
    settle(&mut rt);
    assert!(!rt.state.links.handing_off);
    assert_eq!(rt.state.links.handoff_error, "the instance is away");
    assert!(rt.take_url_requests().is_empty());

    // Signed out, there is no account to open the page as.
    let mut state = opened();
    state.open_web_signed_in("devices");
    assert!(!state.links.handing_off);
}
