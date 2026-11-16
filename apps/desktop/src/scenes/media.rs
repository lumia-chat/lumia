//! Attachments and link cards (specs/media.md 5 and 6) over the sample server, with no
//! instance behind them: the pictures are made by the core from the scenes' own portrait
//! and the brand's icon, as a send would make them, and handed to the state as if fetched.

use lumia_client::app::{
    AttachmentDraft, AttachmentSummary, BlobSummary, CardSummary, UploadLimitsSummary,
};
use lumia_core::media::{Caps, Input, process};

use crate::app::State;
use crate::app::media::fetched;
use crate::session::media::{MediaUpdate, Preview, Stage, Waiting, player_file};
use crate::session::{Entry, Failure, Update};

const PORTRAIT: &[u8] = include_bytes!("portrait.jpg");
/// Three seconds of a test pattern over a tone, and of a tone alone, made for the scenes and
/// the tests from FFmpeg's own sources (`testsrc`, `sine`): 640 by 360, H.264 and AAC.
pub const CLIP: &[u8] = include_bytes!("clip.mp4");
pub const TONE: &[u8] = include_bytes!("tone.m4a");
const ICON: &[u8] = include_bytes!("../../../../assets/brand/lumia-original-icon.png");

/// A picture as the core makes it for sending: its attachment, and its thumbnail's bytes.
fn picture(id: &str, name: &str, bytes: &[u8], alt: &str) -> (AttachmentSummary, Vec<u8>) {
    let made = process(Input {
        bytes: bytes.to_vec(),
        name: name.into(),
        as_file: false,
        pixels: None,
        poster: None,
        duration_ms: None,
        video_size: None,
        caps: Caps::default(),
    })
    .unwrap_or_else(|error| panic!("the sample picture {name}: {error}"));
    let blob = |suffix: &str| BlobSummary {
        blob_id: format!("{id}-{suffix}"),
        ..BlobSummary::default()
    };
    let thumbnail = made.thumbnail.unwrap_or_default();
    (
        AttachmentSummary {
            attachment_id: id.into(),
            kind: "image".into(),
            name: made.name,
            media_type: made.media_type,
            width: made.width,
            height: made.height,
            placeholder: made.placeholder,
            alt: alt.into(),
            size_bytes: made.main.len() as u64,
            main: blob("main"),
            thumbnail: Some(blob("thumb")),
            ..AttachmentSummary::default()
        },
        thumbnail,
    )
}

fn file(id: &str, kind: &str, name: &str, media_type: &str, size: u64) -> AttachmentSummary {
    AttachmentSummary {
        attachment_id: id.into(),
        kind: kind.into(),
        name: name.into(),
        media_type: media_type.into(),
        size_bytes: size,
        main: BlobSummary {
            blob_id: format!("{id}-main"),
            ..BlobSummary::default()
        },
        ..AttachmentSummary::default()
    }
}

