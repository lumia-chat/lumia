//! Attachments and link cards (specs/media.md 5 and 6) through a windowless `Runtime`: a
//! message's picture, file and card as drawn, the viewer, Preferences' Images and Embeds,
//! the composer's strip, paste and drop, the card previews, and an edit taking an
//! attachment and a card out. The pictures are made by the core from a real PNG, as a send
//! would make them. Uploading, sending and fetching need an instance and are in the core's
//! live tests (`lumia-client/tests/live_app.rs`), not here.

#![allow(clippy::expect_used, clippy::unwrap_used)]

mod common;

use std::sync::Arc;

use common::*;
use lumia_client::app::{
    AttachmentDraft, AttachmentSummary, BlobSummary, CardSummary, UploadLimitsSummary,
};
use lumia_shell::app::State;
use lumia_shell::app::media::fetched;
use lumia_shell::session::Update;
use lumia_shell::session::media::{MediaUpdate, Stage, Waiting};
use lumia_shell::settings::Media;
use rune::{ClipboardContent, Key, KeyEvent, Mods, Runtime};

const ICON: &[u8] = include_bytes!("../../../assets/brand/lumia-original-icon.png");

fn limits() -> Option<UploadLimitsSummary> {
    Some(UploadLimitsSummary {
        image_max_bytes: 2 * 1024 * 1024,
        file_max_bytes: 10 * 1024 * 1024,
        thumbnail_max_bytes: 256 * 1024,
    })
}

/// The brand's icon made into an attachment by the core, and its thumbnail's bytes.
fn picture(id: &str) -> (AttachmentSummary, Vec<u8>) {
    let made = lumia_core::media::process(lumia_core::media::Input {
        bytes: ICON.to_vec(),
        name: "mark.png".into(),
        as_file: false,
        pixels: None,
        poster: None,
        duration_ms: None,
        video_size: None,
        caps: lumia_core::media::Caps::default(),
    })
    .unwrap();
    let blob = |part: &str| BlobSummary {
        blob_id: format!("{id}-{part}"),
        ..BlobSummary::default()
    };
    (
        AttachmentSummary {
            attachment_id: id.into(),
            kind: "image".into(),
            name: made.name,
            media_type: made.media_type,
            width: made.width,
            height: made.height,
            placeholder: made.placeholder,
            alt: "The Lumia mark".into(),
            size_bytes: made.main.len() as u64,
            main: blob("main"),
            thumbnail: Some(blob("thumb")),
            ..AttachmentSummary::default()
        },
        made.thumbnail.unwrap(),
    )
}

fn pdf(id: &str) -> AttachmentSummary {
    AttachmentSummary {
        attachment_id: id.into(),
        kind: "file".into(),
        name: "Launch brief.pdf".into(),
        media_type: "application/pdf".into(),
        size_bytes: 248_000,
        main: BlobSummary {
            blob_id: format!("{id}-main"),
            ..BlobSummary::default()
        },
        ..AttachmentSummary::default()
    }
}

fn card() -> CardSummary {
    CardSummary {
        url: "https://example.com/notes".into(),
        title: "Notes on the first compiler".into(),
        description: "How it was built.".into(),
        site_name: "Totally Not Example".into(),
        host: "example.com".into(),
        image_url: "/api/v1/cards/abc".into(),
        ..CardSummary::default()
    }
}

/// Signed in, in #general, where Grace sent a picture and this account a file and a link
/// with its card; the instance takes attachments.
fn with_media() -> State {
    let mut state = signed_in();
    state.session.media.limits = limits();
    let (mark, thumb) = picture("a1");
    let now = chrono::Utc::now().timestamp_micros();
    let mut sent = message("p1", "grace", "The mark, as promised.", now - 120_000_000);
    sent.attachments = vec![mark];
    let mut file = message("p2", "me", "", now - 60_000_000);
    file.attachments = vec![pdf("a2")];
    let mut linked = message("p3", "me", "Notes here: https://example.com/notes", now);
    linked.cards = vec![card()];
    state.session.entries = vec![sent, file, linked];
    state.session.revision += 1;
    state.receive(&fetched("a1-thumb", &thumb));
    state.receive(&fetched("/api/v1/cards/abc", &thumb));
    state.tick(now);
    state.media.wait();
    state
}

fn element(rt: &Runtime<State>, key: &str) -> rune::ElementInfo {
    rt.elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some(key) && !e.exiting)
        .unwrap_or_else(|| panic!("{key} is not on screen"))
}

fn has(rt: &Runtime<State>, key: &str) -> bool {
    rt.elements()
        .into_iter()
        .any(|e| e.key.as_deref() == Some(key) && !e.exiting)
}

