//! A server's own emoji (specs/emoji.md): the core's calls that upload, rename and remove
//! them, reacting with one, and what the session keeps of them.
//!
//! The session computes nothing here. Each server's live emoji, its limit and whether this
//! account may manage them are in the snapshot ([`ServerSummary::emoji`]); a call answers
//! with the snapshot as it then is, which the session takes at once, and someone else's
//! change arrives as the core's `AppEvent::EmojiChanged`, after which the kept snapshot
//! already has it ([`Session::emoji_changed`]). The refs in a message are the core's to find
//! at a send or an edit; a reaction is grouped by the emoji's id by the core. The pictures
//! are fetched as avatars are (`avatars`), by `image_url`, and dropped when the server no
//! longer lists them.

use std::path::Path;
use std::sync::Arc;

use lumia_client::app::{EmojiSummary, ServerSummary, SnapshotSummary};

use super::avatars::read_picture;
use super::{Conversation, Failure, Session, Update};

/// What a finished emoji call reports.
#[derive(Debug, Clone)]
pub enum EmojiUpdate {
    /// A file chosen for Upload emoji was read: its bytes and what it is taken to be, or why
    /// it could not be (too large to send, or unreadable).
    Read {
        path: std::path::PathBuf,
        result: Result<(Arc<[u8]>, &'static str), Failure>,
    },
    /// An emoji uploaded: the snapshot as it then is. `request` is the number the window gave
    /// the call, which names the form that made it.
    Uploaded {
        server_id: String,
        request: u64,
        result: Result<SnapshotSummary, Failure>,
    },
    Renamed {
        server_id: String,
        emoji_id: String,
        request: u64,
        result: Result<SnapshotSummary, Failure>,
    },
    Removed {
        server_id: String,
        emoji_id: String,
        request: u64,
        result: Result<SnapshotSummary, Failure>,
    },
}

impl Session {
    /// A server's live emoji as held, oldest first; none for a server not held.
    pub fn server_emoji(&self, server_id: &str) -> &[EmojiSummary] {
        self.server(server_id).map_or(&[], |s| s.emoji.as_slice())
    }

    /// The server whose emoji the conversation on screen may use: its server's for a channel
    /// or a server DM, none for a personal conversation (specs/emoji.md 1).
    pub fn emoji_server(&self) -> Option<&ServerSummary> {
        let Some(Conversation::Channel(channel_id)) = &self.conversation else {
            return None;
        };
        let server_id = &self.channel_summary(channel_id)?.server_id;
        self.server(server_id)
    }

    /// One of a server's live emoji, by id.
    pub fn emoji_in(&self, server_id: &str, emoji_id: &str) -> Option<&EmojiSummary> {
        self.server_emoji(server_id)
            .iter()
            .find(|e| e.emoji_id == emoji_id)
    }

    /// Reads a file chosen for Upload emoji ([`EmojiUpdate::Read`]), off the window's thread.
    pub fn read_emoji_file(&self, path: &Path) {
        let path = path.to_path_buf();
        self.task(async move {
            let result = read_picture(&path).map(|(bytes, media_type)| (bytes.into(), media_type));
            Update::Emoji(EmojiUpdate::Read { path, result })
        });
    }

    /// Uploads a picture as one of a server's emoji ([`EmojiUpdate::Uploaded`]).
    pub fn upload_emoji(
        &self,
        server_id: &str,
        name: &str,
        picture: Arc<[u8]>,
        media_type: &'static str,
        request: u64,
    ) {
        let Some(app) = self.client() else { return };
        let (server_id, name) = (server_id.to_string(), name.to_string());
        self.task(async move {
            let result = app
                .upload_emoji(&server_id, &name, picture.to_vec(), media_type)
                .await;
            Update::Emoji(EmojiUpdate::Uploaded {
                server_id,
                request,
                result: result.map_err(Failure::from),
            })
        });
    }

