//! What quote replies, reactions and mentions add to the conversation panel (architecture
//! 6.6), drawn as the design canvas's "Inline replies" board has them, the same on the web: a
//! reply's line of reference above it and the chips of a message's reactions under its words,
//! the toolbar over the message under the pointer (Add reaction, Reply, More actions, whose
//! menu edits, copies and deletes), "Delete this message?", the "Quote reply" button over a
//! selection inside one message, and in the composer the strip of the message being replied
//! to (with its "@ On" switch), the @ list and the bar naming those a message mentioned who
//! are not in its channel.
//!
//! The toolbar shows while the pointer is on its message, and over the message being replied
//! to. Its buttons are out of the Tab order: a timeline of messages each with three stops
//! would be no way to move through it. Reaching a message's actions from the keyboard is not
//! built yet (plan step 9).

use lumia_client::app::{QuoteSummary, ReactionSummary};
use rune::{Align, Cursor, El, Side, col, el, icon, row, text};

use super::components::{
    AVATAR_CARD_WIDTH, Look, avatar_of, error_text, link_button, menu, menu_glyph, menu_item,
    popover, primary_button, secondary_button, shadowed, small_button,
};
use crate::app::State;
use crate::app::replies::Choice;
use crate::session::{Entry, display_name};
use crate::settings::Layout;
use crate::tokens::{self as t, icons};

/// The connector of a reply's line of reference: from under the reply's avatar up and over
/// to the quoted person (the board's 26 by 12 stroke, `M1 12V7a5 5 0 0 1 5-5h19`, drawn as a
/// box with two sides and one round corner).
fn connector(look: Look) -> El {
    el().w(t::SPACE_24 + t::SPACE_2)
        .h(t::SPACE_12)
        .mt(t::SPACE_8)
        .a11y_hidden()
        .child(
            el().mt(1.0)
                .w(t::SPACE_24 + 1.0)
                .h(t::SPACE_12 - 1.0)
                .border_sides([t::SPACE_2, 0.0, 0.0, t::SPACE_2], look.c.border_hover)
                .corners([t::RADIUS_6, 0.0, 0.0, 0.0]),
        )
}

/// A reply's line of reference, above its row (the "Inline replies" board): the connector
/// from under the avatar, the quoted person's small avatar and name ("@" and the name when
/// it is this account), and the quoted text on one line. Pressing it finds the original; a
/// deleted original says so instead. `indent` puts the connector under the reply's avatar.
pub fn reference_line(
    s: &State,
    quote: &QuoteSummary,
    indent: f32,
    ty: &t::MessageType,
    look: Look,
) -> El {
    let line = row()
        .key("quote")
        .w_full()
        .pl(indent)
        .gap(t::SPACE_6)
        .items_center()
        .child(connector(look));
    reference(s, quote, line, true, ty, look)
}

/// A reply's line of reference in a one-line row, over the message at its text's start:
/// `↳ Priya: first words…`, without the connector or the avatars. It is `height` tall,
/// which the row's time and name keep clear of above them.
pub fn reference_short(
    s: &State,
    quote: &QuoteSummary,
    height: f32,
    ty: &t::MessageType,
    look: Look,
) -> El {
    let line = row()
        .key("quote")
        .w_full()
        .h(height)
        .gap(t::SPACE_4)
        .items_center()
        .child(
            text("↳")
                .font(ty.reference)
                .color(look.c.ink_muted)
                .a11y_hidden(),
        );
    reference(s, quote, line, false, ty, look)
}

