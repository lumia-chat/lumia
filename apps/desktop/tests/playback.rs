//! Attached videos and recordings played in the conversation (#21, `app::playback`) through
//! a windowless `Runtime`: nothing before a press, the video in its poster's place or under
//! its row, one clip playing at a time, a video pausing when its row leaves the view, every
//! clip ending with its conversation (or a page in its place) and its file removed, a file
//! that could not be fetched asked for again, full screen, Preferences' Images "As links",
//! an expired attachment, and Linux's system player. The clips are the scenes'
//! own (`scenes::media::CLIP`, `TONE`), written as a fetch would write them; headless, Rune
//! decodes the video's frames itself and plays no sound. Fetching needs an instance and is
//! in the live tests.

#![allow(clippy::expect_used, clippy::unwrap_used)]

mod common;

use common::*;
use lumia_client::app::{AttachmentSummary, BlobSummary};
use lumia_shell::app::media::fetched;
use lumia_shell::app::{HomeView, State};
use lumia_shell::scenes::media::{CLIP, TONE, written};
use lumia_shell::session::media::{MediaUpdate, player_file};
use lumia_shell::session::{Failure, Update};
use lumia_shell::settings::Media;
use rune::{Key, KeyEvent, Mods, Runtime};

const ICON: &[u8] = include_bytes!("../../../assets/brand/lumia-original-icon.png");

/// A video with a poster (the brand's icon, made by the core as a send makes one), or a
/// recording. Ids are the test's own: the files go in one temporary directory.
fn clip(id: &str, kind: &str) -> AttachmentSummary {
    let blob = |part: &str| BlobSummary {
        blob_id: format!("{id}-{part}"),
        ..BlobSummary::default()
    };
    let video = kind == "video";
    AttachmentSummary {
        attachment_id: id.into(),
        kind: kind.into(),
        name: if video { "Demo.mp4" } else { "Voice note.m4a" }.into(),
        media_type: if video { "video/mp4" } else { "audio/mp4" }.into(),
        width: if video { 640 } else { 0 },
        height: if video { 360 } else { 0 },
        size_bytes: 24_000,
        main: blob("main"),
        thumbnail: video.then(|| blob("thumb")),
        ..AttachmentSummary::default()
    }
}

fn poster() -> Vec<u8> {
    lumia_core::media::process(lumia_core::media::Input {
        bytes: ICON.to_vec(),
        name: "poster.png".into(),
        as_file: false,
        pixels: None,
        poster: None,
        duration_ms: None,
        video_size: None,
        caps: lumia_core::media::Caps::default(),
    })
    .unwrap()
    .thumbnail
    .unwrap()
}

/// Signed in, in #general, where Grace sent a video with its poster and a recording; the
/// window plays clips itself whatever this platform is.
fn with_clips(tag: &str) -> (State, AttachmentSummary, AttachmentSummary) {
    let mut state = signed_in();
    state.select_channel("general");
    state.playback.inline = true;
    let (video, audio) = (
        clip(&format!("{tag}-v"), "video"),
        clip(&format!("{tag}-a"), "audio"),
    );
    let now = chrono::Utc::now().timestamp_micros();
    let mut entries = vec![message(
        &format!("{tag}-m0"),
        "me",
        "Before.",
        now - 90_000_000,
    )];
    let mut sent = message(&format!("{tag}-p1"), "grace", "The demo.", now - 60_000_000);
    sent.attachments = vec![video.clone()];
    let mut said = message(&format!("{tag}-p2"), "grace", "And over it.", now);
    said.attachments = vec![audio.clone()];
    entries.extend([sent, said]);
    state.session.entries = entries;
    state.session.revision += 1;
    state.receive(&fetched(&format!("{tag}-v-thumb"), &poster()));
    state.tick(now);
    state.media.wait();
    (state, video, audio)
}

fn has(rt: &Runtime<State>, key: &str) -> bool {
    rt.elements()
        .into_iter()
        .any(|e| e.key.as_deref() == Some(key) && !e.exiting)
}

fn label(rt: &Runtime<State>, key: &str) -> String {
    rt.elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some(key) && !e.exiting)
        .and_then(|e| e.label)
        .unwrap_or_else(|| panic!("{key} is not on screen"))
}

