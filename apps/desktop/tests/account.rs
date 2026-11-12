//! The account screens over a state no instance is behind: the magic link on the sign-in
//! card, linking this computer, the devices with the revoke confirmation and the passkeys,
//! and the profile with the status. What the instance would answer is applied with
//! `State::receive`; `tests/live_account.rs` is the same flows against a real one.

#![allow(clippy::expect_used, clippy::unwrap_used)]

mod common;

use std::collections::HashMap;

use common::*;
use lumia_client::app::{DeviceSummary, PasskeySummary, ProfileSummary, RosterSummary};
use lumia_shell::app::{HomeView, Place, Route, State};
use lumia_shell::session::identity::IdentityUpdate;
use lumia_shell::session::{Failure, Update};
use lumia_shell::ui;
use rune::{Env, Runtime};

const PHONE: &str = "5d0c91e4-2222-4000-8000-000000000002";

fn failure(code: &str, message: &str) -> Failure {
    Failure {
        code: code.into(),
        message: message.into(),
        details: HashMap::new(),
    }
}

fn identity(update: IdentityUpdate) -> Update {
    Update::Identity(update)
}

/// A window tall enough that a whole page is on screen, so a press lands on what it names.
fn tall(state: State) -> Runtime<State> {
    let mut rt = Runtime::new(state, ui::view, (1180.0, 1600.0), Env::new(false, 2.0));
    settle(&mut rt);
    rt
}

fn receive(rt: &mut Runtime<State>, update: Update) {
    rt.update(|s, _| s.receive(&update));
    settle(rt);
}

/// An instance open that offers the magic link and identity-proof sign-in.
fn offering() -> State {
    let mut state = opened();
    let mut info = info();
    info.magic_link_enabled = true;
    info.identity_proof_sign_in_enabled = true;
    state.receive(&Update::Info(Ok(info)));
    state
}

fn device(id: &str, kind: &str, origin: &str, this_device: bool, revoked: bool) -> DeviceSummary {
    DeviceSummary {
        device_id: id.into(),
        kind: kind.into(),
        // The core's roster summary carries no label.
        label: String::new(),
        web_origin: origin.into(),
        added_at_micros: if revoked { 0 } else { 1_793_500_000_000_000 },
        this_device,
        revoked,
    }
}

fn roster(phone_revoked: bool) -> RosterSummary {
    RosterSummary {
        identity_id: "9f2c".into(),
        version: if phone_revoked { 5 } else { 4 },
        devices: vec![
            device(
                "0b6e7a52-1111-4000-8000-000000000001",
                "native",
                "",
                true,
                false,
            ),
            device(
                PHONE,
                if phone_revoked { "" } else { "mobile" },
                "",
                false,
                phone_revoked,
            ),
            device(
                "a41f3b77-3333-4000-8000-000000000003",
                "web",
                "https://chat.example.com",
                false,
                false,
            ),
            device("e7c2d019-4444-4000-8000-000000000004", "", "", false, true),
        ],
    }
}

/// Signed in, at Home's devices, with nothing reported for the identity yet.
fn at_devices() -> Runtime<State> {
    let mut state = signed_in();
    state.go_home(HomeView::Devices);
    tall(state)
}

fn at_profile() -> Runtime<State> {
    let mut state = signed_in();
    state.go_home(HomeView::Profile);
    tall(state)
}

// ---- The magic link ---------------------------------------------------------------------

#[test]
fn an_instance_that_offers_neither_shows_no_magic_link_and_no_way_to_linking() {
    let rt = runtime(opened());
    assert!(rt.find("account-card").is_some());
    assert!(rt.find("magic-link").is_none() && rt.find("to-link").is_none());
    let shown = words(&rt);
    assert!(!shown.contains("Email me a sign-in link") && !shown.contains("Link this computer"));
}