#[test]
fn a_message_draws_its_picture_its_file_and_its_card() {
    let mut rt = runtime(with_media());
    // The picture at its own shape, no taller than 400, named by its alt text.
    let tile = element(&rt, "picture-a1");
    assert!(
        tile.rect[3] <= 400.0 && tile.rect[3] > 0.0,
        "{:?}",
        tile.rect
    );
    assert_eq!(tile.label.as_deref(), Some("Picture: The Lumia mark"));
    // The file row: its name, its size, and Save.
    let said = words(&rt);
    assert!(said.contains("Launch brief.pdf"), "{said}");
    assert!(said.contains("248 KB"), "{said}");
    assert!(said.contains("Save Launch brief.pdf"), "{said}");
    // A message of a file alone draws no empty words over it.
    assert!(has(&rt, "file-a2"));
    // The card: the host the link names, never the page's own name for its site.
    assert!(said.contains("example.com"), "{said}");
    assert!(said.contains("Notes on the first compiler"), "{said}");
    assert!(!said.contains("Totally Not Example"), "{said}");
    // Pressing it opens the link as the words have it.
    press(&mut rt, "card-https://example.com/notes");
    assert_eq!(
        rt.take_url_requests(),
        vec!["https://example.com/notes".to_string()]
    );
}

#[test]
fn a_picture_opens_in_the_viewer_and_escape_closes_it() {
    let mut rt = runtime(with_media());
    press(&mut rt, "picture-a1");
    let viewer = rt.state.media.viewer.clone().expect("the viewer is open");
    assert_eq!(viewer.attachment.attachment_id, "a1");
    assert!(has(&rt, "viewer"));
    // Its alt text under the picture, and Save in its bar.
    let said = words(&rt);
    assert!(said.contains("The Lumia mark"), "{said}");
    assert!(has(&rt, "viewer-save"));
    // Zoomed in and out with the keys, back to fitting the window with 0.
    for (key, zoomed) in [("+", true), ("0", false)] {
        rt.key(KeyEvent {
            key: Key::Text(key.into()),
            mods: Mods::default(),
        });
        settle(&mut rt);
        let zoom = rt.state.media.viewer.as_ref().unwrap().zoom;
        assert_eq!(zoom > 1.0, zoomed, "{key}: {zoom}");
    }
    rt.key(KeyEvent {
        key: Key::Escape,
        mods: Mods::default(),
    });
    settle(&mut rt);
    assert!(rt.state.media.viewer.is_none());
    assert!(!has(&rt, "viewer"));
}

#[test]
fn images_as_links_are_file_rows_and_embeds_as_links_draw_no_card() {
    let mut state = with_media();
    state.set_images(Media::Link);
    state.set_embeds(Media::Link);
    let mut rt = runtime(state);
    assert!(!has(&rt, "picture-a1"));
    assert!(has(&rt, "file-a1"));
    assert!(!has(&rt, "card-https://example.com/notes"));
    // The picture's row still opens the viewer.
    press(&mut rt, "open-a1");
    assert!(rt.state.media.viewer.is_some());
}

#[test]
fn the_attach_button_is_there_only_when_the_instance_keeps_attachments() {
    let mut state = signed_in();
    state.select_channel("general");
    let rt = runtime(state);
    assert!(!has(&rt, "attach"));
    let mut state = signed_in();
    state.select_channel("general");
    state.session.media.limits = limits();
    let rt = runtime(state);
    assert_eq!(
        element(&rt, "attach").label.as_deref(),
        Some("Attach a file")
    );
}

fn draft(id: &str, kind: &str, name: &str) -> AttachmentDraft {
    AttachmentDraft {
        draft_id: id.into(),
        channel_id: "general".into(),
        kind: kind.into(),
        name: name.into(),
        size_bytes: 820_000,
        ..AttachmentDraft::default()
    }
}

#[test]
fn the_strip_holds_a_file_until_it_is_uploaded_and_the_send_waits_for_it() {
    let mut state = signed_in();
    state.select_channel("general");
    state.session.media.limits = limits();
    state.session.media.waiting = vec![Waiting {
        local: 1,
        channel_id: "general".into(),
        name: "photo.jpg".into(),
        size_bytes: 3_000_000,
        stage: Stage::Uploading,
    }];
    state.session.revision += 1;
    let mut rt = runtime(state);
    assert!(words(&rt).contains("Uploading…"));
    // Nothing to send while it uploads, even with words.
    assert!(!rt.state.can_send());
    assert!(!element(&rt, "send").focusable);
    // Uploaded: its size, an alt text field (it is a picture), and the send can go.
    rt.update(|s, _| {
        s.receive(&Update::Media(MediaUpdate::Attached {
            local: 1,
            result: Ok(draft("d1", "image", "photo.jpg")),
        }));
    });
    settle(&mut rt);
    let said = words(&rt);
    assert!(said.contains("820 KB"), "{said}");
    assert!(said.contains("Alt text for photo.jpg"), "{said}");
    assert!(rt.state.can_send());
    assert!(element(&rt, "send").focusable);
    // Its × takes it out.
    press(&mut rt, "remove-waiting-1");
    assert_eq!(rt.state.session.waiting_here().count(), 0);
    assert!(!has(&rt, "attachment-strip"));
}