fn playing(rt: &Runtime<State>, id: &str) -> bool {
    rt.state.playback.clip(id).is_some_and(|c| c.playing())
}

/// The clip's file as a fetch would have written it, and a frame drawn over it.
fn arrives(rt: &mut Runtime<State>, attachment: &AttachmentSummary) {
    let bytes = if attachment.kind == "video" {
        CLIP
    } else {
        TONE
    };
    let update = written(attachment, bytes);
    rt.update(move |s, _| s.receive(&update));
    settle(rt);
}

/// What the session reports when the clip's file could not be fetched.
fn refused(attachment: &AttachmentSummary) -> Update {
    Update::Media(MediaUpdate::Written {
        attachment_id: attachment.attachment_id.clone(),
        file: Err(Failure {
            message: "the network is away".into(),
            ..Failure::default()
        }),
    })
}

fn tick(rt: &mut Runtime<State>) {
    rt.update(|s, _| {
        s.tick(chrono::Utc::now().timestamp_micros());
    });
    settle(rt);
}

#[test]
fn nothing_plays_before_a_press_and_a_video_plays_in_its_posters_place() {
    let (state, video, _) = with_clips("press");
    let mut rt = runtime(state);
    let tile = format!("picture-{}", video.attachment_id);
    assert!(rt.state.playback.playing().is_none());
    assert_eq!(label(&rt, &tile), "Play video, Demo.mp4");
    press(&mut rt, &tile);
    // Fetching: the poster stays where the video will be, and says it is loading.
    let player = format!("player-{}", video.attachment_id);
    assert!(has(&rt, &player));
    assert!(!has(&rt, &tile));
    assert!(words(&rt).contains("Loading…"), "{}", words(&rt));
    if !cfg!(any(target_os = "macos", target_os = "windows")) {
        return;
    }
    arrives(&mut rt, &video);
    assert!(playing(&rt, &video.attachment_id));
    // At the video's shape, no taller than 400, with its controls.
    let stage = rt
        .elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some(&*format!("video-{}", video.attachment_id)))
        .unwrap()
        .rect;
    assert!(
        stage[3] <= 400.0 && (stage[2] / stage[3] - 16.0 / 9.0).abs() < 0.02,
        "{stage:?}"
    );
    let toggle = format!("toggle-{}", video.attachment_id);
    assert_eq!(label(&rt, &toggle), "Pause Demo.mp4");
    for control in ["seek", "sound", "full-screen"] {
        assert!(
            has(&rt, &format!("{control}-{}", video.attachment_id)),
            "{control}"
        );
    }
    // Rune starts a video muted; one pressed plays with its sound.
    let sound = format!("sound-{}", video.attachment_id);
    assert_eq!(label(&rt, &sound), "Mute");
    press(&mut rt, &toggle);
    assert!(!playing(&rt, &video.attachment_id));
    assert_eq!(label(&rt, &toggle), "Play video, Demo.mp4");
}

#[cfg(any(target_os = "macos", target_os = "windows"))]
#[test]
fn starting_one_pauses_the_other() {
    let (state, video, audio) = with_clips("one");
    let mut rt = runtime(state);
    press(&mut rt, &format!("picture-{}", video.attachment_id));
    arrives(&mut rt, &video);
    assert!(playing(&rt, &video.attachment_id));
    // The recording's Play: it plays in its row, and the video pauses.
    let play = format!("play-{}", audio.attachment_id);
    assert_eq!(label(&rt, &play), "Play Voice note.m4a");
    press(&mut rt, &play);
    assert!(!playing(&rt, &video.attachment_id));
    arrives(&mut rt, &audio);
    assert!(playing(&rt, &audio.attachment_id));
    assert!(has(&rt, &format!("seek-{}", audio.attachment_id)));
    // Its row's Play has given way to the clip's own button.
    assert!(!has(&rt, &play));
    assert_eq!(
        label(&rt, &format!("toggle-{}", audio.attachment_id)),
        "Pause Voice note.m4a"
    );
    // And back: the video's own button plays it and pauses the recording.
    press(&mut rt, &format!("toggle-{}", video.attachment_id));
    assert!(playing(&rt, &video.attachment_id));
    assert!(!playing(&rt, &audio.attachment_id));
    assert_eq!(
        rt.state.playback.playing(),
        Some(video.attachment_id.as_str())
    );
}

