//! A server's own emoji (specs/emoji.md), over a state no instance is behind: Acme as the
//! step 12 boards have it (`scenes::emoji`), seen by its owner. Server settings' Emoji
//! section (offered, the upload form, a rename, a removal asked first, the limit), the
//! picker's server section from a message and from the composer's emoji button, the `:`
//! list, reaction chips, a body's pictures and what a copy of one takes, and none of it in a
//! personal conversation. What the instance would answer is applied with `State::receive`.

#![allow(clippy::expect_used, clippy::unwrap_used)]

mod common;

use std::sync::Arc;

use common::*;
use lumia_client::app::{EmojiSummary, SnapshotSummary};
use lumia_shell::app::emoji::NAME_RULE;
use lumia_shell::app::{Popover, State};
use lumia_shell::scenes::emoji::{self as sample, acme_emoji, emoji_id};
use lumia_shell::scenes::roles::ACME;
use lumia_shell::session::avatars::{AvatarUpdate, retry_after};
use lumia_shell::session::emoji::EmojiUpdate;
use lumia_shell::session::{Conversation, Update};
use rune::{Key, KeyEvent, Mods, Runtime};

/// Acme's morning in #general, its twelve emoji held.
fn morning() -> Runtime<State> {
    let mut state = opened();
    sample::messages(&mut state);
    runtime(state)
}

/// Acme's settings, on Emoji.
fn settings() -> Runtime<State> {
    let mut state = opened();
    sample::settings(&mut state);
    runtime(state)
}

fn key(rt: &mut Runtime<State>, key: Key) {
    rt.key(KeyEvent {
        key,
        mods: Mods::default(),
    });
    settle(rt);
}

/// The snapshot as held, with Acme's emoji as `emoji`.
fn with(rt: &Runtime<State>, emoji: Vec<EmojiSummary>) -> SnapshotSummary {
    let mut snapshot = rt.state.session.snapshot.clone().unwrap();
    let acme = snapshot
        .servers
        .iter_mut()
        .find(|s| s.server_id == ACME)
        .unwrap();
    acme.emoji = emoji;
    snapshot
}

fn answer(rt: &mut Runtime<State>, update: EmojiUpdate) {
    rt.update(move |s, _| s.receive(&Update::Emoji(update)));
    settle(rt);
}

#[test]
fn the_emoji_section_is_offered_to_whoever_manages_emoji() {
    let mut rt = settings();
    assert!(rt.find("section-emoji").is_some());
    assert!(words(&rt).contains("12 of 50"));
    assert!(words(&rt).contains("Used as :name: in Acme’s channels and server DMs"));
    assert!(words(&rt).contains("Uploaded by Maya Chen · Animated"));
    // Taken away: the section goes, and the page gives way to what is left.
    let mut snapshot = rt.state.session.snapshot.clone().unwrap();
    snapshot.servers[0].can_manage_emoji = false;
    rt.update(move |s, _| s.receive(&Update::Snapshot(Ok(snapshot))));
    settle(&mut rt);
    assert!(rt.find("section-emoji").is_none());
    assert!(rt.find("emoji-section").is_none());
}

