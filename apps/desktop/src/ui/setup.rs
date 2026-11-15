//! Setting up private messages on this computer (architecture 5.4): the recovery phrase and
//! its check (identity spec 4.1 step 7), restoring from the phrase (4.3) and starting over
//! (4.7), each a card on the app ground as linking is; and what Home and Devices show while
//! this computer is not set up ([`needs_setup`]), the reminder in Home's sidebar
//! ([`reminder`]) and the card on Devices ([`phrase_card`]). The content and order of the
//! design canvas's "Setting up private messages (#20)" boards, drawn for the phone, in the
//! Quiet Split's cards, with "computer" where they say "phone".
//!
//! The phrase has no copy button, and its words are not selectable: clipboards sync
//! between devices and other apps read them. Saving it as a file asks first.
//!
//! Not here: Print, since Rune has no print dialog; showing the phrase only after this
//! computer's own unlock, since the shell has no way to ask for one (`app::setup`); and the
//! board's "Also be findable on Lumia", which the core has not built.

use rune::{Cursor, El, Tx, col, el, icon, row, text, text_input};

use super::components::{
    Look, action_button, card, danger_button, empty_state, error_text, field, link_button, notice,
    primary_button, primary_button_tx, secondary_button, small_button, wide_button,
};
use super::link::screen;
use super::sign_in::foot_link;
use crate::app::State;
use crate::app::setup::PHRASE_WORDS;
use crate::session::identity::Setup;
use crate::tokens::{self as t, icons};

/// How many completions a word of the check offers.
const COMPLETIONS: usize = 4;

/// A block in the amber of a caution: the key glyph and the words.
fn caution(words: &str, look: Look) -> El {
    let tone = t::caution(look.c);
    row()
        .w_full()
        .px(t::SPACE_14)
        .py(t::SPACE_12)
        .gap(t::SPACE_10)
        .radius(t::RADIUS_12)
        .fill(tone.fill)
        .border(1.0, tone.edge)
        .child(icon(&icons::KEY, t::SPACE_16).color(tone.glyph))
        .child(
            text(words)
                .font(t::TEXT_COMPACT)
                .color(tone.ink)
                .wrap()
                .grow(1.0)
                .min_w(0.0),
        )
}

fn quiet(words: &str, look: Look) -> El {
    text(words)
        .font(t::TEXT_META)
        .color(look.c.ink_secondary)
        .wrap()
        .w_full()
}

fn body(words: &str, look: Look) -> El {
    text(words)
        .font(t::TEXT_BODY)
        .color(look.c.ink_body)
        .wrap()
        .w_full()
}

/// The way back under a card.
fn foot(key: &'static str, label: &str, press: impl Fn(&mut State) + 'static, look: Look) -> El {
    row()
        .no_window_drag()
        .child(foot_link(key, label, press, look))
}

/// A card with the way back under it, on the app ground.
fn card_screen(key: &'static str, card: El, under: El, look: Look) -> El {
    screen(
        key,
        col()
            .items_center()
            .gap(t::SPACE_24)
            .child(card.no_window_drag())
            .child(under),
        look,
    )
}

// ---- The phrase -------------------------------------------------------------------------

pub fn phrase(s: &State, look: Look) -> El {
    let form = &s.setup.phrase;
    let card = if form.checking {
        check_card(s, look)
    } else {
        words_card(s, look)
    };
    card_screen(
        "phrase",
        card,
        foot("phrase-later", "Not now", State::close_phrase, look),
        look,
    )
}

/// Step 1: the 24 words, numbered, in two columns.
fn words_card(s: &State, look: Look) -> El {
    let form = &s.setup.phrase;
    let mut card = card("Recovery phrase", "Step 1 of 2", look)
        .key("phrase-card")
        .child(caution(
            "Whoever has these 24 words has your identity and your private messages. Lumia can’t recover them for you.",
            look,
        ));
    if form.again {
        card = card.child(
            notice(
                &icons::KEY,
                "One of those words wasn’t right. Here is the phrase again; the check will ask for other words.",
                look,
            )
            .key("phrase-again-note"),
        );
    }
    if form.words.is_empty() {
        if form.loading {
            card = card.child(quiet("Reading the phrase this computer keeps…", look));
        }
    } else {
        card = card.child(grid(&form.words, look)).child(quiet(
            "Write them down in order, and keep them somewhere safe and offline. There’s no copy button: clipboards sync to other devices, and other apps can read them.",
            look,
        ));
    }
    if let Some(path) = &form.saved_to {
        card =
            card.child(notice(&icons::FILE, format!("Saved to {path}."), look).key("phrase-saved"));
    }
    if !form.error.is_empty() {
        card = card.child(error_text(&form.error, look));
    }
    if form.confirming_save {
        return card.child(save_warning(look));
    }
    let ready = !form.words.is_empty();
    card.child(wide_button(
        "save-phrase",
        Some(&icons::DOWNLOAD),
        "Save as a file",
        ready,
        // The warning opens at the card's foot, which may be below the fold.
        |s: &mut State, tx: &mut Tx| {
            s.ask_save_phrase();
            tx.scroll_into_view("save-warning");
        },
        look,
    ))
    .child(primary_button(
        "phrase-written",
        "I’ve written them down",
        ready && !form.loading,
        State::phrase_written,
        look,
    ))
}

