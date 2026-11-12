//! What is worth a system notification (architecture 15.5): the messages an instance would
//! push to a device with no live session, which are a personal conversation's messages and
//! a server message that mentions this account, and nothing else. A device with a live
//! session gets no push, so the shell decides the same thing itself from what the gateway
//! delivers. The words are the ones the core gives a push on the phones
//! (`App::notification_content`): who wrote, and the message.
//!
//! Built: personal conversations. Not built: mentions. The core's `MessageSummary` does not
//! say whom a server message mentions, so the shell cannot tell one from any other message,
//! and it does not notify for every message.

use lumia_client::app::App;

use super::{Session, Update};

/// The longest body a notice carries; the system shows a few lines of it.
const BODY_LIMIT: usize = 240;

/// A message to tell the person about while the app is not in front.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Notice {
    /// The conversation to open when it is clicked.
    pub channel_id: String,
    /// The message's sequence number in it.
    pub seq: u64,
    /// Who wrote.
    pub title: String,
    /// What they wrote.
    pub body: String,
}

/// The answer to [`Session::look_for_notice`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NoticeUpdate {
    pub channel_id: String,
    /// The sequence number asked about.
    pub seq: u64,
    /// `None` when nothing at or after `seq` is a message from someone else: an edit, a
    /// reaction, a delete, or this account's own message from another device.
    pub notice: Option<Notice>,
}

impl Session {
    /// Asks whether the operation that landed at `seq` of a personal conversation is a
    /// message from someone else; the answer is [`Update::Notice`]. The gateway's event says
    /// only that an operation landed, so the conversation is read back from the local store.
    pub fn look_for_notice(&self, channel_id: &str, seq: u64) {
        let Some(app) = self.client() else { return };
        let channel_id = channel_id.to_string();
        self.task_local(move || async move {
            let notice = dm_notice(&app, &channel_id, seq).await;
            Update::Notice(NoticeUpdate {
                channel_id,
                seq,
                notice,
            })
        });
    }
}

/// The newest message from someone else at or after `seq`.
async fn dm_notice(app: &App, channel_id: &str, seq: u64) -> Option<Notice> {
    let log = app.dm_messages(channel_id).await.ok()?;
    let message = log
        .messages
        .iter()
        .rev()
        .find(|m| m.seq >= seq && !m.mine && !m.deleted)?;
    // The sender's name as the core words a push: display name, else handle, else "Someone".
    let sender = if message.sender_account_id.is_empty() {
        None
    } else {
        app.account(&message.sender_account_id).await.ok()
    };
    let title = sender
        .and_then(|person| {
            [person.display_name, person.handle]
                .into_iter()
                .find(|name| !name.is_empty())
        })
        .unwrap_or_else(|| "Someone".into());
    Some(Notice {
        channel_id: channel_id.to_string(),
        seq: message.seq,
        title,
        body: shorten(&message.body),
    })
}

/// `body` cut to [`BODY_LIMIT`] characters, with an ellipsis when it was longer.
pub fn shorten(body: &str) -> String {
    let body = body.trim();
    match body.char_indices().nth(BODY_LIMIT) {
        Some((end, _)) => format!("{}…", body[..end].trim_end()),
        None => body.to_string(),
    }
}