#[test]
fn an_upload_takes_its_name_from_the_file_and_says_why_it_was_refused() {
    let mut rt = settings();
    let path = std::path::PathBuf::from("/tmp/Acme-Heart v2.png");
    rt.update(|s, _| s.read_emoji_file(&std::path::PathBuf::from("/tmp/Acme-Heart v2.png")));
    answer(
        &mut rt,
        EmojiUpdate::Read {
            path,
            result: Ok((Arc::from(&b"picture"[..]), "image/png")),
        },
    );
    let upload = rt.state.emoji.upload.as_ref().expect("the form");
    assert_eq!(upload.name.text, "acme_heart_v2");
    assert!(words(&rt).contains("It is typed as :acme_heart_v2: in a message."));
    // A name the rule refuses is said before anything is sent.
    rt.update(|s, _| s.upload_field().replace_text("party time"));
    press(&mut rt, "upload-save");
    assert_eq!(
        rt.state.emoji.upload.as_ref().unwrap().name_error,
        NAME_RULE
    );
    assert!(!rt.state.emoji.upload.as_ref().unwrap().busy);
    // A name another emoji has: the instance's refusal, in the board's words.
    rt.update(|s, _| s.upload_field().replace_text("partyparrot"));
    press(&mut rt, "upload-save");
    assert!(rt.state.emoji.upload.as_ref().unwrap().busy);
    let request = rt.state.emoji.upload.as_ref().unwrap().request;
    answer(
        &mut rt,
        EmojiUpdate::Uploaded {
            server_id: ACME.into(),
            request,
            result: Err(lumia_shell::session::Failure {
                code: "ALREADY_EXISTS".into(),
                message: "taken".into(),
                details: Default::default(),
            }),
        },
    );
    assert!(words(&rt).contains("Acme already has :partyparrot:. Choose another name."));
    // Kept: the form goes, and the list and the count have it.
    let mut thirteen = acme_emoji(12);
    thirteen.push(EmojiSummary {
        emoji_id: emoji_id("e13"),
        name: "acme_heart".into(),
        ..thirteen[0].clone()
    });
    let snapshot = with(&rt, thirteen);
    // An answer to another call (a form since closed) is not this form's.
    answer(
        &mut rt,
        EmojiUpdate::Uploaded {
            server_id: ACME.into(),
            request: request + 1,
            result: Ok(snapshot.clone()),
        },
    );
    assert!(rt.state.emoji.upload.is_some());
    answer(
        &mut rt,
        EmojiUpdate::Uploaded {
            server_id: ACME.into(),
            request,
            result: Ok(snapshot),
        },
    );
    assert!(rt.state.emoji.upload.is_none());
    assert!(words(&rt).contains("13 of 50"));
    assert!(words(&rt).contains(":acme_heart:"));
}

#[test]
fn at_the_limit_upload_is_off_and_says_why() {
    let mut state = opened();
    sample::full(&mut state);
    let rt = runtime(state);
    assert!(
        words(&rt).contains("Acme has as many emoji as it may (12). Remove one to upload another.")
    );
    let tree = rt.accessibility_json().to_string();
    let at = tree.find("\"Upload emoji\"").expect("the button");
    let near = &tree[at.saturating_sub(300)..(at + 300).min(tree.len())];
    assert!(near.contains("disabled"), "{near}");
}

/// A second server whose emoji Ryan manages too, beside Acme.
const BOLT: &str = "0195a1b2-0000-7000-8000-00000000b017";

fn with_bolt(rt: &mut Runtime<State>) {
    let mut snapshot = rt.state.session.snapshot.clone().unwrap();
    let mut bolt = snapshot
        .servers
        .iter()
        .find(|s| s.server_id == ACME)
        .cloned()
        .unwrap();
    bolt.server_id = BOLT.into();
    bolt.name = "Bolt".into();
    bolt.emoji = Vec::new();
    snapshot.servers.push(bolt);
    rt.update(move |s, _| s.receive(&Update::Snapshot(Ok(snapshot))));
    settle(rt);
}

fn read_heart(rt: &mut Runtime<State>) -> std::path::PathBuf {
    let path = std::path::PathBuf::from("/tmp/Acme-Heart v2.png");
    let asked = path.clone();
    rt.update(move |s, _| s.read_emoji_file(&asked));
    path
}

fn heart_read(rt: &mut Runtime<State>, path: std::path::PathBuf) {
    answer(
        rt,
        EmojiUpdate::Read {
            path,
            result: Ok((Arc::from(&b"picture"[..]), "image/png")),
        },
    );
}

/// Bolt's settings, on Emoji.
fn bolt_settings(rt: &mut Runtime<State>) {
    rt.update(|s, _| {
        s.select_server(BOLT);
        s.open_server_settings();
        s.choose_section(lumia_shell::app::roles::Section::Emoji);
    });
    settle(rt);
}

