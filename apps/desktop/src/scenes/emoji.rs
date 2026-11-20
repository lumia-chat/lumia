//! A server's own emoji (specs/emoji.md), with the sample data of the step 12 boards: Acme
//! (from `roles`, seen by Ryan, who owns it) with its twelve emoji, oldest first, two of them
//! animated (partyparrot and parrotwave) and one twice as wide as tall (lgtm), and the morning
//! in #general the boards draw, where Leo's message from yesterday used :oldlogo:, since
//! removed. The pictures are the boards' drawings made into the pictures the instance would
//! keep (`scenes/emoji/`: PNG at 128, the two animated ones GIF), handed to the session as if
//! fetched. No instance is behind any of it.
//!
//! The boards are 1440 wide: `--size 1440x940` for the conversation's, `1440x1100` for the
//! settings drawn at their full length.

use std::sync::Arc;

use chrono::{Duration, Local, NaiveTime};
use lumia_client::app::{EmojiRefSummary, EmojiSummary, ReactionSummary};

use super::roles::{ACME, Acme, acme, person_id};
use crate::app::State;
use crate::app::roles::Section;
use crate::session::avatars::AvatarUpdate;
use crate::session::{Entry, Update};

/// One of Acme's emoji: its name, its picture, who uploaded it, and its pixels.
struct Sample {
    id: &'static str,
    name: &'static str,
    bytes: &'static [u8],
    by: &'static str,
    animated: bool,
    size: (u32, u32),
}

const fn sample(
    id: &'static str,
    name: &'static str,
    bytes: &'static [u8],
    by: &'static str,
) -> Sample {
    Sample {
        id,
        name,
        bytes,
        by,
        animated: false,
        size: (128, 128),
    }
}

/// Acme's twelve, oldest first, as the boards have them.
const TWELVE: [Sample; 12] = [
    sample("e1", "acme", include_bytes!("emoji/acme.png"), "ryan"),
    Sample {
        size: (256, 128),
        ..sample("e2", "lgtm", include_bytes!("emoji/lgtm.png"), "devon")
    },
    sample("e3", "shipit", include_bytes!("emoji/squirrel.png"), "sam"),
    sample(
        "e4",
        "plusone",
        include_bytes!("emoji/plusone.png"),
        "devon",
    ),
    sample("e5", "coffee", include_bytes!("emoji/coffee.png"), "priya"),
    sample("e6", "blobcat", include_bytes!("emoji/cat.png"), "maya"),
    sample("e7", "thisisfine", include_bytes!("emoji/flame.png"), "sam"),
    Sample {
        animated: true,
        ..sample(
            "e8",
            "partyparrot",
            include_bytes!("emoji/partyparrot.gif"),
            "maya",
        )
    },
    Sample {
        animated: true,
        ..sample(
            "e9",
            "parrotwave",
            include_bytes!("emoji/parrotwave.gif"),
            "devon",
        )
    },
    sample(
        "e10",
        "party_blob",
        include_bytes!("emoji/blob.png"),
        "maya",
    ),
    sample(
        "e11",
        "sadparrot",
        include_bytes!("emoji/sadparrot.png"),
        "leo",
    ),
    sample("e12", "hype", include_bytes!("emoji/star.png"), "priya"),
];

/// The id of a sample emoji, as an id the core would give (hex of 16 bytes).
pub fn emoji_id(key: &str) -> String {
    let mut bytes = [0u8; 16];
    for (i, b) in key.bytes().enumerate().take(16) {
        bytes[i] = b;
    }
    lumia_core::wire::display(&bytes)
}

/// Where a sample's picture is, as an instance names one.
fn image_url(key: &str) -> String {
    let hex: String = key.bytes().map(|b| format!("{b:02x}")).collect();
    format!("/api/v1/avatars/{hex:0<64}")
}

/// Acme's emoji as the snapshot carries them: the first `n` of the twelve.
pub fn acme_emoji(n: usize) -> Vec<EmojiSummary> {
    TWELVE
        .iter()
        .take(n)
        .map(|e| EmojiSummary {
            emoji_id: emoji_id(e.id),
            name: e.name.into(),
            image_url: image_url(e.id),
            animated: e.animated,
            width: e.size.0,
            height: e.size.1,
            created_by_account_id: person_id(e.by),
        })
        .collect()
}

