//! Attachments and link cards as the window draws them (specs/media.md 5 and 6): under a
//! message's words, in the composer, and the full-screen viewer.
//!
//! The design system draws two of these: the file row (`Attachment`, `lu-attach` in the web
//! gallery: the kind's tile, the name, the size, a download button on the inset fill) and
//! the composer's attach button (a plus, "Attach a file", at the field's start). Nothing
//! else has a board: a picture in a message, several in a grid, a link card, the strip of
//! files waiting over the composer, the card previews there, and the viewer are made of the
//! design system's pieces (the inset fill and the 12 radius of the file row and the note
//! over the composer, the scrim of a dialog, the icon buttons) and the spec's numbers, and
//! are to be checked against boards once there are some.
//!
//! A picture is drawn at its own shape, at most 400 tall, its placeholder under it until
//! its thumbnail has decoded; several stand in a grid of equal squares, cropped to fill
//! them. A GIF moves unless the system asks for reduced motion, when its first frame is
//! drawn with a GIF badge and a press plays it. A video is its poster with a play symbol, or
//! a file row with a Play button when it has none; a recording is a file row with Play.
//! Any other file is the file row with Save. Preferences' Images at "As links" draws every
//! attachment as a file row, a picture's opening the viewer; Embeds at "As links" draws no
//! card.
//!
//! A video pressed plays in its poster's place, at its shape and at most 400 tall, or under
//! its file row, with the shell's controls under the picture: play or pause, the position
//! (a slider: a press or a drag, the arrow keys five seconds, Home and End), the time, sound
//! on or off, and full screen, over the whole window (`app::playback`). A recording's row
//! holds the same controls, without full screen, under its name. No board draws a player:
//! the controls are the design system's icon buttons and a 4-point track for the position,
//! in the accent where it has played. On Linux, where Rune has no player, Play opens the
//! system's as before. One the server let go stays the row that says so.
//!
//! A card is the host the link names (never the page's own name for its site), the title,
//! the description and the picture beside them, with a play symbol over a video's; a press
//! opens the link as the message's words have it.

use lumia_client::app::{AttachmentSummary, CardSummary};
use lumia_core::viewer;
use rune::{
    Cursor, El, Icon, MagnifyEvent, Player, SurfaceFit, Tx, WheelEvent, col, el, icon, row, text,
    text_input,
};

use super::components::{Look, drawn, icon_button};
use crate::app::State;
use crate::app::media::{fitted, grid};
use crate::app::playback::{Clip, clock};
use crate::session::Entry;
use crate::session::media::{Stage, Waiting, blob_key, card_key, is_picture, size_words};
use crate::settings::Media;
use crate::tokens::{self as t, icons};

/// A glyph button that takes the `Tx` (a dialog, the browser), `size` across.
fn glyph_button(
    key: impl Into<String>,
    glyph: &'static Icon,
    label: &str,
    size: f32,
    press: impl Fn(&mut State, &mut Tx) + 'static,
    look: Look,
) -> El {
    let c = look.c;
    glyph_face(glyph, size, look)
        .key(key)
        .role("button")
        .label(label)
        .help(label)
        .hover(c.hover_icon)
        .hover_group()
        .cursor(Cursor::Pointer)
        .focusable(true)
        .focus_ring(2.0, 0.0, c.focus)
        .no_window_drag()
        .on_press_tx(press)
}

/// How a [`glyph_button`] looks, with nothing to press: Preferences' preview draws one.
pub(crate) fn glyph_face(glyph: &'static Icon, size: f32, look: Look) -> El {
    let c = look.c;
    el().size(size, size).radius(t::RADIUS_9).center().child(
        icon(glyph, t::SPACE_16)
            .color(c.ink_quiet)
            .group_color(c.ink_field),
    )
}

/// What a picture's button is called: its alt text when it has one, its name otherwise.
fn picture_label(attachment: &AttachmentSummary) -> String {
    if attachment.alt.trim().is_empty() {
        format!("Picture, {}", attachment.name)
    } else {
        format!("Picture: {}", attachment.alt.trim())
    }
}

/// The best picture held for an attachment: a GIF itself while it may move, else the
/// thumbnail. None when nothing has decoded yet.
fn shown_picture<'a>(
    s: &'a State,
    attachment: &AttachmentSummary,
    moving: bool,
) -> Option<&'a rune::Image> {
    let main = (moving && attachment.kind == "gif")
        .then(|| s.media.image(&blob_key(&attachment.main)))
        .flatten()
        .filter(|image| drawn(image));
    let thumbnail = attachment
        .thumbnail
        .as_ref()
        .and_then(|blob| s.media.image(&blob_key(blob)))
        .filter(|image| drawn(image));
    main.or(thumbnail)
}

/// Whether an attachment drawn as a picture may not be: its thumbnail, or a GIF itself,
/// arrived and names more than may be decoded (spec 5). It is drawn as a file row then.
fn undrawable(s: &State, attachment: &AttachmentSummary) -> bool {
    let media = &s.session.media;
    attachment
        .thumbnail
        .as_ref()
        .is_some_and(|blob| media.undrawable(&blob_key(blob)))
        || (attachment.kind == "gif" && media.undrawable(&blob_key(&attachment.main)))
}