/// A picture chosen in one server's settings is that server's: it never becomes the form of
/// another's, whether it was read before the person went there or arrived after.
#[test]
fn a_picture_chosen_for_one_server_is_never_uploaded_to_another() {
    // Read, then elsewhere: the form stays behind.
    let mut rt = settings();
    with_bolt(&mut rt);
    let path = read_heart(&mut rt);
    heart_read(&mut rt, path);
    assert!(rt.state.emoji.upload.is_some());
    bolt_settings(&mut rt);
    assert!(rt.state.emoji.upload.is_none(), "Acme's form is not Bolt's");
    rt.update(|s, _| s.save_upload());
    assert!(words(&rt).contains("0 of 50"));

    // Asked to be read in Acme, read once the person is in Bolt: no form there.
    let mut rt = settings();
    with_bolt(&mut rt);
    let path = read_heart(&mut rt);
    bolt_settings(&mut rt);
    heart_read(&mut rt, path);
    assert!(
        rt.state.emoji.upload.is_none(),
        "a file Acme asked for opens no form in Bolt"
    );
}

/// An answer to a call made by an account that has since signed out belongs to nobody on
/// screen: its snapshot is not taken.
#[test]
fn an_answer_after_signing_out_is_not_taken() {
    let mut rt = settings();
    let path = read_heart(&mut rt);
    heart_read(&mut rt, path);
    rt.update(|s, _| s.upload_field().replace_text("heart"));
    press(&mut rt, "upload-save");
    let mut kept = acme_emoji(12);
    kept.push(EmojiSummary {
        emoji_id: emoji_id("e13"),
        name: "heart".into(),
        ..kept[0].clone()
    });
    let snapshot = with(&rt, kept);
    // As the queue hands it on: the answer to work started for the account signed in then.
    let (generation, request) = (
        rt.state.session.generation(),
        rt.state.emoji.upload.as_ref().unwrap().request,
    );
    rt.update(|s, _| s.sign_out());
    settle(&mut rt);
    assert!(rt.state.session.snapshot.is_none());
    rt.update(move |s, _| {
        s.receive_from(
            generation,
            &Update::Emoji(EmojiUpdate::Uploaded {
                server_id: ACME.into(),
                request,
                result: Ok(snapshot),
            }),
        );
    });
    settle(&mut rt);
    assert!(
        rt.state.session.snapshot.is_none(),
        "the account that left is not put back"
    );
}

/// An operator may give a server no room at all (`emoji-limit <server> 0`): "0 of 0", the
/// section's words at the limit, and Upload off, never taken as unlimited.
#[test]
fn a_limit_of_none_offers_no_upload() {
    let mut state = opened();
    sample::with_emoji(&mut state, 0, 0);
    state.open_server_settings();
    state.choose_section(lumia_shell::app::roles::Section::Emoji);
    let rt = runtime(state);
    assert!(words(&rt).contains("0 of 0"));
    assert!(
        words(&rt).contains("Acme has as many emoji as it may (0). Remove one to upload another.")
    );
    let tree = rt.accessibility_json().to_string();
    let at = tree.find("\"Upload emoji\"").expect("the button");
    let near = &tree[at.saturating_sub(300)..(at + 300).min(tree.len())];
    assert!(near.contains("disabled"), "{near}");
}

#[test]
fn a_row_is_renamed_in_place() {
    let mut rt = settings();
    let wave = emoji_id("e9");
    press(&mut rt, &format!("rename-emoji-{wave}"));
    assert!(words(&rt).contains("New name for :parrotwave:"));
    rt.update(|s, _| s.rename_field().replace_text("parrot_wave"));
    press(&mut rt, "rename-save");
    assert!(rt.state.emoji.renaming.as_ref().unwrap().busy);
    let mut renamed = acme_emoji(12);
    renamed[8].name = "parrot_wave".into();
    let snapshot = with(&rt, renamed);
    let request = rt.state.emoji.renaming.as_ref().unwrap().request;
    answer(
        &mut rt,
        EmojiUpdate::Renamed {
            server_id: ACME.into(),
            emoji_id: wave.clone(),
            request,
            result: Ok(snapshot),
        },
    );
    assert!(rt.state.emoji.renaming.is_none());
    assert!(words(&rt).contains(":parrot_wave:"));
    // A rename open on an emoji removed elsewhere goes with it.
    press(&mut rt, &format!("rename-emoji-{wave}"));
    let snapshot = with(&rt, acme_emoji(8));
    rt.update(move |s, _| s.receive(&Update::Snapshot(Ok(snapshot))));
    settle(&mut rt);
    assert!(rt.state.emoji.renaming.is_none());
}

