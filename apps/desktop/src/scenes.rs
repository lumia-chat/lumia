//! Scenes: named states the window can be drawn in without anyone clicking there, for
//! Rune's headless tooling (`lumia --snap out.png --scene NAME`, `--tree`, `--script`).
//! A scene is a function that puts the state somewhere; [`SCENES`] is the list `main` hands
//! to Rune. Add one for a screen that is otherwise several presses away.

use lumia_client::app::{
    ChannelSummary, InvitePreviewSummary, InviteSummary, MemberSummary, ServerSummary,
    SnapshotSummary,
};

mod account;
mod replies;

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
    // Being in channels, and server DMs (`ui/channels.rs`, architecture 6.8).
    ("channels", channels_sample),
    ("channels-menu", |s| {
        channels_sample(s);
        s.open_channels_menu();
    }),
    ("browse", |s| {
        use crate::session::channels::ChannelUpdate;
        channels_sample(s);
        s.open_browse();
        let channel = |id: &str, name: &str, topic: &str, count| ChannelSummary {
            channel_id: id.into(),
            server_id: "acme".into(),
            name: name.into(),
            kind: "TEXT".into(),
            last_seq: 0,
            participant_account_ids: Vec::new(),
            host: String::new(),
            private: false,
            is_default: false,
            member_count: count,
            topic: topic.into(),
        };
        s.receive(&Update::Channel(ChannelUpdate::Browse {
            server_id: "acme".into(),
            channels: Ok(vec![
                channel("random", "random", "Anything at all", 12),
                channel("research", "research", "Papers worth reading", 4),
                channel("music", "music", "", 1),
            ]),
        }));
    }),
    ("channel-preview", |s| {
        use crate::session::channels::ChannelUpdate;
        channels_sample(s);
        let random = ChannelSummary {
            channel_id: "random".into(),
            server_id: "acme".into(),
            name: "random".into(),
            kind: "TEXT".into(),
            last_seq: 0,
            participant_account_ids: Vec::new(),
            host: String::new(),
            private: false,
            is_default: false,
            member_count: 12,
            topic: "Anything at all".into(),
        };
        s.receive(&Update::Channel(ChannelUpdate::Browse {
            server_id: "acme".into(),
            channels: Ok(vec![random]),
        }));
        s.preview_channel("random");
    }),
    ("server-dm", |s| {
        channels_sample(s);
        s.select_channel("dm-grace");
        s.session.entries.clear();
        s.session.revision += 1;
    }),
    ("add-people", |s| {
        channels_sample(s);
        s.open_add_people();
        s.toggle_chosen("bo");
    }),
    ("new-dm", |s| {
        channels_sample(s);
        s.open_new_dm();
    }),
    ("your-settings", |s| {
        channels_sample(s);
        s.open_your_settings();
        s.set_adds_by_invitation(true);
    }),
    ("person-in-server", |s| {
        channels_sample(s);
        s.open_person("grace", "member-grace");
    }),
    // The menus and the profile card (`ui/menus.rs`).
    ("server-menu", |s| {
        servers_sample(s);
        s.open_server_menu();
    }),
    ("account-menu", |s| {
        servers_sample(s);
        s.open_account_menu();
    }),
    // A window too narrow for the member list in the row (`app::aside`): it slides over the
    // conversation's end. Drawn at `--size 900x700`.
    ("members-over", |s| {
        servers_sample(s);
        s.set_window_width(900.0);
        s.toggle_aside();
    }),
    // A server's settings (`ui/server_settings.rs`): Acme with a picture, which the rail
    // draws too, and a shorter retention chosen and not yet saved, which says what it
    // deletes. The picture is the brand's icon standing in; no instance is asked for it.
    ("server-settings", |s| {
        use crate::session::avatars::AvatarUpdate;
        servers_sample(s);
        let url = format!("/api/v1/avatars/{}", "ac".repeat(32));
        if let Some(mut snapshot) = s.session.snapshot.clone() {
            snapshot.servers[0].icon_url.clone_from(&url);
            snapshot.servers[0].retention_days = 365;
            s.receive(&Update::Snapshot(Ok(snapshot)));
        }
        s.receive(&Update::Avatar(AvatarUpdate::Fetched {
            url,
            bytes: Ok(std::sync::Arc::from(
                &include_bytes!("../../../assets/brand/lumia-original-icon.png")[..],
            )),
        }));
        s.open_server_settings();
        s.choose_retention(30);
        s.tick(chrono::Utc::now().timestamp_micros());
        s.pictures.wait();
    }),
    ("person-card", |s| {
        servers_sample(s);
        s.open_person("grace", "member-grace");
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
    ("profile-photo", account::profile_photo),
    ("profile-none", account::profile_none),
    ("profile-live", account::profile_live),
    ("live-dm", live_dm),
    ("home", home),
    ("home-no-identity", home_no_identity),
    ("home-dm", home_dm),
    ("home-dm-outside", home_dm_outside),
    ("home-requests", home_requests),
    ("home-new-message", home_new_message),
    ("bodies", bodies),
    ("blocks", blocks),
    // Quote replies, reactions, mentions and read state (architecture 6.6).
    ("replies", replies::replies),
    ("mention-list", replies::mention_list),
    ("emoji-picker", replies::emoji_picker),
    ("message-toolbar", replies::message_toolbar),
    ("mentions", replies::mentions),
    // The same conversation at the other density. Set on the state, not through
    // `set_density`: a scene does not write the person's settings.
    ("blocks-compact", |s| {
        blocks(s);
        s.settings.density = crate::settings::Density::Compact;
        s.session.revision += 1;
    }),
    ("preferences", |s| {
        servers_sample(s);
        s.go_home(HomeView::Preferences);
    }),
    ("sign-in-browser", sign_in_browser),
    ("sign-in-waiting", |s| {
        sign_in_browser(s);
        // The passkey button pressed: the browser is out, and the card says so.
        let _ = s.begin_browser_sign_in(false);
    }),
];

/// A signed-in account with two servers, for the scenes that need no instance.
pub(crate) fn servers_sample(state: &mut State) {
    let channel = |id: &str, server: &str, name: &str| ChannelSummary {
        channel_id: id.into(),
        server_id: server.into(),
        name: name.into(),
        kind: "TEXT".into(),
        last_seq: 0,
        participant_account_ids: Vec::new(),
        host: String::new(),
        private: false,
        is_default: false,
        member_count: 0,
        topic: String::new(),
    };
    let member = |id: &str, handle: &str, name: &str, pronouns: &str, bio: &str| MemberSummary {
        account_id: id.into(),
        handle: handle.into(),
        display_name: name.into(),
        nickname: String::new(),
        pronouns: pronouns.into(),
        bio: bio.into(),
        avatar_url: String::new(),
        channel_adds_by_invitation: false,
        server_dms_off: false,
    };
    state.receive(&Update::SignedIn(Ok(())));
    state.receive(&Update::Snapshot(Ok(SnapshotSummary {
        account_id: "me".into(),
        handle: "ada".into(),
        display_name: "Ada Lovelace".into(),
        avatar_url: String::new(),
        servers: vec![
            ServerSummary {
                server_id: "acme".into(),
                name: "Acme Works".into(),
                icon_url: String::new(),
                retention_days: 0,
                can_manage: true,
                can_manage_channels: true,
                can_start_dms: true,
                channel_adds_by_invitation: false,
                server_dms_off: false,
                can_mention_everyone: false,
            },
            ServerSummary {
                server_id: "garden".into(),
                name: "Garden".into(),
                icon_url: String::new(),
                retention_days: 0,
                can_manage: false,
                can_manage_channels: false,
                can_start_dms: false,
                channel_adds_by_invitation: false,
                server_dms_off: false,
                can_mention_everyone: false,
            },
        ],
        channels: vec![
            channel("general", "acme", "general"),
            channel("design", "acme", "design"),
            channel("plots", "garden", "plots"),
        ],
        pending_requests: Vec::new(),
        channel_invitations: Vec::new(),
        read_states: Vec::new(),
    })));
    state.receive(&Update::Members {
        server_id: "acme".into(),
        members: vec![
            member("me", "ada", "Ada Lovelace", "", ""),
            member(
                "grace",
                "grace",
                "Grace Hopper",
                "she/her",
                "Compilers, and the nanosecond in my pocket.",
            ),
            MemberSummary {
                channel_adds_by_invitation: true,
                ..member("bo", "bo", "Bo Diddley", "", "")
            },
            MemberSummary {
                server_dms_off: true,
                ..member("cy", "cy", "Cy Twombly", "", "")
            },
        ],
    });
    for channel in ["general", "design"] {
        state
            .session
            .channel_members
            .insert(channel.into(), vec!["me".into(), "grace".into()]);
    }
}

/// [`servers_sample`] with what being in channels adds (architecture 6.8): a private
/// channel, a server DM with Grace, an invitation from her, and in #general the notices of
/// people coming and going between messages.
fn channels_sample(state: &mut State) {
    use crate::session::Entry;
    use lumia_client::app::ChannelInvitationSummary;
    servers_sample(state);
    let channel = |id: &str, name: &str| ChannelSummary {
        channel_id: id.into(),
        server_id: "acme".into(),
        name: name.into(),
        kind: "TEXT".into(),
        last_seq: 0,
        participant_account_ids: Vec::new(),
        host: String::new(),
        private: false,
        is_default: false,
        member_count: 0,
        topic: String::new(),
    };
    if let Some(mut snapshot) = state.session.snapshot.clone() {
        snapshot.channels.insert(
            2,
            ChannelSummary {
                private: true,
                ..channel("secret", "launch-plans")
            },
        );
        snapshot.channels.push(ChannelSummary {
            kind: "SERVER_DM".into(),
            participant_account_ids: vec!["me".into(), "grace".into()],
            ..channel("dm-grace", "")
        });
        snapshot.channel_invitations.push(ChannelInvitationSummary {
            channel: channel("research", "research"),
            invited_by_account_id: "grace".into(),
            created_at_micros: 0,
        });
        state.receive(&Update::Snapshot(Ok(snapshot)));
    }
    state.select_channel("general");
    let now = chrono::Utc::now().timestamp_micros();
    let entry = |id: &str, author: &str, body: &str, ago: i64, notice: &str, about: &str| Entry {
        id: id.into(),
        seq: 0,
        author_account_id: author.into(),
        body: body.into(),
        at_micros: now - ago * 60_000_000,
        deleted: false,
        notice: notice.into(),
        notice_about: about.into(),
        reply: None,
        mentions: Vec::new(),
        reactions: Vec::new(),
        mentions_me: false,
    };
    state.session.entries = vec![
        entry("n1", "grace", "", 40, "joined", "grace"),
        entry(
            "m1",
            "grace",
            "Morning. The build is green again.",
            38,
            "",
            "",
        ),
        entry("n2", "me", "", 20, "added", "bo"),
        entry(
            "m2",
            "me",
            "Bo, welcome: this is where we talk about the release.",
            19,
            "",
            "",
        ),
        entry("n3", "cy", "", 5, "left", "cy"),
    ];
    state.session.revision += 1;
}

/// The account card of an instance that names an OIDC provider, with the buttons that sign
/// in through the system browser (architecture 6.3). No instance is behind it: the state is
/// replaced by one over a scratch directory, so the scene never touches the person's own
/// data, and the trust card is given rather than asked for.
fn sign_in_browser(state: &mut State) {
    use std::time::{Duration, Instant};

    use crate::session::Update;
    use lumia_client::app::InstanceSummary;

    let scratch = std::env::temp_dir().join(format!("lumia-scene-{}", std::process::id()));
    *state = State::new(scratch);
    // Nothing listens on the discard port. Its refusal is taken first, so it cannot
    // arrive later and take the card away.
    if let Err(error) = state.session.open("http://127.0.0.1:9") {
        eprintln!("lumia: the sign-in scene could not open its scratch stores: {error}");
        return;
    }
    let deadline = Instant::now() + Duration::from_secs(2);
    while Instant::now() < deadline
        && !state
            .session
            .poll()
            .iter()
            .any(|update| matches!(update, Update::Info(_)))
    {
        std::thread::sleep(Duration::from_millis(5));
    }
    state.receive(&Update::Info(Ok(InstanceSummary {
        host: "chat.example.com".into(),
        operator_name: "Example Co".into(),
        operator_url: String::new(),
        terms_url: String::new(),
        privacy_url: String::new(),
        software_version: env!("CARGO_PKG_VERSION").into(),
        identity_proof_sign_in_enabled: false,
        identity_proof_create_enabled: false,
        open_registration: true,
        web_push_public_key: String::new(),
        magic_link_enabled: false,
        oidc_name: "Example SSO".into(),
    })));
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

/// The `live` scene at Home with the account's first personal conversation open, once the
/// engine has said what this device can read of it (or three seconds).
fn live_dm(state: &mut State) {
    use std::time::{Duration, Instant};
    live(state);
    state.go_home(HomeView::Conversations);
    let Some(channel_id) = state
        .session
        .personal_channels()
        .next()
        .map(|c| c.channel_id.clone())
    else {
        return;
    };
    state.open_dm(&channel_id);
    let deadline = Instant::now() + Duration::from_secs(3);
    while Instant::now() < deadline
        && (state.session.dm_log(&channel_id).is_none()
            || state.session.dm_member(&channel_id).is_none())
    {
        state.drain();
        std::thread::sleep(Duration::from_millis(20));
    }
}

// ---- Home, staged -------------------------------------------------------------------------
//
// Home's scenes have no instance behind them: what one would report is applied to the state,
// as the screen tests do. They replace the state with one over a directory of their own under
// the system's temporary directory, so a scene never writes to the person's data directory
// (the staged identity least of all).

fn home(state: &mut State) {
    staged::home(state, true, HomeView::Conversations, None);
}

fn home_no_identity(state: &mut State) {
    staged::home(state, false, HomeView::Conversations, None);
}

fn home_dm(state: &mut State) {
    staged::home(
        state,
        true,
        HomeView::Conversations,
        Some(staged::WITH_GRACE),
    );
}

fn home_dm_outside(state: &mut State) {
    staged::home(
        state,
        true,
        HomeView::Conversations,
        Some(staged::WITH_ALAN),
    );
}

fn home_requests(state: &mut State) {
    staged::home(state, true, HomeView::Requests, None);
}

fn home_new_message(state: &mut State) {
    staged::home(state, true, HomeView::NewMessage, None);
    state.home.note =
        "Request sent to Katherine Johnson. The conversation opens when they accept.".into();
}

mod staged {
    use lumia_client::app::{
        ChannelSummary, DmChannelSummary, DmLogSummary, DmMessageSummary, DmRequestSummary,
        MemberSummary, ServerSummary, SnapshotSummary,
    };

    use crate::app::{HomeView, Route, State};
    use crate::session::dms::DmUpdate;
    use crate::session::{Connection, Update};

    const ME: &str = "ada";
    /// The conversation this computer is in, and the one it is not.
    pub const WITH_GRACE: &str = "with-grace";
    pub const WITH_ALAN: &str = "with-alan";

    fn person(id: &str, name: &str, pronouns: &str, bio: &str) -> MemberSummary {
        MemberSummary {
            account_id: id.into(),
            handle: id.into(),
            display_name: name.into(),
            nickname: String::new(),
            pronouns: pronouns.into(),
            bio: bio.into(),
            avatar_url: String::new(),
            channel_adds_by_invitation: false,
            server_dms_off: false,
        }
    }

    fn channel(id: &str, server: &str, name: &str, host: &str, with: &[&str]) -> ChannelSummary {
        ChannelSummary {
            channel_id: id.into(),
            server_id: server.into(),
            name: name.into(),
            kind: if with.is_empty() {
                "TEXT"
            } else {
                "PERSONAL_DM"
            }
            .into(),
            last_seq: 0,
            participant_account_ids: with.iter().map(|id| (*id).to_string()).collect(),
            host: host.into(),
            private: false,
            is_default: false,
            member_count: 0,
            topic: String::new(),
        }
    }

    fn request(id: &str, from: &str, shared: usize, friends: u32) -> DmRequestSummary {
        DmRequestSummary {
            request_id: id.into(),
            from_account_id: from.into(),
            to_account_id: ME.into(),
            state: "PENDING".into(),
            shared_server_ids: vec!["acme".to_string(); shared],
            mutual_friend_count: friends,
        }
    }

    fn snapshot() -> SnapshotSummary {
        SnapshotSummary {
            account_id: ME.into(),
            handle: ME.into(),
            display_name: "Ada Lovelace".into(),
            avatar_url: String::new(),
            servers: vec![ServerSummary {
                server_id: "acme".into(),
                name: "Acme Works".into(),
                icon_url: String::new(),
                retention_days: 0,
                can_manage: true,
                can_manage_channels: false,
                can_start_dms: false,
                channel_adds_by_invitation: false,
                server_dms_off: false,
                can_mention_everyone: false,
            }],
            channels: vec![
                channel("general", "acme", "general", "", &[]),
                channel(WITH_GRACE, "", "", "", &[ME, "grace"]),
                channel("with-edsger", "", "", "", &[ME, "edsger"]),
                channel(WITH_ALAN, "", "", "chat.turing.example", &[ME, "alan"]),
            ],
            pending_requests: vec![
                request("r1", "barbara", 1, 2),
                request("r2", "donald", 0, 0),
            ],
            channel_invitations: Vec::new(),
            read_states: Vec::new(),
        }
    }

    fn people() -> Vec<MemberSummary> {
        vec![
            person(ME, "Ada Lovelace", "she/her", ""),
            person(
                "grace",
                "Grace Hopper",
                "she/her",
                "Compilers, and the occasional moth.",
            ),
            person("edsger", "Edsger Dijkstra", "", ""),
            person("barbara", "Barbara Liskov", "", ""),
            person("katherine", "Katherine Johnson", "", ""),
        ]
    }

    fn said(id: &str, mine: bool, body: &str, micros: i64) -> DmMessageSummary {
        DmMessageSummary {
            message_id: id.into(),
            seq: 0,
            sender_identity_id: String::new(),
            sender_device_id: String::new(),
            sender_account_id: if mine { ME } else { "grace" }.into(),
            body: body.into(),
            sent_at_micros: micros,
            edited: false,
            deleted: false,
            mine,
            reply: None,
            reactions: Vec::new(),
        }
    }

    /// Applies everything that finishes until nothing has for a moment. What a staged state
    /// starts (lookups nobody answers, the engine reading its empty store) would otherwise
    /// land on top of what the scene staged.
    fn quiet(state: &mut State) {
        use std::time::{Duration, Instant};
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut since = Instant::now();
        while Instant::now() < deadline && Instant::now() - since < Duration::from_millis(300) {
            if state.drain() {
                since = Instant::now();
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    /// What no snapshot carries: the server's members, and the profiles of the people no
    /// server lists.
    fn stage_people(state: &mut State) {
        state.receive(&Update::Members {
            server_id: "acme".into(),
            members: people(),
        });
        for stranger in [
            person(
                "alan",
                "Alan Turing",
                "he/him",
                "Machines and morphogenesis.",
            ),
            person("donald", "Donald Knuth", "", ""),
        ] {
            state.receive(&Update::Profile {
                account_id: stranger.account_id.clone(),
                profile: Some(stranger),
            });
        }
    }

    /// Signed in at Home on a computer that holds an identity, or does not; `open` is the
    /// personal conversation on screen.
    pub fn home(state: &mut State, identity: bool, view: HomeView, open: Option<&str>) {
        let dir = std::env::temp_dir().join(format!("lumia-scene-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        *state = State::new(dir.clone());
        state.sign_in.instance.replace_text("http://127.0.0.1:9");
        state.connect();
        if identity && let Some(app) = state.session.client() {
            let made = tokio::runtime::Builder::new_current_thread()
                .build()
                .map_err(|e| e.to_string())
                .and_then(|runtime| {
                    runtime
                        .block_on(app.identity_create("scene"))
                        .map_err(|e| e.to_string())
                });
            if let Err(error) = made {
                eprintln!("lumia: the scene's identity: {error}");
            }
            // The session reads whether the stores hold an identity when it opens them, so
            // they are opened again now that they do.
            *state = State::new(dir);
            state.sign_in.instance.replace_text("http://127.0.0.1:9");
            state.connect();
        }
        state.session.signed_in = true;
        state.route = Route::Shell;
        stage_people(state);
        state.receive(&Update::Snapshot(Ok(snapshot())));
        state.go_home(view);
        if let Some(channel_id) = open {
            state.open_dm(channel_id);
        }
        quiet(state);

        stage_people(state);
        let channels = [
            (WITH_GRACE, true),
            ("with-edsger", true),
            (WITH_ALAN, false),
        ]
        .into_iter()
        .map(|(id, member)| DmChannelSummary {
            channel_id: id.into(),
            participant_account_ids: Vec::new(),
            member,
        })
        .collect();
        state.receive(&Update::Dm(DmUpdate::Channels(Ok(channels))));
        let now = chrono::Utc::now().timestamp_micros();
        let minute = 60_000_000;
        state.receive(&Update::Dm(DmUpdate::Log {
            channel_id: WITH_GRACE.into(),
            log: Ok(DmLogSummary {
                messages: vec![
                    said("m1", false, "Did the build go out?", now - 26 * 60 * minute),
                    said(
                        "m2",
                        true,
                        "On **Friday**, with the fix for the moth.",
                        now - 25 * 60 * minute,
                    ),
                    said(
                        "m3",
                        false,
                        "Good. I will read the log tonight.",
                        now - 9 * minute,
                    ),
                    said("m4", true, "It is short this time.", now - 2 * minute),
                ],
                before_join: 3,
                undecryptable: 0,
            }),
        }));
        state.session.connection = Connection::Connected;
        state.session.last_error = None;
    }
}

/// What the `bodies` scene lists: every construct of the protocol's Markdown subset
/// (protocol 14), what is outside it and must stay as typed, and a line per script.
pub const BODIES: &[&str] = &[
    "A paragraph with *emphasis*, **strong**, ~~strike~~ and inline `code`, and a link to \
     [the architecture](https://example.com/architecture) beside a bare one, https://example.com/plan.\n\n\
     A second paragraph,\non two lines.",
    "```rust\n// A fenced block with a language.\nfn main() {\n    let answer = 42;\n    \
     println!(\"the answer is {answer}\");\n}\n```\n\n```\nand one without\n```",
    "> A quote, with **strong** inside it.\n>\n> Its second paragraph.\n\nAnd the reply under it.",
    "- an unordered list\n- with a nested one\n  - first\n  - second\n- and a last item\n\n\
     1. an ordered list\n2. its second item\n   1. nested\n   2. numbers\n3. its third",
    "# Not a heading\n\n<b>not bold</b> and ![not an image](https://example.com/cat.png)\n\n\
     | not | a table |\n|-----|---------|\n| 1 | 2 |\n\n[not a web link](file:///etc/passwd)",
    "日本語のメッセージです。句読点は、どうなりますか。",
    "这是一条中文消息，用来检查换行和标点。",
    "한국어 메시지입니다. 줄바꿈과 문장 부호를 확인합니다.",
    "هذه رسالة باللغة العربية مع `code` للتحقق من الاتجاه.",
    "Сообщение на русском языке, чтобы проверить кириллицу.",
    "Emoji with joiners: 👍🏽 👨‍👩‍👧‍👦 🏳️‍🌈 🧑🏻‍💻",
    "A word too long for the line: https://example.com/a/very/long/path/that/keeps/going/and/going/\
     and/going/until/it/is/wider/than/the/conversation/and/has/to/break/somewhere/in/the/middle\n\n\
     ```\nlet line = \"and a line of code that is longer than the block is wide, which wraps rather than running out of its frame\";\n```",
];

/// The `blocks` scene: a channel where people send several messages close together, with no
/// instance behind it. What one person sends within minutes is one block under one name; a
/// longer pause, another person or a send still on its way each show what they do to it.
/// Under the composer, someone is typing.
pub fn blocks(state: &mut State) {
    use crate::session::{Entry, Pending};
    use lumia_client::app::AppEvent;
    servers_sample(state);
    let now = chrono::Utc::now().timestamp_micros();
    let minutes_ago = |minutes: i64| now - minutes * 60_000_000;
    let said = [
        ("grace", 58, "Morning! The compiler notes are up."),
        ("grace", 58, "Also: the nanosecond wires arrived."),
        (
            "grace",
            57,
            "I will bring a few on Thursday, one for everyone who asks.",
        ),
        ("me", 50, "Wonderful, save me one."),
        ("me", 49, "Reading the notes now."),
        ("grace", 12, "Any questions so far?"),
        ("me", 2, "Just one, about the loader."),
    ];
    state.session.entries = said
        .iter()
        .enumerate()
        .map(|(i, (author, ago, body))| Entry {
            id: format!("k{i}"),
            seq: i as u64,
            author_account_id: (*author).into(),
            body: (*body).into(),
            at_micros: minutes_ago(*ago),
            deleted: false,
            notice: String::new(),
            notice_about: String::new(),
            reply: None,
            mentions: Vec::new(),
            reactions: Vec::new(),
            mentions_me: false,
        })
        .collect();
    state.session.pending = vec![Pending {
        id: "k-pending".into(),
        channel_id: "general".into(),
        body: "Does it relocate before or after linking?".into(),
        sent_at_micros: now,
        failed: None,
        reply: None,
    }];
    state.session.revision += 1;
    state.receive(&Update::Gateway(AppEvent::Typing {
        channel_id: "general".into(),
        account_id: "grace".into(),
        until_micros: now + 8_000_000,
    }));
}

/// The `bodies` scene: a channel whose messages are [`BODIES`], with no instance behind it.
/// No instance is opened, so nothing reaches the network. A tall window shows them all:
/// `--snap out.png --scene bodies --size 1180x1900`.
pub fn bodies(state: &mut State) {
    use crate::session::{Entry, Update};
    use lumia_client::app::{ChannelSummary, MemberSummary, ServerSummary, SnapshotSummary};
    state.route = Route::Shell;
    state.receive(&Update::Snapshot(Ok(SnapshotSummary {
        account_id: "me".into(),
        handle: "ada".into(),
        display_name: "Ada Lovelace".into(),
        avatar_url: String::new(),
        servers: vec![ServerSummary {
            server_id: "acme".into(),
            name: "Acme Works".into(),
            icon_url: String::new(),
            retention_days: 0,
            can_manage: true,
            can_manage_channels: false,
            can_start_dms: false,
            channel_adds_by_invitation: false,
            server_dms_off: false,
            can_mention_everyone: false,
        }],
        channels: vec![ChannelSummary {
            channel_id: "general".into(),
            server_id: "acme".into(),
            name: "general".into(),
            kind: "TEXT".into(),
            last_seq: 0,
            participant_account_ids: Vec::new(),
            host: String::new(),
            private: false,
            is_default: false,
            member_count: 0,
            topic: String::new(),
        }],
        pending_requests: Vec::new(),
        channel_invitations: Vec::new(),
        read_states: Vec::new(),
    })));
    let member = |id: &str, name: &str| MemberSummary {
        account_id: id.into(),
        handle: id.into(),
        display_name: name.into(),
        nickname: String::new(),
        pronouns: String::new(),
        bio: String::new(),
        avatar_url: String::new(),
        channel_adds_by_invitation: false,
        server_dms_off: false,
    };
    state.receive(&Update::Members {
        server_id: "acme".into(),
        members: vec![
            member("me", "Ada Lovelace"),
            member("grace", "Grace Hopper"),
        ],
    });
    let now = chrono::Utc::now().timestamp_micros();
    state.session.entries = BODIES
        .iter()
        .enumerate()
        .map(|(i, body)| Entry {
            id: format!("b{i}"),
            seq: i as u64,
            author_account_id: if i.is_multiple_of(2) { "grace" } else { "me" }.into(),
            body: (*body).into(),
            at_micros: now - ((BODIES.len() - i) as i64) * 60_000_000,
            deleted: false,
            notice: String::new(),
            notice_about: String::new(),
            reply: None,
            mentions: Vec::new(),
            reactions: Vec::new(),
            mentions_me: false,
        })
        .collect();
    state.session.revision += 1;
    // The sample ids are not real ones, and the store said so when the channel opened.
    state.session.last_error = None;
}