#[test]
fn a_new_account_is_offered_neither() {
    let mut rt = runtime(offering());
    assert!(rt.find("magic-link").is_some() && rt.find("to-link").is_some());
    press(&mut rt, "to-create");
    assert!(rt.find("magic-link").is_none() && rt.find("to-link").is_none());
}

#[test]
fn a_magic_link_is_asked_for_with_the_address_and_completed_with_its_code() {
    let mut rt = runtime(offering());
    assert!(words(&rt).contains("Email me a sign-in link"));

    // Nothing to mail a link to yet: the link is inert.
    press(&mut rt, "magic-link");
    assert!(!rt.state.sign_in.busy && !rt.state.sign_in.magic_sent);

    press(&mut rt, "handle");
    type_text(&mut rt, "ada@example.com");
    press(&mut rt, "magic-link");
    assert!(rt.state.sign_in.busy, "the instance is being asked");
    assert!(rt.find("magic-code").is_none());

    receive(&mut rt, identity(IdentityUpdate::MagicLinkSent(Ok(()))));
    assert!(rt.state.sign_in.magic_sent && !rt.state.sign_in.busy);
    let shown = words(&rt);
    assert!(
        shown.contains(
            "A link is on its way to ada@example.com if it has an account here. Open it on this device, or paste its code below."
        ),
        "{shown}"
    );
    assert!(shown.contains("Code from the email") && shown.contains("Sign in with the code"));
    assert!(rt.find("magic-link").is_none(), "asked once");

    // No code yet: the button is inert.
    press(&mut rt, "magic-sign-in");
    assert!(!rt.state.sign_in.busy);

    press(&mut rt, "magic-code");
    type_text(&mut rt, "not-the-code");
    press(&mut rt, "magic-sign-in");
    assert!(rt.state.sign_in.busy);
    receive(
        &mut rt,
        identity(IdentityUpdate::MagicLinkSignedIn(Err(failure(
            "UNAUTHENTICATED",
            "bad token",
        )))),
    );
    assert!(!rt.state.sign_in.busy);
    assert!(words(&rt).contains("That code is unknown, used or expired. Ask for a new link."));
    assert_eq!(rt.state.route, Route::SignIn);

    receive(&mut rt, identity(IdentityUpdate::MagicLinkSignedIn(Ok(()))));
    assert_eq!(rt.state.route, Route::Shell);
    assert!(rt.state.session.signed_in);
    assert!(
        !rt.state.sign_in.magic_sent && rt.state.sign_in.magic_code.text.is_empty(),
        "the link is forgotten once it signed in"
    );
}

#[test]
fn a_refused_magic_link_is_worded() {
    let mut rt = runtime(offering());
    press(&mut rt, "handle");
    type_text(&mut rt, "ada@example.com");
    press(&mut rt, "magic-link");
    receive(
        &mut rt,
        identity(IdentityUpdate::MagicLinkSent(Err(failure(
            "RATE_LIMITED",
            "slow down",
        )))),
    );
    assert!(!rt.state.sign_in.busy && !rt.state.sign_in.magic_sent);
    assert!(words(&rt).contains("Too many links in the last hour; try later."));
    assert!(
        rt.find("magic-link").is_some(),
        "and can be asked for again"
    );

    press(&mut rt, "magic-link");
    receive(
        &mut rt,
        identity(IdentityUpdate::MagicLinkSent(Err(failure(
            "POLICY_DENIED",
            "this instance sends no mail",
        )))),
    );
    assert!(words(&rt).contains("this instance sends no mail"));

    // A code the instance refuses for another reason says the instance's own words.
    receive(&mut rt, identity(IdentityUpdate::MagicLinkSent(Ok(()))));
    press(&mut rt, "magic-code");
    type_text(&mut rt, "abc");
    press(&mut rt, "magic-sign-in");
    receive(
        &mut rt,
        identity(IdentityUpdate::MagicLinkSignedIn(Err(failure(
            "",
            "transport: connection refused",
        )))),
    );
    assert!(words(&rt).contains("transport: connection refused"));
}