#[test]
fn remove_asks_first() {
    let mut rt = settings();
    let wave = emoji_id("e9");
    press(&mut rt, &format!("remove-emoji-{wave}"));
    assert_eq!(rt.state.popover, Some(Popover::RemoveEmoji));
    assert!(words(&rt).contains("Remove :parrotwave:?"));
    assert!(words(&rt).contains("Messages that used it will show its name instead."));
    press(&mut rt, "remove-emoji-confirm");
    assert!(rt.state.emoji.removing.as_ref().unwrap().busy);
    let mut left = acme_emoji(12);
    left.remove(8);
    let snapshot = with(&rt, left);
    let request = rt.state.emoji.removing.as_ref().unwrap().request;
    answer(
        &mut rt,
        EmojiUpdate::Removed {
            server_id: ACME.into(),
            emoji_id: wave,
            request,
            result: Ok(snapshot),
        },
    );
    assert_eq!(rt.state.popover, None);
    assert!(words(&rt).contains("11 of 50"));
    assert!(!words(&rt).contains(":parrotwave:"));
}

#[test]
fn the_colon_list_completes_a_name() {
    let mut rt = morning();
    press(&mut rt, "composer");
    type_text(&mut rt, "Shipped :p");
    // One letter lists nothing.
    assert!(rt.find("emoji-colon-list").is_none());
    type_text(&mut rt, "ar");
    assert!(rt.find("emoji-colon-list").is_some());
    let names: Vec<String> = rt
        .state
        .colon_choices()
        .into_iter()
        .map(|e| e.name)
        .collect();
    // The core's order: those that begin with it, then those that hold it.
    assert_eq!(
        names,
        ["parrotwave", "party_blob", "partyparrot", "sadparrot"]
    );
    key(&mut rt, Key::Down);
    key(&mut rt, Key::Enter);
    assert_eq!(rt.state.draft.text, "Shipped :party_blob: ");
    assert!(rt.find("emoji-colon-list").is_none());
    // Escape closes it, and the words stay.
    type_text(&mut rt, ":sad");
    assert!(rt.find("emoji-colon-list").is_some());
    key(&mut rt, Key::Escape);
    assert!(rt.find("emoji-colon-list").is_none());
    assert_eq!(rt.state.draft.text, "Shipped :party_blob: :sad");
}

#[test]
fn the_composers_emoji_button_puts_emoji_in_and_stays_open() {
    let mut rt = morning();
    press(&mut rt, "composer");
    type_text(&mut rt, "Shipped ");
    press(&mut rt, "composer-emoji");
    assert!(rt.state.composer_picker_open());
    assert!(words(&rt).contains("Acme"));
    // One of Acme's: its :name: and a space, at the caret; the picker stays.
    press(&mut rt, &format!("c-{}", emoji_id("e1")));
    assert_eq!(rt.state.draft.text, "Shipped :acme: ");
    assert!(rt.state.composer_picker_open());
    // One of Unicode's: itself. (Acme's is first in "Frequently used" now.)
    let frequent = rt.state.frequent_emoji();
    let n = frequent
        .iter()
        .position(|e| !e.starts_with("custom:"))
        .unwrap();
    let first = frequent[n].clone();
    press(&mut rt, &format!("f{n}"));
    assert_eq!(rt.state.draft.text, format!("Shipped :acme: {first}"));
    // The caret is the field's: typing goes on there, and Escape puts the picker away.
    type_text(&mut rt, "!");
    assert_eq!(rt.state.draft.text, format!("Shipped :acme: {first}!"));
    key(&mut rt, Key::Escape);
    assert!(!rt.state.composer_picker_open());
    // A press outside puts it away too.
    press(&mut rt, "composer-emoji");
    assert!(rt.state.composer_picker_open());
    press(&mut rt, "emoji-picker-outside");
    assert!(!rt.state.composer_picker_open());
    // Kept for "Frequently used", here.
    assert!(
        rt.state
            .settings
            .recent_emoji
            .contains(&format!("custom:{}", emoji_id("e1")))
    );
}

