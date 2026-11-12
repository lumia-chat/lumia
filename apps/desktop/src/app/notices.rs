//! System notifications (architecture 15.5): when a message the instance would push for
//! arrives while the window is not in front, the person is told, and a click on the notice
//! brings the window forward on that conversation. What counts is `session::notices`'
//! decision; this is when to ask, what is owed to the system, and what a click opens.
//!
//! The state only keeps what is owed ([`Notices::to_post`]); `platform` posts it, since
//! only a bundled app on macOS (and a registered one on Windows) can.

use std::collections::HashMap;

use lumia_client::app::AppEvent;

use super::{HomeView, Route, State};
use crate::session::Conversation;
use crate::session::notices::{Notice, NoticeUpdate};

/// What the window knows about being looked at, and the notices it owes the system.
#[derive(Debug, Default)]
pub struct Notices {
    /// Whether the window has the keyboard focus, as its last focus change said. Until it
    /// has had one, it is not in front.
    pub in_front: bool,
    /// Conversations being read back for a notice, with the newest sequence number asked
    /// about while the reading was out.
    looking: HashMap<String, u64>,
    /// The newest message a notice was owed for, by conversation, so one message is told
    /// once.
    told: HashMap<String, u64>,
    /// Notices to post, drained by the window's poll.
    pub to_post: Vec<Notice>,
    /// The conversation a clicked notice asked for, until the snapshot can place it.
    opening: Option<String>,
}

impl State {
    /// The window gained or lost the keyboard focus.
    pub fn window_focused(&mut self, focused: bool) {
        self.notices.in_front = focused;
        if focused {
            // Looking at the app: nothing more is owed.
            self.notices.to_post.clear();
            self.notices.looking.clear();
        }
    }

    /// Whether a message arriving now is worth telling the person about.
    fn away(&self) -> bool {
        self.route == Route::Shell && !self.notices.in_front
    }

    pub(super) fn react_gateway(&mut self, event: &AppEvent) {
        let AppEvent::DmOperation {
            channel_id, seq, ..
        } = event
        else {
            return;
        };
        if !self.away() {
            return;
        }
        match self.notices.looking.get_mut(channel_id) {
            // One reading at a time; its answer asks again for what landed meanwhile.
            Some(newest) => *newest = (*newest).max(*seq),
            None => {
                self.notices.looking.insert(channel_id.clone(), *seq);
                self.session.look_for_notice(channel_id, *seq);
            }
        }
    }

    pub(super) fn react_notice(&mut self, update: &NoticeUpdate) {
        let newest = self.notices.looking.remove(&update.channel_id);
        if !self.away() {
            return;
        }
        if let Some(notice) = &update.notice {
            let told = self.notices.told.get(&notice.channel_id).copied();
            if told.is_none_or(|seq| notice.seq > seq) {
                self.notices
                    .told
                    .insert(notice.channel_id.clone(), notice.seq);
                self.notices.to_post.push(notice.clone());
            }
        }
        // Something landed after the reading was asked for, which it may not have seen.
        if let Some(newest) = newest.filter(|seq| *seq > update.seq) {
            self.notices
                .looking
                .insert(update.channel_id.clone(), newest);
            self.session.look_for_notice(&update.channel_id, newest);
        }
    }

    /// A notice was clicked: puts its conversation on screen, now or once the snapshot
    /// that lists it has arrived.
    pub fn open_notice(&mut self, channel_id: &str) {
        self.notices.opening = Some(channel_id.to_string());
        self.settle_notice();
    }

    /// Opens the conversation a clicked notice asked for when the snapshot lists it; true
    /// when the screen changed. A conversation the account cannot see is dropped: the id
    /// came back from the system, and on Windows from a link anyone can open.
    pub fn settle_notice(&mut self) -> bool {
        let Some(channel_id) = self.notices.opening.clone() else {
            return false;
        };
        if self.route != Route::Shell {
            self.notices.opening = None;
            return false;
        }
        if self.session.snapshot.is_none() {
            return false;
        }
        self.notices.opening = None;
        let Some(server_id) = self
            .session
            .channel_summary(&channel_id)
            .map(|channel| channel.server_id.clone())
        else {
            return false;
        };
        if self
            .session
            .personal_channels()
            .any(|channel| channel.channel_id == channel_id)
        {
            self.go_home(HomeView::Conversations);
            self.session.open_conversation(Conversation::Dm(channel_id));
        } else {
            if self.server_id() != Some(server_id.as_str()) {
                self.select_server(&server_id);
            }
            self.select_channel(&channel_id);
        }
        true
    }
}