/// Acme, seen by Ryan, with its first `n` emoji and their pictures held, `limit` its
/// limit.
pub fn with_emoji(state: &mut State, n: usize, limit: u32) {
    acme(state, &Acme::seen_by("ryan"));
    if let Some(server) = state
        .session
        .snapshot
        .as_mut()
        .and_then(|s| s.servers.iter_mut().find(|s| s.server_id == ACME))
    {
        server.emoji = acme_emoji(n);
        server.emoji_limit = limit;
        server.can_manage_emoji = true;
    }
    for e in TWELVE.iter().take(n) {
        state.receive(&Update::Avatar(AvatarUpdate::Fetched {
            url: image_url(e.id),
            bytes: Ok(Arc::from(e.bytes)),
        }));
    }
    state.tick(chrono::Utc::now().timestamp_micros());
    state.pictures.wait();
}

/// One message of the boards' morning: who, when (`yesterday` or today), what, and its
/// reactions (`emoji`, how many, whether one is Ryan's, and a custom one's key).
type Said = (
    &'static str,
    bool,
    (u32, u32),
    &'static str,
    &'static [(&'static str, u32, bool)],
);

const MORNING: [Said; 5] = [
    (
        "leo",
        true,
        (16, 12),
        "The launch badge is up: :oldlogo: :shipit:",
        &[(":oldlogo:", 4, false), ("🚀", 2, false)],
    ),
    (
        "maya",
        false,
        (9, 4),
        "Onboarding shipped this morning :partyparrot: thanks, all.",
        &[
            (":partyparrot:", 5, true),
            ("🎉", 3, false),
            (":hype:", 2, false),
        ],
    ),
    (
        "devon",
        false,
        (9, 11),
        ":lgtm: on the copy changes. Merging after lunch :coffee:",
        &[(":lgtm:", 2, false)],
    ),
    (
        "priya",
        false,
        (9, 24),
        "The build is red again :thisisfine:",
        &[],
    ),
    (
        "ryan",
        false,
        (9, 31),
        "Fixed, it was the cache. Green now :party_blob: :blobcat:",
        &[(":blobcat:", 1, false), ("👍", 2, true)],
    ),
];

/// The morning in #general, its refs found as the core finds them at a send, by the names
/// Acme's emoji have, and :oldlogo: as it was before it was removed.
fn talk(state: &mut State) {
    let mut names: Vec<(String, String)> = TWELVE
        .iter()
        .map(|e| (e.name.to_string(), emoji_id(e.id)))
        .collect();
    names.push(("oldlogo".into(), emoji_id("removed")));
    let today = Local::now().date_naive();
    let me = person_id("ryan");
    state.session.entries = MORNING
        .iter()
        .enumerate()
        .map(|(i, (who, yesterday, (hour, minute), body, reactions))| {
            let day = if *yesterday {
                today - Duration::days(1)
            } else {
                today
            };
            let at = NaiveTime::from_hms_opt(*hour, *minute, 0)
                .and_then(|time| day.and_time(time).and_local_timezone(Local).single())
                .map_or(0, |at| at.timestamp_micros());
            let emoji = lumia_core::emoji::find_refs(body, &names)
                .into_iter()
                .map(|(range, emoji_id)| EmojiRefSummary {
                    emoji_id,
                    start: range.start as u32,
                    end: range.end as u32,
                })
                .collect();
            let reactions = reactions
                .iter()
                .map(|(reaction, count, mine)| {
                    let emoji_id = reaction
                        .strip_prefix(':')
                        .and_then(|r| r.strip_suffix(':'))
                        .and_then(|name| names.iter().find(|(n, _)| n == name))
                        .map(|(_, id)| id.clone())
                        .unwrap_or_default();
                    ReactionSummary {
                        reaction: (*reaction).into(),
                        count: *count,
                        me: *mine,
                        account_ids: if *mine { vec![me.clone()] } else { Vec::new() },
                        emoji_id,
                    }
                })
                .collect();
            Entry {
                id: format!("m{i}"),
                seq: i as u64,
                author_account_id: person_id(who),
                body: (*body).into(),
                at_micros: at,
                deleted: false,
                edited: false,
                notice: String::new(),
                notice_about: String::new(),
                reply: None,
                mentions: Vec::new(),
                reactions,
                mentions_me: false,
                attachments: Vec::new(),
                cards: Vec::new(),
                emoji,
            }
        })
        .collect();
    state.session.revision += 1;
}

/// "Emoji in messages": the morning, Acme's emoji in its text and its reactions.
pub fn messages(state: &mut State) {
    with_emoji(state, 12, 50);
    talk(state);
}

/// "Emoji picker": the picker over Maya's message, opened from its Add reaction, with
/// "Frequently used" holding two of Acme's.
pub fn picker(state: &mut State) {
    messages(state);
    state.settings.recent_emoji = vec![
        "👍".into(),
        format!("custom:{}", emoji_id("e8")),
        "❤️".into(),
        "😂".into(),
        "🎉".into(),
        format!("custom:{}", emoji_id("e2")),
        "😮".into(),
        "🙏".into(),
    ];
    state.open_picker("m1", "add-reaction-m1");
}