#[test]
fn a_message_is_reacted_to_with_one_of_the_servers_emoji() {
    let mut rt = morning();
    rt.update(|s, _| s.open_picker("m3", "add-reaction-m3"));
    settle(&mut rt);
    assert!(words(&rt).contains("Acme"));
    // The search finds Acme's first.
    rt.update(|s, _| {
        if let Some(picker) = &mut s.replies.picker {
            picker.search.replace_text("par");
        }
    });
    settle(&mut rt);
    let found: Vec<String> = rt
        .state
        .custom_found("par")
        .into_iter()
        .map(|e| e.name)
        .collect();
    assert_eq!(
        found,
        ["partyparrot", "parrotwave", "party_blob", "sadparrot"]
    );
    press(&mut rt, &format!("c-{}", emoji_id("e8")));
    assert_eq!(rt.state.popover, None);
    assert_eq!(
        rt.state.settings.recent_emoji.first().cloned(),
        Some(format!("custom:{}", emoji_id("e8")))
    );
}

#[test]
fn chips_draw_a_held_emoji_and_a_removed_ones_name() {
    let rt = morning();
    let tree = rt.accessibility_json().to_string();
    // Held: named by its :name: now, and pressable.
    assert!(tree.contains("You and 4 others reacted with :partyparrot:, 5"));
    // Removed: its name, still counted, offering no new one.
    assert!(tree.contains("reacted with :oldlogo:, 4"));
    assert!(words(&rt).contains(":oldlogo:"));
    // One reactor no list names is one person (it said "1 people").
    assert!(tree.contains("1 person reacted with :blobcat:, 1"));
}

#[test]
fn a_body_draws_held_emoji_and_copies_them_as_typed() {
    let mut rt = morning();
    // Assistive technology reads each by its name now; :oldlogo: is its text.
    let tree = rt.accessibility_json().to_string();
    assert!(tree.contains("Onboarding shipped this morning :partyparrot: thanks, all."));
    assert!(words(&rt).contains("The launch badge is up: :oldlogo:"));
    // Renamed, a message is drawn with the name it has now and keeps what was typed.
    let mut renamed = acme_emoji(12);
    renamed[7].name = "parrot_party".into();
    let snapshot = with(&rt, renamed);
    rt.update(move |s, _| s.receive(&Update::Snapshot(Ok(snapshot))));
    settle(&mut rt);
    let tree = rt.accessibility_json().to_string();
    assert!(tree.contains("Onboarding shipped this morning :parrot_party: thanks, all."));
    // A copy takes the words as typed, with nothing of the frame around a picture.
    let clipboard = rune::clipboard::memory();
    rt.use_clipboard(&clipboard);
    let (x, y) = rt.find("message-m1/body").unwrap();
    rt.pointer_move(x, y);
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    settle(&mut rt);
    rt.key(KeyEvent {
        key: Key::Command('a'),
        mods: Mods {
            cmd: true,
            ..Mods::default()
        },
    });
    settle(&mut rt);
    let copied = rt.selected_text().unwrap();
    assert!(copied.contains("Onboarding shipped this morning :partyparrot: thanks, all."));
    assert!(copied.contains(":lgtm: on the copy changes. Merging after lunch :coffee:"));
    assert!(!copied.contains('\u{2060}') && !copied.contains('\u{a0}'));
}

