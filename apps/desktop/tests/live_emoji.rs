//! A server's own emoji against a real instance (specs/emoji.md), through the session and the
//! state the screens call, on two shells: a server's owner and a member who joined by invite.
//! The owner uploads a still picture and an animated one from Server settings' Emoji
//! section, each named from its file and the second renamed in the form first, and is refused
//! a third under a name taken, in the board's words; the member's shell hears of both from
//! its account log and fetches their pictures, and is offered no Emoji section. The member
//! sends a message with one in its words and another in code, and reacts to it with one; the
//! owner's shell holds the message with one ref, draws it as the picture, and holds the
//! reaction by the emoji's id. The owner's composer lists the emoji after `:` and its emoji
//! button's picker puts one in at the caret. The owner renames one, and both shells draw the
//! message and the reaction by the new name, still one reaction; removes the other, asked
//! first, and the member's shell lets its picture go; and removes the first, after which its
//! reaction stays, counted and named as it was last called, and the message shows its text.
//! Runs only with `LUMIA_DEV_INSTANCE` set; without it it passes having checked nothing, and
//! says so. It makes its own accounts, so it needs an instance with open registration, and
//! one that keeps pictures (a blob store and a master key).

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use lumia_client::app::EmojiSummary;
use lumia_shell::app::roles::{self, Section};
use lumia_shell::app::{HomeView, Place, Popover, Route, State};
use lumia_shell::session::Connection;
use rune::TextEdit;

const PASSWORD: &str = "correct-horse-battery-staple";

fn wait(state: &mut State, what: &str, done: impl Fn(&State) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(20);
    while !done(state) {
        assert!(
            Instant::now() < deadline,
            "timed out waiting for {what}; session error {:?}, upload {:?}, rename {:?}, removing {:?}",
            state.session.last_error,
            state
                .emoji
                .upload
                .as_ref()
                .map(|u| (&u.name_error, &u.picture_error, u.busy)),
            state.emoji.renaming.as_ref().map(|r| &r.error),
            state.emoji.removing.as_ref().map(|r| &r.error),
        );
        state.drain();
        state.tick(chrono::Utc::now().timestamp_micros());
        std::thread::sleep(Duration::from_millis(20));
    }
}

