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
    // The menus and the profile card (`ui/menus.rs`).
    ("server-menu", |s| {
        servers_sample(s);
        s.open_server_menu();
    }),
    ("account-menu", |s| {
        servers_sample(s);
        s.open_account_menu();
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
    ("sign-in-browser", sign_in_browser),
    ("sign-in-waiting", |s| {
        sign_in_browser(s);
        // The passkey button pressed: the browser is out, and the card says so.
        let _ = s.begin_browser_sign_in(false);
    }),
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
    let member = |id: &str, handle: &str, name: &str, pronouns: &str, bio: &str| MemberSummary {
        account_id: id.into(),
        handle: handle.into(),
        display_name: name.into(),
        nickname: String::new(),
        pronouns: pronouns.into(),
        bio: bio.into(),
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
            member("me", "ada", "Ada Lovelace", "", ""),
            member(
                "grace",
                "grace",
                "Grace Hopper",
                "she/her",
                "Compilers, and the nanosecond in my pocket.",
            ),
        ],
    });
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
            servers: vec![ServerSummary {
                server_id: "acme".into(),
                name: "Acme Works".into(),
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
        servers: vec![ServerSummary {
            server_id: "acme".into(),
            name: "Acme Works".into(),
        }],
        channels: vec![ChannelSummary {
            channel_id: "general".into(),
            server_id: "acme".into(),
            name: "general".into(),
            kind: "TEXT".into(),
            last_seq: 0,
            participant_account_ids: Vec::new(),
            host: String::new(),
        }],
        pending_requests: Vec::new(),
    })));
    let member = |id: &str, name: &str| MemberSummary {
        account_id: id.into(),
        handle: id.into(),
        display_name: name.into(),
        nickname: String::new(),
        pronouns: String::new(),
        bio: String::new(),
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
        })
        .collect();
    state.session.revision += 1;
    // The sample ids are not real ones, and the store said so when the channel opened.
    state.session.last_error = None;
}
