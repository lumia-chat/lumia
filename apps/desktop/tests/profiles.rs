//! A changed name or picture (#15), over a state no instance is behind: the core's
//! `AppEvent::ProfileUpdated` reaches everything that shows the person at once, the
//! messages, the member list, a server DM's row, a mention, a quote and who is typing, with
//! no member list read again, and their picture follows its new address. A server's
//! nickname stays that server's, and nobody else changes.

#![allow(clippy::expect_used, clippy::unwrap_used)]

mod common;

use std::sync::Arc;

use common::*;
use lumia_client::app::{AppEvent, ChannelSummary, MentionSummary, QuoteSummary};
use lumia_shell::app::State;
use lumia_shell::session::Update;
use lumia_shell::session::avatars::AvatarUpdate;
use rune::Runtime;

fn avatar(n: &str) -> String {
    format!("/api/v1/avatars/{}", n.repeat(32))
}

/// What the core hands on when `account_id` is now `name` with the picture at `avatar_url`.
fn renamed(account_id: &str, handle: &str, name: &str, avatar_url: &str) -> Update {
    Update::Gateway(AppEvent::ProfileUpdated {
        account_id: account_id.into(),
        handle: handle.into(),
        display_name: name.into(),
        avatar_url: avatar_url.into(),
        pronouns: "she/her".into(),
        bio: "Wrote the first compiler.".into(),
    })
}

fn receive(rt: &mut Runtime<State>, update: Update) {
    rt.update(move |s, _| s.receive(&update));
    draw(rt);
}

/// What a window's poll does before every frame, the pictures held decoded for it.
fn draw(rt: &mut Runtime<State>) {
    rt.update(|s, _| {
        s.tick(chrono::Utc::now().timestamp_micros());
    });
    settle(rt);
    rt.state.pictures.wait();
    settle(rt);
}

fn picture_bytes() -> Arc<[u8]> {
    Arc::from(
        std::fs::read(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../assets/brand/lumia-original-icon.png"
        ))
        .unwrap(),
    )
}

fn label_of(rt: &Runtime<State>, key: &str) -> String {
    rt.elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some(key))
        .and_then(|e| e.label)
        .unwrap_or_else(|| panic!("{key} is not on screen"))
}

fn pictures_drawn(rt: &Runtime<State>) -> usize {
    rt.elements()
        .into_iter()
        .filter(|e| e.key.as_deref() == Some("picture"))
        .count()
}

/// Signed in with Grace in both servers (a nickname in Garden), her picture held, a server
/// DM with her, a reply quoting her that mentions her, and her typing in the open channel.
fn with_grace() -> Runtime<State> {
    let old = avatar("aa");
    let mut state = signed_in();
    // Put in what was held, not received: a snapshot would read the open channel again.
    let snapshot = state.session.snapshot.as_mut().unwrap();
    snapshot.channels.push(ChannelSummary {
        channel_id: "dm1".into(),
        server_id: "acme".into(),
        name: String::new(),
        kind: "SERVER_DM".into(),
        last_seq: 0,
        participant_account_ids: vec!["me".into(), "grace".into()],
        host: String::new(),
        private: false,
        is_default: false,
        member_count: 0,
        topic: String::new(),
    });
    let mut grace = member("grace", "grace", "Grace Hopper");
    grace.avatar_url.clone_from(&old);
    let mut admiral = grace.clone();
    admiral.nickname = "Admiral".into();
    admiral.channel_adds_by_invitation = true;
    state.receive(&Update::Members {
        server_id: "acme".into(),
        members: vec![member("me", "ada", "Ada Lovelace"), grace],
    });
    state.receive(&Update::Members {
        server_id: "garden".into(),
        members: vec![member("me", "ada", "Ada Lovelace"), admiral],
    });
    state.receive(&Update::Avatar(AvatarUpdate::Fetched {
        url: old,
        bytes: Ok(picture_bytes()),
    }));
    let reply = &mut state.session.entries[1];
    reply.body = "@grace reading it now.".into();
    reply.mentions = vec![MentionSummary {
        account_id: "grace".into(),
        everyone: false,
        start: 0,
        end: 6,
    }];
    reply.reply = Some(QuoteSummary {
        message_id: "m1".into(),
        author_account_id: "grace".into(),
        text: "Shipped on Friday.".into(),
        original_deleted: false,
        silent: false,
    });
    state.session.revision += 1;
    state.receive(&Update::Gateway(AppEvent::Typing {
        channel_id: "general".into(),
        account_id: "grace".into(),
        until_micros: chrono::Utc::now().timestamp_micros() + 60_000_000,
    }));
    let mut rt = runtime(state);
    draw(&mut rt);
    rt
}