/// The words as the boards lay them out: 1 to 12 down the first column, 13 to 24 down the
/// second, each with its number.
fn grid(words: &[String], look: Look) -> El {
    let c = look.c;
    let half = words.len().div_ceil(2);
    let column = |from: usize, to: usize| {
        let mut column = col().grow(1.0).w(0.0).min_w(0.0).gap(t::SPACE_6);
        for (n, word) in words.iter().enumerate().take(to).skip(from) {
            let number = n + 1;
            column = column.child(
                row()
                    .key(format!("phrase-word-{number}"))
                    .w_full()
                    .h(t::PHRASE_CELL)
                    .px(t::SPACE_10)
                    .gap(t::SPACE_8)
                    .items_center()
                    .radius(t::RADIUS_10)
                    .fill(c.inset)
                    .role("listitem")
                    .label(format!("{number}, {word}"))
                    .child(
                        text(number.to_string())
                            .font(t::TEXT_META)
                            .color(c.ink_muted)
                            .align(rune::Align::End)
                            .w(t::PHRASE_NUMBER)
                            .a11y_hidden(),
                    )
                    .child(
                        text(word.clone())
                            .font(t::TEXT_PHRASE)
                            .color(c.ink)
                            .truncate()
                            .a11y_hidden(),
                    ),
            );
        }
        column
    };
    row()
        .key("phrase-words")
        .w_full()
        .gap(t::SPACE_8)
        .role("list")
        .label("Your recovery phrase")
        .child(column(0, half))
        .child(column(half, words.len()))
}

/// What "Save as a file" asks first: the file is the identity, and a synced folder puts a
/// copy in a cloud.
fn save_warning(look: Look) -> El {
    let c = look.c;
    col()
        .key("save-warning")
        .w_full()
        .gap(t::SPACE_12)
        .role("group")
        .label("Save your phrase as a file?")
        .child(
            text("Save your phrase as a file?")
                .font(t::TEXT_AUTHOR)
                .color(c.ink)
                .role("heading"),
        )
        .child(body(
            "This file is your identity: anyone who opens it can read your private messages.",
            look,
        ))
        .child(caution(
            "A folder that syncs to iCloud, Google Drive or OneDrive puts a copy in that cloud. Paper, or a drive you keep offline, is safer.",
            look,
        ))
        .child(
            col()
                .w_full()
                .gap(t::SPACE_8)
                .child(primary_button_tx(
                    "save-anyway",
                    "Save anyway",
                    true,
                    State::save_phrase,
                    look,
                ))
                .child(wide_button(
                    "write-instead",
                    None,
                    "Write it down instead",
                    true,
                    |s, _| s.keep_writing(),
                    look,
                )),
        )
}

fn confirm(s: &mut State, _: &mut Tx) {
    s.confirm_phrase();
}

/// Step 2: three of the words, each typed with the wordlist completing it.
fn check_card(s: &State, look: Look) -> El {
    let form = &s.setup.phrase;
    let mut card = card("Check your phrase", "Step 2 of 2", look)
        .key("phrase-check-card")
        .child(body(
            "Type these three words from what you wrote down, to make sure it’s right.",
            look,
        ));
    if form.loading {
        card = card.child(quiet("Choosing the words…", look));
    }
    for (i, (position, answer)) in form.quiz.iter().zip(&form.answers).enumerate() {
        card = card.child(field(
            format!("phrase-answer-{i}"),
            &format!("Word {position}"),
            answer,
            "",
            move |s: &mut State| s.setup.phrase.answer_mut(i),
            confirm,
            look,
        ));
        if let Some(list) = completions(i, &answer.text, look) {
            card = card.child(list);
        }
    }
    if !form.error.is_empty() {
        card = card.child(error_text(&form.error, look));
    }
    let filled = !form.answers.is_empty() && form.answers.iter().all(|a| !a.text.trim().is_empty());
    card.child(
        col()
            .w_full()
            .gap(t::SPACE_8)
            .child(primary_button(
                "phrase-confirm",
                "Confirm",
                filled && !form.busy,
                State::confirm_phrase,
                look,
            ))
            .child(wide_button(
                "phrase-show-again",
                None,
                "Show the phrase again",
                !form.busy,
                |s, _| s.show_phrase_again(),
                look,
            )),
    )
}

