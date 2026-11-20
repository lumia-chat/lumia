//! Server settings' Emoji section (specs/emoji.md 10, the "Emoji settings" board): the count
//! against the server's limit beside the heading, what an emoji is (or, at the limit, that
//! the server has as many as it may), Upload emoji (off at the limit), the upload form over
//! the list once a picture is chosen, and the list, oldest first, each row the emoji's
//! picture, its `:name:`, who uploaded it and whether it is animated, Rename (the row becomes
//! its field, in place) and Remove (asked first, against the button).
//!
//! The upload form's picture is the file as chosen, decoded here to be looked at; what the
//! instance keeps is the picture it makes of it (spec 3).

use lumia_client::app::{EmojiSummary, ServerSummary};
use rune::{Align, Cursor, El, Side, TextEdit, Tx, col, el, row, text, text_input};

use super::components::{
    Look, danger_action, error_text, shadowed, small_button_with, text_button,
};
use super::emoji::picture;
use crate::app::State;
use crate::app::emoji::{self as emoji, Upload};
use crate::tokens::{self as t, icons};

/// The section.
pub fn section(s: &State, server: &ServerSummary, look: Look) -> El {
    let c = look.c;
    let full = emoji::at_limit(server);
    let heading = row()
        .gap(t::SPACE_8)
        .items_center()
        .child(
            text("Emoji")
                .font(t::read(t::TITLE_SERVER))
                .color(c.ink)
                .role("heading"),
        )
        .child(
            text(emoji::count_words(server))
                .font(t::TEXT_LINK_SM)
                .color(c.ink_muted)
                .label(format!("{} emoji", emoji::count_words(server))),
        );
    let about = if full {
        emoji::full_words(&server.name, Some(server.emoji_limit))
    } else {
        emoji::about_words(&server.name)
    };
    let head = row()
        .w_full()
        .gap(t::SPACE_16)
        .items_start()
        .child(
            col()
                .grow(1.0)
                .min_w(0.0)
                .gap(t::SPACE_4)
                .child(heading)
                .child(
                    text(about)
                        .key("emoji-about")
                        .font(t::TEXT_CARD_LINE)
                        .color(c.ink_muted)
                        .wrap()
                        .w_full(),
                ),
        )
        .child(small_button_with(
            "upload-emoji",
            &icons::PLUS,
            "Upload emoji",
            !full && !s.emoji.upload.as_ref().is_some_and(|u| u.busy),
            |s: &mut State, tx: &mut Tx| s.choose_emoji_file(tx),
            look,
        ));
    let mut page = col()
        .key("emoji-section")
        .w_full()
        .max_w(t::SETTINGS_PAGE)
        .gap(t::SPACE_16)
        .child(head);
    if !s.emoji.error.is_empty() {
        page = page.child(error_text(&s.emoji.error, look));
    }
    if let Some(upload) = &s.emoji.upload {
        page = page.child(upload_form(upload, look));
    }
    if server.emoji.is_empty() {
        return page;
    }
    let mut list = col()
        .key("emoji-list")
        .w_full()
        .radius(t::RADIUS_14)
        .border(1.0, c.line_strong)
        .role("list")
        .label("Emoji");
    let last = server.emoji.len() - 1;
    for (i, e) in server.emoji.iter().enumerate() {
        let mut item = emoji_row(s, e, look);
        if i > 0 {
            item = item.border_sides([1.0, 0.0, 0.0, 0.0], c.line);
        }
        // The renamed row's wash keeps to the list's corners.
        let (top, bottom) = (
            if i == 0 { t::RADIUS_14 } else { 0.0 },
            if i == last { t::RADIUS_14 } else { 0.0 },
        );
        list = list.child(item.corners([top, top, bottom, bottom]));
    }
    page.child(list)
}