#[test]
fn changing_instance_forgets_a_magic_link() {
    let mut rt = runtime(offering());
    press(&mut rt, "handle");
    type_text(&mut rt, "ada@example.com");
    press(&mut rt, "magic-link");
    receive(&mut rt, identity(IdentityUpdate::MagicLinkSent(Ok(()))));
    press(&mut rt, "change-instance");
    assert!(!rt.state.sign_in.magic_sent && !rt.state.sign_in.busy);
}

// ---- Linking this computer --------------------------------------------------------------

#[test]
fn this_computer_is_linked_from_sign_in_with_a_request_and_a_code() {
    let mut rt = runtime(offering());
    assert!(words(&rt).contains("Link this computer to your identity"));
    press(&mut rt, "to-link");
    assert_eq!(rt.state.route, Route::Link);
    let shown = words(&rt);
    for expected in [
        "Link this computer",
        "A device you already use approves this one.",
        "This computer gets its own device key.",
        "Start linking",
        "Back to sign in",
    ] {
        assert!(shown.contains(expected), "{expected:?} in:\n{shown}");
    }
    assert!(rt.find("link-code").is_none() && rt.find("claim-link").is_none());

    press(&mut rt, "start-link");
    assert!(rt.state.account.link.starting);
    receive(
        &mut rt,
        identity(IdentityUpdate::LinkRequested(Ok("UkVRVUVTVA".into()))),
    );
    let shown = words(&rt);
    for expected in [
        "On a device that holds your identity, approve this request and enter the code it gives you.",
        "UkVRVUVTVA",
        "Link request",
        "Copy request",
        "Code from the approving device",
        "Claim and sign in",
    ] {
        assert!(shown.contains(expected), "{expected:?} in:\n{shown}");
    }
    assert!(rt.find("start-link").is_none());

    press(&mut rt, "copy-request");
    assert!(rt.state.account.link.copied);
    assert!(words(&rt).contains("Copied"));

    // No code yet: claiming is inert.
    press(&mut rt, "claim-link");
    assert!(!rt.state.account.link.busy);

    press(&mut rt, "link-code");
    type_text(&mut rt, "K7Q2M9XD");
    press(&mut rt, "claim-link");
    assert!(rt.state.account.link.busy);
    receive(
        &mut rt,
        identity(IdentityUpdate::Linked(Err(failure(
            "NOT_FOUND",
            "no link bundle for that code",
        )))),
    );
    assert!(!rt.state.account.link.busy);
    assert!(words(&rt).contains("no link bundle for that code"));
    assert_eq!(rt.state.route, Route::Link);

    receive(&mut rt, identity(IdentityUpdate::Linked(Ok(roster(false)))));
    assert_eq!(rt.state.route, Route::Shell);
    assert!(rt.state.session.signed_in);
    assert!(rt.state.session.identity.roster.is_some());
    assert!(
        rt.state.account.link.request.is_empty(),
        "the form is spent"
    );
}

#[test]
fn a_link_request_that_cannot_be_made_says_why_and_back_returns_to_sign_in() {
    let mut rt = runtime(offering());
    press(&mut rt, "to-link");
    press(&mut rt, "start-link");
    receive(
        &mut rt,
        identity(IdentityUpdate::LinkRequested(Err(failure(
            "",
            "keystore: locked",
        )))),
    );
    assert!(!rt.state.account.link.starting);
    assert!(words(&rt).contains("keystore: locked"));
    assert!(rt.find("start-link").is_some(), "and can be tried again");

    press(&mut rt, "link-back");
    assert_eq!(rt.state.route, Route::SignIn);
    assert!(rt.find("account-card").is_some());
}

// ---- Devices ----------------------------------------------------------------------------