/// A picture, a GIF or a video's poster, `size` points, cropped to fill when `fill`.
fn picture_tile(
    s: &State,
    attachment: &AttachmentSummary,
    size: [f32; 2],
    fill: bool,
    look: Look,
) -> El {
    let c = look.c;
    let video = attachment.kind == "video";
    let gif = attachment.kind == "gif";
    let moving = !look.still || s.media.playing.contains(&attachment.attachment_id);
    let fit = if fill {
        SurfaceFit::Fill
    } else {
        SurfaceFit::Fit
    };
    let mut tile = el()
        .key(format!("picture-{}", attachment.attachment_id))
        .size(size[0], size[1])
        .radius(t::RADIUS_12)
        .clip_rounded(t::RADIUS_12)
        .fill(c.inset)
        .role("button")
        .label(if video {
            play_label(attachment)
        } else {
            picture_label(attachment)
        })
        .cursor(Cursor::Pointer)
        .focusable(true)
        .focus_ring(2.0, 2.0, c.focus);
    let layer = |image: &rune::Image| {
        el().absolute(0.0, 0.0)
            .size(size[0], size[1])
            .pixels_fit(image.pixels(), fit)
            .a11y_hidden()
    };
    let picture = shown_picture(s, attachment, moving);
    // The placeholder until the picture has decoded: a picture with transparency would
    // show it through.
    if picture.is_none()
        && let Some(placeholder) = s.media.placeholder(&attachment.attachment_id)
    {
        tile = tile.child(layer(placeholder));
    }
    if let Some(picture) = picture {
        tile = tile.child(layer(picture));
    }
    let a = attachment.clone();
    if video {
        tile = tile
            .on_press(move |s: &mut State| s.play_attachment(&a))
            .center()
            .child(play_disc(look));
    } else if gif && !moving {
        let id = attachment.attachment_id.clone();
        tile = tile
            .on_press(move |s: &mut State| s.play_gif(&id))
            .child(gif_badge(look));
    } else {
        tile = tile.on_press(move |s: &mut State| s.open_viewer(&a));
    }
    tile
}

/// The play symbol over a poster or a card's picture: a disc of the scrim, the glyph in the
/// ink that stands on the accent.
fn play_disc(look: Look) -> El {
    let c = look.c;
    el().size(t::PLAY_DISC, t::PLAY_DISC)
        .radius(t::PLAY_DISC / 2.0)
        .fill(c.scrim)
        .center()
        .a11y_hidden()
        .child(icon(&icons::PLAY, t::SPACE_24).color(c.on_accent))
}

/// "GIF" at a still GIF's corner: the design system's small tag.
fn gif_badge(look: Look) -> El {
    let c = look.c;
    el().absolute(t::SPACE_8, t::SPACE_8)
        .px(t::SPACE_6)
        .py(t::SPACE_2)
        .radius(t::RADIUS_5)
        .fill(c.raised)
        .a11y_hidden()
        .child(text("GIF").font(t::TEXT_TAG_SMALL).color(c.ink))
}

/// The design system's file row: the kind's tile, the name, the size, and what can be done
/// with it. A picture's row (Images "As links", or one that may not be decoded) opens the
/// viewer when pressed; a video's and a recording's have Play. One the server let go says
/// so where its size was, and does nothing (specs/media.md 4.5).
fn file_row(
    s: &State,
    attachment: &AttachmentSummary,
    ty: &t::MessageType,
    editing: bool,
    look: Look,
) -> El {
    let c = look.c;
    let gone = attachment.expired;
    let playable = !gone && matches!(attachment.kind.as_str(), "video" | "audio");
    let opens = is_picture(attachment) && !undrawable(s, attachment);
    let glyph = if playable { &icons::PLAY } else { &icons::FILE };
    let size = if gone {
        "No longer available".to_string()
    } else {
        size_words(attachment.size_bytes)
    };
    let mut words = file_words(glyph, &attachment.name, &size, ty, look);
    if opens {
        let a = attachment.clone();
        words = words
            .key(format!("open-{}", attachment.attachment_id))
            .role("button")
            .label(format!("{}, {size}", picture_label(attachment)))
            .cursor(Cursor::Pointer)
            .focusable(true)
            .focus_ring(2.0, 2.0, c.focus)
            .on_press(move |s: &mut State| s.open_viewer(&a));
    }
    let mut frame = row().w_full().gap(t::SPACE_10).items_center().child(words);
    // Once started, the clip's own controls play and pause it: the row's Play goes.
    let clip = s
        .playback
        .clip(&attachment.attachment_id)
        .filter(|_| playable);
    if playable && clip.is_none() {
        let a = attachment.clone();
        frame = frame.child(glyph_button(
            format!("play-{}", attachment.attachment_id),
            &icons::PLAY,
            &play_label(attachment),
            t::FILE_ACTION,
            move |s: &mut State, _| s.play_attachment(&a),
            look,
        ));
    }
    if !gone {
        let a = attachment.clone();
        frame = frame.child(glyph_button(
            format!("save-{}", attachment.attachment_id),
            &icons::DOWNLOAD,
            &format!("Save {}", attachment.name),
            t::FILE_ACTION,
            move |s: &mut State, tx| s.save_attachment(&a, tx),
            look,
        ));
    }
    if editing {
        frame = frame.child(remove_button(attachment, look));
    }
    let mut framed = file_box(frame, look).key(format!("file-{}", attachment.attachment_id));
    // A recording plays in its row; a video under it (`attachments`).
    if let Some(clip) = clip.filter(|clip| !clip.is_video()) {
        framed = framed.child(clip_controls(clip, ty, false, look));
    }
    framed
}