#[test]
fn a_clip_pressed_before_another_arrives_waits_paused() {
    let (mut state, video, audio) = with_clips("wait");
    state.play_attachment(&video);
    state.play_attachment(&audio);
    // The video's file comes after the recording was pressed: it is ready and does not play.
    state.receive(&written(&video, CLIP));
    let clip = state.playback.clip(&video.attachment_id).unwrap();
    assert!(clip.player().is_some());
    assert!(!clip.playing());
    state.receive(&written(&audio, TONE));
    if cfg!(any(target_os = "macos", target_os = "windows")) {
        assert_eq!(state.playback.playing(), Some(audio.attachment_id.as_str()));
    }
}

#[cfg(any(target_os = "macos", target_os = "windows"))]
#[test]
fn a_video_pauses_when_its_row_leaves_the_view_and_a_recording_plays_on() {
    let (state, video, audio) = with_clips("scroll");
    let mut rt = runtime(state);
    press(&mut rt, &format!("picture-{}", video.attachment_id));
    arrives(&mut rt, &video);
    assert!(playing(&rt, &video.attachment_id));
    // The rows on screen are the last one only: the video's (the one before) is not.
    let count = rt.state.session.entries.len();
    assert!(rt.state.rows_visible(&(count - 1..count)));
    assert!(!playing(&rt, &video.attachment_id));
    // The recording plays on through any scroll.
    press(&mut rt, &format!("play-{}", audio.attachment_id));
    arrives(&mut rt, &audio);
    assert!(!rt.state.rows_visible(&(0..1)));
    assert!(playing(&rt, &audio.attachment_id));
}

#[cfg(any(target_os = "macos", target_os = "windows"))]
#[test]
fn a_video_whose_file_arrives_after_its_row_left_the_view_waits_paused() {
    let (state, video, audio) = with_clips("late");
    let mut rt = runtime(state);
    press(&mut rt, &format!("picture-{}", video.attachment_id));
    // Scrolled away before the file is there: the last row only is in view.
    let count = rt.state.session.entries.len();
    rt.update(move |s, _| {
        s.rows_visible(&(count - 1..count));
    });
    arrives(&mut rt, &video);
    let clip = rt.state.playback.clip(&video.attachment_id).unwrap();
    assert!(
        clip.player().is_some(),
        "ready for when it is scrolled back"
    );
    assert!(!clip.playing(), "nothing plays out of view");
    // A recording pressed and scrolled away plays when it arrives, as one playing goes on.
    press(&mut rt, &format!("play-{}", audio.attachment_id));
    rt.update(|s, _| {
        s.rows_visible(&(0..1));
    });
    arrives(&mut rt, &audio);
    assert!(playing(&rt, &audio.attachment_id));
}

#[test]
fn a_clip_whose_file_could_not_be_fetched_is_asked_for_again() {
    let (state, video, audio) = with_clips("again");
    let mut rt = runtime(state);
    for (attachment, pressed) in [(&video, "picture"), (&audio, "play")] {
        let id = &attachment.attachment_id;
        press(&mut rt, &format!("{pressed}-{id}"));
        let update = refused(attachment);
        rt.update(move |s, _| s.receive(&update));
        settle(&mut rt);
        assert!(words(&rt).contains("Could not fetch it: the network is away"));
        // Its Play is still there, and a press asks for the file again.
        assert!(label(&rt, &format!("toggle-{id}")).starts_with("Play "));
        press(&mut rt, &format!("toggle-{id}"));
        assert!(words(&rt).contains("Loading…"));
        assert!(rt.state.playback.clip(id).unwrap().failure().is_none());
        assert!(!has(&rt, &format!("toggle-{id}")));
    }
}

