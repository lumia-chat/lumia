//! Quote replies, reactions, mentions and read state (architecture 6.6) over a state no
//! instance is behind: the message toolbar's Reply and the quote over the composer, "Quote
//! reply" over a selection, reaction chips, the emoji picker, the @ list, the bar naming
//! those a message mentioned who are not in the channel, unread rows and counts, the "New
//! messages" line and the Mentions page.

#![allow(clippy::expect_used, clippy::unwrap_used)]

mod common;

use common::*;
use lumia_client::app::{
    AppEvent, ChannelAddSummary, MentionSummary, MessageSummary, QuoteSummary, ReactionSummary,
    ReadStateSummary,
};
use lumia_shell::app::State;
use lumia_shell::app::replies::Outside;
use lumia_shell::session::Update;
use lumia_shell::session::channels::ChannelUpdate;
use lumia_shell::session::reads::MentionsPage;
use rune::{Key, KeyEvent, Mods, Runtime};

fn key(rt: &mut Runtime<State>, key: Key) {
    rt.key(KeyEvent {
        key,
        mods: Mods::default(),
    });
    settle(rt);
}

/// Puts the pointer on a message's row, which brings its toolbar.
fn hover(rt: &mut Runtime<State>, id: &str) {
    let (x, y) = rt
        .find(id)
        .unwrap_or_else(|| panic!("{id} is not on screen"));
    rt.pointer_move(x, y);
    settle(rt);
}

/// The fixture with Bo, a member of the server who is in none of its channels.
fn with_bo() -> State {
    let mut state = signed_in();
    state.receive(&Update::Members {
        server_id: "acme".into(),
        members: vec![
            member("me", "ada", "Ada Lovelace"),
            member("grace", "grace", "Grace Hopper"),
            member("bo", "bo", "Bo Diddley"),
        ],
    });
    state
}

#[test]
fn reply_puts_the_quote_over_the_composer_and_escape_takes_it_away() {
    let mut rt = runtime(signed_in());
    assert!(
        rt.find("reply-m1").is_none(),
        "no toolbar until the pointer is on a row"
    );
    hover(&mut rt, "m1");
    press(&mut rt, "reply-m1");
    let quote = rt.state.replies.replying.clone().expect("replying");
    assert_eq!(quote.message_id, "m1");
    assert_eq!(quote.text, "Shipped on **Friday**.");
    assert!(words(&rt).contains("Replying to Grace Hopper"));
    assert!(rt.find("composer").is_some());
    key(&mut rt, Key::Escape);
    assert!(rt.state.replies.replying.is_none());
    assert!(rt.find("reply-bar").is_none());
}

/// The rect `[x, y, w, h]` of the element keyed `key`.
fn rect(rt: &Runtime<State>, key: &str) -> [f32; 4] {
    rt.elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some(key))
        .unwrap_or_else(|| panic!("{key} is not on screen"))
        .rect
}

#[test]
fn the_toolbar_sits_at_the_top_end_of_the_row_under_the_pointer() {
    let mut rt = runtime(signed_in());
    hover(&mut rt, "m2");
    let row = rect(&rt, "message-m2");
    let bar = rect(&rt, "actions");
    // At the row's top end as the board hangs it: over its top edge, at its right one.
    assert!(
        (bar[0] + bar[2] - (row[0] + row[2])).abs() < 1.0,
        "{bar:?} in {row:?}"
    );
    assert!(
        bar[1] < row[1] && bar[1] + bar[3] > row[1],
        "{bar:?} over the top of {row:?}"
    );
}

#[test]
fn a_selection_inside_one_message_offers_quote_reply_of_just_that() {
    let mut rt = runtime(signed_in());
    rt.update(|_, tx| tx.select_text("message-m1/body/b0", 0..7));
    settle(&mut rt);
    let selection = rt.state.replies.selection.clone().expect("a selection");
    assert_eq!(
        (selection.0.as_str(), selection.1.as_str()),
        ("m1", "Shipped")
    );
    press(&mut rt, "quote-reply");
    let quote = rt.state.replies.replying.clone().expect("replying");
    assert_eq!(
        (quote.message_id.as_str(), quote.text.as_str()),
        ("m1", "Shipped")
    );

    // Across two messages there is nothing to quote.
    rt.update(|_, tx| tx.select_all_text("rows"));
    settle(&mut rt);
    assert!(rt.state.replies.selection.is_none());
    assert!(rt.find("quote-reply").is_none());
}