/// `media`: a channel where a picture the server has since let go (specs/media.md 4.5), a
/// picture, a file, two pictures together, a link's card, a video without a poster and a
/// recording were sent, and the composer holding a picture
/// with its alt text, a file uploading, one refused, and a link's card previewed.
/// `--snap out.png --scene media --size 1180x1500`.
pub fn media(state: &mut State) {
    super::servers_sample(state);
    state.session.media.limits = Some(UploadLimitsSummary {
        image_max_bytes: 2 * 1024 * 1024,
        file_max_bytes: 10 * 1024 * 1024,
        thumbnail_max_bytes: 512 * 1024,
    });
    let (portrait, portrait_thumb) = picture(
        "a-portrait",
        "portrait.jpg",
        PORTRAIT,
        "Grace at the bench, smiling",
    );
    let (second, _) = picture("a-portrait-2", "portrait.jpg", PORTRAIT, "");
    let (icon, icon_thumb) = picture("a-icon", "lumia.png", ICON, "The Lumia mark");
    let now = chrono::Utc::now().timestamp_micros();
    let entry = |id: &str, author: &str, body: &str, ago: i64| Entry {
        id: id.into(),
        seq: 0,
        author_account_id: author.into(),
        body: body.into(),
        at_micros: now - ago * 60_000_000,
        deleted: false,
        notice: String::new(),
        notice_about: String::new(),
        reply: None,
        mentions: Vec::new(),
        reactions: Vec::new(),
        mentions_me: false,
        edited: false,
        attachments: Vec::new(),
        cards: Vec::new(),
    };
    let card = CardSummary {
        url: "https://example.com/notes/compilers".into(),
        title: "Notes on the first compiler".into(),
        description: "How a program that writes programs was built, and why nobody believed it \
                      could be done until it was."
            .into(),
        site_name: "Not shown: a page names itself what it likes".into(),
        host: "example.com".into(),
        image_url: "/api/v1/cards/9f2c".into(),
        image_width: 400,
        image_height: 400,
        ..CardSummary::default()
    };
    state.session.entries = vec![
        Entry {
            attachments: vec![AttachmentSummary {
                attachment_id: "a-gone".into(),
                kind: "image".into(),
                name: "Offsite, last spring.jpg".into(),
                media_type: "image/jpeg".into(),
                expired: true,
                ..AttachmentSummary::default()
            }],
            ..entry("x0", "grace", "From the offsite.", 40)
        },
        Entry {
            attachments: vec![portrait.clone()],
            ..entry("x1", "grace", "The new portrait, as promised.", 30)
        },
        Entry {
            attachments: vec![file(
                "a-brief",
                "file",
                "Launch brief, final.pdf",
                "application/pdf",
                248_000,
            )],
            ..entry("x2", "me", "And the brief:", 25)
        },
        Entry {
            attachments: vec![second, icon],
            ..entry("x3", "grace", "", 20)
        },
        Entry {
            cards: vec![card],
            ..entry(
                "x4",
                "me",
                "Notes here: https://example.com/notes/compilers",
                10,
            )
        },
        Entry {
            attachments: vec![
                file("a-video", "video", "Demo.mov", "video/quicktime", 8_400_000),
                file("a-audio", "audio", "Voice note.m4a", "audio/mp4", 412_000),
            ],
            ..entry("x5", "grace", "The demo, and what I said over it.", 4)
        },
    ];
    state.session.revision += 1;
    for (key, bytes) in [
        ("a-portrait-thumb", &portrait_thumb),
        ("a-portrait-2-thumb", &portrait_thumb),
        ("a-icon-thumb", &icon_thumb),
        ("/api/v1/cards/9f2c", &icon_thumb),
    ] {
        state.receive(&fetched(key, bytes));
    }
    // The composer: a picture uploaded with its alt text, a file on its way, one refused,
    // and the card of the link in the words.
    let draft = AttachmentDraft {
        draft_id: "d-portrait".into(),
        channel_id: "general".into(),
        kind: "image".into(),
        name: portrait.name.clone(),
        media_type: portrait.media_type.clone(),
        width: portrait.width,
        height: portrait.height,
        placeholder: portrait.placeholder.clone(),
        alt: "Grace at the bench".into(),
        size_bytes: portrait.size_bytes,
        ..AttachmentDraft::default()
    };
    let waiting = |local: u64, name: &str, size: u64, stage: Stage| Waiting {
        local,
        channel_id: "general".into(),
        name: name.into(),
        size_bytes: size,
        stage,
    };
    state.session.media.waiting = vec![
        waiting(1, &portrait.name, portrait.size_bytes, Stage::Ready(draft)),
        waiting(2, "Quarterly numbers.xlsx", 1_250_000, Stage::Uploading),
        waiting(
            3,
            "Whole talk.mov",
            48_000_000,
            Stage::Failed("Too large to attach: files can be up to 10 MB here.".into()),
        ),
    ];
    state
        .media
        .alts
        .insert(1, rune::TextEdit::new("Grace at the bench"));
    let link = "https://lumia.chat/blog/attachments";
    state
        .draft
        .replace_text(format!("The write-up is at {link}"));
    state.session.media.previews = vec![Preview {
        channel_id: "general".into(),
        url: link.into(),
        card: Some(CardSummary {
            url: link.into(),
            title: "Attachments and link cards".into(),
            host: "lumia.chat".into(),
            ..CardSummary::default()
        }),
        loading: false,
        dismissed: false,
    }];
    state.session.media.revision += 1;
    state.session.last_error = None;
    state.tick(now);
    state.media.wait();
}

