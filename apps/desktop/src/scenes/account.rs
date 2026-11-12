//! The account screens' scenes: sign-in with the magic link, linking this computer, the
//! devices and the profile, each over a state no instance is behind (what an instance
//! would report is applied with `State::receive`). The `*-live` ones wait for a real
//! instance instead, with the `LUMIA_DEV_*` environment.

use std::time::{Duration, Instant};

use lumia_client::app::{
    DeviceSummary, InstanceSummary, PasskeySummary, ProfileSummary, RosterSummary, SnapshotSummary,
};

use crate::app::{HomeView, State};
use crate::session::Update;
use crate::session::identity::IdentityUpdate;

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
        servers: Vec::new(),
        channels: Vec::new(),
        pending_requests: Vec::new(),
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

/// Signed in, and this computer holds no identity.
pub fn devices_none(s: &mut State) {
    signed_in(s);
    s.go_home(HomeView::Devices);
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