fn wait_both(a: &mut State, b: &mut State, what: &str, done: impl Fn(&State, &State) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(20);
    while !done(a, b) {
        assert!(
            Instant::now() < deadline,
            "timed out waiting for {what}; session errors {:?} and {:?}",
            a.session.last_error,
            b.session.last_error,
        );
        for s in [&mut *a, &mut *b] {
            s.drain();
            s.tick(chrono::Utc::now().timestamp_micros());
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

fn new_account(instance: &str, dir: &Path, handle: &str) -> State {
    let mut state = State::new(dir.to_path_buf());
    state.sign_in.instance = TextEdit::new(instance);
    state.connect();
    wait(&mut state, "the trust card", |s| s.session.info.is_some());
    state.sign_in.creating = true;
    state.sign_in.handle = TextEdit::new(handle);
    state.sign_in.password.replace_text(PASSWORD);
    state.submit();
    wait(&mut state, "the shell", |s| s.route == Route::Shell);
    wait(&mut state, "the first snapshot", |s| {
        s.session.snapshot.is_some()
    });
    state
}

/// The core's own test pictures.
fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../core/crates/lumia-client/tests/fixtures")
        .join(name)
}

fn emoji_of(state: &State, server_id: &str) -> Vec<EmojiSummary> {
    state.session.server_emoji(server_id).to_vec()
}

fn named(state: &State, server_id: &str, name: &str) -> Option<EmojiSummary> {
    emoji_of(state, server_id)
        .into_iter()
        .find(|e| e.name == name)
}

/// Uploads the picture at `file` from the open Emoji section under `name`, the form's
/// name first checked against what the core makes of the file's.
fn upload(owner: &mut State, file: &Path, from_file: &str, name: &str) {
    owner.read_emoji_file(file);
    wait(owner, "the upload form", |s| s.emoji.upload.is_some());
    let form = owner.emoji.upload.as_ref().unwrap();
    assert_eq!(form.name.text, from_file);
    assert!(form.picture_error.is_empty());
    owner.upload_field().replace_text(name);
    owner.save_upload();
    assert!(owner.emoji.upload.as_ref().unwrap().busy);
}

#[test]
fn a_servers_emoji_are_managed_and_used_on_two_shells() {
    let Ok(instance) = std::env::var("LUMIA_DEV_INSTANCE") else {
        eprintln!("skipped: no LUMIA_DEV_INSTANCE");
        return;
    };
    let dir = std::env::temp_dir().join(format!("lumia-shell-emoji-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis()
        % 100_000_000;

    // ---- A server, a channel, and a member who joins by invite --------------------------
    let mut owner = new_account(&instance, &dir.join("owner"), &format!("emoown{stamp}"));
    owner.go_home(HomeView::AddServer);
    owner
        .server_forms
        .add
        .name
        .replace_text(format!("Shell emoji {stamp}"));
    owner.create_server();
    wait(&mut owner, "the new server on screen", |s| {
        matches!(s.place, Place::Server(_))
    });
    let server_id = owner.server_id().unwrap().to_string();
    owner.open_new_channel();
    owner.server_forms.channel.name.replace_text("general");
    owner.create_channel();
    wait(&mut owner, "the new channel on screen", |s| {
        s.session.channel().is_some()
    });
    let channel_id = owner.session.channel().unwrap().to_string();
    let first = format!("Before any emoji, {stamp}");
    assert!(
        owner
            .session
            .send(&first, chrono::Utc::now().timestamp_micros())
    );
    wait(&mut owner, "the first message", |s| {
        s.session.entries.iter().any(|e| e.body == first)
    });
    owner.open_invites();
    owner.make_invite();
    wait(&mut owner, "the invite link", |s| {
        s.server_forms.invite.made.is_some()
    });
    let link = owner.server_forms.invite.made.clone().unwrap().url;
    owner.close_popover();

    let mut member = new_account(&instance, &dir.join("member"), &format!("emomem{stamp}"));
    member.go_home(HomeView::AddServer);
    member.server_forms.add.invite.replace_text(&link);
    member.look_up_invite();
    wait(&mut member, "the invite's preview", |s| {
        s.server_forms.add.preview.is_some()
    });
    member.join_invite();
    wait(&mut member, "the joined channel on screen", |s| {
        s.session.channel() == Some(channel_id.as_str())
    });
    wait_both(&mut owner, &mut member, "both gateways", |a, b| {
        a.session.connection == Connection::Connected
            && b.session.connection == Connection::Connected
    });

    // ---- What each is offered ------------------------------------------------------------
    let mine = owner.session.server(&server_id).cloned().unwrap();
    assert!(mine.can_manage_emoji && mine.emoji.is_empty());
    assert_eq!(mine.emoji_limit, 50, "the instance's default");
    assert!(roles::settings_sections(&mine).contains(&Section::Emoji));
    let theirs = member.session.server(&server_id).cloned().unwrap();
    assert!(!theirs.can_manage_emoji);
    assert!(!roles::settings_sections(&theirs).contains(&Section::Emoji));

    // ---- Two uploads, and a name taken ----------------------------------------------------
    owner.open_server_settings();
    owner.choose_section(Section::Emoji);
    upload(&mut owner, &fixture("logo.png"), "logo", "logo");
    wait(&mut owner, "the still one kept", |s| {
        s.emoji.upload.is_none() && named(s, &server_id, "logo").is_some()
    });
    upload(&mut owner, &fixture("animated.gif"), "animated", "party");
    wait(&mut owner, "the animated one kept", |s| {
        s.emoji.upload.is_none() && named(s, &server_id, "party").is_some()
    });
    let logo = named(&owner, &server_id, "logo").unwrap();
    let party = named(&owner, &server_id, "party").unwrap();
    assert!(!logo.animated && party.animated);
    assert!(logo.width <= 128 && logo.height <= 128);
    upload(&mut owner, &fixture("logo.png"), "logo", "party");
    wait(&mut owner, "the taken name refused", |s| {
        s.emoji
            .upload
            .as_ref()
            .is_some_and(|u| !u.busy && !u.name_error.is_empty())
    });
    assert_eq!(
        owner.emoji.upload.as_ref().unwrap().name_error,
        format!("Shell emoji {stamp} already has :party:. Choose another name.")
    );
    owner.cancel_upload();
    assert_eq!(emoji_of(&owner, &server_id).len(), 2);

    // ---- The member's shell hears of them, and fetches their pictures ---------------------
    wait(&mut member, "the member's shell to hold both", |s| {
        emoji_of(s, &server_id).len() == 2
            && emoji_of(s, &server_id)
                .iter()
                .all(|e| s.session.avatars.bytes_of(&e.image_url).is_some())
    });
    let held = named(&member, &server_id, "party").unwrap();
    wait(&mut member, "the pictures decoded", |s| {
        s.emoji_image(&held, false)
            .is_some_and(rune::Image::is_ready)
            && s.emoji_image(&held, true)
                .is_some_and(rune::Image::is_ready)
    });

    // ---- A message with one in its words and one in code, and a reaction -------------------
    let said = format!("Shipped :logo: but not `:party:` {stamp}");
    assert!(
        member
            .session
            .send(&said, chrono::Utc::now().timestamp_micros())
    );
    wait_both(
        &mut owner,
        &mut member,
        "the message at the owner",
        |o, _| o.session.entries.iter().any(|e| e.body == said),
    );
    let message = owner
        .session
        .entries
        .iter()
        .find(|e| e.body == said)
        .cloned()
        .unwrap();
    assert_eq!(message.emoji.len(), 1, "the one in code is no ref");
    assert_eq!(message.emoji[0].emoji_id, logo.emoji_id);
    let start = message.emoji[0].start as usize;
    assert_eq!(&said[start..message.emoji[0].end as usize], ":logo:");
    assert!(
        owner
            .body_picture(&server_id, &logo.emoji_id)
            .is_some_and(|p| p.name == "logo")
    );
    member.react_custom(&message.id, &party.emoji_id);
    let reacted = (message.id.clone(), party.emoji_id.clone());
    wait_both(
        &mut owner,
        &mut member,
        "the reaction at the owner",
        |o, _| {
            o.session
                .entries
                .iter()
                .find(|e| e.id == reacted.0)
                .is_some_and(|e| {
                    e.reactions
                        .iter()
                        .any(|r| r.emoji_id == reacted.1 && r.count == 1 && r.reaction == ":party:")
                })
        },
    );

    // ---- The owner's composer: the `:` list and the emoji button's picker ------------------
    owner.close_server_settings();
    owner.select_channel(&channel_id);
    owner.draft.replace_text("Look :lo");
    owner.draft.set_caret(owner.draft.text.len());
    owner.follow_colon();
    let listed: Vec<String> = owner.colon_choices().into_iter().map(|e| e.name).collect();
    assert_eq!(listed, ["logo"]);
    owner.pick_colon(0);
    assert_eq!(owner.draft.text, "Look :logo: ");
    owner.open_composer_picker();
    assert!(owner.composer_picker_open());
    owner.pick_custom(&party.emoji_id);
    assert_eq!(owner.draft.text, "Look :logo: :party: ");
    assert!(owner.composer_picker_open(), "it stays open for more");
    owner.close_popover();
    owner.draft.clear();

    // ---- Renamed: both draw the message and the reaction by the new name -------------------
    owner.open_server_settings();
    owner.choose_section(Section::Emoji);
    owner.begin_rename(&party.emoji_id);
    owner.rename_field().replace_text("party_time");
    owner.save_rename();
    wait(&mut owner, "the rename kept", |s| {
        s.emoji.renaming.is_none() && named(s, &server_id, "party_time").is_some()
    });
    let renamed = party.emoji_id.clone();
    wait_both(&mut owner, &mut member, "the new name on both", |o, m| {
        [o, m].iter().all(|s| {
            named(s, &server_id, "party_time").is_some_and(|e| e.emoji_id == renamed)
                && s.session.entries.iter().any(|e| {
                    e.reactions
                        .iter()
                        .filter(|r| r.emoji_id == renamed)
                        .map(|r| (r.count, r.reaction.as_str()))
                        .eq([(1, ":party_time:")])
                })
        })
    });

    // ---- Removed, asked first: the member's shell lets its picture go -----------------------
    let gone = named(&owner, &server_id, "logo").unwrap();
    owner.ask_remove_emoji(&gone.emoji_id, &format!("remove-emoji-{}", gone.emoji_id));
    assert_eq!(owner.popover, Some(Popover::RemoveEmoji));
    owner.confirm_remove_emoji();
    wait(&mut owner, "the removal", |s| {
        s.emoji.removing.is_none() && named(s, &server_id, "logo").is_none()
    });
    assert_eq!(owner.popover, None);
    wait(&mut member, "the member's shell to let it go", |s| {
        named(s, &server_id, "logo").is_none()
            && s.session.avatars.bytes_of(&gone.image_url).is_none()
    });
    // Its message keeps the ref, and is drawn as its text: no picture is held for it.
    assert!(owner.body_picture(&server_id, &gone.emoji_id).is_none());
    assert!(
        member
            .session
            .entries
            .iter()
            .any(|e| e.body == said && e.emoji.len() == 1)
    );

    // ---- The one reacted with removed: the reaction stays, named as last called ------------
    owner.ask_remove_emoji(&renamed, &format!("remove-emoji-{renamed}"));
    owner.confirm_remove_emoji();
    wait(&mut owner, "the second removal", |s| {
        emoji_of(s, &server_id).is_empty()
    });
    let left = renamed.clone();
    wait_both(
        &mut owner,
        &mut member,
        "the reaction left on both",
        |o, m| {
            [o, m].iter().all(|s| {
                emoji_of(s, &server_id).is_empty()
                    && s.session.entries.iter().any(|e| {
                        e.reactions.iter().any(|r| {
                            r.emoji_id == left && r.count == 1 && r.reaction == ":party_time:"
                        })
                    })
            })
        },
    );
    // Its own reactor still takes it back.
    member.react_custom(&message.id, &renamed);
    let taken = (message.id.clone(), renamed.clone());
    wait_both(
        &mut owner,
        &mut member,
        "the reaction taken back",
        |o, _| {
            o.session
                .entries
                .iter()
                .find(|e| e.id == taken.0)
                .is_some_and(|e| e.reactions.iter().all(|r| r.emoji_id != taken.1))
        },
    );

    let _ = std::fs::remove_dir_all(&dir);
}
