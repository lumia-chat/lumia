//! What is typed in a conversation's composer and not sent (#23, architecture 13,
//! "Local-first"): each conversation keeps its own, the words and the reply they answer,
//! in the core's local store (`App::draft`, `App::set_draft`), so another channel, a quit or
//! a crash loses none of it. This is the window's side: which conversation the composer's
//! words belong to, and when they are read and kept.
//!
//! They are read when a conversation comes under the composer, at a launch too. They are
//! kept once they have rested (the composer has no hook for an edit, so [`State::tick`]
//! looks), and at once when the conversation leaves, the window goes behind another or
//! closes, and the app quits. A send, and a field emptied, keep nothing, which clears what
//! was kept. Words still being composed by an input method are not kept until they are
//! settled. The files waiting over the composer are not kept. Nothing here is sent to the
//! instance, and signing out forgets every draft in the core.
//!
//! The write is made on the window's thread, through the core's second connection to the
//! instance's store, and so waits for whoever holds the file's write lock (the store is in
//! WAL mode: the read on opening a conversation waits for nobody). Measured on this Mac
//! with a release build of the core's store, 2026-11-19: with messages arriving as they are
//! sent a write took 0.4 ms, 4 ms at most; while the gateway's connection applied a catch-up
//! in pages of 200 events (2 to 25 ms a transaction) with a request between pages, 0.1 to
//! 0.4 ms at the median, 22 ms at the 99th percentile, and once in some five hundred writes
//! 190 ms, SQLite's busy handler sleeping in growing steps while it waits. Nothing holds a
//! write transaction near the connection's ten-second timeout. So it stays here: at most
//! a frame or two late, after the keys have rested, while a catch-up runs. Moving it to a
//! thread of the session's would not take that wait off this one as the core stands:
//! `App::set_draft` waits for the store's lock while it holds the core's own, which every
//! read this thread makes of the core takes too (the read counts, after each applied batch).
//! It would take a write in the core that does not hold that lock while it waits.

use lumia_client::app::QuoteSummary;

use super::{Route, State};

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

impl State {
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