#[test]
fn none_of_it_in_a_personal_conversation() {
    let mut rt = morning();
    rt.update(|s, _| {
        s.session.conversation = Some(Conversation::Dm("dm-1".into()));
        s.draft.replace_text(":par");
        s.follow_colon();
        s.settings.recent_emoji = vec![format!("custom:{}", emoji_id("e8"))];
    });
    assert!(rt.state.session.emoji_server().is_none());
    assert!(rt.state.colon_choices().is_empty());
    assert!(rt.state.custom_found("par").is_empty());
    assert!(
        rt.state
            .frequent_emoji()
            .iter()
            .all(|e| !e.starts_with("custom:"))
    );
}

#[test]
fn an_animated_emoji_is_still_under_reduce_motion() {
    let rt = morning();
    let parrot = rt
        .state
        .session
        .emoji_in(ACME, &emoji_id("e8"))
        .unwrap()
        .clone();
    let moving = rt.state.emoji_image(&parrot, false).unwrap();
    let still = rt.state.emoji_image(&parrot, true).unwrap();
    assert_ne!(moving.id(), still.id());
    // A still one is its own picture either way.
    let acme = rt
        .state
        .session
        .emoji_in(ACME, &emoji_id("e1"))
        .unwrap()
        .clone();
    assert_eq!(
        rt.state.emoji_image(&acme, true).unwrap().id(),
        rt.state.emoji_image(&acme, false).unwrap().id()
    );
}

/// The pages of a channel's history the instance gave are not in the core's store, which
/// cannot rename their reactions: when the server's emoji change they are read again from
/// the instance, which names each reaction as its emoji is now called (or was last called,
/// once removed), and the chips say so.
#[test]
fn history_the_instance_gave_is_read_again_when_the_emoji_change() {
    let mut rt = morning();
    let channel = rt.state.session.channel().unwrap().to_string();
    let parrot = emoji_id("e8");
    let mut old = rt.state.session.entries[0].clone();
    old.id = "h1".into();
    old.reactions = vec![lumia_client::app::ReactionSummary {
        reaction: ":partyparrot:".into(),
        count: 2,
        me: false,
        account_ids: Vec::new(),
        emoji_id: parrot.clone(),
    }];
    let page = vec![old.clone()];
    let asked = channel.clone();
    rt.update(move |s, _| {
        s.receive(&Update::Older {
            channel_id: asked,
            page: Ok((page, true)),
        })
    });
    settle(&mut rt);
    assert!(rt.state.session.held_history().iter().any(|e| e.id == "h1"));
    // Renamed and removed in one burst: Acme no longer lists it.
    let snapshot = with(&rt, acme_emoji(7));
    rt.update(move |s, _| s.receive(&Update::Snapshot(Ok(snapshot))));
    rt.update(|s, _| {
        s.receive(&Update::Gateway(
            lumia_client::app::AppEvent::EmojiChanged {
                server_id: ACME.into(),
            },
        ))
    });
    settle(&mut rt);
    assert!(
        rt.state.session.rereading_history(),
        "the pages the instance gave are asked for again"
    );
    // What the instance says of them now: the name it was left with.
    old.reactions[0].reaction = ":parrot_party:".into();
    let asked = channel.clone();
    rt.update(move |s, _| {
        s.receive(&Update::HistoryRead {
            channel_id: asked,
            page: Ok(vec![old]),
        })
    });
    settle(&mut rt);
    assert!(!rt.state.session.rereading_history());
    let held = rt.state.session.held_history()[0].clone();
    assert_eq!(held.reactions[0].reaction, ":parrot_party:");
}