#[test]
fn a_refused_file_says_why_in_words() {
    let mut state = signed_in();
    state.select_channel("general");
    state.session.media.limits = limits();
    // A file over the cap is refused before it is read.
    let file = data_dir().with_extension("mov");
    std::fs::write(&file, b"not really a video").unwrap();
    let big = std::fs::File::options().write(true).open(&file).unwrap();
    big.set_len(11 * 1024 * 1024).unwrap();
    state.session.attach_path(&file);
    let rt = runtime(state);
    let said = words(&rt);
    assert!(
        said.contains("Too large to attach: files can be up to 10.5 MB here."),
        "{said}"
    );
    let _ = std::fs::remove_file(file);
}

#[test]
fn a_pasted_picture_and_dropped_files_wait_in_the_strip() {
    let mut state = signed_in();
    state.select_channel("general");
    state.session.media.limits = limits();
    // Words paste as words.
    assert!(!state.composer_paste(&ClipboardContent::new("hello")));
    // A screenshot is attached.
    let png = rune::clipboard::ClipboardImage::from_png(Arc::<[u8]>::from(ICON)).unwrap();
    assert!(state.composer_paste(&ClipboardContent::new("").with_image(png)));
    let names: Vec<String> = state
        .session
        .waiting_here()
        .map(|w| w.name.clone())
        .collect();
    assert_eq!(names, ["Pasted image.png"]);
    // A file dropped onto the conversation.
    let mut rt = runtime(state);
    let file = data_dir().with_extension("txt");
    std::fs::write(&file, b"notes").unwrap();
    let (x, y) = rt.find("messages").unwrap();
    rt.drag_files(vec![file.clone()], Some((x, y)));
    settle(&mut rt);
    assert!(has(&rt, "drop-target"));
    rt.drop_files(vec![file.clone()], Some((x, y)));
    settle(&mut rt);
    assert!(!has(&rt, "drop-target"));
    let name = file.file_name().unwrap().to_string_lossy().into_owned();
    assert!(
        rt.state.session.waiting_here().any(|w| w.name == name),
        "dropped: {name}"
    );
    let _ = std::fs::remove_file(file);
}

#[test]
fn a_link_is_previewed_once_the_words_rest_and_its_x_puts_it_away() {
    let mut state = signed_in();
    state.select_channel("general");
    let link = "https://lumia.chat/blog";
    state.draft.replace_text(format!("Read {link}"));
    let now = chrono::Utc::now().timestamp_micros();
    state.tick(now);
    assert_eq!(state.session.previews_here().count(), 0, "not while typing");
    state.tick(now + 600_000);
    let asked: Vec<String> = state
        .session
        .previews_here()
        .map(|p| p.url.clone())
        .collect();
    assert_eq!(asked, [link]);
    state.receive(&Update::Media(MediaUpdate::Preview {
        channel_id: "general".into(),
        url: link.into(),
        card: Ok(Some(CardSummary {
            url: link.into(),
            title: "Attachments".into(),
            host: "lumia.chat".into(),
            ..CardSummary::default()
        })),
    }));
    let mut rt = runtime(state);
    assert!(words(&rt).contains("Attachments"));
    press(&mut rt, &format!("dismiss-preview-{link}"));
    assert_eq!(rt.state.session.previews_here().count(), 0);
}

#[test]
fn an_edit_takes_an_attachment_and_a_card_out_with_their_x() {
    let mut rt = runtime(with_media());
    // This account's file, edited: its × takes it out of the message being edited.
    rt.update(|s, tx| s.begin_edit("p2", tx));
    settle(&mut rt);
    press(&mut rt, "remove-a2");
    assert_eq!(rt.state.media.edit_removed, ["a2"]);
    assert!(!has(&rt, "file-a2"));
    rt.update(|s, tx| s.cancel_edit(tx));
    settle(&mut rt);
    assert!(rt.state.media.edit_removed.is_empty());
    assert!(has(&rt, "file-a2"));
    // The card of this account's link.
    rt.update(|s, tx| s.begin_edit("p3", tx));
    settle(&mut rt);
    press(&mut rt, "drop-card-https://example.com/notes");
    assert_eq!(rt.state.media.edit_dropped, ["https://example.com/notes"]);
    assert!(!has(&rt, "card-https://example.com/notes"));
}

#[test]
fn an_edit_names_the_cards_it_keeps_only_when_it_changes_them() {
    use lumia_shell::session::reads::edited_cards;
    let now = chrono::Utc::now().timestamp_micros();
    let mut entry = message(
        "e1",
        "me",
        "See https://a.example and https://b.example",
        now,
    );
    entry.cards = vec![CardSummary {
        url: "https://a.example".into(),
        ..CardSummary::default()
    }];
    // Words changed, links not: the instance keeps what it has.
    assert_eq!(
        edited_cards(&entry, "Look: https://a.example and https://b.example", &[]),
        None
    );
    // A card put away: the others the message had stay.
    assert_eq!(
        edited_cards(&entry, &entry.body.clone(), &["https://a.example".into()]),
        Some(Vec::new())
    );
    // A link added is asked for; one the author had put away before is not.
    assert_eq!(
        edited_cards(
            &entry,
            "https://a.example https://b.example https://c.example",
            &[]
        ),
        Some(vec![
            "https://a.example".to_string(),
            "https://c.example".to_string()
        ])
    );
}