/// A file row's kind tile, name and size, as the conversation draws them.
pub(crate) fn file_words(
    glyph: &'static Icon,
    name: &str,
    size: &str,
    ty: &t::MessageType,
    look: Look,
) -> El {
    let c = look.c;
    row()
        .grow(1.0)
        .min_w(0.0)
        .gap(t::SPACE_10)
        .items_center()
        .child(
            el().size(t::FILE_TILE, t::FILE_TILE)
                .radius(t::RADIUS_9)
                .fill(c.file_pdf)
                .center()
                .a11y_hidden()
                .child(icon(glyph, t::FILE_GLYPH).color(c.file_pdf_ink)),
        )
        .child(
            text(name.to_string())
                .font(ty.file)
                .color(c.ink)
                .truncate()
                .shrink()
                .min_w(0.0),
        )
        .child(text(size.to_string()).font(ty.meta).color(c.ink_muted))
}

/// The box a file row stands in: its words and its buttons in `frame`.
pub(crate) fn file_box(frame: El, look: Look) -> El {
    col()
        .max_w(t::MEDIA_MAX)
        .padding(t::SPACE_8)
        .gap(t::SPACE_6)
        .radius(t::RADIUS_12)
        .fill(look.c.inset)
        .child(frame)
}

/// What a video's or a recording's Play is called.
fn play_label(attachment: &AttachmentSummary) -> String {
    if attachment.kind == "video" {
        format!("Play video, {}", attachment.name)
    } else {
        format!("Play {}", attachment.name)
    }
}

/// A clip's controls: play or pause, the position, the time, sound, and for a video full
/// screen (or, in full screen, the way out). "Loading…", or why it cannot play, until it can.
fn clip_controls(clip: &Clip, ty: &t::MessageType, full_screen: bool, look: Look) -> El {
    let c = look.c;
    let a = &clip.attachment;
    let id = a.attachment_id.clone();
    let line = row()
        .key(format!("controls-{id}"))
        .w_full()
        .min_h(t::ICON_BUTTON)
        .gap(t::SPACE_6)
        .items_center();
    if let Some(why) = clip.failure() {
        let why = text(why)
            .font(ty.meta)
            .color(c.ink_muted)
            .wrap()
            .grow(1.0)
            .min_w(0.0)
            .role("alert");
        // One whose file could not be fetched keeps its Play, which asks for it again.
        return if clip.fetch_failed() {
            let again = id.clone();
            line.child(icon_button(
                format!("toggle-{id}"),
                &icons::PLAY,
                &play_label(a),
                move |s: &mut State| s.toggle_clip(&again),
                look,
            ))
            .child(why)
        } else {
            line.child(why)
        };
    }
    let Some(player) = clip.player() else {
        return line.child(
            text("Loading…")
                .font(ty.meta)
                .color(c.ink_muted)
                .role("status"),
        );
    };
    let playing = player.is_playing();
    let time = player.time();
    let duration = player.duration().filter(|d| d.is_finite() && *d > 0.0);
    // A length is said to the nearest second (a 2.98-second clip is 0:03), a time as it runs.
    let length = |d: f64| clock(d.round());
    #[allow(clippy::cast_possible_truncation)]
    let played = duration.map_or(0.0, |d| (time / d).clamp(0.0, 1.0)) as f32;
    let toggle = {
        let id = id.clone();
        icon_button(
            format!("toggle-{id}"),
            if playing { &icons::PAUSE } else { &icons::PLAY },
            &if playing {
                format!("Pause {}", a.name)
            } else {
                play_label(a)
            },
            move |s: &mut State| s.toggle_clip(&id),
            look,
        )
    };
    let (drag_id, step_id, end_id) = (id.clone(), id.clone(), id.clone());
    let position = el()
        .key(format!("seek-{id}"))
        .grow(1.0)
        .min_w(t::SPACE_40)
        .h(t::ICON_BUTTON)
        .center()
        .role("slider")
        .label(format!("Position in {}", a.name))
        .range_value(time, 0.0, duration.unwrap_or(0.0))
        .a11y_value(format!(
            "{} of {}",
            clock(time),
            length(duration.unwrap_or(0.0))
        ))
        .cursor(Cursor::Pointer)
        .focus_ring(2.0, 0.0, c.focus)
        .no_window_drag()
        .on_drag(move |s: &mut State, drag, _| {
            if drag.w > 0.0 {
                s.seek_clip(&drag_id, drag.x / drag.w);
            }
        })
        .on_step(move |s: &mut State, steps, _| s.step_clip(&step_id, steps))
        .on_step_limit(move |s: &mut State, end, _| s.clip_to_end(&end_id, end))
        .child(
            el().w_full()
                .h(t::SPACE_4)
                .radius(t::SPACE_2)
                .fill(c.line_strong)
                .a11y_hidden()
                .child(
                    el().w_frac(played)
                        .h(t::SPACE_4)
                        .radius(t::SPACE_2)
                        .fill(c.accent),
                ),
        );
    let shown = format!(
        "{} / {}",
        clock(time),
        duration.map_or_else(|| "–:––".to_string(), length)
    );
    let muted = player.muted();
    let sound = {
        let id = id.clone();
        icon_button(
            format!("sound-{id}"),
            if muted {
                &icons::VOLUME_OFF
            } else {
                &icons::VOLUME
            },
            if muted { "Unmute" } else { "Mute" },
            move |s: &mut State| s.toggle_clip_sound(&id),
            look,
        )
    };
    let mut line = line
        .child(toggle)
        .child(position)
        .child(text(shown).font(ty.meta).color(c.ink_muted).a11y_hidden())
        .child(sound);
    if clip.is_video() {
        line = line.child(if full_screen {
            icon_button(
                "exit-full-screen",
                &icons::CLOSE,
                "Exit full screen",
                State::exit_full_screen,
                look,
            )
        } else {
            icon_button(
                format!("full-screen-{id}"),
                &icons::FULL_SCREEN,
                "Full screen",
                move |s: &mut State| s.enter_full_screen(&id),
                look,
            )
        });
    }
    line
}