/// The wordlist's words that begin with what a field holds, under it, while it holds the
/// start of one and not a whole word.
fn completions(field: usize, typed: &str, look: Look) -> Option<El> {
    let c = look.c;
    let typed = typed.trim().to_lowercase();
    let words = lumia_core::identity::phrase::completions(&typed, COMPLETIONS);
    if words.is_empty() || words.contains(&typed.as_str()) {
        return None;
    }
    let mut list = col()
        .key(format!("completions-{field}"))
        .w_full()
        .padding(t::SPACE_4)
        .radius(t::RADIUS_12)
        .fill(c.surface)
        .border(1.0, c.border_form)
        .role("list")
        .label("Words that fit");
    for word in words {
        list = list.child(
            el().w_full().role("listitem").child(
                row()
                    .key(format!("completion-{field}-{word}"))
                    .w_full()
                    .h(t::ROW_HEIGHT)
                    .px(t::SPACE_10)
                    .items_center()
                    .radius(t::RADIUS_8)
                    .hover(c.hover_member)
                    .role("button")
                    .label(word)
                    .cursor(Cursor::Pointer)
                    .focusable(true)
                    .focus_ring(2.0, 0.0, c.focus)
                    .on_press(move |s: &mut State| s.complete_word(field, word))
                    .child(text(word).font(t::TEXT_PHRASE).color(c.ink_field)),
            ),
        );
    }
    Some(list)
}

// ---- Restoring --------------------------------------------------------------------------

pub fn restore(s: &State, look: Look) -> El {
    let c = look.c;
    let form = &s.setup.restore;
    let count = form.count();
    let words = text_input(&form.words, t::TEXT_PHRASE)
        .key("restore-words")
        .label("Your 24 words, in order")
        .placeholder("orbit cactus timber …", c.ink_muted)
        .color(c.ink_field)
        .caret(c.accent, c.accent_soft)
        .w_full()
        .on_edit(|s: &mut State| &mut s.setup.restore.words);
    let mut field_frame = el()
        .w_full()
        .h(t::RESTORE_FIELD)
        .padding(t::SPACE_14)
        .radius(t::RADIUS_12)
        .fill(c.field)
        .border(1.0, c.border_form)
        .focus_within_ring(1.0, -1.0, c.border_focus)
        .child(words);
    if let Some(glow) = look.shadows.ring_field.first() {
        field_frame = field_frame.focus_within_ring(glow.spread, 0.0, glow.color);
    }
    let mut card = card("Restore", "With your recovery phrase", look)
        .key("restore-card")
        .child(
            col()
                .w_full()
                .gap(t::SPACE_6)
                .child(
                    row()
                        .w_full()
                        .items_center()
                        .child(
                            text("Your 24 words, in order")
                                .font(t::TEXT_LABEL)
                                .color(c.ink_body)
                                .grow(1.0)
                                .a11y_hidden(),
                        )
                        .child(
                            text(format!("{count} of {PHRASE_WORDS}"))
                                .key("restore-count")
                                .font(t::TEXT_META)
                                .color(c.ink_muted),
                        ),
                )
                .child(field_frame),
        )
        .child(notice(
            &icons::MONITOR,
            "Restoring makes this computer your identity’s only device. Every other device of yours stops reading private messages, so a lost one can’t keep reading them.",
            look,
        ))
        .child(notice(
            &icons::SHIELD,
            "Messages from before can’t be read here unless history backup was on.",
            look,
        ));
    if !form.error.is_empty() {
        card = card.child(error_text(&form.error, look));
    }
    card = card.child(primary_button(
        "restore",
        "Restore",
        count == PHRASE_WORDS && !form.busy,
        State::restore_identity,
        look,
    ));
    if count < PHRASE_WORDS {
        card = card.child(quiet("Restore turns on once all 24 words are in.", look));
    } else if count > PHRASE_WORDS {
        card = card.child(quiet("That’s more than 24 words.", look));
    }
    let back = if s.session.signed_in {
        "Not now"
    } else {
        "Back to sign in"
    };
    card_screen(
        "restore-screen",
        card,
        foot("restore-back", back, State::close_restore, look),
        look,
    )
}