#[test]
fn an_account_with_no_identity_here_is_led_to_linking_from_devices() {
    let mut rt = at_devices();
    let shown = words(&rt);
    for expected in [
        "Devices",
        "Everything that holds your identity",
        "Passkeys",
        "No passkeys yet.",
        "Link this computer to your identity",
        "This computer has no device key yet; a device you already use approves it.",
    ] {
        assert!(shown.contains(expected), "{expected:?} in:\n{shown}");
    }
    assert!(rt.find("devices-list").is_none());
    assert!(
        !shown.contains("Add a passkey"),
        "no control for what this shell cannot do yet"
    );

    press(&mut rt, "link-this-computer");
    assert_eq!(rt.state.route, Route::Link);
    assert!(
        words(&rt).contains("Not now"),
        "an account that is signed in goes back to the shell, not to sign-in"
    );
    press(&mut rt, "link-back");
    assert_eq!(rt.state.route, Route::Shell);
    assert_eq!(rt.state.home_view, HomeView::Devices);

    // Linked from here: the shell again, with the roster where the notice was.
    press(&mut rt, "link-this-computer");
    receive(&mut rt, identity(IdentityUpdate::Linked(Ok(roster(false)))));
    assert_eq!(rt.state.route, Route::Shell);
    assert!(rt.find("devices-list").is_some() && rt.find("no-identity").is_none());
}

#[test]
fn the_roster_names_devices_by_kind_and_marks_this_one() {
    let mut rt = at_devices();
    receive(&mut rt, identity(IdentityUpdate::Roster(Ok(roster(false)))));
    let shown = words(&rt);
    for expected in [
        "Your identity lives on these devices, not on any instance. Revoking one takes effect on every instance that holds your roster.",
        "This computer",
        "This device",
        "Phone",
        "Browser",
        "https://chat.example.com · added ",
        " · a41f3b77",
        "Revoked",
        "e7c2d019",
        "This computer, this device",
        "e7c2d019, revoked",
        "Revoke Phone",
        "Revoke Browser",
    ] {
        assert!(shown.contains(expected), "{expected:?} in:\n{shown}");
    }
    assert!(rt.find("no-identity").is_none());
    // This device and a revoked one cannot be revoked from here.
    assert!(
        rt.find("revoke-0b6e7a52-1111-4000-8000-000000000001")
            .is_none()
    );
    assert!(
        rt.find("revoke-e7c2d019-4444-4000-8000-000000000004")
            .is_none()
    );
    assert!(rt.find(&format!("revoke-{PHONE}")).is_some());
    assert!(rt.find("revoke-card").is_none());
}

#[test]
fn revoking_a_device_asks_for_the_password_and_words_a_wrong_one() {
    let mut rt = at_devices();
    receive(&mut rt, identity(IdentityUpdate::Roster(Ok(roster(false)))));
    press(&mut rt, &format!("revoke-{PHONE}"));
    assert_eq!(rt.state.account.devices.revoking.as_deref(), Some(PHONE));
    let shown = words(&rt);
    for expected in [
        "Revoke Phone?",
        "It stops reading new messages the moment every instance has the change. Confirm with your password.",
        "Keep it",
    ] {
        assert!(shown.contains(expected), "{expected:?} in:\n{shown}");
    }

    // No password yet: confirming is inert.
    press(&mut rt, "revoke-confirm");
    assert!(!rt.state.account.devices.busy);

    press(&mut rt, "revoke-password");
    type_text(&mut rt, "not-my-password");
    assert!(
        !words(&rt).contains("not-my-password"),
        "the password is not in the tree"
    );
    press(&mut rt, "revoke-confirm");
    assert!(rt.state.account.devices.busy);
    receive(
        &mut rt,
        identity(IdentityUpdate::Revoked(Err(failure(
            "UNAUTHENTICATED",
            "bad credentials",
        )))),
    );
    assert!(!rt.state.account.devices.busy);
    assert!(words(&rt).contains("That password was not accepted."));
    assert!(rt.find("revoke-card").is_some(), "still asking");

    receive(
        &mut rt,
        identity(IdentityUpdate::Revoked(Err(failure(
            "CONFLICT",
            "the instance holds a newer roster",
        )))),
    );
    assert!(words(&rt).contains("the instance holds a newer roster"));

    receive(&mut rt, identity(IdentityUpdate::Revoked(Ok(roster(true)))));
    assert!(rt.find("revoke-card").is_none());
    assert!(rt.state.account.devices.revoking.is_none());
    assert!(rt.state.account.devices.password.text.is_empty());
    let shown = words(&rt);
    assert!(
        shown.contains("5d0c91e4, revoked") && !shown.contains("Revoke Phone"),
        "{shown}"
    );
    assert!(rt.find(&format!("revoke-{PHONE}")).is_none());
}