/// A video started in a message, in its poster's place: the picture at the video's shape,
/// no wider than `max_width` nor taller than 400 (16:9 until its size is known), with its
/// controls under it. While it is fetched, and while it is full screen, its poster stands
/// there.
fn video_player(s: &State, clip: &Clip, max_width: f32, ty: &t::MessageType, look: Look) -> El {
    let c = look.c;
    let a = &clip.attachment;
    let id = &a.attachment_id;
    let (w, h) = if a.width > 0 && a.height > 0 {
        (a.width, a.height)
    } else {
        // A shape more than a size: the column's width, at 16:9.
        clip.player()
            .and_then(Player::natural_size)
            .unwrap_or((1600, 900))
    };
    let size = fitted(w, h, max_width, t::MEDIA_MAX);
    // Never narrower than its controls need.
    let width = size[0].max(t::MEDIA_CELL).min(max_width);
    let away = s.playback.full_screen.as_deref() == Some(id.as_str());
    let mut stage = el()
        .key(format!("video-{id}"))
        .w(width)
        .h(size[1])
        .center()
        .clip_rounded(0.0);
    match clip.player() {
        Some(player) if !away && clip.failure().is_none() => {
            stage = stage
                .video(player, SurfaceFit::Fit)
                .role("img")
                .label(format!("Video, {}", a.name));
        }
        _ => {
            let picture = shown_picture(s, a, false).or_else(|| s.media.placeholder(id));
            if let Some(picture) = picture {
                stage = stage.child(
                    el().absolute(0.0, 0.0)
                        .size(width, size[1])
                        .pixels_fit(picture.pixels(), SurfaceFit::Fit)
                        .a11y_hidden(),
                );
            }
        }
    }
    col()
        .key(format!("player-{id}"))
        .w(width)
        .radius(t::RADIUS_12)
        .clip_rounded(t::RADIUS_12)
        .fill(c.inset)
        .role("group")
        .label(format!("Video, {}", a.name))
        .child(stage)
        .child(
            el().w_full()
                .px(t::SPACE_6)
                .py(t::SPACE_2)
                .child(clip_controls(clip, ty, false, look)),
        )
}

/// The video in full screen: over the whole window, as the viewer is, its controls under it.
/// Escape or the button at the controls' end brings it back into its message.
pub fn full_screen_video(s: &State, look: Look) -> Option<El> {
    let id = s.playback.full_screen.as_deref()?;
    let clip = s.playback.clip(id)?;
    let player = clip.player()?;
    let c = look.c;
    let ty = super::timeline::type_of(s);
    let id = id.to_string();
    Some(
        col()
            .key("full-screen")
            .pin(Some(0.0), Some(0.0), Some(0.0), Some(0.0))
            .fill(c.bg)
            .role("dialog")
            .label(format!("Video, {}", clip.attachment.name))
            .modal(true, State::exit_full_screen)
            .on_key(|s: &mut State, key, _| s.full_screen_key(key))
            .no_window_drag()
            .child(
                el().key("full-screen-stage")
                    .w_full()
                    .grow(1.0)
                    .min_h(0.0)
                    .video(player, SurfaceFit::Fit)
                    .role("img")
                    .label(format!("Video, {}", clip.attachment.name))
                    .on_press(move |s: &mut State| s.toggle_clip(&id)),
            )
            .child(
                el().w_full()
                    .px(t::VIEWER_MARGIN)
                    .py(t::SPACE_8)
                    .fill(c.surface)
                    .border_sides([1.0, 0.0, 0.0, 0.0], c.line)
                    .child(clip_controls(clip, &ty, true, look)),
            ),
    )
}

/// While editing, an attachment's ×.
fn remove_button(attachment: &AttachmentSummary, look: Look) -> El {
    let id = attachment.attachment_id.clone();
    icon_button(
        format!("remove-{}", attachment.attachment_id),
        &icons::CLOSE,
        &format!("Remove {}", attachment.name),
        move |s: &mut State| s.remove_while_editing(&id),
        look,
    )
}

