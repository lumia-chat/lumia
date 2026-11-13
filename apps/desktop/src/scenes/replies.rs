//! Scenes of quote replies, reactions, mentions and read state (architecture 6.6), over
//! [`super::servers_sample`] with no instance behind it.

use lumia_client::app::{MessageSummary, QuoteSummary, ReactionSummary, ReadStateSummary};

use crate::app::State;
use crate::app::replies::MentionQuery;
use crate::session::Entry;
use crate::session::reads::MentionsPage;

fn entry(id: &str, seq: u64, author: &str, body: &str, ago: i64) -> Entry {
    let now = chrono::Utc::now().timestamp_micros();
    Entry {
        id: id.into(),
        seq,
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
    }
}

fn mention(account_id: &str, body: &str, words: &str) -> lumia_client::app::MentionSummary {
    let start = body.find(words).unwrap_or(0);
    lumia_client::app::MentionSummary {
        account_id: account_id.into(),
        everyone: false,
        start: u32::try_from(start).unwrap_or(0),
        end: u32::try_from(start + words.len()).unwrap_or(0),
    }
}

/// `replies`: #general with a quote reply, reactions (one of them this account's), a
/// message that mentions this account, a quote of a deleted message and the "New messages"
/// line; #design unread with a mention; a quote over the composer and the bar after a send
/// that mentioned someone not in the channel.
pub fn replies(state: &mut State) {
    super::servers_sample(state);
    state.select_channel("general");
    let asked =
        "Does the loader relocate before or after linking? I could not tell from the notes.";
    let mut quoting = entry("r2", 2, "me", "After, as far as I can see.", 30);
    quoting.reply = Some(QuoteSummary {
        message_id: "r1".into(),
        author_account_id: "grace".into(),
        text: asked.into(),
        original_deleted: false,
    });
    quoting.reactions = vec![
        ReactionSummary {
            reaction: "👍".into(),
            count: 2,
            me: true,
            account_ids: vec!["me".into(), "grace".into()],
        },
        ReactionSummary {
            reaction: "🎉".into(),
            count: 1,
            me: false,
            account_ids: vec!["grace".into()],
        },
    ];
    let named = "@ada can you check the relocation table?";
    let mut mentioning = entry("r3", 4, "grace", named, 6);
    mentioning.mentions = vec![mention("me", named, "@ada")];
    mentioning.mentions_me = true;
    let mut gone = entry("r4", 5, "grace", "Never mind that one.", 5);
    gone.reply = Some(QuoteSummary {
        message_id: "r0".into(),
        author_account_id: "grace".into(),
        text: String::new(),
        original_deleted: true,
    });
    state.session.entries = vec![
        entry("r1", 1, "grace", asked, 32),
        quoting,
        mentioning,
        gone,
    ];
    state.session.reads.new_since = Some(("general".into(), 3));
    state.session.take_read_states(&[
        ReadStateSummary {
            channel_id: "design".into(),
            read_seq: 0,
            unread: 4,
            mentions: 1,
        },
        ReadStateSummary {
            channel_id: "plots".into(),
            read_seq: 0,
            unread: 2,
            mentions: 0,
        },
    ]);
    state.replies.replying = Some(QuoteSummary {
        message_id: "r3".into(),
        author_account_id: "grace".into(),
        text: named.into(),
        original_deleted: false,
    });
    state.replies.outside = Some(crate::app::replies::Outside {
        channel_id: "general".into(),
        account_ids: vec!["bo".into()],
        adding: false,
        said: None,
        error: String::new(),
    });
    state.session.revision += 1;
}

/// `mention-list`: the @ list over the composer, Bo marked as not in the channel.
pub fn mention_list(state: &mut State) {
    replies(state);
    state.replies.replying = None;
    state.replies.outside = None;
    state.draft.replace_text("Thanks @");
    state.draft.set_caret("Thanks @".len());
    state.replies.mention = Some(MentionQuery {
        start: "Thanks ".len(),
        end: "Thanks @".len(),
        query: String::new(),
        chosen: 0,
    });
}

/// `emoji-picker`: the picker open over a message's add-reaction button.
pub fn emoji_picker(state: &mut State) {
    replies(state);
    state.replies.replying = None;
    state.replies.outside = None;
    state.open_picker("r2", "add-reaction-r2");
}

/// `mentions`: the server's Mentions page.
pub fn mentions(state: &mut State) {
    replies(state);
    state.replies.replying = None;
    state.replies.outside = None;
    let now = chrono::Utc::now().timestamp_micros();
    let message = |id: &str, channel: &str, author: &str, body: &str, ago: i64| MessageSummary {
        message_id: id.into(),
        channel_id: channel.into(),
        seq: 1,
        author_account_id: author.into(),
        body: body.into(),
        created_at_micros: now - ago * 60_000_000,
        deleted: false,
        notice: String::new(),
        notice_account_id: String::new(),
        reply: None,
        mentions: vec![mention("me", body, "@ada")],
        reactions: Vec::new(),
        mentions_me: true,
    };
    state.session.reads.mentions.insert(
        "acme".into(),
        MentionsPage {
            messages: vec![
                message(
                    "x1",
                    "general",
                    "grace",
                    "@ada can you check the relocation table?",
                    6,
                ),
                message("x2", "design", "grace", "@ada the new icons are in.", 90),
            ],
            more: false,
            loading: false,
            error: None,
        },
    );
    state.open_server_mentions();
}
