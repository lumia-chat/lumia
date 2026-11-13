//! Making and joining servers, channels and invites (architecture 6.1, 6.2 and 6.6).
//!
//! Every call here reaches the instance and reports as a [`ServerUpdate`]. The session keeps
//! none of the answers: they belong to the form that asked (`app::server`). What it does
//! with them is ask for the snapshot again once a server or a channel was made or an invite
//! accepted, so the rail and the sidebar list it; the snapshot arrives after the answer.
//!
//! An invite is looked up and accepted on the open instance, whatever host its link names:
//! following a link to another instance (architecture 6.1's join flow) is not built.
//!
//! A server's settings (architecture 6.7) are the exception to keeping nothing: a change
//! answers with the account's snapshot as it then is, which the session takes at once, so
//! the rail and the sidebar show the new name and picture before the account log says so.

use std::path::Path;

use lumia_client::app::{
    ChannelSummary, InvitePreviewSummary, InviteSummary, ServerSummary, SnapshotSummary,
};

use super::avatars::read_picture;
use super::{Failure, Session, Update};

/// Which of a server's settings a call changed, for the card that asked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Setting {
    Name,
    Retention,
    Picture,
}

/// What a finished server-management call reports.
#[derive(Debug, Clone)]
pub enum ServerUpdate {
    /// A server this account now owns.
    ServerCreated(Result<ServerSummary, Failure>),
    /// A text channel made in `server_id`.
    ChannelCreated {
        server_id: String,
        channel: Result<ChannelSummary, Failure>,
    },
    /// What an invite link or code leads to, before joining.
    InvitePreview(Result<InvitePreviewSummary, Failure>),
    /// An invite accepted: the id of the server this account joined.
    InviteAccepted(Result<String, Failure>),
    /// An invite link made for `server_id`.
    InviteCreated {
        server_id: String,
        invite: Result<InviteSummary, Failure>,
    },
    /// A one-use invite mailed to an address.
    InviteEmailed {
        server_id: String,
        invite: Result<InviteSummary, Failure>,
    },
    /// The server's invites, newest first.
    Invites {
        server_id: String,
        invites: Result<Vec<InviteSummary>, Failure>,
    },
    /// One of a server's settings changed: the account's snapshot as it then is, or why
    /// not.
    SettingSaved {
        server_id: String,
        setting: Setting,
        result: Result<SnapshotSummary, Failure>,
    },
    /// The invite `code` of `server_id` revoked, or why not.
    InviteRevoked {
        server_id: String,
        code: String,
        result: Result<(), Failure>,
    },
}

impl Session {
    /// Creates a server this account owns ([`ServerUpdate::ServerCreated`]).
    pub fn create_server(&self, name: &str) {
        let Some(app) = self.client() else { return };
        let name = name.trim().to_string();
        self.task(async move {
            let server = app.create_server(&name).await;
            Update::Server(ServerUpdate::ServerCreated(server.map_err(Failure::from)))
        });
    }

    /// Creates a text channel in a server, private when asked
    /// ([`ServerUpdate::ChannelCreated`]).
    pub fn create_channel(&self, server_id: &str, name: &str, private: bool) {
        let Some(app) = self.client() else { return };
        let (server_id, name) = (server_id.to_string(), name.trim().to_string());
        self.task(async move {
            let channel = app.create_channel(&server_id, &name, private).await;
            Update::Server(ServerUpdate::ChannelCreated {
                server_id,
                channel: channel.map_err(Failure::from),
            })
        });
    }

    /// Asks what an invite link or code leads to ([`ServerUpdate::InvitePreview`]). Nothing
    /// about this account is sent.
    pub fn invite_preview(&self, link_or_code: &str) {
        let Some(app) = self.client() else { return };
        let text = link_or_code.trim().to_string();
        self.task(async move {
            let preview = app.invite_preview(&text).await;
            Update::Server(ServerUpdate::InvitePreview(preview.map_err(Failure::from)))
        });
    }

    /// Joins the server an invite names ([`ServerUpdate::InviteAccepted`]).
    pub fn accept_invite(&self, link_or_code: &str) {
        let Some(app) = self.client() else { return };
        let text = link_or_code.trim().to_string();
        self.task(async move {
            let joined = app.accept_invite(&text).await;
            Update::Server(ServerUpdate::InviteAccepted(joined.map_err(Failure::from)))
        });
    }

    /// Makes an invite link for a server ([`ServerUpdate::InviteCreated`]): `max_uses` of 0
    /// is unlimited, and no `expires_in_hours` never expires.
    pub fn create_invite(&self, server_id: &str, max_uses: u32, expires_in_hours: Option<u32>) {
        let Some(app) = self.client() else { return };
        let server_id = server_id.to_string();
        self.task(async move {
            let invite = app
                .create_invite(&server_id, max_uses, expires_in_hours)
                .await;
            Update::Server(ServerUpdate::InviteCreated {
                server_id,
                invite: invite.map_err(Failure::from),
            })
        });
    }