/// What both lines of reference hold after their start: who is quoted, with their avatar
/// when `avatar`, and the quoted words, or that the original was deleted.
fn reference(
    s: &State,
    quote: &QuoteSummary,
    line: El,
    avatar: bool,
    ty: &t::MessageType,
    look: Look,
) -> El {
    let c = look.c;
    if quote.original_deleted {
        let mut line = line;
        if avatar {
            line = line.child(
                el().size(t::REFERENCE_AVATAR, t::REFERENCE_AVATAR)
                    .radius(t::REFERENCE_AVATAR / 2.0)
                    .fill(c.chip)
                    .center()
                    .a11y_hidden()
                    .child(
                        el().w(t::SPACE_8 + 1.0)
                            .h(t::SPACE_2)
                            .radius(1.0)
                            .fill(c.ink_muted),
                    ),
            );
        }
        return line.child(
            rune::rich(
                "Original message was deleted",
                vec![rune::Span::new(0, "Original message was deleted".len()).italic()],
            )
            .font(ty.reference)
            .color(c.ink_muted),
        );
    }
    let mine = quote.author_account_id == s.session.me();
    let name =
        (!quote.author_account_id.is_empty()).then(|| s.session.name_of(&quote.author_account_id));
    let mut line = line;
    if let Some(name) = &name {
        if avatar {
            line = line.child(el().a11y_hidden().child(avatar_of(
                s,
                name,
                &quote.author_account_id,
                t::REFERENCE_AVATAR,
                look,
            )));
        }
        let shown = match (mine, avatar) {
            (true, true) => format!("@{name}"),
            (true, false) => format!("@{name}:"),
            (false, true) => name.clone(),
            (false, false) => format!("{name}:"),
        };
        line = line.child(text(shown).font(ty.link_sm).color(if mine {
            c.accent_ink
        } else {
            c.ink_body
        }));
    }
    let id = quote.message_id.clone();
    let label = match (&name, mine) {
        (_, true) => format!("Replying to you: {}. Jump to message", quote.text),
        (Some(name), false) => format!("Replying to {name}: {}. Jump to message", quote.text),
        (None, false) => format!("Replying to: {}. Jump to message", quote.text),
    };
    line.hover_group()
        .role("button")
        .label(label)
        .cursor(Cursor::Pointer)
        .focusable(true)
        .focus_ring(2.0, 0.0, c.focus)
        .on_press_tx(move |s: &mut State, tx| s.jump_to(&id, tx))
        .child(
            text(quote.text.replace('\n', " "))
                .font(ty.reference)
                .color(c.ink_muted)
                .group_color(c.ink_body)
                .truncate()
                .shrink()
                .min_w(0.0),
        )
}

/// Who reacted with an emoji, for its chip's tooltip: "You and Ada reacted with 👍".
fn reacted_words(s: &State, reaction: &ReactionSummary) -> String {
    let me = s.session.me();
    let mut names: Vec<String> = Vec::new();
    if reaction.me {
        names.push("You".into());
    }
    names.extend(
        reaction
            .account_ids
            .iter()
            .filter(|id| id.as_str() != me)
            .map(|id| {
                let name = s.session.name_of(id);
                if name.is_empty() {
                    "Someone".into()
                } else {
                    name
                }
            }),
    );
    let count = usize::try_from(reaction.count).unwrap_or(usize::MAX);
    let unnamed = count.saturating_sub(names.len());
    let who = match (names.as_slice(), unnamed) {
        ([], n) => format!("{n} people"),
        ([one], 0) => one.clone(),
        ([one, two], 0) => format!("{one} and {two}"),
        ([one, two], n) => format!("{one}, {two} and {}", super::others(n)),
        ([one], n) => format!("{one} and {}", super::others(n)),
        ([one, two, rest @ ..], n) => format!("{one}, {two} and {}", super::others(rest.len() + n)),
    };
    format!("{who} reacted with {}", reaction.reaction)
}

/// A message's reactions as chips, the one this account has in the accent, with the way to
/// add another at the end.
pub fn reactions(s: &State, entry: &Entry, ty: &t::MessageType, look: Look) -> El {
    let c = look.c;
    let mut chips = row()
        .key("reactions")
        .w_full()
        .flex_wrap()
        .gap(t::SPACE_6)
        .items_center()
        .role("list")
        .label("Reactions");
    for (n, reaction) in entry.reactions.iter().enumerate() {
        let (id, emoji) = (entry.id.clone(), reaction.reaction.clone());
        let words = reacted_words(s, reaction);
        let mut chip = row()
            .key(format!("reaction-{n}"))
            .min_h(t::SPACE_28)
            .px(t::SPACE_10)
            .gap(t::SPACE_6)
            .items_center()
            .radius(t::RADIUS_PILL)
            .role("button")
            .label(format!("{words}, {}", reaction.count))
            .help(words)
            .checked(reaction.me)
            .cursor(Cursor::Pointer)
            .focusable(true)
            .focus_ring(2.0, 0.0, c.focus)
            .on_press(move |s: &mut State| s.react_with(&id, &emoji))
            .child(text(reaction.reaction.clone()).font(ty.reaction))
            .child(
                text(reaction.count.to_string())
                    .font(ty.reaction_count)
                    .color(if reaction.me {
                        c.accent_ink
                    } else {
                        c.ink_body
                    }),
            );
        chip = if reaction.me {
            chip.fill(c.accent_soft).border(1.0, c.reaction_on_border)
        } else {
            chip.fill(c.reaction)
                .border(1.0, c.reaction)
                .hover(c.hover_icon)
        };
        chips = chips.child(chip);
    }
    let (id, anchor) = (entry.id.clone(), format!("add-reaction-{}", entry.id));
    chips.child(
        el().key(anchor.clone())
            .size(t::SPACE_28, t::SPACE_28)
            .radius(t::RADIUS_PILL)
            .center()
            .role("button")
            .label("Add reaction")
            .help("Add reaction")
            .hover(c.hover_icon)
            .cursor(Cursor::Pointer)
            .focusable(true)
            .focus_ring(2.0, 0.0, c.focus)
            .on_press(move |s: &mut State| s.open_picker(&id, &anchor))
            .child(icon(&icons::REACTION_ADD, t::SPACE_16).color(c.ink_muted)),
    )
}