/// `media-links`: the same with Preferences' Images and Embeds at "As links": every
/// attachment a file row, and no card.
pub fn media_links(state: &mut State) {
    media(state);
    state.settings.images = crate::settings::Media::Link;
    state.settings.embeds = crate::settings::Media::Link;
    state.session.revision += 1;
}

/// `media-viewer`: the portrait opened in the viewer, its display picture arrived.
pub fn media_viewer(state: &mut State) {
    media(state);
    let portrait = state.session.entries[0].attachments[0].clone();
    state.open_viewer(&portrait);
    state.receive(&fetched("a-portrait-main", PORTRAIT));
    state.tick(chrono::Utc::now().timestamp_micros());
    state.media.wait();
}

/// `media-edit`: this account's message with a card being edited, its card's × there.
pub fn media_edit(state: &mut State) {
    media(state);
    let field = rune::TextEdit::new("Notes here: https://example.com/notes/compilers");
    state.replies.editing = Some(("x4".into(), field));
    state.session.revision += 1;
}

/// `media-playing`: the demo video started from its row and playing under it, and the voice
/// note started first and paused when the video began (one plays at a time). On Linux,
/// where Rune has no player, Play opens the system's and this is `media` again.
/// `--snap out.png --scene media-playing --size 1180x1500`.
pub fn media_playing(state: &mut State) {
    media(state);
    let Some(entry) = state.session.entries.iter_mut().find(|e| e.id == "x5") else {
        return;
    };
    // The size a send would have read from the video (spec 3.3).
    if let Some(video) = entry.attachments.first_mut() {
        video.width = 640;
        video.height = 360;
    }
    let attachments = entry.attachments.clone();
    state.session.revision += 1;
    for (attachment, bytes) in attachments.iter().rev().zip([TONE, CLIP]) {
        state.play_attachment(attachment);
        state.receive(&written(attachment, bytes));
    }
}

/// `media-failed`: the demo video and the voice note pressed and neither file fetched: what
/// each says, beside the Play that asks for it again.
pub fn media_failed(state: &mut State) {
    media(state);
    let Some(entry) = state.session.entries.iter().find(|e| e.id == "x5") else {
        return;
    };
    for attachment in entry.attachments.clone() {
        state.play_attachment(&attachment);
        state.receive(&Update::Media(MediaUpdate::Written {
            attachment_id: attachment.attachment_id,
            file: Err(Failure {
                message: "the instance could not be reached".into(),
                ..Failure::default()
            }),
        }));
    }
}

/// What the session reports once an attachment's bytes are written for the window's player,
/// here with the bytes given: for a scene or a test, with no instance to fetch them from.
pub fn written(attachment: &AttachmentSummary, bytes: &[u8]) -> Update {
    let file = player_file(attachment)
        .ok_or_else(|| "no player takes this media type".to_string())
        .and_then(|file| {
            file.parent()
                .map_or(Ok(()), std::fs::create_dir_all)
                .and_then(|()| std::fs::write(&file, bytes))
                .map(|()| file)
                .map_err(|e| e.to_string())
        })
        .map_err(|message| Failure {
            message,
            ..Failure::default()
        });
    Update::Media(MediaUpdate::Written {
        attachment_id: attachment.attachment_id.clone(),
        file,
    })
}