/// What a message carries, under its words: its pictures, then its other files. `width` is
/// what its row lays the words out at, when the list has been laid out. While the message is
/// edited each has a × (an edit can take an attachment out, not put one in).
pub fn attachments(
    s: &State,
    entry: &Entry,
    width: Option<f32>,
    ty: &t::MessageType,
    editing: bool,
    look: Look,
) -> Option<El> {
    let removed: &[String] = if editing { &s.media.edit_removed } else { &[] };
    let shown: Vec<&AttachmentSummary> =
        crate::session::media::kept_attachments(entry, removed).collect();
    if shown.is_empty() {
        return None;
    }
    let images = s.settings.images == Media::Shown;
    let as_picture = |a: &AttachmentSummary| {
        images
            && (is_picture(a) || (a.kind == "video" && !a.expired && a.thumbnail.is_some()))
            && !undrawable(s, a)
    };
    let (pictures, files): (Vec<&AttachmentSummary>, Vec<&AttachmentSummary>) =
        shown.iter().partition(|a| as_picture(a));
    // A video started here leaves the grid and plays at its own shape after it.
    let clip_of = |a: &AttachmentSummary| {
        (a.kind == "video" && !a.expired)
            .then(|| s.playback.clip(&a.attachment_id))
            .flatten()
    };
    let (players, pictures): (Vec<&AttachmentSummary>, Vec<&AttachmentSummary>) =
        pictures.into_iter().partition(|a| clip_of(a).is_some());
    let max_width = width.unwrap_or(t::MEDIA_MAX).min(t::MEDIA_MAX * 1.5);
    let mut column = col().key("attachments").w_full().gap(t::MEDIA_GAP);
    match pictures.as_slice() {
        [] => {}
        [one] => {
            let size = fitted(one.width, one.height, max_width, t::MEDIA_MAX);
            column = column.child(with_remove(
                picture_tile(s, one, size, false, look),
                one,
                editing,
                look,
            ));
        }
        many => {
            let (columns, side) = grid(many.len(), max_width, t::MEDIA_CELL, t::MEDIA_GAP);
            let mut rows = col().gap(t::MEDIA_GAP);
            for line in many.chunks(columns) {
                let mut cells = row().gap(t::MEDIA_GAP);
                for a in line {
                    cells = cells.child(with_remove(
                        picture_tile(s, a, [side, side], true, look),
                        a,
                        editing,
                        look,
                    ));
                }
                rows = rows.child(cells);
            }
            column = column.child(rows);
        }
    }
    for a in players {
        if let Some(clip) = clip_of(a) {
            column = column.child(with_remove(
                video_player(s, clip, max_width, ty, look),
                a,
                editing,
                look,
            ));
        }
    }
    for a in files {
        column = column.child(file_row(s, a, ty, editing, look));
        if let Some(clip) = clip_of(a) {
            column = column.child(video_player(s, clip, max_width, ty, look));
        }
    }
    Some(column)
}

/// A picture with its × at its corner while the message is edited.
fn with_remove(tile: El, attachment: &AttachmentSummary, editing: bool, look: Look) -> El {
    if !editing {
        return tile;
    }
    el().child(tile).child(
        el().absolute(t::SPACE_4, t::SPACE_4)
            .radius(t::RADIUS_9)
            .fill(look.c.raised)
            .child(remove_button(attachment, look)),
    )
}

/// A message's link cards, under its attachments; none when Preferences draws no cards. While
/// the message is edited each has a ×, which takes the card away and leaves the link.
pub fn cards(
    s: &State,
    entry: &Entry,
    width: Option<f32>,
    ty: &t::MessageType,
    editing: bool,
    look: Look,
) -> Option<El> {
    if s.settings.embeds != Media::Shown || entry.cards.is_empty() {
        return None;
    }
    let dropped: &[String] = if editing { &s.media.edit_dropped } else { &[] };
    let max_width = width.map_or(t::MEDIA_MAX, |w| w.min(t::MEDIA_MAX));
    let mut column = col().key("cards").w_full().gap(t::MEDIA_GAP);
    let mut any = false;
    for card in entry.cards.iter().filter(|c| !dropped.contains(&c.url)) {
        any = true;
        let mut drawn = link_card(s, card, max_width, ty, look);
        if editing {
            let url = card.url.clone();
            drawn = drawn.child(icon_button(
                format!("drop-card-{}", card.url),
                &icons::CLOSE,
                &format!("Remove the preview of {}", card.host),
                move |s: &mut State| s.drop_card_while_editing(&url),
                look,
            ));
        }
        column = column.child(drawn);
    }
    any.then_some(column)
}

/// One link's card: the host, the title and the description, with its picture whole under
/// them at the card's width when it is a banner (`CardSummary::picture_under`, the core's
/// rule for every client), and small beside them when it is squarer or small. `max_width`
/// is the room the message's column gives it.
fn link_card(s: &State, card: &CardSummary, max_width: f32, ty: &t::MessageType, look: Look) -> El {
    let c = look.c;
    let words = card_words(&card.host, &card.title, &card.description, ty, look);
    let url = card.url.clone();
    let picture = card_key(card)
        .and_then(|key| s.media.image(&key))
        .filter(|i| drawn(i));
    let under = picture.is_some() && card.picture_under();
    let frame = if under {
        col().gap(t::SPACE_10)
    } else {
        row().gap(t::SPACE_12)
    };
    let body = frame
        .key(format!("card-{}", card.url))
        .grow(1.0)
        .min_w(0.0)
        .gap(t::SPACE_12)
        .role("link")
        .label(format!("{}, {}", card.title, card.host))
        .cursor(Cursor::Pointer)
        .focusable(true)
        .focus_ring(2.0, 2.0, c.focus)
        .on_press_tx(move |s: &mut State, tx| s.open_card(&url, tx))
        .child(words);
    let Some(picture) = picture else {
        return card_box(body, look);
    };
    // A banner's card is as wide as cards go, whatever its words come to.
    let wide = max_width.min(t::MEDIA_MAX);
    let mut tile = if under {
        // At the card's width and the picture's own shape, no taller than the web's 240:
        // one taller than that at this width is cropped to it.
        let width = wide - 2.0 * t::SPACE_12;
        #[allow(clippy::cast_precision_loss)]
        let shape = card.image_width as f32 / card.image_height as f32;
        el().key(format!("card-picture-{}", card.url))
            .size(width, (width / shape).min(t::CARD_PICTURE_UNDER))
            .pixels_fit(picture.pixels(), SurfaceFit::Fill)
    } else {
        el().key(format!("card-picture-{}", card.url))
            .size(t::CARD_PICTURE, t::CARD_PICTURE)
            .child(
                el().absolute(0.0, 0.0)
                    .size(t::CARD_PICTURE, t::CARD_PICTURE)
                    .pixels_fit(picture.pixels(), SurfaceFit::Fill),
            )
    }
    .radius(t::RADIUS_8)
    .clip_rounded(t::RADIUS_8)
    .fill(c.inset)
    .center()
    .a11y_hidden();
    if card.video {
        tile = tile.child(play_disc(look));
    }
    let card = card_box(body.child(tile), look);
    if under { card.w(wide) } else { card }
}