// ---- Starting over ----------------------------------------------------------------------

fn start(s: &mut State, _: &mut Tx) {
    s.start_over();
}

pub fn start_over(s: &State, look: Look) -> El {
    let c = look.c;
    let form = &s.setup.start_over;
    let people = s.session.private_contacts();
    let changed = match people {
        0 => {
            "Nobody has talked with you privately yet, so nobody will see a key change.".to_string()
        }
        1 => "The 1 person you talk to privately will see that your key changed.".to_string(),
        n => format!("The {n} people you talk to privately will see that your key changed."),
    };
    let mut points = col()
        .key("start-over-points")
        .w_full()
        .gap(t::SPACE_8)
        .role("list")
        .label("What starting over does");
    for point in [
        "Your account, servers and channels stay as they are.",
        "Private messages from before stay unreadable.",
        changed.as_str(),
        "You’ll get a new recovery phrase to save.",
    ] {
        points = points.child(
            row()
                .w_full()
                .gap(t::SPACE_10)
                .role("listitem")
                .label(point)
                .child(
                    el().mt(t::SPACE_8)
                        .size(t::SPACE_6, t::SPACE_6)
                        .radius(t::SPACE_6 / 2.0)
                        .fill(c.ink_muted),
                )
                .child(
                    text(point)
                        .font(t::TEXT_COMPACT)
                        .color(c.ink_body)
                        .wrap()
                        .grow(1.0)
                        .min_w(0.0)
                        .a11y_hidden(),
                ),
        );
    }
    let mut card = card("Start over", "With a new identity", look)
        .key("start-over-card")
        .child(body(
            "Without a device or your recovery phrase, your old identity can’t come back. You can start over on this computer with a new one.",
            look,
        ))
        .child(points)
        .child(caution(
            "Only do this if the old phrase is truly gone. If you find it later, your old identity can be restored, but not onto this account.",
            look,
        ))
        .child(field(
            "start-over-password",
            "Confirm with your password",
            &form.password,
            "Your password",
            |s| &mut s.setup.start_over.password,
            start,
            look,
        ));
    if !form.error.is_empty() {
        card = card.child(error_text(&form.error, look));
    }
    card = card.child(
        col()
            .w_full()
            .gap(t::SPACE_8)
            .child(danger_button(
                "start-over-confirm",
                "Start over with a new identity",
                !form.busy && !form.password.text.is_empty(),
                State::start_over,
                look,
            ))
            .child(wide_button(
                "start-over-keep",
                None,
                "Keep looking for the phrase",
                true,
                |s, _| s.close_start_over(),
                look,
            )),
    );
    card_screen("start-over", card, el(), look)
}

// ---- Home and Devices -------------------------------------------------------------------

fn finish(s: &mut State, _: &mut Tx) {
    s.finish_setup();
}

/// What Home's panes and Devices show while personal conversations cannot be had from this
/// computer, by where setting up stands: the choices when the account has private messages
/// on another device, the password when binding needs it, or the check itself.
pub fn needs_setup(s: &State, look: Look) -> El {
    let form = &s.setup;
    match &s.session.identity.setup {
        Setup::Elsewhere => empty_state(
            &icons::SHIELD,
            "Private messages need this computer",
            "Your account has private messages on another device. Bring this computer in to read and send them here.",
            look,
        )
        .key("needs-setup")
        .child(
            col()
                .items_center()
                .gap(t::SPACE_10)
                .pt(t::SPACE_8)
                .child(action_button(
                    "setup-link",
                    "Link from a device you use",
                    true,
                    State::start_linking,
                    look,
                ))
                .child(secondary_button(
                    "setup-restore",
                    "Restore with your recovery phrase",
                    true,
                    State::start_restore,
                    look,
                ))
                .child(link_button(
                    "setup-start-over",
                    "Lost every device and the phrase? Start over",
                    State::open_start_over,
                    look,
                )),
        ),
        Setup::NeedsReauth => {
            let mut confirm = col()
                .w(t::AUTH_WIDTH)
                .gap(t::SPACE_12)
                .pt(t::SPACE_8)
                .child(field(
                    "setup-password",
                    "Confirm with your password",
                    &form.password,
                    "Your password",
                    |s| &mut s.setup.password,
                    finish,
                    look,
                ));
            if !form.error.is_empty() {
                confirm = confirm.child(error_text(&form.error, look));
            }
            empty_state(
                &icons::SHIELD,
                "Set up private messages on this computer",
                "Your account doesn’t have private messages yet. Confirm it’s you, and they’re set up here with your account.",
                look,
            )
            .key("needs-setup")
            .child(confirm.child(row().child(action_button(
                "setup-confirm",
                "Set up private messages",
                !form.busy && !form.password.text.is_empty(),
                State::finish_setup,
                look,
            ))))
        }
        Setup::Failed(message) => empty_state(
            &icons::SHIELD,
            "Private messages aren’t set up on this computer",
            &format!("The instance couldn’t be asked: {message}"),
            look,
        )
        .key("needs-setup")
        .child(el().pt(t::SPACE_8).child(action_button(
            "setup-retry",
            "Try again",
            true,
            State::retry_setup,
            look,
        ))),
        Setup::Checking | Setup::Ready => empty_state(
            &icons::SHIELD,
            "Setting up private messages…",
            "Private messages are set up on this computer with your account.",
            look,
        )
        .key("needs-setup"),
    }
}