/// The toolbar over the message under the pointer, or over the one being replied to: Add
/// reaction, Reply (the board's pressed pill while this message is being replied to) and More
/// actions, at the row's top end and inside it (so the pointer on it is still on the row).
pub fn toolbar(s: &State, entry_id: &str, look: Look) -> El {
    let c = look.c;
    let button = |key: String, label: &str, glyph: &'static rune::Icon| {
        el().key(key)
            .size(t::TOOLBAR_BUTTON, t::TOOLBAR_BUTTON)
            .radius(t::RADIUS_7)
            .center()
            .role("button")
            .label(label)
            .help(label)
            .hover(c.hover_icon)
            .hover_group()
            .cursor(Cursor::Pointer)
            .child(
                icon(glyph, t::SPACE_16)
                    .color(c.icon_strong)
                    .group_color(c.ink_field),
            )
    };
    let anchor = format!("react-{entry_id}");
    let (id, at) = (entry_id.to_string(), anchor.clone());
    let add = button(anchor, "Add reaction", &icons::EMOJI)
        .on_press(move |s: &mut State| s.open_picker(&id, &at));
    let replying = s
        .replies
        .replying
        .as_ref()
        .is_some_and(|q| q.message_id == entry_id);
    let id = entry_id.to_string();
    let reply = if replying {
        row()
            .key(format!("reply-{entry_id}"))
            .h(t::TOOLBAR_BUTTON)
            .pl(t::SPACE_8)
            .pr(t::SPACE_10)
            .gap(t::SPACE_6)
            .items_center()
            .radius(t::RADIUS_7)
            .fill(c.accent_soft)
            .role("button")
            .label("Reply")
            .checked(true)
            .cursor(Cursor::Pointer)
            .on_press(State::cancel_reply)
            .child(icon(&icons::REPLY, t::SPACE_16).color(c.accent_ink))
            .child(text("Reply").font(t::TEXT_LINK_SM).color(c.accent_ink))
    } else {
        button(format!("reply-{entry_id}"), "Reply", &icons::REPLY)
            .on_press_tx(move |s: &mut State, tx| s.reply_to(&id, None, tx))
    };
    let id = entry_id.to_string();
    let more = button(format!("more-{entry_id}"), "More actions", &icons::MORE)
        .on_press(move |s: &mut State| s.open_message_menu(&id));
    let bar = row()
        .key("actions")
        .padding(3.0)
        .gap(t::SPACE_2)
        .items_center()
        .radius(t::RADIUS_10)
        .fill(c.raised)
        .border(1.0, c.line_strong)
        .role("toolbar")
        .label("Message actions")
        .child(add)
        .child(reply)
        .child(more);
    // As the board hangs it, its top 14 over the row's top edge (its end at the row's, where
    // the board's is 16 in: an anchored overlay takes no margin). The pointer moving onto it
    // stays on the list, so the row stays the one under it. A one-line row is not as tall as
    // the bar, which hung so would cover the end of its words: there it hangs over the row
    // above, overlapping this one by a little, so the pointer still reaches it directly.
    let tall = t::TOOLBAR_BUTTON + 2.0 * 3.0 + 2.0;
    let overlap = match s.settings.layout {
        Layout::Grouped => tall - t::SPACE_14,
        Layout::OneLine => t::SPACE_4,
    };
    shadowed(bar, look.shadows.floating).anchored_fit(
        super::timeline::frame_key(entry_id),
        Side::Top,
        Align::End,
        -overlap,
    )
}

