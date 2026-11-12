//! Scenes: named states the window can be drawn in without anyone clicking there, for
//! Rune's headless tooling (`lumia --snap out.png --scene NAME`, `--tree`, `--script`).
//! A scene is a function that puts the state somewhere; [`SCENES`] is the list `main` hands
//! to Rune. Add one for a screen that is otherwise several presses away.

use lumia_client::app::{
    ChannelSummary, InvitePreviewSummary, InviteSummary, MemberSummary, ServerSummary,
    SnapshotSummary,
};

mod account;

use crate::app::{HomeView, Route, State};
use crate::session::servers::ServerUpdate;
use crate::session::{Connection, Update};

/// A scene: its name for `--scene`, and what it does to the state.
pub type Scene = (&'static str, fn(&mut State));

/// Every scene.
pub const SCENES: &[Scene] = &[
    ("live", live),
    ("live-home", |s| {
        live(s);
        s.go_home(HomeView::Conversations);
    }),
    // Making and managing servers, over a sample account with no instance behind it.
    ("add-server", |s| {
        servers_sample(s);
        s.go_home(HomeView::AddServer);
    }),
    ("add-server-preview", |s| {
        servers_sample(s);
        s.go_home(HomeView::AddServer);
        s.server_forms.add.name.replace_text("Our place");
        s.server_forms
            .add
            .invite
            .replace_text("https://chat.example.com/invite/k3v9x2");
        s.receive(&Update::Server(ServerUpdate::InvitePreview(Ok(
            InvitePreviewSummary {
                code: "k3v9x2".into(),
                server_id: "garden".into(),
                server_name: "Garden".into(),
                channel_name: String::new(),
                member_count: 12,
                as_guest: false,
                host: "chat.example.com".into(),
                operator_name: "Example Co".into(),
                open_registration: true,
                identity_proof_create_enabled: false,
            },
        ))));
    }),
    ("add-server-refused", |s| {
        servers_sample(s);
        s.go_home(HomeView::AddServer);
        s.server_forms.add.invite.replace_text("nothing");
        s.server_forms.add.error = "That invite is unknown, expired or used up.".into();
    }),
    ("new-channel", |s| {
        servers_sample(s);
        s.open_new_channel();
        s.server_forms.channel.name.replace_text("planning");
    }),
    ("invite", |s| {
        servers_sample(s);
        s.open_invites();
    }),
    ("invite-made", |s| {
        servers_sample(s);
        s.open_invites();
        let invite = |code: &str, max_uses, uses, hours: Option<i64>, email: &str| InviteSummary {
            code: code.into(),
            url: format!("https://chat.example.com/invite/{code}"),
            server_id: "acme".into(),
            expires_at_micros: hours
                .map(|h| chrono::Utc::now().timestamp_micros() + h * 3_600_000_000),
            max_uses,
            uses,
            email: email.into(),
        };
        s.receive(&Update::Server(ServerUpdate::Invites {
            server_id: "acme".into(),
            invites: Ok(vec![
                invite("p8m2q7", 1, 0, Some(168), "pat@example.com"),
                invite("h4t6w1", 0, 3, None, ""),
            ]),
        }));
        s.set_invite_expiry(24);
        s.set_invite_uses(10);
        s.receive(&Update::Server(ServerUpdate::InviteCreated {
            server_id: "acme".into(),
            invite: Ok(invite("k3v9x2", 10, 0, Some(24), "")),
        }));
        s.server_forms.invite.emailed = "pat@example.com".into();
    }),
    // The account screens (`scenes/account.rs`).
    ("sign-in", account::sign_in),
    ("magic-sent", account::magic_sent),
    ("link", account::link),
    ("link-request", account::link_request),
    ("devices", account::devices),
    ("devices-revoke", account::devices_revoke),
    ("devices-none", account::devices_none),
    ("devices-live", account::devices_live),
    ("profile", account::profile),
    ("profile-none", account::profile_none),
    ("profile-live", account::profile_live),
];

/// A signed-in account with two servers, for the scenes that need no instance.
fn servers_sample(state: &mut State) {
    let channel = |id: &str, server: &str, name: &str| ChannelSummary {
        channel_id: id.into(),
        server_id: server.into(),
        name: name.into(),
        kind: "TEXT".into(),
        last_seq: 0,
        participant_account_ids: Vec::new(),
        host: String::new(),
    };
    let member = |id: &str, handle: &str, name: &str| MemberSummary {
        account_id: id.into(),
        handle: handle.into(),
        display_name: name.into(),
        nickname: String::new(),
        pronouns: String::new(),
        bio: String::new(),
    };
    state.receive(&Update::SignedIn(Ok(())));
    state.receive(&Update::Snapshot(Ok(SnapshotSummary {
        account_id: "me".into(),
        handle: "ada".into(),
        display_name: "Ada Lovelace".into(),
        servers: vec![
            ServerSummary {
                server_id: "acme".into(),
                name: "Acme Works".into(),
            },
            ServerSummary {
                server_id: "garden".into(),
                name: "Garden".into(),
            },
        ],
        channels: vec![
            channel("general", "acme", "general"),
            channel("design", "acme", "design"),
            channel("plots", "garden", "plots"),
        ],
        pending_requests: Vec::new(),
    })));
    state.receive(&Update::Members {
        server_id: "acme".into(),
        members: vec![
            member("me", "ada", "Ada Lovelace"),
            member("grace", "grace", "Grace Hopper"),
        ],
    });
}

/// The `live` scene, for a headless picture of the real thing (`--snap out.png --scene live`
/// with the `LUMIA_DEV_*` environment): waits, on the wall clock, until the instance has
/// answered and the open channel has its messages, or ten seconds. A headless run has a
/// virtual clock and one frame, so nothing else would give the network time to answer.
pub fn live(state: &mut State) {
    use std::time::{Duration, Instant};
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut settled = None;
    while Instant::now() < deadline {
        if state.drain() {
            settled = None;
        }
        let loaded = state.route == Route::Shell
            && state.session.connection == Connection::Connected
            && state.session.snapshot.as_ref().is_some_and(|snap| {
                snap.servers
                    .iter()
                    .all(|x| state.session.members.contains_key(&x.server_id))
            });
        // Loaded, and quiet for a moment: the channel's replay has landed.
        match (loaded, settled) {
            (true, Some(since)) if Instant::now() - since > Duration::from_millis(600) => return,
            (true, None) => settled = Some(Instant::now()),
            (false, _) => settled = None,
            _ => {}
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    eprintln!(
        "lumia: the live scene gave up waiting; connection {:?}, error {:?}",
        state.session.connection, state.session.last_error
    );
}