/// The picker's search for "par": Acme's that begin with it, then those that hold it, then
/// Unicode's.
pub fn picker_search(state: &mut State) {
    picker(state);
    if let Some(picker) = &mut state.replies.picker {
        picker.search.replace_text("par");
    }
}

/// The composer's `:` list: "Shipped the importer :par", the first row chosen.
pub fn composer(state: &mut State) {
    messages(state);
    state.draft.replace_text("Shipped the importer :par");
    state.follow_colon();
}

/// The composer's emoji button (Ryan, 2026-11-20): the picker hung from it, over the
/// composer, staying open while what is chosen goes in at the caret.
pub fn composer_picker(state: &mut State) {
    messages(state);
    state.draft.replace_text("Shipped the importer ");
    state.open_composer_picker();
}

/// The settings' Emoji section, its twelve, as the owner sees them.
pub fn settings(state: &mut State) {
    with_emoji(state, 12, 50);
    state.open_server_settings();
    state.choose_section(Section::Emoji);
}

/// The upload form over the list, a picture chosen: "Acme-Heart v2.png".
pub fn upload(state: &mut State) {
    settings(state);
    choose_heart(state);
}

/// A picture chosen for the form as the session reads one.
fn choose_heart(state: &mut State) {
    use crate::session::emoji::EmojiUpdate;
    let path = std::path::PathBuf::from("/Users/ryan/Pictures/Acme-Heart v2.png");
    state.read_emoji_file(&path);
    state.receive(&Update::Emoji(EmojiUpdate::Read {
        path,
        result: Ok((
            Arc::from(&include_bytes!("emoji/heart.png")[..]),
            "image/png",
        )),
    }));
    if let Some(upload) = &state.emoji.upload {
        let _ = upload.picture.as_ref().map(rune::Image::wait);
    }
}

/// The upload refused: the name is taken.
pub fn upload_taken(state: &mut State) {
    use crate::session::Failure;
    use crate::session::emoji::EmojiUpdate;
    upload(state);
    if let Some(upload) = &mut state.emoji.upload {
        upload.name.replace_text("partyparrot");
        upload.busy = true;
    }
    state.receive(&Update::Emoji(EmojiUpdate::Uploaded {
        server_id: ACME.into(),
        result: Err(Failure {
            code: "ALREADY_EXISTS".into(),
            message: "taken".into(),
            details: [("existing_id".to_string(), emoji_id("e8"))].into(),
        }),
    }));
}

/// The upload refused: the picture is too large.
pub fn upload_too_large(state: &mut State) {
    use crate::session::Failure;
    use crate::session::emoji::EmojiUpdate;
    upload(state);
    if let Some(upload) = &mut state.emoji.upload {
        upload.busy = true;
    }
    state.receive(&Update::Emoji(EmojiUpdate::Uploaded {
        server_id: ACME.into(),
        result: Err(Failure {
            code: "INVALID_ARGUMENT".into(),
            message: "too large".into(),
            details: [
                ("field".to_string(), "picture".to_string()),
                ("reason".to_string(), "too_large".to_string()),
            ]
            .into(),
        }),
    }));
}

/// Rename, in place: parrotwave being renamed parrot_wave.
pub fn rename(state: &mut State) {
    settings(state);
    rename_wave(state, "parrot_wave");
}

fn rename_wave(state: &mut State, to: &str) {
    state.begin_rename(&emoji_id("e9"));
    if let Some(form) = &mut state.emoji.renaming {
        form.name.replace_text(to);
    }
}

/// A rename refused: the name is taken.
pub fn rename_taken(state: &mut State) {
    use crate::session::Failure;
    use crate::session::emoji::EmojiUpdate;
    settings(state);
    rename_wave(state, "partyparrot");
    if let Some(form) = &mut state.emoji.renaming {
        form.busy = true;
    }
    state.receive(&Update::Emoji(EmojiUpdate::Renamed {
        server_id: ACME.into(),
        emoji_id: emoji_id("e9"),
        result: Err(Failure {
            code: "ALREADY_EXISTS".into(),
            message: "taken".into(),
            details: [("existing_id".to_string(), emoji_id("e8"))].into(),
        }),
    }));
}

/// "Remove :parrotwave:?", against its row's Remove.
pub fn remove(state: &mut State) {
    settings(state);
    let id = emoji_id("e9");
    state.ask_remove_emoji(&id, &format!("remove-emoji-{id}"));
}

/// At the limit: 12 of 12, Upload off, the section saying Acme has as many as it may.
pub fn full(state: &mut State) {
    with_emoji(state, 12, 12);
    state.open_server_settings();
    state.choose_section(Section::Emoji);
}