/// The "More actions" menu of a message: Edit message (this account's own), Copy text, and
/// Delete message (its own, or another's for a moderator; Shift deletes without asking).
pub fn message_menu(s: &State, look: Look) -> Option<El> {
    let id = s.replies.menu_for.clone()?;
    let entry = s.session.entries.iter().find(|e| e.id == id)?;
    let mine = entry.author_account_id == s.session.me();
    let mut list = menu(
        "message-menu",
        &format!("more-{id}"),
        "Message actions",
        Side::Bottom,
        Align::End,
        look,
    );
    if mine {
        let at = id.clone();
        list = list.child(
            menu_item(
                "menu-edit",
                Some(menu_glyph(&icons::COMPOSE, look)),
                "Edit message",
                None,
                move |_s: &mut State| {},
                look,
            )
            .on_press_tx(move |s: &mut State, tx| s.begin_edit(&at, tx)),
        );
    }
    let at = id.clone();
    list = list.child(
        menu_item(
            "menu-copy",
            Some(menu_glyph(&icons::FILE, look)),
            "Copy text",
            None,
            move |_s: &mut State| {},
            look,
        )
        .on_press_tx(move |s: &mut State, tx| s.copy_message(&at, tx)),
    );
    if s.session.may_delete(entry) {
        let at = id.clone();
        list = list.child(
            menu_item(
                "menu-delete",
                Some(icon(&icons::CLOSE, t::SPACE_16).color(look.c.dnd)),
                "Delete message",
                None,
                move |_s: &mut State| {},
                look,
            )
            .on_press_tx(move |s: &mut State, tx| s.delete_from_menu(&at, tx)),
        );
    }
    Some(list)
}

/// "Delete this message?": the message's first line, and Cancel or Delete.
pub fn delete_dialog(s: &State, look: Look) -> Option<El> {
    let id = s.replies.deleting.clone()?;
    let entry = s.session.entries.iter().find(|e| e.id == id)?;
    let first = entry.body.lines().next().unwrap_or_default().to_string();
    Some(
        popover(
            "delete-message",
            &super::timeline::frame_key(&id),
            "Delete this message?",
            "It goes for everyone in the conversation.",
            super::components::CARD_WIDTH,
            look,
        )
        .child(
            text(first)
                .font(t::TEXT_COMPACT)
                .color(look.c.ink_muted)
                .max_lines(2)
                .w_full(),
        )
        .child(
            row()
                .w_full()
                .gap(t::SPACE_8)
                // Both a pane's button high (40), as the profile card's pair.
                .child(
                    secondary_button("delete-cancel", "Cancel", true, State::close_popover, look)
                        .h(t::CONTROL_PANE),
                )
                .child(
                    primary_button(
                        "delete-confirm",
                        "Delete",
                        true,
                        State::confirm_delete,
                        look,
                    )
                    .h(t::CONTROL_PANE)
                    .shrink()
                    .min_w(0.0),
                ),
        ),
    )
}

/// "Quote reply" over a selection inside one message, at `at` (window points): a press
/// leaves the selection as it is and quotes it.
pub fn quote_button(message_id: &str, words: &str, at: [f32; 2], look: Look) -> El {
    let c = look.c;
    let (id, part) = (message_id.to_string(), words.to_string());
    shadowed(
        row()
            .key("quote-reply")
            .absolute(at[0], at[1])
            .h(t::ICON_BUTTON)
            .px(t::SPACE_10)
            .gap(t::SPACE_6)
            .items_center()
            .radius(t::RADIUS_9)
            .fill(c.surface)
            .border(1.0, c.line_strong)
            .hover(c.hover_member)
            .role("button")
            .label("Quote reply")
            .cursor(Cursor::Pointer)
            .focusable(true)
            .focus_ring(2.0, 0.0, c.focus)
            .keeps_text_selection()
            .on_press_tx(move |s: &mut State, tx| s.reply_to(&id, Some(&part), tx))
            .child(icon(&icons::REPLY, t::SPACE_14).color(c.ink_quiet))
            .child(text("Quote reply").font(t::TEXT_MENU).color(c.ink_body)),
        look.shadows.floating,
    )
}