/// A link card's host, title and description, as the conversation draws them.
pub(crate) fn card_words(
    host: &str,
    title: &str,
    description: &str,
    ty: &t::MessageType,
    look: Look,
) -> El {
    let c = look.c;
    let mut words = col().grow(1.0).min_w(0.0).gap(t::SPACE_2).child(
        text(host.to_string())
            .font(ty.meta)
            .color(c.ink_muted)
            .truncate(),
    );
    if !title.is_empty() {
        words = words.child(
            text(title.to_string())
                .font(ty.file)
                .color(c.link)
                .wrap()
                .max_lines(2),
        );
    }
    if !description.is_empty() {
        words = words.child(
            text(description.to_string())
                .font(ty.compact)
                .color(c.ink_secondary)
                .wrap()
                .max_lines(3),
        );
    }
    words
}

/// The box a link card stands in, around its words and picture in `body`.
pub(crate) fn card_box(body: El, look: Look) -> El {
    row()
        .max_w(t::MEDIA_MAX)
        .padding(t::SPACE_12)
        .gap(t::SPACE_6)
        .items_start()
        .radius(t::RADIUS_12)
        .fill(look.c.inset)
        .child(body)
}

/// A send on its way: the names of the files it carries, as quiet rows.
pub fn pending(names: &[String], ty: &t::MessageType, look: Look) -> Option<El> {
    if names.is_empty() {
        return None;
    }
    let c = look.c;
    let mut column = col().gap(t::MEDIA_GAP);
    for name in names {
        column = column.child(
            row()
                .max_w(t::MEDIA_MAX)
                .padding(t::SPACE_8)
                .gap(t::SPACE_10)
                .items_center()
                .radius(t::RADIUS_12)
                .fill(c.inset)
                .child(
                    el().size(t::FILE_TILE, t::FILE_TILE)
                        .radius(t::RADIUS_9)
                        .fill(c.file_pdf)
                        .center()
                        .child(icon(&icons::FILE, t::FILE_GLYPH).color(c.file_pdf_ink)),
                )
                .child(
                    text(name.clone())
                        .font(ty.file)
                        .color(c.ink)
                        .truncate()
                        .shrink(),
                ),
        );
    }
    Some(column)
}

// ---- The composer ---------------------------------------------------------------------

/// The composer's attach button, at the field's start: the design system's plus.
pub fn attach_button(look: Look) -> El {
    glyph_button(
        "attach",
        &icons::PLUS,
        "Attach a file",
        t::ICON_BUTTON,
        |s: &mut State, tx| s.choose_attachment(tx),
        look,
    )
}

/// The files waiting over the composer: each with its picture or kind, its name, its size or
/// where its upload is, an alt text field for a picture, and its ×.
pub fn strip(s: &State, look: Look) -> Option<El> {
    let waiting: Vec<&Waiting> = s.session.waiting_here().collect();
    if waiting.is_empty() {
        return None;
    }
    let mut column = col()
        .key("attachment-strip")
        .w_full()
        .padding(t::SPACE_8)
        .gap(t::SPACE_6)
        .radius(t::RADIUS_12)
        .fill(look.c.inset)
        .role("list")
        .label("Attachments");
    for w in waiting {
        column = column.child(waiting_row(s, w, look));
    }
    Some(column)
}

fn waiting_row(s: &State, w: &Waiting, look: Look) -> El {
    let c = look.c;
    let placeholder = w.draft().and_then(|d| s.media.placeholder(&d.draft_id));
    let mut tile = el()
        .size(t::GLYPH_TILE, t::GLYPH_TILE)
        .radius(t::RADIUS_9)
        .clip_rounded(t::RADIUS_9)
        .fill(c.file_pdf)
        .center()
        .a11y_hidden();
    tile = match placeholder {
        Some(image) => tile.child(
            el().absolute(0.0, 0.0)
                .size(t::GLYPH_TILE, t::GLYPH_TILE)
                .pixels_fit(image.pixels(), SurfaceFit::Fill),
        ),
        None => tile.child(icon(&icons::FILE, t::FILE_GLYPH).color(c.file_pdf_ink)),
    };
    let status = match &w.stage {
        Stage::Uploading => text("Uploading…")
            .font(t::TEXT_META)
            .color(c.ink_muted)
            .role("status"),
        Stage::Ready(_) => text(size_words(w.size()))
            .font(t::TEXT_META)
            .color(c.ink_muted),
        Stage::Failed(why) => text(why.clone())
            .font(t::TEXT_META)
            .color(c.dnd)
            .wrap()
            .role("alert"),
    };
    let words = col()
        .grow(1.0)
        .min_w(0.0)
        .gap(t::SPACE_2)
        .child(
            text(w.name.clone())
                .font(t::TEXT_MEMBER)
                .color(c.ink)
                .truncate(),
        )
        .child(status);
    let mut line = row()
        .key(format!("waiting-{}", w.local))
        .w_full()
        .gap(t::SPACE_10)
        .items_center()
        .role("listitem")
        .label(w.name.clone())
        .child(tile)
        .child(words);
    if w.is_picture() {
        let local = w.local;
        let empty = rune::TextEdit::new("");
        let field = s.media.alt(local).unwrap_or(&empty);
        line = line.child(
            row()
                .w(t::ALT_FIELD)
                .h(t::CONTROL_SMALL)
                .px(t::SPACE_10)
                .items_center()
                .radius(t::RADIUS_9)
                .fill(c.field)
                .border(1.0, c.border_form)
                .focus_within_ring(1.0, -1.0, c.border_focus)
                .child(
                    text_input(field, t::TEXT_META)
                        .key(format!("alt-{local}"))
                        .single_line()
                        .label(format!("Alt text for {}", w.name))
                        .placeholder("Describe this picture", c.ink_muted)
                        .color(c.ink_field)
                        .caret(c.accent, c.accent_soft)
                        .w_full()
                        .on_edit(move |s: &mut State| s.alt_field(local))
                        .on_input_change(move |s: &mut State, _| s.alt_changed(local)),
                ),
        );
    }
    let local = w.local;
    line.child(icon_button(
        format!("remove-waiting-{local}"),
        &icons::CLOSE,
        &format!("Remove {}", w.name),
        move |s: &mut State| s.remove_waiting(local),
        look,
    ))
}