#[test]
fn keeping_a_device_puts_the_confirmation_away() {
    let mut rt = at_devices();
    receive(&mut rt, identity(IdentityUpdate::Roster(Ok(roster(false)))));
    press(&mut rt, &format!("revoke-{PHONE}"));
    press(&mut rt, "revoke-password");
    type_text(&mut rt, "secret");
    press(&mut rt, "revoke-keep");
    assert!(rt.find("revoke-card").is_none());
    assert!(rt.state.account.devices.revoking.is_none());
    assert!(rt.state.account.devices.password.text.is_empty());
    assert!(words(&rt).contains("Phone"), "and the device stays");
}

#[test]
fn the_confirmation_comes_into_view_in_a_short_window() {
    let mut state = signed_in();
    state.go_home(HomeView::Devices);
    state.receive(&identity(IdentityUpdate::Roster(Ok(roster(false)))));
    let mut rt = Runtime::new(state, ui::view, (1180.0, 620.0), Env::new(false, 2.0));
    settle(&mut rt);
    press(&mut rt, &format!("revoke-{PHONE}"));
    for _ in 0..120 {
        rt.frame(1.0 / 60.0);
    }
    let (_, y) = rt.find("revoke-confirm").expect("the confirmation");
    assert!(
        (0.0..620.0).contains(&y),
        "the confirm button is on screen, at {y}"
    );
    let carried = rt
        .elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some("revoke-confirm"))
        .map(|e| e.scrolled[1])
        .expect("the confirm button");
    assert!(
        carried > 0.0,
        "because the page scrolled to it: it opens below the fold"
    );
    assert!(
        rt.state.account.devices.password.focused,
        "and the password field has the keyboard"
    );
}

#[test]
fn passkeys_are_listed_and_removed() {
    let mut rt = at_devices();
    receive(
        &mut rt,
        identity(IdentityUpdate::Passkeys(Ok(vec![
            PasskeySummary {
                passkey_id: "pk1".into(),
                label: "This laptop".into(),
                created_at_micros: Some(1_793_000_000_000_000),
                last_used_at_micros: Some(1_793_600_000_000_000),
            },
            PasskeySummary {
                passkey_id: "pk2".into(),
                label: String::new(),
                created_at_micros: None,
                last_used_at_micros: None,
            },
        ]))),
    );
    let shown = words(&rt);
    assert!(shown.contains("This laptop · added ") && shown.contains(" · last used "));
    assert!(
        shown.contains("Passkey · added — · last used never"),
        "{shown}"
    );
    assert!(!shown.contains("No passkeys yet."));
    assert!(
        shown.contains("Remove This laptop") && shown.contains("Remove Passkey"),
        "each removal says which passkey it removes: {shown}"
    );

    press(&mut rt, "remove-passkey-pk1");
    receive(
        &mut rt,
        identity(IdentityUpdate::PasskeyRemoved(Err(failure(
            "NOT_FOUND",
            "no such passkey",
        )))),
    );
    assert!(words(&rt).contains("no such passkey"));
    // Another try clears what the last one said.
    press(&mut rt, "remove-passkey-pk2");
    assert!(rt.state.account.devices.passkey_error.is_empty());

    // The list the instance sends after a removal replaces this one.
    receive(&mut rt, identity(IdentityUpdate::Passkeys(Ok(Vec::new()))));
    assert!(words(&rt).contains("No passkeys yet."));
    assert!(rt.find("passkeys").is_none());
}