/// The strip inside the composer's frame while replying (the board's): "Replying to Name",
/// the "@ On" switch (whether the reply notifies them) and the way out of it.
pub fn reply_strip(s: &State, quote: &QuoteSummary, look: Look) -> El {
    let c = look.c;
    let name = s.session.name_of(&quote.author_account_id);
    let words = rune::rich(
        format!("Replying to {name}"),
        vec![
            rune::Span::new("Replying to ".len(), "Replying to ".len() + name.len())
                .weight(t::WEIGHT_STRONG)
                .color(c.ink),
        ],
    )
    .font(t::TEXT_STRIP)
    .color(c.ink_secondary)
    .truncate()
    .grow(1.0)
    .min_w(0.0);
    let mut strip = row()
        .key("reply-bar")
        .w_full()
        .pl(t::SPACE_14)
        .pr(t::SPACE_8)
        .py(t::SPACE_6 + 1.0)
        .gap(t::SPACE_10)
        .items_center()
        .fill(c.inset)
        // Its own top corners, the composer frame's inside its 1-point border: the frame's
        // rounded clip does not reach a child's fill, which drew square over its corners.
        .corners([t::RADIUS_16 - 1.0, t::RADIUS_16 - 1.0, 0.0, 0.0])
        .border_sides([0.0, 0.0, 1.0, 0.0], c.border_composer)
        .child(icon(&icons::REPLY, t::SPACE_14 + 1.0).color(c.ink_secondary))
        .child(words);
    // No switch on a reply to oneself: there is nobody to notify.
    if quote.author_account_id != s.session.me() {
        let on = !quote.silent;
        strip = strip.child(
            row()
                .key("reply-mention")
                .h(t::SPACE_28)
                .px(t::SPACE_10)
                .gap(t::SPACE_4)
                .items_center()
                .radius(t::RADIUS_PILL)
                .fill(if on { c.accent_soft } else { c.chip })
                .role("button")
                .label(format!("Mention {name}"))
                .checked(on)
                .cursor(Cursor::Pointer)
                .focusable(true)
                .focus_ring(2.0, 0.0, c.focus)
                .on_press(State::toggle_reply_mention)
                .child(icon(&icons::MENTION, t::SPACE_12 + 1.0).color(if on {
                    c.accent_ink
                } else {
                    c.ink_quiet
                }))
                .child(
                    text(if on { "On" } else { "Off" })
                        .font(t::TEXT_SECTION_STRONG)
                        .color(if on { c.accent_ink } else { c.ink_quiet }),
                ),
        );
    }
    strip.child(
        el().key("cancel-reply")
            .size(t::SPACE_28, t::SPACE_28)
            .radius(t::RADIUS_8)
            .center()
            .role("button")
            .label("Cancel reply")
            .help("Cancel reply")
            .hover(c.hover_icon)
            .cursor(Cursor::Pointer)
            .focusable(true)
            .focus_ring(2.0, 0.0, c.focus)
            .on_press(State::cancel_reply)
            .child(icon(&icons::CLOSE, t::SPACE_14).color(c.ink_quiet)),
    )
}

/// A bar over the composer: what it says, and what can be done at its end.
fn bar(key: &'static str, words: El, end: Option<El>, look: Look) -> El {
    let c = look.c;
    let mut bar = row()
        .key(key)
        .w_full()
        .px(t::SPACE_12)
        .py(t::SPACE_8)
        .gap(t::SPACE_10)
        .items_center()
        .radius(t::RADIUS_12)
        .fill(c.inset)
        .border_sides([0.0, 0.0, 0.0, t::SPACE_2], c.accent)
        .child(words);
    if let Some(end) = end {
        bar = bar.child(end);
    }
    bar
}