/// One emoji in the list, or its field while it is renamed.
fn emoji_row(s: &State, e: &EmojiSummary, look: Look) -> El {
    let c = look.c;
    let renaming = s
        .emoji
        .renaming
        .as_ref()
        .filter(|r| r.emoji_id == e.emoji_id);
    let item = row()
        .key(format!("emoji-{}", e.emoji_id))
        .w_full()
        .min_h(t::EMOJI_ROW)
        .gap(t::SPACE_12)
        .role("listitem")
        .label(format!(":{}:", e.name));
    let pic = picture(s, e, t::EMOJI_ROW_PICTURE, look);
    let Some(form) = renaming else {
        let by = s.session.name_of(&e.created_by_account_id);
        let by = if by.is_empty() { "someone".into() } else { by };
        let sub = if e.animated {
            format!("Uploaded by {by} · Animated")
        } else {
            format!("Uploaded by {by}")
        };
        let (rename_id, remove_id) = (e.emoji_id.clone(), e.emoji_id.clone());
        let remove_key = format!("remove-emoji-{}", e.emoji_id);
        let anchor = remove_key.clone();
        return item
            .items_center()
            .pl(t::SPACE_14)
            .pr(t::SPACE_10)
            .py(t::SPACE_8)
            .child(pic)
            .child(
                col()
                    .grow(1.0)
                    .min_w(0.0)
                    .gap(1.0)
                    .child(
                        text(format!(":{}:", e.name))
                            .font(t::TEXT_EMOJI_NAME)
                            .color(c.ink)
                            .truncate(),
                    )
                    .child(text(sub).font(t::TEXT_META).color(c.ink_muted).truncate()),
            )
            .child(
                row()
                    .gap(t::SPACE_6)
                    .items_center()
                    .child(
                        row_button(
                            format!("rename-emoji-{}", e.emoji_id),
                            "Rename",
                            false,
                            look,
                        )
                        .label(format!("Rename :{}:", e.name))
                        .on_press_tx(move |s: &mut State, tx| {
                            s.begin_rename(&rename_id);
                            tx.focus(format!("emoji-rename-{rename_id}"), false);
                        }),
                    )
                    .child(
                        row_button(remove_key, "Remove", true, look)
                            .label(format!("Remove :{}:", e.name))
                            .on_press(move |s: &mut State| s.ask_remove_emoji(&remove_id, &anchor)),
                    ),
            );
    };
    let mut form_col = col()
        .grow(1.0)
        .min_w(0.0)
        .gap(t::SPACE_6)
        .child(
            text(format!("New name for :{}:", form.was))
                .font(t::read(t::TEXT_LABEL))
                .color(c.ink_body)
                .a11y_hidden(),
        )
        .child(
            row()
                .w_full()
                .gap(t::SPACE_12)
                .items_center()
                .child(
                    name_field(
                        format!("emoji-rename-{}", e.emoji_id),
                        &format!("New name for :{}:", form.was),
                        &form.name,
                        form.busy,
                        State::rename_field,
                        |s: &mut State, _: &mut Tx| s.save_rename(),
                        look,
                    )
                    .grow(1.0)
                    .min_w(0.0),
                )
                .child(
                    row()
                        .gap(t::SPACE_6)
                        .child(cancel(
                            "rename-cancel",
                            !form.busy,
                            t::EMOJI_ACTION,
                            State::cancel_rename,
                            look,
                        ))
                        .child(small_primary(
                            "rename-save",
                            "Save",
                            !form.busy && !form.name.text.trim().is_empty(),
                            t::EMOJI_ACTION,
                            State::save_rename,
                            look,
                        )),
                ),
        );
    if !form.error.is_empty() {
        form_col = form_col.child(error_text(&form.error, look).role("alert"));
    }
    item.items_start()
        .pl(t::SPACE_14)
        .pr(t::SPACE_10)
        .py(t::SPACE_12)
        .fill(c.inset)
        .child(el().mt(t::SPACE_24 + t::SPACE_6).child(pic))
        .child(form_col)
}

/// A row's Rename or Remove (32 tall): outlined, Remove in the danger ink as Kick is.
fn row_button(key: String, words: &str, danger: bool, look: Look) -> El {
    outline(key, words, danger, t::EMOJI_ACTION, look)
}

/// [`row_button`] at `height`: the upload form's Cancel is a field's height (40).
fn outline(key: String, words: &str, danger: bool, height: f32, look: Look) -> El {
    let c = look.c;
    let (edge, ink, hover) = if danger {
        (c.danger_line, c.danger, c.danger_soft)
    } else {
        (c.border, c.ink_body, c.hover_icon)
    };
    row()
        .key(key)
        .h(height)
        .px(t::SPACE_12)
        .center()
        .radius(t::RADIUS_10)
        .fill(c.surface)
        .border(1.0, edge)
        .role("button")
        .hover(hover)
        .cursor(Cursor::Pointer)
        .focusable(true)
        .focus_ring(2.0, 2.0, c.focus)
        .child(text(words.to_string()).font(t::TEXT_LABEL).color(ink))
}