// ---- Profile and status -----------------------------------------------------------------

#[test]
fn the_profile_of_an_account_with_no_identity_here_leads_to_linking() {
    let mut rt = at_profile();
    let shown = words(&rt);
    for expected in [
        "Profile",
        "How people see you",
        "Status",
        "What people see beside your name in a server's member list.",
        "Your profile travels with your identity, signed by its devices. This computer holds no device key yet; a device you already use approves it.",
        "Link this computer",
    ] {
        assert!(shown.contains(expected), "{expected:?} in:\n{shown}");
    }
    assert!(rt.find("profile-card").is_none() && rt.find("display-name").is_none());
    press(&mut rt, "profile-link");
    assert_eq!(rt.state.route, Route::Link);
}

#[test]
fn a_status_is_chosen_kept_and_shown_again() {
    let mut rt = at_profile();
    assert_eq!(rt.state.account.profile.status, "ONLINE");
    let shown = words(&rt);
    assert!(shown.contains("Online") && shown.contains("Away") && shown.contains("Do not disturb"));

    press(&mut rt, "status-2");
    assert_eq!(rt.state.account.profile.status, "DO_NOT_DISTURB");
    press(&mut rt, "custom-status");
    type_text(&mut rt, "Heads down");
    assert!(!words(&rt).contains("Status saved."));
    press(&mut rt, "save-status");
    assert!(words(&rt).contains("Status saved."));
    // The core keeps it in this instance's store.
    let kept = rt.state.session.status().expect("a status");
    assert_eq!(
        (kept.status.as_str(), kept.custom_status.as_str()),
        ("DO_NOT_DISTURB", "Heads down")
    );

    // Choosing again is unsaved until saved.
    press(&mut rt, "status-1");
    assert!(!words(&rt).contains("Status saved."));
    assert_eq!(rt.state.account.profile.status, "AWAY");

    // Leaving and coming back shows what was kept, not what was only chosen.
    rt.update(|s, _| s.go_home(HomeView::Devices));
    rt.update(|s, _| s.go_home(HomeView::Profile));
    settle(&mut rt);
    assert_eq!(rt.state.account.profile.status, "DO_NOT_DISTURB");
    assert_eq!(rt.state.account.profile.custom_status.text, "Heads down");
    assert!(!words(&rt).contains("Status saved."));
}