/// After a send that mentioned people not in the channel: who, and "Add to #name"; after an
/// add, who was invited instead.
pub fn outside_bar(s: &State, look: Look) -> Option<El> {
    let c = look.c;
    let outside = s.replies.outside.as_ref()?;
    let words = s.outside_words()?;
    let channel = s
        .session
        .channel_summary(&outside.channel_id)
        .map(|c| c.name.clone())
        .unwrap_or_default();
    let mut said = col().grow(1.0).min_w(0.0).gap(t::SPACE_4).child(
        text(words)
            .font(t::TEXT_META)
            .color(c.ink_body)
            .wrap()
            .w_full()
            .role("status"),
    );
    if !outside.error.is_empty() {
        said = said.child(error_text(&outside.error, look));
    }
    let end = outside.said.is_none().then(|| {
        row()
            .gap(t::SPACE_10)
            .items_center()
            .child(small_button(
                "add-outside",
                &format!("Add to #{channel}"),
                !outside.adding,
                |s, _| s.add_outside(),
                look,
            ))
            .child(link_button(
                "dismiss-outside",
                "Dismiss",
                State::dismiss_outside,
                look,
            ))
    });
    Some(bar("outside-bar", said, end, look))
}

/// The @ list over the composer: everyone first for one who may, then the server's members
/// that match, each marked when not in the channel.
pub fn mention_list(s: &State, look: Look) -> Option<El> {
    let c = look.c;
    let choices = s.mention_choices();
    if choices.is_empty() {
        return None;
    }
    let chosen = s.replies.mention.as_ref().map_or(0, |q| q.chosen);
    let channel = s
        .session
        .channel()
        .and_then(|id| s.session.channel_summary(id))
        .map(|c| c.name.clone())
        .unwrap_or_default();
    let mut list = col()
        .key("mention-list")
        .w(AVATAR_CARD_WIDTH)
        .padding(t::SPACE_6)
        .gap(t::SPACE_2)
        .radius(t::RADIUS_14)
        .fill(c.surface)
        .border(1.0, c.line_strong)
        .role("listbox")
        .label("People to mention");
    for (n, choice) in choices.iter().enumerate() {
        let (name, handle, note, lead) = match choice {
            Choice::Everyone => (
                "@everyone".to_string(),
                String::new(),
                format!("Notify everyone in #{channel}"),
                el().size(t::AVATAR_XS, t::AVATAR_XS)
                    .radius(t::AVATAR_XS / 2.0)
                    .fill(c.accent_soft)
                    .center()
                    .child(icon(&icons::MENTION, t::SPACE_14).color(c.accent_ink)),
            ),
            Choice::Member { member, inside } => {
                let name = display_name(member);
                (
                    name.clone(),
                    format!("@{}", member.handle),
                    if *inside {
                        String::new()
                    } else {
                        format!("Not in #{channel}")
                    },
                    avatar_of(s, &name, &member.account_id, t::AVATAR_XS, look),
                )
            }
        };
        let on = n == chosen;
        let mut item = row()
            .key(format!("mention-{n}"))
            .w_full()
            .h(t::MENU_ITEM_HEIGHT)
            .px(t::SPACE_10)
            .gap(t::SPACE_10)
            .items_center()
            .radius(t::RADIUS_8)
            .role("option")
            .label(if note.is_empty() {
                format!("{name} {handle}")
            } else {
                format!("{name} {handle}, {note}")
            })
            .checked(on)
            .cursor(Cursor::Pointer)
            .hover(c.hover_member)
            .on_press(move |s: &mut State| s.pick_mention(n))
            .child(lead)
            .child(
                text(name)
                    .font(t::TEXT_MENU)
                    .color(c.ink)
                    .truncate()
                    .shrink()
                    .min_w(0.0),
            );
        if !handle.is_empty() {
            item = item.child(
                text(handle)
                    .font(t::TEXT_META)
                    .color(c.ink_muted)
                    .truncate(),
            );
        }
        item = item.child(el().grow(1.0));
        if !note.is_empty() {
            item = item.child(text(note).font(t::TEXT_META).color(c.ink_muted).truncate());
        }
        if on {
            item = item.fill(c.hover_member);
        }
        list = list.child(item);
    }
    Some(shadowed(list, look.shadows.floating).anchored_fit(
        "composer-box",
        Side::Top,
        rune::Align::Start,
        t::SPACE_6,
    ))
}

/// Where the "Quote reply" button goes for a selection whose line boxes are `rects`: under
/// the end the pointer is at.
pub fn quote_button_at(rects: &[[f32; 4]], backward: bool) -> Option<[f32; 2]> {
    let rect = if backward {
        rects.first()
    } else {
        rects.last()
    }?;
    let x = if backward { rect[0] } else { rect[0] + rect[2] };
    Some([x, rect[1] + rect[3] + t::SPACE_4])
}