    /// Renames one of a server's emoji ([`EmojiUpdate::Renamed`]).
    pub fn rename_emoji(&self, server_id: &str, emoji_id: &str, name: &str, request: u64) {
        let Some(app) = self.client() else { return };
        let (server_id, emoji_id, name) = (
            server_id.to_string(),
            emoji_id.to_string(),
            name.to_string(),
        );
        self.task(async move {
            let result = app.rename_emoji(&server_id, &emoji_id, &name).await;
            Update::Emoji(EmojiUpdate::Renamed {
                server_id,
                emoji_id,
                request,
                result: result.map_err(Failure::from),
            })
        });
    }

    /// Removes one of a server's emoji ([`EmojiUpdate::Removed`]).
    pub fn remove_emoji(&self, server_id: &str, emoji_id: &str, request: u64) {
        let Some(app) = self.client() else { return };
        let (server_id, emoji_id) = (server_id.to_string(), emoji_id.to_string());
        self.task(async move {
            let result = app.remove_emoji(&server_id, &emoji_id).await;
            Update::Emoji(EmojiUpdate::Removed {
                server_id,
                emoji_id,
                request,
                result: result.map_err(Failure::from),
            })
        });
    }

    /// Adds this account's reaction with one of the server's emoji to a message of the
    /// channel on screen, or takes it away when it is already there (specs/emoji.md 9).
    /// The instance refuses adding one that was removed; taking one's own away works.
    pub fn react_custom(&mut self, message_id: &str, emoji_id: &str) {
        let (Some(app), Some(Conversation::Channel(channel_id))) =
            (self.client(), self.conversation.clone())
        else {
            return;
        };
        let mine = self
            .entries
            .iter()
            .find(|e| e.id == message_id)
            .and_then(|e| e.reactions.iter().find(|r| r.emoji_id == emoji_id))
            .is_some_and(|r| r.me);
        if let Err(error) = app.react_emoji(&channel_id, message_id, emoji_id, mine) {
            self.last_error = Some(error.to_string());
        }
    }

    /// The core's `AppEvent::EmojiChanged`: the kept snapshot has the server's emoji already,
    /// so they are taken from it now rather than when the snapshot asked for next arrives;
    /// the pictures follow, and what draws them is drawn again.
    pub(super) fn emoji_changed(&mut self, server_id: &str) {
        // Only from a kept snapshot that holds the server: one that does not says nothing
        // of its emoji, which is not that it has none.
        let kept = self
            .client()
            .and_then(|app| app.kept_snapshot().ok().flatten())
            .and_then(|kept| kept.servers.into_iter().find(|s| s.server_id == server_id));
        if let Some(kept) = kept {
            let emoji = kept.emoji;
            if let Some(server) = self
                .snapshot
                .as_mut()
                .and_then(|s| s.servers.iter_mut().find(|s| s.server_id == server_id))
            {
                server.emoji = emoji;
            }
        }
        // The core renames what it keeps of the reactions with them (a removed one's chip
        // reads as it was last called): the open conversation is read again from it, and
        // the pages of its history the instance gave, which the core does not keep, are read
        // again from the instance.
        if let Some(Conversation::Channel(channel_id)) = &self.conversation {
            let here = self
                .channel_summary(channel_id)
                .is_some_and(|c| c.server_id == server_id);
            self.read_entries();
            if here {
                self.read_history_again();
            }
        }
        self.want_pictures();
        self.revision += 1;
    }

    pub(super) fn apply_emoji(&mut self, update: &EmojiUpdate) {
        match update {
            EmojiUpdate::Uploaded {
                result: Ok(snapshot),
                ..
            }
            | EmojiUpdate::Renamed {
                result: Ok(snapshot),
                ..
            }
            | EmojiUpdate::Removed {
                result: Ok(snapshot),
                ..
            } => {
                self.hold_snapshot(snapshot.clone());
                self.want_pictures();
                self.revision += 1;
            }
            _ => {}
        }
    }
}