/// Home's reminder, under the sidebar's tiles, while the phrase waits to be saved and "In a
/// week" has not put it away.
pub fn reminder(s: &State, look: Look) -> Option<El> {
    if !s.phrase_reminder(chrono::Utc::now().timestamp()) {
        return None;
    }
    let tone = t::caution(look.c);
    let words = col()
        .grow(1.0)
        .min_w(0.0)
        .gap(t::SPACE_2)
        .child(
            text("Save your recovery phrase")
                .font(t::TEXT_MEMBER)
                .color(tone.ink)
                .role("heading"),
        )
        .child(
            text("It’s the only way back to your private messages if you lose this computer.")
                .font(t::TEXT_META)
                .color(tone.ink)
                .wrap()
                .w_full(),
        );
    Some(
        col()
            .key("phrase-reminder")
            .w_full()
            .mb(t::SPACE_12)
            .padding(t::SPACE_10)
            .gap(t::SPACE_10)
            .radius(t::RADIUS_14)
            .fill(tone.fill)
            .border(1.0, tone.edge)
            .role("region")
            .label("Save your recovery phrase")
            .child(
                row()
                    .w_full()
                    .gap(t::SPACE_8)
                    .child(icon(&icons::KEY, t::SPACE_16).color(tone.glyph))
                    .child(words),
            )
            .child(
                row()
                    .w_full()
                    .gap(t::SPACE_6)
                    .child(
                        action_button(
                            "reminder-save",
                            "Save it now",
                            true,
                            State::open_phrase,
                            look,
                        )
                        .grow(1.0)
                        .min_w(0.0)
                        .px(t::SPACE_10),
                    )
                    .child(small_button(
                        "reminder-later",
                        "In a week",
                        true,
                        |s, _| s.snooze_phrase_reminder(),
                        look,
                    )),
            ),
    )
}

/// Devices' card while the phrase waits to be saved. "In a week" does not put this one
/// away.
pub fn phrase_card(s: &State, look: Look) -> Option<El> {
    if !s.session.identity.phrase_kept {
        return None;
    }
    let tone = t::caution(look.c);
    Some(
        col()
            .key("phrase-not-saved")
            .w_full()
            .padding(t::SPACE_14)
            .gap(t::SPACE_12)
            .radius(t::RADIUS_14)
            .fill(tone.fill)
            .border(1.0, tone.edge)
            .role("region")
            .label("Recovery phrase not saved")
            .child(
                row()
                    .w_full()
                    .gap(t::SPACE_10)
                    .child(icon(&icons::KEY, t::SPACE_16).color(tone.glyph))
                    .child(
                        col()
                            .grow(1.0)
                            .min_w(0.0)
                            .gap(t::SPACE_2)
                            .child(
                                text("Recovery phrase not saved")
                                    .font(t::TEXT_AUTHOR)
                                    .color(tone.ink)
                                    .role("heading"),
                            )
                            .child(
                                text("If you lose this computer before you save it, your private messages can’t be recovered.")
                                    .font(t::TEXT_COMPACT)
                                    .color(tone.ink)
                                    .wrap()
                                    .w_full(),
                            ),
                    ),
            )
            .child(row().child(action_button(
                "save-phrase-now",
                "Save your recovery phrase",
                true,
                State::open_phrase,
                look,
            ))),
    )
}
