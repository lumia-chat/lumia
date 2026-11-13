//! What the screen tests share: a state no instance is behind, a windowless `Runtime` over
//! it, the gestures a person makes, and what an instance would have reported. A test file
//! takes it with `mod common;`.

#![allow(dead_code, clippy::expect_used, clippy::unwrap_used)]

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};

use lumia_client::app::{
    ChannelSummary, InstanceSummary, MemberSummary, ServerSummary, SnapshotSummary,
};
use lumia_shell::app::State;
use lumia_shell::session::{Entry, Failure, Update};
use lumia_shell::ui;
use rune::{Env, Key, KeyEvent, Mods, Runtime};

pub fn data_dir() -> PathBuf {
    static NEXT: AtomicU32 = AtomicU32::new(0);
    let dir = std::env::temp_dir().join(format!(
        "lumia-shell-test-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

pub fn runtime(state: State) -> Runtime<State> {
    let mut rt = Runtime::new(state, ui::view, (1180.0, 760.0), Env::new(false, 2.0));
    settle(&mut rt);
    rt
}

pub fn settle(rt: &mut Runtime<State>) {
    rt.frame(0.0);
    for _ in 0..20 {
        rt.frame(1.0 / 60.0);
    }
}

/// Presses the element keyed `key`, as a person does: scrolled to first when it is in a
/// list or a card taller than the window.
pub fn press(rt: &mut Runtime<State>, key: &str) {
    let target = key.to_string();
    rt.update(move |_, tx| tx.scroll_into_view(target));
    settle(rt);
    let (x, y) = rt
        .find(key)
        .unwrap_or_else(|| panic!("{key} is not on screen"));
    rt.pointer_move(x, y);
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    settle(rt);
}

/// "New channel", from the menu under the server's name.
pub fn open_new_channel(rt: &mut Runtime<State>) {
    press(rt, "server-title");
    press(rt, "menu-new-channel");
}

/// "Invite people", from the menu under the server's name.
pub fn open_invites(rt: &mut Runtime<State>) {
    press(rt, "server-title");
    press(rt, "menu-invite");
}

/// The profile page, from the account menu on the rail's self avatar.
pub fn open_profile(rt: &mut Runtime<State>) {
    press(rt, "self");
    press(rt, "menu-profile");
}

pub fn type_text(rt: &mut Runtime<State>, text: &str) {
    for c in text.chars() {
        rt.key(KeyEvent {
            key: Key::Text(c.to_string()),
            mods: Mods::default(),
        });
    }
    settle(rt);
}

/// Every text and label on screen, to look for wording in.
pub fn words(rt: &Runtime<State>) -> String {
    rt.elements()
        .into_iter()
        .flat_map(|e| [e.text, e.label])
        .flatten()
        .collect::<Vec<_>>()
        .join("\n")
}

pub fn info() -> InstanceSummary {
    InstanceSummary {
        host: "chat.example.com".into(),
        operator_name: "Example Co".into(),
        operator_url: String::new(),
        terms_url: String::new(),
        privacy_url: String::new(),
        software_version: "0.2.1".into(),
        identity_proof_sign_in_enabled: false,
        identity_proof_create_enabled: false,
        open_registration: true,
        web_push_public_key: String::new(),
        magic_link_enabled: false,
        oidc_name: String::new(),
    }
}

/// A state with an instance open that nothing answers for, its trust card as given.
pub fn opened() -> State {
    let mut state = State::new(data_dir());
    state.sign_in.instance.replace_text("http://127.0.0.1:9");
    state.connect();
    assert!(
        state.session.is_open(),
        "opening needs only the local stores: {}",
        state.sign_in.error
    );
    state.receive(&Update::Info(Ok(info())));
    state
}

pub fn refusal(code: &str, details: &[(&str, &str)]) -> Update {
    let details: HashMap<String, String> = details
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();
    Update::SignedIn(Err(Failure {
        code: code.into(),
        message: format!("{code} from the instance"),
        details,
    }))
}

pub fn snapshot() -> SnapshotSummary {
    let channel = |id: &str, server: &str, name: &str, kind: &str| ChannelSummary {
        channel_id: id.into(),
        server_id: server.into(),
        name: name.into(),
        kind: kind.into(),
        last_seq: 0,
        participant_account_ids: Vec::new(),
        host: String::new(),
        private: false,
        is_default: false,
        member_count: 0,
        topic: String::new(),
    };
    SnapshotSummary {
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
                can_manage_channels: false,
                can_start_dms: false,
                channel_adds_by_invitation: false,
                server_dms_off: false,
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
            },
        ],
        channels: vec![
            channel("general", "acme", "general", "TEXT"),
            channel("design", "acme", "design", "TEXT"),
            channel("plots", "garden", "plots", "TEXT"),
        ],
        pending_requests: Vec::new(),
        channel_invitations: Vec::new(),
    }
}

pub fn member(id: &str, handle: &str, name: &str) -> MemberSummary {
    MemberSummary {
        account_id: id.into(),
        handle: handle.into(),
        display_name: name.into(),
        nickname: String::new(),
        pronouns: String::new(),
        bio: String::new(),
        avatar_url: String::new(),
        channel_adds_by_invitation: false,
        server_dms_off: false,
    }
}

pub fn message(id: &str, author: &str, body: &str, micros: i64) -> Entry {
    Entry {
        id: id.into(),
        seq: 0,
        author_account_id: author.into(),
        body: body.into(),
        at_micros: micros,
        deleted: false,
    }
}

/// Signed in, with two servers and their members, and three messages in the open channel.
pub fn signed_in() -> State {
    let mut state = opened();
    state.receive(&Update::SignedIn(Ok(())));
    state.receive(&Update::Snapshot(Ok(snapshot())));
    state.receive(&Update::Members {
        server_id: "acme".into(),
        members: vec![
            member("me", "ada", "Ada Lovelace"),
            member("grace", "grace", "Grace Hopper"),
        ],
    });
    let now = chrono::Utc::now().timestamp_micros();
    let day = 86_400_000_000;
    state.session.entries = vec![
        message("m1", "grace", "Shipped on **Friday**.", now - 3 * day),
        message("m2", "me", "Reading it now.", now - 60_000_000),
        message("m3", "stranger", "Who am I?", now),
    ];
    state.session.revision += 1;
    state
}
