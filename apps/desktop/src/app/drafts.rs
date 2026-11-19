//! What is typed in a conversation's composer and not sent (#23, architecture 13,
//! "Local-first"): each conversation keeps its own, the words and the reply they answer,
//! in the core's local store (`App::draft`, `App::set_draft`), so another channel, a quit or
//! a crash loses none of it. This is the window's side: which conversation the composer's
//! words belong to, and when they are read and kept.
//!
//! They are read when a conversation comes under the composer, at a launch too. They are
//! kept once they have rested (the composer has no hook for an edit, so [`State::tick`]
//! looks), and at once when the conversation leaves, the window goes behind another or
//! closes, and the app quits. A field emptied keeps nothing, which clears what was kept.
//! Words still being composed by an input method are not kept until they are settled. The
//! files waiting over the composer are not kept. Nothing here is sent to the instance, and
//! signing out forgets every draft in the core.
//!
//! A send lets the words go only once it has returned without an error, from the field and
//! from what is kept alike. In a server's channel or server DM that is at once: the core
//! has the message in its outbox when `send_message…` returns, and cleared the kept draft in
//! the same write; the field is emptied then, in the same call, so nothing rested can write
//! the sent words back. In a personal conversation the engine answers later
//! (`DmUpdate::Sent`): until it does the words stay in the field (kept as any others, and
//! not sent twice: `State::can_send` waits), which is [`Leaving`]. Queued, they go from the
//! field and from what is kept. Refused, they are where they were: in the field, or, the
//! conversation having left the composer meanwhile, in its kept draft, and what is said of
//! the failure says where ([`kept_elsewhere`]).
//!
//! The write is made on the window's thread, through the core's second connection to the
//! instance's store, and so waits for whoever holds the file's write lock (the store is in
//! WAL mode: the read on opening a conversation waits for nobody). Measured on this Mac
//! with a release build of the core's store, 2026-11-19: with messages arriving as they are
//! sent a write took 0.4 ms, 4 ms at most; while the gateway's connection applied a catch-up
//! in pages of 200 events (2 to 25 ms a transaction) with a request between pages, 0.1 to
//! 0.4 ms at the median, 22 ms at the 99th percentile, and once in some five hundred writes
//! 190 ms, SQLite's busy handler sleeping in growing steps while it waited. Nothing holds a
//! write transaction near the connection's ten-second timeout. So it stays here: at most
//! a frame or two late, after the keys have rested, while a catch-up runs. Since then the
//! core's writes (`App::write`) try the lock a millisecond apart and do not hold the
//! client's own lock while they wait, so the long sleeps are gone and nothing else this
//! thread asks of the core stands behind a draft's write; that has not been measured again.

use lumia_client::app::QuoteSummary;

use super::{Route, State};
use crate::session::dms::DmUpdate;

/// How long the composer rests before what it holds is kept: a crash loses that much typing
/// at most, and a run of keys is one write.
pub const KEEP_AFTER_MICROS: i64 = 400_000;

/// A composer's words and the reply they answer.
type Held = (String, Option<QuoteSummary>);

/// The draft under the composer, against what the store holds of it.
#[derive(Debug, Default)]
pub struct Drafts {
    /// The conversation the composer's words belong to; none while nothing takes words.
    of: Option<String>,
    /// What the store holds of it, as last read or kept.
    kept: Held,
    /// The composer as last looked at, and when it last changed.
    seen: Held,
    changed_at: i64,
}

/// The composer's words handed to a send that has not returned: a personal conversation's,
/// which the engine answers later. They are still the composer's until it has.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Leaving {
    /// The ticket the engine's answer names.
    pub ticket: String,
    /// The conversation they were written in.
    pub conversation: String,
    /// The words as the field held them.
    pub words: String,
}

/// What is said when a send failed in a conversation that is no longer the one under the
/// composer: why, and where the message waits (`place` is "your conversation with Ada").
/// The web's `keptElsewhere`, word for word.
pub fn kept_elsewhere(why: &str, place: &str) -> String {
    let said = why.trim();
    let stop = if said.ends_with(['.', '!', '?', '…']) {
        ""
    } else {
        "."
    };
    format!("{said}{stop} Your message is kept in {place}.")
}

impl State {
    /// Whether the conversation under the composer has a send that has not returned: its
    /// words are in the field still, and are not sent again.
    pub fn sending_here(&self) -> bool {
        self.composing_in()
            .is_some_and(|here| self.leaving.iter().any(|l| l.conversation == here))
    }

    /// The composer's words went: the field is emptied, with the reply they answered, and
    /// nothing is kept of them, now and not once the field has rested.
    pub(super) fn words_sent(&mut self) {
        self.draft.clear();
        self.draft_seen.clear();
        self.forget_typed();
        self.replies.replying = None;
        self.replies.mention = None;
        self.keep_draft();
    }

    /// The composer's words were handed to a send that answers later, under `ticket`: they
    /// stay where they are until it does.
    pub(super) fn words_handed(&mut self, ticket: String) {
        if let Some(conversation) = self.composing_in().map(str::to_string) {
            self.leaving.push(Leaving {
                ticket,
                conversation,
                words: self.draft.text.clone(),
            });
        }
    }