#[cfg(any(target_os = "macos", target_os = "windows"))]
#[test]
fn a_clip_pressed_again_while_its_file_is_on_its_way_waits_for_that_file() {
    let (state, video, _) = with_clips("twice");
    let mut rt = runtime(state);
    let id = video.attachment_id.clone();
    let entries = rt.state.session.entries.clone();
    press(&mut rt, &format!("picture-{id}"));
    assert!(rt.state.playback.writing(&id));
    // Another conversation and back before the file is there, and the video pressed again:
    // the first write is still the one waited for, and the clip plays from it.
    rt.update(|s, _| s.select_channel("design"));
    tick(&mut rt);
    assert!(rt.state.playback.clip(&id).is_none());
    assert!(rt.state.playback.writing(&id), "its file is still coming");
    rt.update(move |s, _| {
        s.select_channel("general");
        s.session.entries = entries;
        s.session.revision += 1;
    });
    tick(&mut rt);
    press(&mut rt, &format!("picture-{id}"));
    arrives(&mut rt, &video);
    assert!(playing(&rt, &id));
    assert!(!rt.state.playback.writing(&id));
}

#[cfg(any(target_os = "macos", target_os = "windows"))]
#[test]
fn scrolling_the_list_away_from_a_video_pauses_it() {
    // Enough messages before the video that the list's head is far from it.
    let (mut state, video, _) = with_clips("list");
    let now = chrono::Utc::now().timestamp_micros();
    let before = (0..60).map(|i| {
        let at = now - 10_000_000_000 + i * 1_000_000;
        message(&format!("list-before{i}"), "me", "And another thing.", at)
    });
    state.session.entries.splice(0..0, before);
    state.session.revision += 1;
    let mut rt = runtime(state);
    press(&mut rt, &format!("picture-{}", video.attachment_id));
    arrives(&mut rt, &video);
    assert!(playing(&rt, &video.attachment_id));
    rt.update(|_, tx| tx.scroll_to_y("messages", 0.0));
    settle(&mut rt);
    assert!(!playing(&rt, &video.attachment_id));
}

#[test]
fn another_conversation_ends_every_clip_and_removes_its_file() {
    let (state, video, _) = with_clips("leave");
    let mut rt = runtime(state);
    press(&mut rt, &format!("picture-{}", video.attachment_id));
    arrives(&mut rt, &video);
    let file = player_file(&video).unwrap();
    assert!(file.exists());
    rt.update(|s, _| s.select_channel("design"));
    tick(&mut rt);
    assert!(rt.state.playback.clip(&video.attachment_id).is_none());
    assert!(!file.exists());
    // A file that arrives for a clip no longer held is removed too.
    let late = clip("leave-late", "video");
    rt.update(|s, _| s.receive(&written(&late, CLIP)));
    assert!(!player_file(&late).unwrap().exists());
}

#[test]
fn a_page_in_the_conversations_place_ends_its_clips() {
    // A server's page over its channel: the session keeps the channel open under it.
    let (mut state, video, audio) = with_clips("page");
    for open in [
        State::open_server_settings,
        State::open_browse,
        State::open_server_mentions,
    ] {
        state.play_attachment(&audio);
        state.receive(&written(&audio, TONE));
        assert!(state.conversation_shown());
        open(&mut state);
        assert!(!state.conversation_shown());
        assert!(state.session.conversation.is_some());
        state.tick(chrono::Utc::now().timestamp_micros());
        assert!(state.playback.clip(&audio.attachment_id).is_none());
        assert!(!player_file(&audio).unwrap().exists());
        state.close_server_settings();
        state.close_page();
    }
    // A personal conversation at Home, then Preferences from the account menu.
    state.open_dm("page-dm");
    let mut sent = message("page-p3", "grace", "Here too.", 1);
    sent.attachments = vec![video.clone()];
    state.session.entries = vec![sent];
    state.play_attachment(&video);
    state.receive(&written(&video, CLIP));
    state.tick(chrono::Utc::now().timestamp_micros());
    assert!(state.playback.clip(&video.attachment_id).is_some());
    state.go_home(HomeView::Preferences);
    assert!(!state.conversation_shown());
    state.tick(chrono::Utc::now().timestamp_micros());
    assert!(state.playback.clip(&video.attachment_id).is_none());
    assert!(!player_file(&video).unwrap().exists());
    // Back at the conversation, which the session kept.
    state.go_home(HomeView::Conversations);
    assert!(state.conversation_shown());
    assert!(state.session.conversation.is_some());
}