/// Cancel beside a form's Save, at its height.
fn cancel(
    key: &'static str,
    enabled: bool,
    height: f32,
    press: impl Fn(&mut State) + 'static,
    look: Look,
) -> El {
    let button = outline(key.into(), "Cancel", false, height, look).label("Cancel");
    if enabled {
        button.on_press(press)
    } else {
        button.disabled(true).opacity(0.5)
    }
}

/// A primary button at `height`: Save in a row's rename (32) and in the upload form (40).
fn small_primary(
    key: &'static str,
    label: &str,
    enabled: bool,
    height: f32,
    press: impl Fn(&mut State) + 'static,
    look: Look,
) -> El {
    let c = look.c;
    let button = row()
        .key(key)
        .h(height)
        .px(t::SPACE_14)
        .center()
        .radius(t::RADIUS_10)
        .fill(c.accent)
        .role("button")
        .label(label)
        .disabled(!enabled)
        .child(text(label).font(t::TEXT_LABEL).color(c.on_accent));
    if enabled {
        button
            .hover(c.accent_hover)
            .cursor(Cursor::Pointer)
            .focusable(true)
            .focus_ring(2.0, 2.0, c.focus)
            .on_press(press)
    } else {
        button.opacity(0.5)
    }
}

/// A name typed between colons: `:` before it and after it, as it is written in a message.
/// Past 32 characters nothing more is kept, as the web's field takes no more.
fn name_field(
    key: String,
    label: &str,
    edit: &TextEdit,
    busy: bool,
    access: fn(&mut State) -> &mut TextEdit,
    submit: impl Fn(&mut State, &mut Tx) + 'static,
    look: Look,
) -> El {
    let c = look.c;
    let colon = || {
        text(":")
            .font(t::TEXT_BODY)
            .color(c.ink_muted)
            .a11y_hidden()
    };
    let input = text_input(edit, t::TEXT_BODY)
        .key(key)
        .single_line()
        .label(label)
        .color(c.ink_field)
        .caret(c.accent, c.accent_soft)
        .disabled(busy)
        .on_edit(access)
        .on_input_change(move |s: &mut State, _: &mut Tx| {
            crate::app::role_settings::keep_to_label(access(s));
        })
        .on_submit(submit);
    let mut frame = row()
        .w_full()
        .h(t::EMOJI_FIELD)
        .px(t::SPACE_14)
        .gap(1.0)
        .items_center()
        .radius(t::RADIUS_12)
        .fill(c.field)
        .border(1.0, c.border_form)
        .focus_within_ring(1.0, -1.0, c.border_focus)
        .child(colon())
        .child(input.min_w(t::SPACE_24))
        .child(colon())
        .child(el().grow(1.0));
    if let Some(glow) = look.shadows.ring_field.first() {
        frame = frame.focus_within_ring(glow.spread, 0.0, glow.color);
    }
    frame
}