/// The cards previewed over the composer, each with its ×. A link whose page gave none is
/// not drawn; one still being asked is its host and "Loading preview…".
pub fn previews(s: &State, look: Look) -> Option<El> {
    let c = look.c;
    let mut column = col().key("card-previews").w_full().gap(t::SPACE_6);
    let mut any = false;
    for preview in s.session.previews_here() {
        let (host, title) = match (&preview.card, preview.loading) {
            (Some(card), _) => (card.host.clone(), card.title.clone()),
            (None, true) => (host_of(&preview.url), "Loading preview…".to_string()),
            (None, false) => continue,
        };
        any = true;
        let url = preview.url.clone();
        let picture = preview
            .card
            .as_ref()
            .and_then(card_key)
            .and_then(|key| s.media.image(&key))
            .filter(|i| drawn(i));
        let mut line = row()
            .key(format!("preview-{}", preview.url))
            .w_full()
            .padding(t::SPACE_8)
            .pl(t::SPACE_14)
            .gap(t::SPACE_10)
            .items_center()
            .radius(t::RADIUS_12)
            .fill(c.inset)
            .child(
                col()
                    .grow(1.0)
                    .min_w(0.0)
                    .gap(t::SPACE_2)
                    .child(
                        text(host.clone())
                            .font(t::TEXT_META)
                            .color(c.ink_muted)
                            .truncate(),
                    )
                    .child(text(title).font(t::TEXT_MEMBER).color(c.ink).truncate()),
            );
        if let Some(picture) = picture {
            line = line.child(
                el().size(t::GLYPH_TILE, t::GLYPH_TILE)
                    .radius(t::RADIUS_8)
                    .clip_rounded(t::RADIUS_8)
                    .a11y_hidden()
                    .pixels_fit(picture.pixels(), SurfaceFit::Fill),
            );
        }
        column = column.child(line.child(icon_button(
            format!("dismiss-preview-{}", preview.url),
            &icons::CLOSE,
            &format!("Remove the preview of {host}"),
            move |s: &mut State| s.session.dismiss_preview(&url),
            look,
        )));
    }
    any.then_some(column)
}

/// The host of a link, for a preview still being asked.
fn host_of(url: &str) -> String {
    let rest = url.split_once("://").map_or(url, |(_, rest)| rest);
    let host = rest.split(['/', '?', '#']).next().unwrap_or_default();
    host.trim_start_matches("www.").to_lowercase()
}

/// What stands over the conversation while files are dragged over it.
pub fn drop_target(look: Look) -> El {
    let c = look.c;
    el().key("drop-target")
        .pin(
            Some(t::SPACE_8),
            Some(t::SPACE_8),
            Some(t::SPACE_8),
            Some(t::SPACE_8),
        )
        .radius(t::RADIUS_16)
        .fill(c.accent_wash)
        .border(2.0, c.accent_line)
        .center()
        .role("status")
        .child(
            text("Drop to attach")
                .font(t::TEXT_SECTION_STRONG)
                .color(c.accent_ink),
        )
}

// ---- The viewer ---------------------------------------------------------------------