#[cfg(any(target_os = "macos", target_os = "windows"))]
#[test]
fn a_video_goes_full_screen_and_escape_brings_it_back() {
    let (state, video, _) = with_clips("full");
    let mut rt = runtime(state);
    press(&mut rt, &format!("picture-{}", video.attachment_id));
    arrives(&mut rt, &video);
    press(&mut rt, &format!("full-screen-{}", video.attachment_id));
    assert_eq!(
        rt.state.playback.full_screen.as_deref(),
        Some(video.attachment_id.as_str())
    );
    assert!(has(&rt, "full-screen"));
    // Space pauses it there.
    rt.key(KeyEvent {
        key: Key::Text(" ".into()),
        mods: Mods::default(),
    });
    settle(&mut rt);
    assert!(!playing(&rt, &video.attachment_id));
    rt.key(KeyEvent {
        key: Key::Escape,
        mods: Mods::default(),
    });
    settle(&mut rt);
    assert!(rt.state.playback.full_screen.is_none());
    assert!(!has(&rt, "full-screen"));
    assert!(has(&rt, &format!("player-{}", video.attachment_id)));
}

#[cfg(any(target_os = "macos", target_os = "windows"))]
#[test]
fn the_position_moves_by_a_press_and_by_the_arrow_keys() {
    let (state, video, _) = with_clips("seek");
    let mut rt = runtime(state);
    press(&mut rt, &format!("picture-{}", video.attachment_id));
    arrives(&mut rt, &video);
    press(&mut rt, &format!("toggle-{}", video.attachment_id));
    let id = video.attachment_id.clone();
    let player = rt
        .state
        .playback
        .clip(&id)
        .unwrap()
        .player()
        .unwrap()
        .clone();
    let duration = player.duration().expect("the clip's length, once drawn");
    // The bar is a stop for the keyboard, where the arrow keys move it.
    let bar = format!("seek-{}", video.attachment_id);
    assert!(
        rt.elements()
            .into_iter()
            .any(|e| e.key.as_deref() == Some(bar.as_str()) && e.focusable)
    );
    rt.update(move |s, _| s.seek_clip(&id, 0.5));
    settle(&mut rt);
    assert!(
        (player.time() - duration / 2.0).abs() < 0.2,
        "{}",
        player.time()
    );
    let id = video.attachment_id.clone();
    rt.update(move |s, _| s.step_clip(&id, -1));
    settle(&mut rt);
    assert!(player.time() < 0.2, "{}", player.time());
}

#[test]
fn images_as_links_plays_a_video_from_its_file_row() {
    let (mut state, video, _) = with_clips("links");
    state.set_images(Media::Link);
    let mut rt = runtime(state);
    assert!(!has(&rt, &format!("picture-{}", video.attachment_id)));
    let play = format!("play-{}", video.attachment_id);
    assert_eq!(label(&rt, &play), "Play video, Demo.mp4");
    press(&mut rt, &play);
    // The row stays, and the player stands under it.
    assert!(has(&rt, &format!("file-{}", video.attachment_id)));
    assert!(has(&rt, &format!("player-{}", video.attachment_id)));
}

#[test]
fn an_attachment_the_server_let_go_never_plays() {
    let (mut state, mut video, _) = with_clips("gone");
    video.expired = true;
    video.thumbnail = None;
    state.session.entries[1].attachments = vec![video.clone()];
    state.session.revision += 1;
    let mut rt = runtime(state);
    assert!(words(&rt).contains("No longer available"), "{}", words(&rt));
    assert!(!has(&rt, &format!("play-{}", video.attachment_id)));
    let gone = video.clone();
    rt.update(move |s, _| s.play_attachment(&gone));
    settle(&mut rt);
    assert!(rt.state.playback.clip(&video.attachment_id).is_none());
    assert!(!has(&rt, &format!("player-{}", video.attachment_id)));
}

#[test]
fn where_rune_has_no_player_play_holds_no_clip() {
    // Linux: Play asks for the file for the system's player, as before, and draws nothing.
    let (mut state, video, audio) = with_clips("linux");
    state.playback.inline = false;
    let mut rt = runtime(state);
    press(&mut rt, &format!("picture-{}", video.attachment_id));
    press(&mut rt, &format!("play-{}", audio.attachment_id));
    assert!(rt.state.playback.clip(&video.attachment_id).is_none());
    assert!(rt.state.playback.clip(&audio.attachment_id).is_none());
    assert!(has(&rt, &format!("picture-{}", video.attachment_id)));
    assert!(!has(&rt, &format!("player-{}", video.attachment_id)));
}