/// An emoji whose picture could not be fetched is its `:name:`, never a blank: in a
/// message's text and on its reaction chip. A failure that may pass is asked again later;
/// one the instance says has nothing there is not.
#[test]
fn an_emoji_whose_picture_could_not_be_had_says_its_name() {
    let mut rt = morning();
    let coffee = rt
        .state
        .session
        .emoji_in(ACME, &emoji_id("e5"))
        .unwrap()
        .clone();
    let url = coffee.image_url.clone();
    // As if its bytes had been lost: let go, to be fetched again.
    rt.update(move |s, _| s.session.picture_failed(&url));
    settle(&mut rt);
    assert!(rt.state.session.avatars.failed(&coffee.image_url));
    assert!(
        rt.state.body_picture(ACME, &coffee.emoji_id).is_none(),
        "the text says :coffee:"
    );
    // Asked again once its wait is over.
    let due = rt.state.session.avatars.next_retry().expect("a retry");
    assert!(due <= std::time::Instant::now() + retry_after(0));
    // A fetch with no answer: the name, and asked again later.
    let blob = rt
        .state
        .session
        .emoji_in(ACME, &emoji_id("e6"))
        .unwrap()
        .clone();
    let url = blob.image_url.clone();
    rt.update(move |s, _| {
        s.session.picture_failed(&url);
        s.receive(&Update::Avatar(AvatarUpdate::Fetched {
            url,
            bytes: Err(lumia_shell::session::Failure::default()),
        }))
    });
    settle(&mut rt);
    let tree = rt.accessibility_json().to_string();
    assert!(tree.contains("reacted with :blobcat:"), "the chip names it");
    assert!(words(&rt).contains(":blobcat:"), "and shows its name");
    assert!(rt.state.session.avatars.failed(&blob.image_url));
}

/// Bytes that would not decode (a file half written when the computer stopped): the name
/// is drawn, and the bytes are let go to be fetched again.
#[test]
fn a_picture_that_will_not_decode_is_fetched_again() {
    let mut rt = morning();
    let coffee = rt
        .state
        .session
        .emoji_in(ACME, &emoji_id("e5"))
        .unwrap()
        .clone();
    let url = coffee.image_url.clone();
    rt.update(move |s, _| {
        s.session.picture_failed(&url);
        s.receive(&Update::Avatar(AvatarUpdate::Fetched {
            url,
            bytes: Ok(Arc::from(&b"not a picture"[..])),
        }))
    });
    // Drawn, refused by the decoder, noticed before a frame.
    for _ in 0..5 {
        settle(&mut rt);
        rt.state.pictures.wait();
        rt.update(|s, _| {
            s.tick(chrono::Utc::now().timestamp_micros());
        });
    }
    settle(&mut rt);
    assert!(rt.state.session.avatars.failed(&coffee.image_url));
    assert!(rt.state.body_picture(ACME, &coffee.emoji_id).is_none());
}

/// A server's emoji are decoded when something draws them, not when their bytes arrive,
/// and under Reduce Motion an animated one is decoded as its first frame alone.
#[test]
fn emoji_are_decoded_only_as_drawn() {
    // A server whose twelve are held, seen from Home: none decoded.
    let mut state = opened();
    sample::held_not_drawn(&mut state);
    state.go_home(lumia_shell::app::HomeView::default());
    let mut rt = runtime(state);
    rt.update(|s, _| {
        s.tick(chrono::Utc::now().timestamp_micros());
    });
    settle(&mut rt);
    assert_eq!(rt.state.pictures.emoji_decoded(), (0, 0));
    // Held as emoji, and not decoded as people's pictures are either.
    for e in acme_emoji(12) {
        assert!(rt.state.session.avatars.bytes_of(&e.image_url).is_some());
        assert!(rt.state.session.avatars.is_emoji(&e.image_url));
        assert!(rt.state.picture_at(&e.image_url).is_none());
    }
    // The composer's picker under Reduce Motion: every one drawn, the two animated as their
    // first frames alone.
    rt.set_system_reduced_motion(true);
    rt.update(|s, _| {
        s.select_server(ACME);
        s.open_composer_picker();
    });
    for _ in 0..3 {
        settle(&mut rt);
        rt.update(|s, _| {
            s.tick(chrono::Utc::now().timestamp_micros());
        });
    }
    assert_eq!(rt.state.pictures.emoji_decoded(), (10, 2));
}
