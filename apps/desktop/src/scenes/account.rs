//! The account screens' scenes: sign-in with the magic link, linking this computer, setting
//! up private messages (the phrase, its check, restoring, starting over, and what Home and
//! Devices show meanwhile), the devices and the profile, each over a state no instance is
//! behind (what an instance would report is applied with `State::receive`). The `*-live`
//! ones wait for a real instance instead, with the `LUMIA_DEV_*` environment.

use std::time::{Duration, Instant};

use lumia_client::app::{
    DeviceSummary, InstanceSummary, LinkProgress, LinkPrompt, LinkRequestSummary, PasskeySummary,
    ProfileSummary, RosterSummary, SnapshotSummary,
};

use crate::app::account::{Link, Prompted};
use crate::app::{HomeView, Route, State};
use crate::session::Update;
use crate::session::identity::{IdentityUpdate, Setup};

/// An instance nothing answers for, offering every way in this part of the shell draws.
fn opened(s: &mut State) {
    s.sign_in.instance.replace_text("http://127.0.0.1:9");
    s.connect();
    // The real request for the trust card fails first, or its failure would land on the
    // frame and take the card below away.
    let deadline = Instant::now() + Duration::from_secs(3);
    while s.sign_in.reaching && Instant::now() < deadline {
        s.drain();
        std::thread::sleep(Duration::from_millis(10));
    }
    s.sign_in.error.clear();
    s.receive(&Update::Info(Ok(InstanceSummary {
        host: "chat.example.com".into(),
        operator_name: "Example Co".into(),
        operator_url: String::new(),
        terms_url: String::new(),
        privacy_url: String::new(),
        software_version: "0.2.1".into(),
        identity_proof_sign_in_enabled: true,
        identity_proof_create_enabled: true,
        open_registration: true,
        web_push_public_key: String::new(),
        magic_link_enabled: true,
        oidc_name: String::new(),
    })));
}

/// Signed in as Ada, at Home, with no server.
fn signed_in(s: &mut State) {
    opened(s);
    s.receive(&Update::SignedIn(Ok(())));
    s.receive(&Update::Snapshot(Ok(SnapshotSummary {
        account_id: "me".into(),
        handle: "ada".into(),
        display_name: "Ada Lovelace".into(),
        avatar_url: String::new(),
        servers: Vec::new(),
        channels: Vec::new(),
        pending_requests: Vec::new(),
        channel_invitations: Vec::new(),
        read_states: Vec::new(),
        rail: Vec::new(),
    })));
}

fn device(id: &str, kind: &str, origin: &str, this_device: bool, revoked: bool) -> DeviceSummary {
    DeviceSummary {
        device_id: id.into(),
        kind: kind.into(),
        label: String::new(),
        web_origin: origin.into(),
        added_at_micros: if revoked { 0 } else { 1_793_500_000_000_000 },
        this_device,
        revoked,
    }
}