/// Upload emoji's form, once a picture is chosen: the picture as chosen, its file's name and
/// size, "Choose another…", the name to change, Cancel and Save.
fn upload_form(upload: &Upload, look: Look) -> El {
    let c = look.c;
    let mut frame = el()
        .size(t::EMOJI_UPLOAD_FRAME, t::EMOJI_UPLOAD_FRAME)
        .radius(t::RADIUS_12)
        .fill(c.inset)
        .center();
    if !upload.picture_error.is_empty() {
        frame = frame.border(1.0, c.danger_line);
    }
    if let Some(image) = &upload.picture {
        frame = frame.child(
            el().size(t::EMOJI_UPLOAD_PICTURE, t::EMOJI_UPLOAD_PICTURE)
                .pixels_fit(image.pixels(), rune::SurfaceFit::Fit)
                .a11y_hidden(),
        );
    }
    let file = col()
        .min_w(0.0)
        .gap(t::SPACE_2)
        .child(
            text(upload.file.clone())
                .font(t::TEXT_LABEL)
                .color(c.ink)
                .truncate(),
        )
        .child(
            text(State::upload_size(upload))
                .font(t::TEXT_META)
                .color(c.ink_muted),
        )
        .child(
            el().mt(t::SPACE_4).child(
                text_button(
                    "upload-choose",
                    "Choose another…",
                    "Choose another picture",
                    |_: &mut State| {},
                    look,
                )
                .on_press_tx(|s: &mut State, tx| s.choose_emoji_file(tx)),
            ),
        );
    let mut form = col()
        .key("emoji-upload")
        .w_full()
        .padding(t::SPACE_16)
        .gap(t::SPACE_14)
        .radius(t::RADIUS_14)
        .fill(c.surface)
        .border(1.0, c.line_strong)
        .role("group")
        .label("Upload emoji")
        .child(
            row()
                .gap(t::SPACE_14)
                .items_center()
                .child(frame)
                .child(file),
        );
    if !upload.picture_error.is_empty() {
        form = form.child(error_text(&upload.picture_error, look).role("alert"));
    }
    let typed = upload.name.text.trim();
    let under = if upload.name_error.is_empty() {
        text(format!(
            "2 to 32 letters, digits and _. It is typed as :{}: in a message.",
            if typed.is_empty() { "name" } else { typed }
        ))
        .font(t::TEXT_META)
        .color(c.ink_muted)
        .wrap()
        .w_full()
    } else {
        error_text(&upload.name_error, look).role("alert")
    };
    let name = col()
        .w_full()
        .gap(t::SPACE_6)
        .child(
            text("Name")
                .font(t::TEXT_LABEL)
                .color(c.ink_body)
                .a11y_hidden(),
        )
        .child(name_field(
            "emoji-name".into(),
            "Name",
            &upload.name,
            upload.busy,
            State::upload_field,
            |s: &mut State, _: &mut Tx| s.save_upload(),
            look,
        ))
        .child(under);
    let ready = !upload.busy && !typed.is_empty() && !upload.bytes.is_empty();
    form.child(name).child(
        row()
            .w_full()
            .gap(t::SPACE_8)
            .justify_end()
            .child(cancel(
                "upload-cancel",
                !upload.busy,
                t::EMOJI_FIELD,
                State::cancel_upload,
                look,
            ))
            .child(small_primary(
                "upload-save",
                "Save",
                ready,
                t::EMOJI_FIELD,
                State::save_upload,
                look,
            )),
    )
}

/// "Remove :name:?", against the row's Remove.
pub fn remove_question(s: &State, look: Look) -> Option<El> {
    let c = look.c;
    let removing = s.emoji.removing.as_ref()?;
    let title = emoji::remove_title(&removing.name);
    let mut surface = shadowed(
        col()
            .key("remove-emoji-question")
            .w(t::ASK_WIDTH)
            .padding(t::SPACE_18)
            .gap(t::SPACE_14)
            .radius(t::RADIUS_18)
            .fill(c.surface)
            .border(1.0, c.line_strong),
        look.shadows.floating,
    )
    .anchored_fit(
        removing.anchor.clone(),
        Side::Bottom,
        Align::End,
        t::SPACE_6,
    )
    .modal(true, State::close_popover)
    .role("dialog")
    .label(title.clone())
    .child(
        text(title)
            .font(t::TITLE_SERVER)
            .color(c.ink)
            .wrap()
            .role("heading"),
    )
    .child(
        text(emoji::REMOVE_LINE)
            .font(t::TEXT_CARD_LINE)
            .color(c.ink_secondary)
            .wrap()
            .w_full(),
    );
    if !removing.error.is_empty() {
        surface = surface.child(error_text(&removing.error, look));
    }
    Some(
        surface.child(
            row()
                .w_full()
                .gap(t::SPACE_8)
                .justify_end()
                .child(super::components::secondary_button(
                    "remove-emoji-cancel",
                    "Cancel",
                    true,
                    State::close_popover,
                    look,
                ))
                .child(danger_action(
                    "remove-emoji-confirm",
                    "Remove",
                    !removing.busy,
                    State::confirm_remove_emoji,
                    look,
                )),
        ),
    )
}