#[test]
fn a_quote_and_reactions_are_drawn_with_who_reacted() {
    let mut state = signed_in();
    state.session.entries[1].reply = Some(QuoteSummary {
        message_id: "m1".into(),
        author_account_id: "grace".into(),
        text: "Shipped on Friday.".into(),
        original_deleted: false,
        silent: false,
    });
    state.session.entries[1].reactions = vec![ReactionSummary {
        reaction: "👍".into(),
        count: 2,
        me: true,
        account_ids: vec!["me".into(), "grace".into()],
    }];
    state.session.entries[0].reply = Some(QuoteSummary {
        message_id: "m0".into(),
        author_account_id: String::new(),
        text: String::new(),
        original_deleted: true,
        silent: false,
    });
    state.session.revision += 1;
    let rt = runtime(state);
    let all = words(&rt);
    assert!(
        all.contains("Replying to Grace Hopper: Shipped on Friday.. Jump to message"),
        "{all}"
    );
    assert!(
        all.contains("You and Grace Hopper reacted with 👍, 2"),
        "{all}"
    );
    assert!(all.contains("Original message was deleted"), "{all}");
    assert!(rt.find("add-reaction-m2").is_some());
}

#[test]
fn the_picker_finds_an_emoji_by_name() {
    let mut rt = runtime(signed_in());
    hover(&mut rt, "m1");
    press(&mut rt, "react-m1");
    assert!(rt.find("emoji-picker").is_some());
    assert!(words(&rt).contains("Frequently used"));
    press(&mut rt, "emoji-search");
    type_text(&mut rt, "party popper");
    assert!(words(&rt).contains("party popper"));
    assert!(rt.find("s0").is_some());
}

#[test]
fn the_at_list_completes_a_handle_and_marks_who_is_not_in_the_channel() {
    let mut rt = runtime(with_bo());
    press(&mut rt, "composer");
    type_text(&mut rt, "hi @");
    let all = words(&rt);
    assert!(all.contains("Grace Hopper @grace"), "{all}");
    assert!(all.contains("Bo Diddley @bo, Not in #general"), "{all}");
    type_text(&mut rt, "gr");
    assert!(rt.find("mention-1").is_none(), "only Grace matches");
    key(&mut rt, Key::Enter);
    assert_eq!(rt.state.draft.text, "hi @grace ");
    assert!(rt.find("mention-list").is_none());
}

#[test]
fn the_bar_after_a_send_adds_or_says_who_was_invited() {
    let mut state = with_bo();
    state.replies.outside = Some(Outside {
        channel_id: "general".into(),
        account_ids: vec!["bo".into()],
        adding: false,
        said: None,
        error: String::new(),
    });
    let mut rt = runtime(state);
    assert!(words(&rt).contains("Bo Diddley isn’t in #general, so they won’t be notified."));
    press(&mut rt, "add-outside");
    assert!(rt.state.replies.outside.as_ref().unwrap().adding);
    rt.update(|s, _| {
        s.receive(&Update::Channel(ChannelUpdate::Added {
            channel_id: "general".into(),
            result: Ok(ChannelAddSummary {
                added_account_ids: Vec::new(),
                invited_account_ids: vec!["bo".into()],
            }),
        }));
    });
    settle(&mut rt);
    assert!(words(&rt).contains("Invited Bo Diddley to #general."));
}

#[test]
fn unread_rows_and_counts_and_the_new_messages_line() {
    let mut state = signed_in();
    state.session.take_read_states(&[ReadStateSummary {
        channel_id: "design".into(),
        read_seq: 0,
        unread: 3,
        mentions: 2,
    }]);
    for (n, entry) in state.session.entries.iter_mut().enumerate() {
        entry.seq = n as u64 + 1;
    }
    state.session.reads.new_since = Some(("general".into(), 1));
    state.session.revision += 1;
    let rt = runtime(state);
    let all = words(&rt);
    assert!(all.contains("design channel, 2 mentions"), "{all}");
    assert!(all.contains("Acme Works, 2 mentions"), "{all}");
    assert!(all.contains("Mentions, 2 mentions"), "{all}");
    assert!(rt.find("new-messages").is_some());
}

#[test]
fn a_mention_opens_its_channel_at_it() {
    let mut state = signed_in();
    let mention = MessageSummary {
        message_id: "m1".into(),
        channel_id: "general".into(),
        seq: 1,
        author_account_id: "grace".into(),
        body: "@ada shipped".into(),
        created_at_micros: 0,
        deleted: false,
        notice: String::new(),
        notice_account_id: String::new(),
        reply: None,
        mentions: vec![MentionSummary {
            account_id: "me".into(),
            everyone: false,
            start: 0,
            end: 4,
        }],
        reactions: Vec::new(),
        mentions_me: true,
        edited: false,
    };
    state.session.reads.mentions.insert(
        "acme".into(),
        MentionsPage {
            messages: vec![mention],
            ..MentionsPage::default()
        },
    );
    let mut rt = runtime(state);
    press(&mut rt, "server-mentions");
    let all = words(&rt);
    assert!(all.contains("Grace Hopper in #general"), "{all}");
    press(&mut rt, "mention-m1");
    assert_eq!(rt.state.session.channel(), Some("general"));
    assert!(rt.state.server_page().is_none());
}