#[test]
fn the_profile_form_waits_for_the_profile_held_and_words_each_refusal() {
    let mut rt = at_profile();
    receive(
        &mut rt,
        identity(IdentityUpdate::Profile(Ok(Some(ProfileSummary {
            version: 3,
            display_name: "Ada Lovelace".into(),
            pronouns: "she/her".into(),
            bio: "Notes on the Analytical Engine.".into(),
            avatar_url: String::new(),
        })))),
    );
    let shown = words(&rt);
    for expected in [
        "@ada",
        "Version 3, signed by this computer's device key; every instance you sign in to takes it.",
        "Display name",
        "Pronouns",
        "Bio",
    ] {
        assert!(shown.contains(expected), "{expected:?} in:\n{shown}");
    }
    let form = &rt.state.account.profile;
    assert_eq!(
        (
            form.display_name.text.as_str(),
            form.pronouns.text.as_str(),
            form.bio.text.as_str()
        ),
        ("Ada Lovelace", "she/her", "Notes on the Analytical Engine.")
    );
    assert!(rt.find("profile-link").is_none());

    press(&mut rt, "pronouns");
    type_text(&mut rt, "/they");
    press(&mut rt, "save-profile");
    assert!(rt.state.account.profile.busy);
    // A profile that lands late does not take the form from under the typing.
    receive(&mut rt, identity(IdentityUpdate::Profile(Ok(None))));
    assert_eq!(rt.state.account.profile.pronouns.text, "she/her/they");

    receive(
        &mut rt,
        identity(IdentityUpdate::ProfileSaved(Err(failure(
            "FAILED_PRECONDITION",
            "account has no identity",
        )))),
    );
    assert!(!rt.state.account.profile.busy);
    assert!(words(&rt).contains(
        "This account is not linked to your identity, so the instance keeps the profile it has."
    ));

    press(&mut rt, "save-profile");
    receive(
        &mut rt,
        identity(IdentityUpdate::ProfileSaved(Err(failure(
            "CONFLICT",
            "stale version",
        )))),
    );
    assert!(words(&rt).contains("A newer profile is already held. Reload and try again."));

    press(&mut rt, "save-profile");
    receive(
        &mut rt,
        identity(IdentityUpdate::ProfileSaved(Err(failure(
            "",
            "transport: timed out",
        )))),
    );
    assert!(words(&rt).contains("transport: timed out"));

    press(&mut rt, "save-profile");
    assert!(
        !words(&rt).contains("transport: timed out"),
        "a new try clears the last refusal"
    );
    receive(
        &mut rt,
        identity(IdentityUpdate::ProfileSaved(Ok(ProfileSummary {
            version: 4,
            display_name: "Ada Lovelace".into(),
            pronouns: "she/her/they".into(),
            bio: "Notes on the Analytical Engine.".into(),
            avatar_url: String::new(),
        }))),
    );
    let shown = words(&rt);
    assert!(
        shown.contains("Saved. This instance shows it now."),
        "{shown}"
    );
    assert!(shown.contains("Version 4, signed by this computer's device key"));
}

#[test]
fn an_identity_with_no_profile_yet_says_people_see_the_handle() {
    let mut rt = at_profile();
    receive(&mut rt, identity(IdentityUpdate::Profile(Ok(None))));
    let shown = words(&rt);
    assert!(
        shown.contains("Nothing set yet: people see your handle."),
        "{shown}"
    );
    assert!(rt.find("display-name").is_some() && rt.find("save-profile").is_some());
    assert!(rt.state.account.profile.display_name.text.is_empty());
}

#[test]
fn a_profile_that_cannot_be_read_says_why() {
    let mut rt = at_profile();
    receive(
        &mut rt,
        identity(IdentityUpdate::Profile(Err(failure(
            "",
            "transport: connection refused",
        )))),
    );
    assert!(words(&rt).contains("transport: connection refused"));
}

#[test]
fn sign_out_is_on_the_profile_page_and_nowhere_in_the_sidebar() {
    let mut rt = tall(signed_in());
    assert_eq!(rt.state.server_id(), Some("acme"));
    assert!(
        rt.find("sign-out").is_none() && !words(&rt).contains("Sign out"),
        "the sidebar's foot holds no account strip"
    );

    press(&mut rt, "self");
    assert_eq!(rt.state.place, Place::Home);
    assert_eq!(rt.state.home_view, HomeView::Profile);
    assert!(words(&rt).contains("Signed in to 127.0.0.1:9 as Ada Lovelace."));
    press(&mut rt, "sign-out");
    assert_eq!(rt.state.route, Route::SignIn);
    assert!(!rt.state.session.signed_in);
}

#[test]
fn pressing_the_avatar_again_keeps_what_is_being_typed() {
    let mut rt = at_profile();
    press(&mut rt, "custom-status");
    type_text(&mut rt, "Brb");
    press(&mut rt, "self");
    assert_eq!(rt.state.account.profile.custom_status.text, "Brb");
}