/// The picture opened from a message, over the whole window: the display picture (its
/// thumbnail, then its placeholder, until it has arrived), fitted to the window and never
/// drawn larger than its own size, its alt text under it. It zooms as specs/media.md 5 says
/// (`lumia_core::viewer`), up to four times its own size: about the pointer with the wheel or
/// a trackpad's pinch, to its own size and back with a press on it, and by the bar's buttons
/// or the keys (+ and − a step, 0 fitted, 1 its own size). A drag moves it while it is
/// zoomed. Escape, the × or a press beside the picture closes it.
pub fn viewer(s: &State, look: Look) -> Option<El> {
    let v = s.media.viewer.as_ref()?;
    let c = look.c;
    let a = &v.attachment;
    let main_key = blob_key(&a.main);
    let undrawable = s.session.media.undrawable(&main_key);
    let picture = s
        .media
        .image(&main_key)
        .filter(|i| drawn(i))
        .or_else(|| {
            a.thumbnail
                .as_ref()
                .and_then(|b| s.media.image(&blob_key(b)))
                .filter(|i| drawn(i))
        })
        .or_else(|| s.media.placeholder(&a.attachment_id));
    let a_save = a.clone();
    // The room the picture is fitted into, as laid out; the window's until then.
    let room = rune::memo::layout_with("viewer-stage", |rect| [rect[2].round(), rect[3].round()]);
    let frame = v.frame(room.unwrap_or([t::MEDIA_MAX * 2.0, t::MEDIA_MAX * 1.5]));
    // As the stage is now: a window made smaller holds a zoomed picture further in.
    let view = viewer::held(v.view, &frame);
    let bar = row()
        .key("viewer-bar")
        .w_full()
        .h(t::VIEWER_BAR)
        .px(t::VIEWER_MARGIN)
        .gap(t::SPACE_8)
        .items_center()
        .fill(c.surface)
        .border_sides([0.0, 0.0, 1.0, 0.0], c.line)
        .child(
            text(a.name.clone())
                .font(t::TEXT_MEMBER)
                .color(c.ink)
                .truncate()
                .grow(1.0)
                .min_w(0.0)
                .role("heading"),
        )
        .child(zoom_button(
            "viewer-zoom-out",
            "−",
            "Zoom out",
            view.scale > 1.0,
            move |s: &mut State| s.zoom_viewer(-1, &frame),
            look,
        ))
        .child(zoom_button(
            "viewer-fit",
            "Fit",
            "Fit to window",
            view.scale > 1.0,
            move |s: &mut State| s.zoom_viewer(0, &frame),
            look,
        ))
        .child(zoom_button(
            "viewer-own-size",
            "1:1",
            "Actual size",
            (view.scale - frame.own).abs() > f32::EPSILON,
            move |s: &mut State| s.viewer_own_size(&frame),
            look,
        ))
        .child(zoom_button(
            "viewer-zoom-in",
            "+",
            "Zoom in",
            view.scale < frame.max_scale(),
            move |s: &mut State| s.zoom_viewer(1, &frame),
            look,
        ))
        .child(glyph_button(
            "viewer-save",
            &icons::DOWNLOAD,
            &format!("Save {}", a.name),
            t::ICON_BUTTON,
            move |s: &mut State, tx| s.save_attachment(&a_save, tx),
            look,
        ))
        .child(icon_button(
            "viewer-close",
            &icons::CLOSE,
            "Close",
            State::close_viewer,
            look,
        ));
    let size = [frame.width * view.scale, frame.height * view.scale];
    let mut stage = el()
        .key("viewer-stage")
        .w_full()
        .grow(1.0)
        .min_h(0.0)
        .center()
        .clip_rounded(0.0)
        .on_background_press(State::close_viewer)
        .on_wheel(move |s: &mut State, wheel: WheelEvent, _: &mut Tx| {
            s.wheel_viewer(&wheel, &frame);
            true
        })
        .on_magnify(move |s: &mut State, pinch: MagnifyEvent, _: &mut Tx| {
            s.pinch_viewer(&pinch, &frame);
            true
        });
    if undrawable {
        stage = stage.child(
            text("This picture can’t be shown here. Save it to open it in another app.")
                .font(t::TEXT_BODY)
                .color(c.ink_body)
                .role("status"),
        );
    } else if let Some(picture) = picture {
        // Centred in the room, and moved from there by the view. It follows the hand: no
        // glide between one wheel event's size and the next.
        let shown = el()
            .key("viewer-picture")
            .size(size[0], size[1])
            .offset(view.x, view.y)
            .pixels_fit(picture.pixels(), SurfaceFit::Fit)
            .role("img")
            .label(picture_label(a))
            .instant_changes()
            .cursor(if view.scale > 1.0 {
                Cursor::Grab
            } else {
                Cursor::ZoomIn
            })
            .on_drag(move |s: &mut State, drag, _| s.drag_viewer(drag, &frame));
        stage = stage.child(shown);
    }
    let mut frame = col()
        .key("viewer")
        .pin(Some(0.0), Some(0.0), Some(0.0), Some(0.0))
        .fill(c.bg)
        .role("dialog")
        .label(format!("Picture, {}", a.name))
        .modal(true, State::close_viewer)
        .on_key(move |s: &mut State, key, _| s.viewer_key(key, &frame))
        .no_window_drag()
        .child(bar)
        .child(stage);
    if !a.alt.trim().is_empty() {
        frame = frame.child(
            el().key("viewer-alt")
                .w_full()
                .px(t::VIEWER_MARGIN)
                .py(t::SPACE_14)
                .fill(c.surface)
                .border_sides([1.0, 0.0, 0.0, 0.0], c.line)
                .child(
                    text(a.alt.trim().to_string())
                        .font(t::TEXT_BODY)
                        .color(c.ink_body)
                        .wrap()
                        .selectable(true),
                ),
        );
    }
    Some(frame)
}

/// One of the viewer's zoom buttons: a word in the quiet ink, the design system's small link.
fn zoom_button(
    key: &str,
    glyph: &str,
    label: &str,
    enabled: bool,
    on_press: impl Fn(&mut State) + 'static,
    look: Look,
) -> El {
    let c = look.c;
    let mut button = el()
        .key(key)
        .h(t::ICON_BUTTON)
        .min_w(t::ICON_BUTTON)
        .px(t::SPACE_8)
        .radius(t::RADIUS_9)
        .center()
        .role("button")
        .label(label)
        .help(label)
        .disabled(!enabled)
        .child(text(glyph).font(t::TEXT_LABEL).color(c.ink_quiet));
    if enabled {
        button = button
            .hover(c.hover_icon)
            .cursor(Cursor::Pointer)
            .focusable(true)
            .focus_ring(2.0, 0.0, c.focus)
            .on_press(on_press);
    } else {
        button = button.opacity(0.5);
    }
    button
}

#[cfg(test)]
mod tests {
    use super::host_of;

    #[test]
    fn a_preview_still_asked_is_named_by_its_host() {
        assert_eq!(host_of("https://www.Example.com/a?b#c"), "example.com");
        assert_eq!(host_of("http://lumia.chat"), "lumia.chat");
    }
}
