//! What quote replies, reactions and mentions add to the conversation panel (architecture
//! 6.6), the same on the web: a quote above a message's words and the chips of its reactions
//! under them, the toolbar over a message under the pointer (six quick reactions, Add
//! reaction, Reply), the "Quote reply" button over a selection inside one message, and over
//! the composer the quote being replied with, the @ list and the bar naming those a message
//! mentioned who are not in its channel.
//!
//! The toolbar shows while the pointer is on its message. Its buttons are out of the Tab
//! order: a timeline of messages each with eight stops would be no way to move through it.
//! Reaching a message's actions from the keyboard is not built yet (plan step 9).

use lumia_client::app::{QuoteSummary, ReactionSummary};
use rune::{Cursor, El, Side, col, el, icon, row, text};

use super::components::{
    AVATAR_CARD_WIDTH, Look, avatar_of, error_text, link_button, shadowed, small_button,
};
use crate::app::State;
use crate::app::replies::{Choice, QUICK};
use crate::session::{Entry, display_name};
use crate::tokens::{self as t, icons};

/// A quote above a message's words: the accent's rule, the quoted author and the text,
/// two lines at most. Pressing it finds the original; a deleted original says so instead.
pub fn quote_block(s: &State, quote: &QuoteSummary, look: Look) -> El {
    let c = look.c;
    let rule = el()
        .w(t::SPACE_2)
        .h_full()
        .radius(t::SPACE_2)
        .fill(c.accent);
    let mut words = col().grow(1.0).min_w(0.0).gap(t::SPACE_2);
    if quote.original_deleted {
        return row()
            .key("quote")
            .w_full()
            .gap(t::SPACE_8)
            .child(rule.fill(c.line_strong))
            .child(
                text("Original message was deleted")
                    .font(t::TEXT_META)
                    .color(c.ink_muted),
            );
    }
    let name =
        (!quote.author_account_id.is_empty()).then(|| s.session.name_of(&quote.author_account_id));
    if let Some(name) = &name {
        words = words.child(
            text(name.clone())
                .font(t::TEXT_SECTION)
                .color(c.ink_emphasis),
        );
    }
    words = words.child(
        text(quote.text.clone())
            .font(t::TEXT_COMPACT)
            .color(c.ink_secondary)
            .max_lines(2)
            .w_full(),
    );
    let id = quote.message_id.clone();
    let label = match &name {
        Some(name) => format!("Quote of {name}: {}. Go to the original", quote.text),
        None => format!("Quote: {}. Go to the original", quote.text),
    };
    row()
        .key("quote")
        .w_full()
        .gap(t::SPACE_8)
        .py(t::SPACE_2)
        .radius(t::RADIUS_8)
        .hover(c.hover_row)
        .role("button")
        .label(label)
        .cursor(Cursor::Pointer)
        .focusable(true)
        .focus_ring(2.0, 0.0, c.focus)
        .on_press(move |s: &mut State| s.jump_to(&id))
        .child(rule)
        .child(words)
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
pub fn reactions(s: &State, entry: &Entry, look: Look) -> El {
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
        let mut chip =
            row()
                .key(format!("reaction-{n}"))
                .h(t::SPACE_28)
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
                .child(text(reaction.reaction.clone()).font(t::TEXT_COMPACT))
                .child(text(reaction.count.to_string()).font(t::TEXT_LABEL).color(
                    if reaction.me {
                        c.accent_ink
                    } else {
                        c.ink_body
                    },
                ));
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

/// The toolbar over the message under the pointer: the six quick reactions, Add reaction
/// and Reply, inside the top end of its row (so the pointer on it is still on the row).
pub fn toolbar(entry_id: &str, look: Look) -> El {
    let c = look.c;
    let button = |key: String, label: String| {
        el().key(key)
            .size(t::ICON_BUTTON, t::ICON_BUTTON)
            .radius(t::RADIUS_9)
            .center()
            .role("button")
            .label(label.clone())
            .help(label)
            .hover(c.hover_icon)
            .cursor(Cursor::Pointer)
    };
    let mut bar = row()
        .key("actions")
        .padding(t::SPACE_2)
        .gap(t::SPACE_2)
        .items_center()
        .radius(t::RADIUS_10)
        .fill(c.surface)
        .border(1.0, c.line_strong)
        .role("toolbar")
        .label("Message actions");
    for (n, emoji) in QUICK.iter().enumerate() {
        let id = entry_id.to_string();
        bar = bar.child(
            button(format!("quick-{n}"), format!("React with {emoji}"))
                .on_press(move |s: &mut State| s.react_with(&id, emoji))
                .child(text(*emoji).font(t::TEXT_MESSAGE)),
        );
    }
    bar = bar.child(el().w(1.0).h(t::SPACE_18).mx(t::SPACE_2).fill(c.line));
    let anchor = format!("react-{entry_id}");
    let (id, at) = (entry_id.to_string(), anchor.clone());
    bar = bar.child(
        button(anchor, "Add reaction".into())
            .on_press(move |s: &mut State| s.open_picker(&id, &at))
            .child(icon(&icons::REACTION_ADD, t::SPACE_16).color(c.ink_quiet)),
    );
    let id = entry_id.to_string();
    bar = bar.child(
        button(format!("reply-{entry_id}"), "Reply".into())
            .on_press_tx(move |s: &mut State, tx| s.reply_to(&id, None, tx))
            .child(icon(&icons::REPLY, t::SPACE_16).color(c.ink_quiet)),
    );
    // Above the row's top edge by the bar's own height less its padding: inside the row.
    let tall = t::ICON_BUTTON + 2.0 * t::SPACE_2 + 2.0;
    shadowed(bar, look.shadows.floating).anchored_fit(
        super::timeline::frame_key(entry_id),
        Side::Top,
        rune::Align::End,
        -(tall + t::SPACE_2),
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

/// Over the composer while replying: "Replying to Ada", the quote on a line, and the way
/// out of it.
pub fn reply_bar(s: &State, quote: &QuoteSummary, look: Look) -> El {
    let c = look.c;
    let name = s.session.name_of(&quote.author_account_id);
    let close = el()
        .key("cancel-reply")
        .size(t::SPACE_24, t::SPACE_24)
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
        .child(icon(&icons::CLOSE, t::SPACE_14).color(c.ink_quiet));
    let first = rune::rich(
        format!("Replying to {name}"),
        vec![
            rune::Span::new("Replying to ".len(), "Replying to ".len() + name.len())
                .weight(t::WEIGHT_STRONG)
                .color(c.ink_emphasis),
        ],
    )
    .font(t::TEXT_META)
    .color(c.ink_muted)
    .truncate();
    bar(
        "reply-bar",
        col()
            .grow(1.0)
            .min_w(0.0)
            .gap(t::SPACE_2)
            .child(first)
            .child(
                text(quote.text.clone())
                    .font(t::TEXT_COMPACT)
                    .color(c.ink_secondary)
                    .truncate(),
            ),
        Some(close),
        look,
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