    /// Mails a one-use invite, good for a week, to an address
    /// ([`ServerUpdate::InviteEmailed`]).
    pub fn email_invite(&self, server_id: &str, email: &str) {
        let Some(app) = self.client() else { return };
        let (server_id, email) = (server_id.to_string(), email.trim().to_string());
        self.task(async move {
            let invite = app.email_invite(&server_id, &email).await;
            Update::Server(ServerUpdate::InviteEmailed {
                server_id,
                invite: invite.map_err(Failure::from),
            })
        });
    }

    /// Asks for a server's invites ([`ServerUpdate::Invites`]).
    pub fn invites(&self, server_id: &str) {
        let Some(app) = self.client() else { return };
        let server_id = server_id.to_string();
        self.task(async move {
            let invites = app.invites(&server_id).await;
            Update::Server(ServerUpdate::Invites {
                server_id,
                invites: invites.map_err(Failure::from),
            })
        });
    }

    /// Revokes an invite of a server ([`ServerUpdate::InviteRevoked`]).
    pub fn revoke_invite(&self, server_id: &str, code: &str) {
        let Some(app) = self.client() else { return };
        let (server_id, code) = (server_id.to_string(), code.to_string());
        self.task(async move {
            let result = app.revoke_invite(&code).await;
            Update::Server(ServerUpdate::InviteRevoked {
                server_id,
                code,
                result: result.map_err(Failure::from),
            })
        });
    }

    /// Renames a server ([`ServerUpdate::SettingSaved`]).
    pub fn rename_server(&self, server_id: &str, name: &str) {
        let Some(app) = self.client() else { return };
        let (server_id, name) = (server_id.to_string(), name.trim().to_string());
        self.task(async move {
            let result = app.update_server(&server_id, Some(&name), None).await;
            Update::Server(ServerUpdate::SettingSaved {
                server_id,
                setting: Setting::Name,
                result: result.map_err(Failure::from),
            })
        });
    }

    /// Sets how long a server keeps its messages, in days; 0 for ever
    /// ([`ServerUpdate::SettingSaved`]).
    pub fn set_server_retention(&self, server_id: &str, days: u32) {
        let Some(app) = self.client() else { return };
        let server_id = server_id.to_string();
        self.task(async move {
            let result = app.update_server(&server_id, None, Some(days)).await;
            Update::Server(ServerUpdate::SettingSaved {
                server_id,
                setting: Setting::Retention,
                result: result.map_err(Failure::from),
            })
        });
    }

    /// Sends the file at `path` to be a server's picture ([`ServerUpdate::SettingSaved`]).
    pub fn set_server_icon(&self, server_id: &str, path: &Path) {
        let Some(app) = self.client() else { return };
        let (server_id, path) = (server_id.to_string(), path.to_path_buf());
        self.task(async move {
            let result = match read_picture(&path) {
                Ok((bytes, media_type)) => app
                    .set_server_icon(&server_id, bytes, media_type)
                    .await
                    .map_err(Failure::from),
                Err(refusal) => Err(refusal),
            };
            Update::Server(ServerUpdate::SettingSaved {
                server_id,
                setting: Setting::Picture,
                result,
            })
        });
    }

    /// Takes a server's picture away ([`ServerUpdate::SettingSaved`]).
    pub fn remove_server_icon(&self, server_id: &str) {
        let Some(app) = self.client() else { return };
        let server_id = server_id.to_string();
        self.task(async move {
            let result = app.remove_server_icon(&server_id).await;
            Update::Server(ServerUpdate::SettingSaved {
                server_id,
                setting: Setting::Picture,
                result: result.map_err(Failure::from),
            })
        });
    }

    pub(super) fn apply_server(&mut self, update: &ServerUpdate) {
        match update {
            ServerUpdate::SettingSaved {
                result: Ok(snapshot),
                ..
            } => {
                self.snapshot = Some(snapshot.clone());
                self.want_pictures();
            }
            // The account now has something the snapshot does not list yet.
            ServerUpdate::ServerCreated(Ok(_))
            | ServerUpdate::ChannelCreated { channel: Ok(_), .. }
            | ServerUpdate::InviteAccepted(Ok(_)) => self.load(),
            ServerUpdate::ServerCreated(Err(_))
            | ServerUpdate::ChannelCreated { .. }
            | ServerUpdate::InviteAccepted(Err(_))
            | ServerUpdate::InvitePreview(_)
            | ServerUpdate::InviteCreated { .. }
            | ServerUpdate::InviteEmailed { .. }
            | ServerUpdate::Invites { .. }
            | ServerUpdate::InviteRevoked { .. }
            | ServerUpdate::SettingSaved { .. } => {}
        }
    }
}
