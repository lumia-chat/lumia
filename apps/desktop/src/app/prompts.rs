//! Linking by a prompt (identity spec 4.2.1), this computer as the device that holds the
//! identity: another device of the account asks to be linked, and a prompt over the window
//! asks the person, one at a time and the oldest first. "Continue" answers it, both screens
//! then show the same six digits, and "They match" approves the new device and hands it its
//! code; "Not me" and "They don’t match" decline it. What the prompt says at each stage is
//! here ([`title`], [`line`]), with when the sessions are asked about: at launch and
//! sign-in once this computer is set up, when the window comes in front, on every account
//! event, and every two seconds ([`due`]) while a prompt is open.
//!
//! "They match" approves without this computer's own unlock first, as Approve a device on
//! Devices does: Rune reaches no system prompt (Touch ID, Windows Hello, polkit), and the
//! shell has no other way to ask.

use lumia_client::app::{LinkPrompt, LinkRequestSummary};

use super::State;
use crate::session::Failure;
use crate::session::identity::{IdentityUpdate, Setup};

/// How often an open link session is asked about, in microseconds.
pub const POLL: i64 = 2_000_000;

/// How long a prompt's end ("Pixel 9 is linked.") stays before it goes, in microseconds.
pub const ENDED_FOR: i64 = 3_000_000;

/// What the prompt says once the person stopped it at "They don’t match".
pub const STOPPED: &str = "Linking was stopped.";

/// Whether a session last asked about at `asked_at` is due to be asked again at `now`.
pub fn due(asked_at: i64, now: i64) -> bool {
    now - asked_at >= POLL
}

/// The prompts this computer shows, and the calls on them.
#[derive(Debug, Default)]
pub struct Prompts {
    /// The account's link sessions that prompt here, oldest first, as last asked.
    pub list: Vec<LinkPrompt>,
    /// The prompt on screen, by session id: it stays while it is listed.
    pub shown: Option<String>,
    /// A question about the sessions is on its way; `again` asks once more when it lands.
    pub asking: bool,
    pub again: bool,
    /// When the sessions were last asked about, in microseconds.
    pub asked_at: i64,
    /// An answer, an approval or a decline is on its way.
    pub busy: bool,
    pub error: String,
    /// How the prompt on screen ended, while it stays.
    pub ended: Option<Ended>,
    /// What the prompt's end will say once the call made for it lands.
    closing: Option<String>,
}

/// A prompt's end, shown in its place for [`ENDED_FOR`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ended {
    pub words: String,
    /// When it goes, in microseconds.
    pub until: i64,
}

impl Prompts {
    /// The prompt on screen: the one shown already while it is listed, else the oldest.
    pub fn current(&self) -> Option<&LinkPrompt> {
        self.shown
            .as_ref()
            .and_then(|id| self.list.iter().find(|p| p.session_id == *id))
            .or_else(|| self.list.first())
    }

    /// Whether anything is over the window: a prompt, or how one ended.
    pub fn open(&self) -> bool {
        self.ended.is_some() || !self.list.is_empty()
    }
}

/// The device asking, at the start of a sentence: its label, "A browser on <origin>", or
/// "A new device".
fn subject(request: &LinkRequestSummary) -> String {
    let label = request.label.trim();
    match (request.kind.as_str(), label.is_empty()) {
        ("web", _) => format!("A browser on {}", request.web_origin),
        (_, false) => label.to_string(),
        (_, true) => "A new device".into(),
    }
}

/// The device asking, inside a sentence: its label, "the browser on <origin>", or "the new
/// device".
fn object(request: &LinkRequestSummary) -> String {
    let label = request.label.trim();
    match (request.kind.as_str(), label.is_empty()) {
        ("web", _) => format!("the browser on {}", request.web_origin),
        (_, false) => label.to_string(),
        (_, true) => "the new device".into(),
    }
}

/// The prompt's title at its stage.
pub fn title(prompt: &LinkPrompt) -> String {
    let request = &prompt.request;
    match prompt.stage.as_str() {
        "waiting" => format!("Waiting for {}…", object(request)),
        "compare" => format!("Does {} show this number?", object(request)),
        _ => format!("{} wants your private messages", subject(request)),
    }
}

/// The line under the title, where the stage has one.
pub fn line(prompt: &LinkPrompt) -> Option<&'static str> {
    match prompt.stage.as_str() {
        "new" => Some("Only continue if you’re linking a device of yours right now."),
        "refused" => Some("This request can’t be trusted. Don’t approve it."),
        _ => None,
    }
}

/// What the prompt says once the device asking is linked: "Pixel 9 is linked."
pub fn linked(prompt: &LinkPrompt) -> String {
    let mut name = object(&prompt.request);
    if let Some(rest) = name.strip_prefix("the ") {
        name = format!("The {rest}");
    }
    format!("{name} is linked.")
}

/// Why a call on a prompt was refused.
fn refusal(failure: &Failure) -> String {
    let reason = failure.details.get("reason").map(String::as_str);
    match (failure.code.as_str(), reason) {
        ("FAILED_PRECONDITION", Some("answered")) => {
            "Another of your devices answered it first.".into()
        }
        // A device past the roster's twenty (identity spec 11).
        (_, Some("too_many_devices")) => {
            "Your identity already has as many devices as it can hold. Revoke one first.".into()
        }
        ("NOT_FOUND", _) => "That request has ended.".into(),
        _ => failure.message.clone(),
    }
}