#[test]
fn a_new_name_and_picture_show_everywhere_at_once() {
    let mut rt = with_grace();
    let shown = words(&rt);
    assert!(shown.contains("Grace Hopper is typing…"), "{shown}");
    assert_eq!(label_of(&rt, "dm-dm1"), "Conversation with Grace Hopper");
    assert_eq!(label_of(&rt, "member-grace"), "Grace Hopper, offline");
    let before = pictures_drawn(&rt);
    assert!(
        before >= 2,
        "her picture in the list and on her message: {before}"
    );

    let new = avatar("bb");
    receive(&mut rt, renamed("grace", "amazing", "Amazing Grace", &new));
    let shown = words(&rt);
    assert!(
        !shown.contains("Grace Hopper"),
        "nothing names her as she was: {shown}"
    );
    assert!(shown.contains("Amazing Grace is typing…"), "{shown}");
    assert!(shown.contains("@Amazing Grace"), "the mention: {shown}");
    assert!(
        shown.lines().filter(|l| *l == "Amazing Grace").count() >= 2,
        "her message and the member list: {shown}"
    );
    assert!(
        shown.contains("@amazing · she/her"),
        "the list's handle: {shown}"
    );
    assert_eq!(label_of(&rt, "dm-dm1"), "Conversation with Amazing Grace");
    assert_eq!(label_of(&rt, "member-grace"), "Amazing Grace, offline");

    // Every list that holds her has her as she is, each server's nickname and switches kept.
    let session = &rt.state.session;
    let acme = &session.members["acme"][1];
    let garden = &session.members["garden"][1];
    for held in [acme, garden, &session.people["grace"]] {
        assert_eq!(
            (held.handle.as_str(), held.display_name.as_str()),
            ("amazing", "Amazing Grace")
        );
        assert_eq!(held.avatar_url, new);
        assert_eq!(held.pronouns, "she/her");
        assert_eq!(held.bio, "Wrote the first compiler.");
    }
    assert_eq!(garden.nickname, "Admiral");
    assert!(garden.channel_adds_by_invitation);
    assert_eq!(acme.nickname, "");

    // The new picture is a new address, asked for as any other: until its bytes arrive she
    // has her initials, not the picture she had.
    assert_eq!(session.avatar_url_of("grace"), new);
    assert!(session.picture_of("grace").is_none());
    assert_eq!(pictures_drawn(&rt), 0);
    assert!(words(&rt).lines().any(|l| l == "AG"), "her initials");
    receive(
        &mut rt,
        Update::Avatar(AvatarUpdate::Fetched {
            url: new.clone(),
            bytes: Ok(picture_bytes()),
        }),
    );
    assert_eq!(rt.state.session.picture_of("grace"), Some(new.as_str()));
    assert_eq!(pictures_drawn(&rt), before);

    // A picture taken away is the initials again.
    receive(&mut rt, renamed("grace", "amazing", "Amazing Grace", ""));
    assert_eq!(rt.state.session.avatar_url_of("grace"), "");
    assert_eq!(pictures_drawn(&rt), 0);
}

#[test]
fn nobody_else_changes() {
    let mut rt = with_grace();
    let session = &rt.state.session;
    // Compared as written out: the core's summaries have no equality.
    let (me, members, revision) = (
        session.people["me"].clone(),
        session.members.clone(),
        session.revision,
    );
    let snapshot = session.snapshot.clone().unwrap();

    receive(&mut rt, renamed("grace", "amazing", "Amazing Grace", ""));
    let session = &rt.state.session;
    assert!(session.revision > revision, "the rows draw again");
    assert_eq!(format!("{:?}", session.people["me"]), format!("{me:?}"));
    assert_eq!(session.self_name(), "Ada Lovelace");
    let snapshot_now = session.snapshot.as_ref().unwrap();
    assert_eq!(
        (&snapshot_now.display_name, &snapshot_now.avatar_url),
        (&snapshot.display_name, &snapshot.avatar_url)
    );
    for (server, listed) in &members {
        for (was, is) in listed.iter().zip(&session.members[server]) {
            if was.account_id != "grace" {
                assert_eq!(format!("{was:?}"), format!("{is:?}"));
            }
        }
        assert_eq!(listed.len(), session.members[server].len());
    }
    assert!(words(&rt).contains("Ada Lovelace"));
}

#[test]
fn someone_only_met_in_a_conversation_is_named_as_they_now_are() {
    let mut rt = with_grace();
    // The fixture's third message is from someone the servers do not list.
    assert_eq!(rt.state.session.name_of("stranger"), "stranger");
    receive(&mut rt, renamed("stranger", "kay", "Kay Nakamura", ""));
    assert_eq!(rt.state.session.name_of("stranger"), "Kay Nakamura");
    assert!(
        !rt.state.session.people.contains_key("stranger"),
        "not taken for someone a server lists"
    );
    assert!(words(&rt).contains("Kay Nakamura"));
}

#[test]
fn this_accounts_own_change_reaches_the_rail_and_the_account_menu() {
    let mut rt = with_grace();
    let mine = avatar("cc");
    receive(
        &mut rt,
        renamed("me", "countess", "Countess Lovelace", &mine),
    );
    let session = &rt.state.session;
    assert_eq!(session.self_name(), "Countess Lovelace");
    assert_eq!(session.avatar_url_of("me"), mine);
    assert_eq!(session.people["me"].display_name, "Countess Lovelace");
    assert_eq!(session.members["acme"][0].handle, "countess");
    assert_eq!(label_of(&rt, "self"), "Countess Lovelace, account menu");
    press(&mut rt, "self");
    assert!(words(&rt).contains("@countess"), "the account menu");
    assert_eq!(
        rt.state.session.name_of("grace"),
        "Grace Hopper",
        "nobody else"
    );
}