    /// A send handed to the engine came back ([`DmUpdate::Sent`]). Queued, its words go:
    /// from the field when their conversation is still under the composer (what was typed
    /// after them stays), and from what is kept of that conversation when it is these
    /// words. Refused, they stay: in the field, or in the kept draft of the conversation
    /// they were written in when another is under the composer, and what the session says
    /// of the failure says where.
    pub(super) fn words_answered(&mut self, update: &DmUpdate) {
        let DmUpdate::Sent { pending_id, result } = update else {
            return;
        };
        let Some(at) = self.leaving.iter().position(|l| l.ticket == *pending_id) else {
            return;
        };
        let left = self.leaving.remove(at);
        let here = self.composing_in() == Some(left.conversation.as_str());
        if let Err(failure) = result {
            if !here {
                let place = self
                    .session
                    .channel_summary(&left.conversation)
                    .map(|channel| self.session.conversation_name(channel));
                let why = format!("Not sent: {}", failure.message);
                self.session.last_error = Some(match place {
                    Some(name) => kept_elsewhere(&why, &format!("your conversation with {name}")),
                    None => why,
                });
            }
            return;
        }
        if here {
            if self.draft.text == left.words {
                self.words_sent();
            } else if let Some(after) = self.draft.text.strip_prefix(left.words.as_str()) {
                // More was typed after Return and before the engine answered: that stays.
                let after = after.trim_start().to_string();
                self.draft.replace_text(after.clone());
                self.draft.set_caret(after.len());
                self.replies.replying = None;
                self.keep_draft();
            }
            self.sent_now = true;
        }
        // What is kept of that conversation, when it is these words: kept while the send
        // was out (the field had rested, or the conversation left the composer with them).
        let kept = self.session.draft(&left.conversation);
        if kept.is_some_and(|draft| draft.body.trim() == left.words.trim()) {
            self.session.keep_draft(&left.conversation, "", None);
            if self.drafts.of.as_deref() == Some(left.conversation.as_str()) {
                self.drafts.kept = Held::default();
            }
        }
    }

    /// The conversation the composer writes to: the one on screen, once signed in, unless
    /// it is a channel read before joining, which has "Join" where the composer would be.
    fn composing_in(&self) -> Option<&str> {
        if self.route != Route::Shell || self.session.previewing.is_some() {
            return None;
        }
        self.session.channel().or(self.session.dm())
    }

    fn composer_holds(&self, held: &Held) -> bool {
        self.draft.text == held.0 && self.replies.replying == held.1
    }

    /// Keeps the composer's words and reply as its conversation's draft, when they are not
    /// what is kept already. Empty words keep nothing.
    pub fn keep_draft(&mut self) {
        let Some(conversation) = &self.drafts.of else {
            return;
        };
        if self.draft.is_composing() || self.composer_holds(&self.drafts.kept) {
            return;
        }
        let held = (self.draft.text.clone(), self.replies.replying.clone());
        self.session
            .keep_draft(conversation, &held.0, held.1.as_ref());
        self.drafts.kept = held;
    }

    /// The conversation is leaving the composer: its draft is kept and the field emptied.
    /// Called before the reply it answers is forgotten.
    pub(crate) fn leave_draft(&mut self) {
        self.keep_draft();
        self.forget_draft();
    }

    /// Empties the composer with nothing kept: on the way out of the account, whose drafts
    /// the core forgets.
    pub(crate) fn forget_draft(&mut self) {
        self.drafts = Drafts::default();
        self.draft.clear();
        self.draft_seen.clear();
    }

    /// Looks at the composer: puts a conversation's kept draft in it when another comes
    /// under it, and keeps what was typed once it has rested. True when it changed what is
    /// on screen.
    pub(super) fn tick_draft(&mut self, now_micros: i64) -> bool {
        let open = self.composing_in();
        if open != self.drafts.of.as_deref() {
            let open = open.map(str::to_string);
            // Words typed since the field was emptied, before this look: they are this
            // conversation's, and take the place of what was kept of it.
            let typed_here = self.drafts.of.is_none() && !self.draft.text.is_empty();
            // One that left without saying so still has its words in the field.
            self.keep_draft();
            let kept = open
                .as_deref()
                .and_then(|conversation| self.session.draft(conversation))
                .map(|draft| (draft.body, draft.reply))
                .unwrap_or_default();
            let changed = !typed_here && !self.composer_holds(&kept);
            if changed {
                if self.draft.text != kept.0 {
                    self.draft.replace_text(kept.0.clone());
                    self.draft.set_caret(kept.0.len());
                }
                self.replies.replying.clone_from(&kept.1);
            }
            if !typed_here {
                // Read, not typed: nobody is told this person is typing.
                self.draft_seen.clone_from(&kept.0);
            }
            self.drafts = Drafts {
                of: open,
                seen: (self.draft.text.clone(), self.replies.replying.clone()),
                kept,
                changed_at: now_micros,
            };
            return changed;
        }
        if !self.composer_holds(&self.drafts.seen) {
            self.drafts.seen = (self.draft.text.clone(), self.replies.replying.clone());
            self.drafts.changed_at = now_micros;
        }
        if now_micros - self.drafts.changed_at >= KEEP_AFTER_MICROS {
            self.keep_draft();
        }
        false
    }

    /// When the window should look at the composer again to keep what was typed, in
    /// microseconds from `now_micros`: nothing else wakes it once the keys stop.
    pub fn draft_wake(&self, now_micros: i64) -> Option<i64> {
        // Words an input method is still composing are not kept, and ask for no wake.
        let owed = self.drafts.of.is_some()
            && !self.draft.is_composing()
            && !self.composer_holds(&self.drafts.kept);
        owed.then(|| (self.drafts.changed_at + KEEP_AFTER_MICROS - now_micros).max(0))
    }
}
