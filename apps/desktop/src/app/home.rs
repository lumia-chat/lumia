//! Home's own state: which personal conversation is open, the requests being answered and
//! the new message being started, and the reactions to what `session::dms` reports.

use rune::TextEdit;

use super::{HomeView, Place, State};
use crate::session::dms::{Answer, DmUpdate};

/// What the Home screens hold between frames.
#[derive(Debug, Default)]
pub struct Home {
    /// New message: the name or handle being looked for.
    pub query: TextEdit,
    /// New message: what the last press came to (a request sent, or why not).
    pub note: String,
    /// New message: the person whose conversation is being opened or asked for.
    pub starting: Option<String>,
    /// Requests: the request being answered.
    pub answering: Option<String>,
    /// Requests: why the last answer failed.
    pub request_error: String,
    /// The conversation: a rejoin is running.
    pub rejoining: bool,
    /// The conversation: why the last rejoin failed.
    pub rejoin_error: String,
}

impl State {
    /// Puts a personal conversation on screen, at Home.
    pub fn open_dm(&mut self, channel_id: &str) {
        if self.session.dm() != Some(channel_id) {
            self.draft.clear();
            self.home.rejoining = false;
            self.home.rejoin_error.clear();
        }
        self.go_home(HomeView::Conversations);
        self.session.open_dm(channel_id);
    }

    /// New message: opens the conversation with someone, or asks them for one.
    pub fn start_dm(&mut self, account_id: &str) {
        if self.home.starting.is_some() {
            return;
        }
        self.home.starting = Some(account_id.to_string());
        self.home.note.clear();
        self.session.dm_start(account_id);
    }

    /// Requests: accepts, declines or blocks one.
    pub fn answer_request(&mut self, request_id: &str, answer: Answer) {
        if self.home.answering.is_some() {
            return;
        }
        self.home.answering = Some(request_id.to_string());
        self.home.request_error.clear();
        self.session.dm_answer(request_id, answer);
    }

    /// The conversation: rejoins the one on screen from the instance's group info.
    pub fn rejoin_dm(&mut self) {
        let Some(channel_id) = self.session.dm().map(str::to_string) else {
            return;
        };
        if self.home.rejoining {
            return;
        }
        self.home.rejoining = true;
        self.home.rejoin_error.clear();
        self.session.dm_rejoin(&channel_id);
    }

    /// Whether the shell is still where an answer about Home belongs. An answer that comes
    /// back after the person went to a server does not pull them out of it.
    fn at_home(&self, view: HomeView) -> bool {
        self.place == Place::Home && self.home_view == view
    }

    pub(super) fn react_dm(&mut self, update: &DmUpdate) {
        match update {
            DmUpdate::Started { account_id, result } => {
                if self.home.starting.as_deref() != Some(account_id.as_str()) {
                    return;
                }
                self.home.starting = None;
                match result {
                    Ok(Some(channel_id)) => {
                        if self.at_home(HomeView::NewMessage) {
                            self.home.query.clear();
                            self.open_dm(channel_id);
                        }
                    }
                    Ok(None) => {
                        self.home.note = format!(
                            "Request sent to {}. The conversation opens when they accept.",
                            self.session.name_of(account_id)
                        );
                    }
                    Err(failure) => {
                        self.home.note = if failure.code == "ALREADY_EXISTS" {
                            "You already asked; waiting for their answer.".into()
                        } else {
                            failure.message.clone()
                        };
                    }
                }
            }
            DmUpdate::Answered {
                request_id,
                answer,
                result,
            } => {
                if self.home.answering.as_deref() != Some(request_id.as_str()) {
                    return;
                }
                self.home.answering = None;
                match result {
                    Ok(Some(channel_id)) => {
                        if *answer == Answer::Accept && self.at_home(HomeView::Requests) {
                            self.open_dm(channel_id);
                        }
                    }
                    Ok(None) => {}
                    Err(failure) => self.home.request_error.clone_from(&failure.message),
                }
            }
            DmUpdate::Rejoined { channel_id, result } => {
                if self.session.dm() != Some(channel_id.as_str()) {
                    return;
                }
                self.home.rejoining = false;
                if let Err(failure) = result {
                    self.home.rejoin_error.clone_from(&failure.message);
                }
            }
            DmUpdate::Channels(_) | DmUpdate::Log { .. } | DmUpdate::Sent { .. } => {}
        }
    }
}