#[test]
fn the_toolbar_has_add_reaction_reply_and_more_and_reply_is_pressed_while_replying() {
    let mut rt = runtime(signed_in());
    hover(&mut rt, "m1");
    for key in ["react-m1", "reply-m1", "more-m1"] {
        assert!(rt.find(key).is_some(), "{key}");
    }
    assert!(rt.find("quick-0").is_none(), "no quick reactions");
    press(&mut rt, "reply-m1");
    // Replying: the strip in the composer with its switch, and the hint.
    let all = words(&rt);
    assert!(all.contains("Replying to Grace Hopper"), "{all}");
    assert!(
        all.contains("Return to send · Esc to cancel the reply"),
        "{all}"
    );
    assert!(!rt.state.replies.replying.as_ref().unwrap().silent);
    press(&mut rt, "reply-mention");
    assert!(
        rt.state.replies.replying.as_ref().unwrap().silent,
        "switched off"
    );
    assert!(words(&rt).contains("Off"));
}

#[test]
fn a_mention_is_drawn_as_the_persons_name() {
    let mut state = signed_in();
    state.session.entries[0].body = "@grace see this".into();
    state.session.entries[0].mentions = vec![MentionSummary {
        account_id: "grace".into(),
        everyone: false,
        start: 0,
        end: 6,
    }];
    state.session.revision += 1;
    let rt = runtime(state);
    let all = words(&rt);
    assert!(all.contains("@Grace Hopper see this"), "{all}");
}

#[test]
fn an_own_message_is_edited_in_place_and_up_edits_the_last() {
    let mut rt = runtime(signed_in());
    // m2 is this account's.
    hover(&mut rt, "m2");
    press(&mut rt, "more-m2");
    assert!(rt.find("menu-edit").is_some());
    assert!(rt.find("menu-delete").is_some());
    press(&mut rt, "menu-edit");
    assert!(rt.find("edit-m2").is_some(), "the body is a field");
    assert!(words(&rt).contains("Esc to cancel · Return to save"));
    key(&mut rt, Key::Escape);
    assert!(rt.find("edit-m2").is_none());

    // Up in the empty composer edits this account's newest message.
    press(&mut rt, "composer");
    key(&mut rt, Key::Up);
    assert_eq!(
        rt.state.replies.editing.as_ref().map(|(id, _)| id.as_str()),
        Some("m2")
    );
}

#[test]
fn another_s_message_offers_no_edit_and_deleting_asks_first() {
    let mut rt = runtime(signed_in());
    hover(&mut rt, "m1");
    press(&mut rt, "more-m1");
    assert!(rt.find("menu-edit").is_none(), "not this account's");
    assert!(rt.find("menu-copy").is_some());
    // Not a moderator in the fixture: no delete of another's.
    assert!(rt.find("menu-delete").is_none());
    key(&mut rt, Key::Escape);

    hover(&mut rt, "m2");
    press(&mut rt, "more-m2");
    press(&mut rt, "menu-delete");
    assert!(words(&rt).contains("Delete this message?"));
    press(&mut rt, "delete-cancel");
    assert!(rt.find("delete-message").is_none());
}

#[test]
fn an_edited_message_says_so() {
    let mut state = signed_in();
    state.session.entries[1].edited = true;
    state.session.revision += 1;
    let rt = runtime(state);
    assert!(rt.find("message-m2/edited").is_some() || words(&rt).contains("(edited)"));
}

#[test]
fn a_refusal_shows_under_the_edit_it_names() {
    let mut state = signed_in();
    state.session.reads.editing = Some(("m2".into(), "was".into()));
    state.session.reads.editing_command = Some("edit1".into());
    let refused = |command: &str| {
        Update::Gateway(AppEvent::Error {
            code: "PERMISSION_DENIED".into(),
            message: "not yours".into(),
            command_id: command.into(),
        })
    };
    // Another command's refusal, a reaction's say, is not the edit's.
    state.receive(&refused("react1"));
    assert!(state.session.reads.refused.is_none());
    state.receive(&refused("edit1"));
    let rt = runtime(state);
    assert!(words(&rt).contains("Not saved: not yours"));
}