impl State {
    /// Asks which link sessions prompt here, unless a question is on its way already (then
    /// once more when it lands). Only on a computer set up with the account's identity.
    pub fn ask_prompts(&mut self, now: i64) {
        if !(self.session.signed_in && self.session.private_ready()) {
            return;
        }
        let prompts = &mut self.prompts;
        if prompts.asking {
            prompts.again = true;
            return;
        }
        prompts.asking = true;
        prompts.asked_at = now;
        self.session.link_prompts();
    }

    /// An account event arrived: a link session may have moved, this computer's or one
    /// that prompts here.
    pub(super) fn account_event(&mut self) {
        let now = chrono::Utc::now().timestamp_micros();
        self.ask_link_progress(now);
        self.ask_prompts(now);
    }

    /// "Continue": answers the prompt on screen.
    pub fn answer_prompt(&mut self) {
        let Some(id) = self.prompts.current().map(|p| p.session_id.clone()) else {
            return;
        };
        if self.prompts.busy {
            return;
        }
        self.calling(id.clone(), None);
        self.session.link_prompt_answer(&id);
    }

    /// "They match": approves the device asking and hands it its code.
    pub fn confirm_prompt(&mut self) {
        let Some(prompt) = self.prompts.current() else {
            return;
        };
        if self.prompts.busy || prompt.stage != "compare" {
            return;
        }
        let (id, words) = (prompt.session_id.clone(), linked(prompt));
        self.calling(id.clone(), Some(words));
        self.session.link_prompt_confirm(&id);
    }

    /// "Not me": declines the prompt on screen, which goes.
    pub fn decline_prompt(&mut self) {
        self.declining(None);
    }

    /// "They don’t match": declines it, saying so.
    pub fn mismatch_prompt(&mut self) {
        self.declining(Some(STOPPED.into()));
    }

    fn declining(&mut self, closing: Option<String>) {
        let Some(id) = self.prompts.current().map(|p| p.session_id.clone()) else {
            return;
        };
        if self.prompts.busy {
            return;
        }
        self.calling(id.clone(), closing);
        self.session.link_prompt_decline(&id);
    }

    fn calling(&mut self, id: String, closing: Option<String>) {
        let prompts = &mut self.prompts;
        prompts.shown = Some(id);
        prompts.busy = true;
        prompts.error.clear();
        prompts.closing = closing;
    }

    /// Puts away how the last prompt ended, before it goes by itself.
    pub fn close_ended(&mut self) {
        self.prompts.ended = None;
    }

    /// What the passing of time does to the prompts: an end goes after [`ENDED_FOR`], and
    /// an open prompt's session is asked about every [`POLL`]. True when that changed
    /// what is on screen.
    pub(super) fn tick_prompts(&mut self, now: i64) -> bool {
        let link = &self.account.link;
        if link.prompted.followed() && due(link.asked_at, now) {
            self.ask_link_progress(now);
        }
        if !self.prompts.list.is_empty() && due(self.prompts.asked_at, now) {
            self.ask_prompts(now);
        }
        let gone = self.prompts.ended.as_ref().is_some_and(|e| now >= e.until);
        if gone {
            self.prompts.ended = None;
        }
        gone
    }

    /// When the window next has something to do for linking by a prompt, in microseconds
    /// from `now`: ask about a session, or put a prompt's end away.
    pub fn prompt_wake(&self, now: i64) -> Option<i64> {
        let link = &self.account.link;
        let asks = [
            link.prompted.followed().then_some(link.asked_at + POLL),
            (!self.prompts.list.is_empty()).then_some(self.prompts.asked_at + POLL),
            self.prompts.ended.as_ref().map(|e| e.until),
        ];
        asks.into_iter()
            .flatten()
            .min()
            .map(|at| at.saturating_sub(now).clamp(0, POLL))
    }

    /// The prompts' part of an identity update; true when it was theirs.
    pub(super) fn react_prompts(&mut self, update: &IdentityUpdate) -> bool {
        let now = chrono::Utc::now().timestamp_micros();
        match update {
            // Set up at launch or sign-in: what is waiting for this computer prompts now.
            IdentityUpdate::SetUp {
                setup: Setup::Ready,
                ..
            } => {
                self.ask_prompts(now);
                return false;
            }
            IdentityUpdate::Prompts(answer) => {
                let prompts = &mut self.prompts;
                prompts.asking = false;
                // Not answered: asked again in two seconds while a prompt is open.
                if let Ok(list) = answer {
                    let arriving = prompts.list.is_empty() && !list.is_empty();
                    prompts.list.clone_from(list);
                    // A prompt whose session went (approved, declined, expired, answered
                    // elsewhere) closes.
                    let listed = |id: &String| list.iter().any(|p| p.session_id == *id);
                    if !prompts.shown.as_ref().is_some_and(listed) {
                        prompts.busy = false;
                        prompts.error.clear();
                    }
                    prompts.shown = prompts.current().map(|p| p.session_id.clone());
                    if arriving {
                        // The prompt takes the window: a popover open under it goes.
                        self.popover = None;
                    }
                }
                if std::mem::take(&mut self.prompts.again) {
                    self.ask_prompts(now);
                }
            }
            IdentityUpdate::PromptAnswered(answer)
            | IdentityUpdate::PromptConfirmed(answer)
            | IdentityUpdate::PromptDeclined(answer) => {
                let prompts = &mut self.prompts;
                prompts.busy = false;
                let closing = prompts.closing.take();
                match answer {
                    Ok(()) => {
                        if let Some(words) = closing {
                            prompts.ended = Some(Ended {
                                words,
                                until: now + ENDED_FOR,
                            });
                        }
                    }
                    Err(failure) => prompts.error = refusal(failure),
                }
                self.ask_prompts(now);
            }
            _ => return false,
        }
        true
    }
}