fn roster() -> RosterSummary {
    RosterSummary {
        identity_id: "9f2c".into(),
        version: 4,
        devices: vec![
            device(
                "0b6e7a52-1111-4000-8000-000000000001",
                "native",
                "",
                true,
                false,
            ),
            device(
                "5d0c91e4-2222-4000-8000-000000000002",
                "mobile",
                "",
                false,
                false,
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

pub fn sign_in(s: &mut State) {
    opened(s);
    s.sign_in.handle.replace_text("ada@example.com");
}

pub fn magic_sent(s: &mut State) {
    sign_in(s);
    s.receive(&Update::Identity(IdentityUpdate::MagicLinkSent(Ok(()))));
}

pub fn link(s: &mut State) {
    opened(s);
    s.start_linking();
}

pub fn link_request(s: &mut State) {
    link(s);
    s.receive(&Update::Identity(IdentityUpdate::LinkRequested(Ok(
        "CiQwYjZlN2E1Mi0xMTExLTQwMDAtODAwMC0wMDAwMDAwMDAwMDESIgogq83vASNFZ4mrze8BI0VniavN7wEjRWeJq83vASNFZ4kaIP7cuph2VDIQ/ty6mHZUMhD+3LqYdlQyEP7cuph2VDIQIAEqB2Rlc2t0b3A6IBEiM0RVZneImaq7zN3u/wARIjNEVWZ3iJmqu8zd7v8A".into(),
    ))));
    s.account.link.code.replace_text("K7Q2M9XD");
}

pub fn devices(s: &mut State) {
    signed_in(s);
    s.go_home(HomeView::Devices);
    s.receive(&Update::Identity(IdentityUpdate::Roster(Ok(roster()))));
    s.receive(&Update::Identity(IdentityUpdate::Passkeys(Ok(vec![
        PasskeySummary {
            passkey_id: "pk1".into(),
            label: "This laptop".into(),
            created_at_micros: Some(1_793_000_000_000_000),
            last_used_at_micros: Some(1_793_600_000_000_000),
        },
        PasskeySummary {
            passkey_id: "pk2".into(),
            label: String::new(),
            created_at_micros: Some(1_793_200_000_000_000),
            last_used_at_micros: None,
        },
    ]))));
}

pub fn devices_revoke(s: &mut State) {
    devices(s);
    s.ask_revoke("5d0c91e4-2222-4000-8000-000000000002");
    s.receive(&Update::Identity(IdentityUpdate::Revoked(Err(
        crate::session::Failure {
            code: "UNAUTHENTICATED".into(),
            message: "bad credentials".into(),
            ..Default::default()
        },
    ))));
}

/// A phone's link request, as its Link screen shows it: a native device named "Pixel 9".
const PHONE_REQUEST: &str = "ChABoeDCfUpwAIAAAAAAAKF+GiAAAQIDBAUGBwgJCgsMDQ4PEBESExQVFhcYGRobHB0eHyABKgdQaXhlbCA5OiAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA==";

/// Devices approving another device: a phone's request pasted, saying what it adds.
pub fn devices_approve(s: &mut State) {
    devices(s);
    s.open_approve();
    s.account
        .devices
        .approve
        .request
        .replace_text(PHONE_REQUEST);
}

/// Devices after approving: the code the phone types.
pub fn devices_approved(s: &mut State) {
    devices_approve(s);
    s.receive(&Update::Identity(IdentityUpdate::Approved(Ok(
        "K7QM2XRA".into()
    ))));
}

/// Signed in, and this computer holds no identity.
pub fn devices_none(s: &mut State) {
    signed_in(s);
    s.go_home(HomeView::Devices);
}

/// The `profile-photo` scene: the profile of an account with a picture, which the rail,
/// the account card and the member list all draw. The picture is a drawing kept beside
/// this file; no instance is asked for it.
pub fn profile_photo(s: &mut State) {
    use crate::session::avatars::AvatarUpdate;
    signed_in(s);
    let url = format!("/api/v1/avatars/{}", "5c".repeat(32));
    let Some(mut snapshot) = s.session.snapshot.clone() else {
        return;
    };
    snapshot.avatar_url.clone_from(&url);
    s.receive(&Update::Avatar(AvatarUpdate::Saved(Ok(snapshot))));
    s.receive(&Update::Avatar(AvatarUpdate::Fetched {
        url,
        bytes: Ok(std::sync::Arc::from(&include_bytes!("portrait.jpg")[..])),
    }));
    s.go_home(HomeView::Profile);
    s.tick(chrono::Utc::now().timestamp_micros());
    s.pictures.wait();
}

pub fn profile(s: &mut State) {
    signed_in(s);
    s.go_home(HomeView::Profile);
    s.pick_status("AWAY");
    s.account.profile.custom_status.replace_text("At lunch");
    s.receive(&Update::Identity(IdentityUpdate::Profile(Ok(Some(
        ProfileSummary {
            version: 3,
            display_name: "Ada Lovelace".into(),
            pronouns: "she/her".into(),
            bio: "Notes on the Analytical Engine.".into(),
            avatar_url: String::new(),
        },
    )))));
}

pub fn profile_none(s: &mut State) {
    signed_in(s);
    s.go_home(HomeView::Profile);
}

/// The devices of the account in `LUMIA_DEV_*`, once the instance has answered.
pub fn devices_live(s: &mut State) {
    super::live(s);
    s.go_home(HomeView::Devices);
    quiet(s);
}

/// The profile of the account in `LUMIA_DEV_*`, once the instance has answered.
pub fn profile_live(s: &mut State) {
    super::live(s);
    s.go_home(HomeView::Profile);
    quiet(s);
}

// ---- Setting up private messages (#20) --------------------------------------------------

/// The words the scenes show: the boards' phrase.
const PHRASE: &str = "orbit cactus timber velvet harbor pencil meadow copper rescue glimpse fabric lunar anchor whisper salad ticket marble frozen clever sunny violin garden token mirror";

/// Signed in, the silent setup's real answer (nothing is there to answer it) landed, and
/// then `setup` staged in its place.
fn set_up(s: &mut State, setup: Setup, phrase_kept: bool) {
    signed_in(s);
    let deadline = Instant::now() + Duration::from_secs(3);
    while s.session.identity.setup == Setup::Checking && Instant::now() < deadline {
        s.drain();
        std::thread::sleep(Duration::from_millis(10));
    }
    s.receive(&Update::Identity(IdentityUpdate::SetUp {
        setup,
        phrase_kept,
    }));
}

/// A new account's card: the line about private messages, and "Already use Lumia on
/// another device?" under it.
pub fn sign_up(s: &mut State) {
    opened(s);
    s.sign_in.creating = true;
    s.sign_in.handle.replace_text("sam");
}

/// Home's staged account (`scenes::staged`), on a computer that holds an identity or not,
/// with setting up come to `setup`.
fn staged(s: &mut State, identity: bool, view: HomeView, setup: Setup, phrase_kept: bool) {
    super::staged::home(s, identity, view, None);
    s.receive(&Update::Identity(IdentityUpdate::SetUp {
        setup,
        phrase_kept,
    }));
}

/// Home with the reminder to save the phrase in the sidebar.
pub fn home_reminder(s: &mut State) {
    staged(s, true, HomeView::Conversations, Setup::Ready, true);
}

/// Devices with the "Recovery phrase not saved" card over the roster.
pub fn devices_phrase(s: &mut State) {
    staged(s, true, HomeView::Devices, Setup::Ready, true);
    s.receive(&Update::Identity(IdentityUpdate::Roster(Ok(
        RosterSummary {
            identity_id: "9f2c".into(),
            version: 1,
            devices: vec![device(
                "0b6e7a52-1111-4000-8000-000000000001",
                "native",
                "",
                true,
                false,
            )],
        },
    ))));
}

/// "Private messages need this computer": the account's identity is on another device.
pub fn needs_setup(s: &mut State) {
    staged(s, false, HomeView::Conversations, Setup::Elsewhere, false);
}

/// The same on Devices.
pub fn devices_elsewhere(s: &mut State) {
    staged(s, false, HomeView::Devices, Setup::Elsewhere, false);
}

/// The account has no identity yet, and binding one here needs the password.
pub fn needs_reauth(s: &mut State) {
    staged(s, true, HomeView::NewMessage, Setup::NeedsReauth, false);
}

/// Step 1: the phrase.
pub fn phrase(s: &mut State) {
    set_up(s, Setup::Ready, true);
    s.open_phrase();
    // The real read finds no phrase (nothing is set up here) and puts the screen away: it
    // lands first, and the staged phrase after it.
    quiet(s);
    s.route = Route::Phrase;
    s.receive(&Update::Identity(IdentityUpdate::Phrase(Ok(Some(
        PHRASE.into(),
    )))));
}

/// "Save as a file" pressed: the warning first.
pub fn phrase_save(s: &mut State) {
    phrase(s);
    s.ask_save_phrase();
}

/// Step 2: three words, one being typed with the wordlist completing it.
pub fn phrase_check(s: &mut State) {
    phrase(s);
    s.phrase_written();
    // As for the phrase: the real quiz refuses first.
    quiet(s);
    s.setup.phrase.checking = true;
    s.setup.phrase.error.clear();
    s.receive(&Update::Identity(IdentityUpdate::Quiz(Ok(vec![4, 11, 19]))));
    s.complete_word(0, "velvet");
    if let Some(field) = s.setup.phrase.answers.get_mut(1) {
        field.replace_text("fa");
    }
}

/// Restoring, from sign-in, twelve words in.
pub fn restore(s: &mut State) {
    opened(s);
    s.start_restore();
    let twelve: Vec<&str> = PHRASE.split(' ').take(12).collect();
    s.setup.restore.words.replace_text(twelve.join(" "));
}

/// Starting over, from "Private messages need this computer".
pub fn start_over(s: &mut State) {
    needs_setup(s);
    s.open_start_over();
    debug_assert_eq!(s.route, Route::StartOver);
}

// ---- Linking by a prompt (identity spec 4.2.1) ------------------------------------------

/// Never due: a scene's session is not asked about, or the real answer (nothing is there
/// to give one) would land on what it staged.
const NOT_DUE: i64 = i64::MAX / 2;

/// This computer, its private messages elsewhere, linking by a prompt with the session at
/// `stage`.
fn linking(s: &mut State, stage: &str, digits: &str) {
    needs_setup(s);
    s.route = Route::Link;
    s.account.link = Link {
        prompted: Prompted::Starting,
        ..Link::default()
    };
    s.receive(&Update::Identity(IdentityUpdate::LinkStarted(Ok(
        LinkProgress {
            stage: stage.into(),
            digits: digits.into(),
        },
    ))));
    s.account.link.asked_at = NOT_DUE;
}

/// Waiting for a device that holds the identity to answer.
pub fn link_waiting(s: &mut State) {
    linking(s, "waiting", "");
}

/// Another device answered: the digits both show.
pub fn link_compare(s: &mut State) {
    linking(s, "compare", "482 913");
}

/// Declined on the other device.
pub fn link_declined(s: &mut State) {
    linking(s, "declined", "");
}

/// Ten minutes went by.
pub fn link_expired(s: &mut State) {
    linking(s, "expired", "");
}

/// "Link another way" from the waiting screen: the copied request, signed in.
pub fn link_another_way(s: &mut State) {
    link_waiting(s);
    s.link_another_way();
}

/// A device of the account asking this computer, which holds the identity.
fn asking(kind: &str, label: &str, origin: &str, stage: &str, digits: &str) -> LinkPrompt {
    LinkPrompt {
        session_id: "0196f1c2-5555-7000-8000-000000000005".into(),
        request: LinkRequestSummary {
            device_id: "0196f1c2-6666-7000-8000-000000000006".into(),
            kind: kind.into(),
            label: label.into(),
            web_origin: origin.into(),
        },
        stage: stage.into(),
        digits: digits.into(),
    }
}

/// Home on a computer set up with the identity, the prompt over it.
fn prompted(s: &mut State, prompt: LinkPrompt) {
    staged(s, true, HomeView::Conversations, Setup::Ready, false);
    // The real question about the sessions (nothing is there to answer it) lands first.
    quiet(s);
    s.receive(&Update::Identity(IdentityUpdate::Prompts(Ok(vec![prompt]))));
    s.prompts.asked_at = NOT_DUE;
}

/// A phone asks: Continue, or Not me.
pub fn prompt_new(s: &mut State) {
    prompted(s, asking("native", "Pixel 9", "", "new", ""));
}

/// A browser asks, named by its origin.
pub fn prompt_web(s: &mut State) {
    prompted(s, asking("web", "", "https://chat.example.com", "new", ""));
}

/// Answered here, waiting for the phone to show its digits.
pub fn prompt_waiting(s: &mut State) {
    prompted(s, asking("native", "Pixel 9", "", "waiting", ""));
}

/// The digits to compare with the phone's.
pub fn prompt_compare(s: &mut State) {
    prompted(s, asking("native", "Pixel 9", "", "compare", "482 913"));
}

/// The phone's secret did not match what it committed to: never approve.
pub fn prompt_refused(s: &mut State) {
    prompted(s, asking("native", "Pixel 9", "", "refused", ""));
}

/// "They match", and the phone is linked: what the prompt says for a moment after. Staged,
/// as the approval itself would be refused with nothing to answer it.
pub fn prompt_linked(s: &mut State) {
    let phone = asking("native", "Pixel 9", "", "compare", "482 913");
    let words = crate::app::prompts::linked(&phone);
    prompted(s, phone);
    s.receive(&Update::Identity(IdentityUpdate::Prompts(Ok(Vec::new()))));
    s.prompts.ended = Some(crate::app::prompts::Ended {
        words,
        until: NOT_DUE,
    });
}

/// Drains until nothing has arrived for a moment, or three seconds.
fn quiet(s: &mut State) {
    let deadline = Instant::now() + Duration::from_secs(3);
    let mut since = Instant::now();
    while Instant::now() < deadline && Instant::now() - since < Duration::from_millis(500) {
        if s.drain() {
            since = Instant::now();
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}
