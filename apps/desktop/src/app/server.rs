//! The forms that make and manage servers: add a server (create one, or join by invite),
//! a new channel, invite people. They hold what the person typed and chose, start the
//! session's calls, and take the answers `session::servers` reports, worded as the other
//! shells word them (the iOS app's `Screens.swift`, the web's `app.tsx`).
//!
//! A server or a channel just made is not in the snapshot when the call answers; what to
//! open is kept here until a snapshot lists it ([`State::open_made`]).

use lumia_client::app::{InvitePreviewSummary, InviteSummary};
use rune::{TextEdit, Tx};

use super::{HomeView, Place, Popover, State};
use crate::session::Failure;
use crate::session::servers::ServerUpdate;

/// What the server-management forms hold between frames.
#[derive(Debug, Default)]
pub struct ServerForms {
    pub add: AddServer,
    pub channel: NewChannel,
    pub invite: Invites,
    /// The settings page of a server (`app::server_settings`).
    pub settings: super::server_settings::ServerSettings,
    /// What was just made or joined, to put on screen once a snapshot lists it.
    opening: Option<Opening>,
}

/// Home's "Add a server": a server of your own, or one an invite leads to.
#[derive(Debug, Default)]
pub struct AddServer {
    pub name: TextEdit,
    /// Waiting for the instance to make the server.
    pub creating: bool,
    pub create_error: String,
    /// The invite link or code as pasted.
    pub invite: TextEdit,
    /// What the invite leads to, shown before joining.
    pub preview: Option<InvitePreviewSummary>,
    /// Waiting for the invite's preview or for the join.
    pub busy: bool,
    pub error: String,
}

/// The "New channel" popover.
#[derive(Debug, Default)]
pub struct NewChannel {
    pub name: TextEdit,
    pub busy: bool,
    pub error: String,
}

/// The "Invite people" popover: a new link with its limits, a mailed invite, and the
/// server's invites.
#[derive(Debug, Default)]
pub struct Invites {
    /// The server everything below belongs to.
    server_id: String,
    /// The link just made, to copy.
    pub made: Option<InviteSummary>,
    /// The server's invites, newest first.
    pub list: Vec<InviteSummary>,
    /// 0 never expires.
    pub expires_in_hours: u32,
    /// 0 is unlimited.
    pub max_uses: u32,
    pub email: TextEdit,
    /// The address the last mailed invite went to.
    pub emailed: String,
    pub error: String,
    /// Waiting for a link, a mail or a revocation.
    pub busy: bool,
    pub copied: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Opening {
    Server(String),
    Channel {
        server_id: String,
        channel_id: String,
    },
}

/// A refusal to list or make invites, as the other shells word it.
fn invite_refusal(failure: &Failure) -> String {
    match failure.code.as_str() {
        "PERMISSION_DENIED" => "You cannot invite people to this server.".into(),
        "RATE_LIMITED" => "Too many links in the last hour; try later.".into(),
        _ => failure.message.clone(),
    }
}

/// A refusal to mail an invite.
fn email_refusal(failure: &Failure) -> String {
    match failure.code.as_str() {
        "FAILED_PRECONDITION" => "This instance sends no mail; make a link instead.".into(),
        "INVALID_ARGUMENT" => "That is not an email address.".into(),
        "RATE_LIMITED" => "Too many invites in the last hour; try later.".into(),
        "PERMISSION_DENIED" => "You cannot invite people to this server.".into(),
        _ => failure.message.clone(),
    }
}

impl State {
    // ---- Add a server -----------------------------------------------------------------

    /// Creates the server named in the form; it opens once the snapshot lists it.
    pub fn create_server(&mut self) {
        let form = &mut self.server_forms.add;
        if form.creating || form.name.text.trim().is_empty() {
            return;
        }
        form.creating = true;
        form.create_error.clear();
        self.session.create_server(&form.name.text);
    }

    /// Asks what the pasted invite leads to, which shows before anything is joined.
    pub fn look_up_invite(&mut self) {
        let form = &mut self.server_forms.add;
        if form.busy || form.invite.text.trim().is_empty() {
            return;
        }
        form.busy = true;
        form.error.clear();
        form.preview = None;
        self.session.invite_preview(&form.invite.text);
    }

    /// Joins the server the previewed invite leads to; it opens once the snapshot lists it.
    pub fn join_invite(&mut self) {
        let form = &mut self.server_forms.add;
        let Some(preview) = &form.preview else { return };
        if form.busy {
            return;
        }
        form.busy = true;
        form.error.clear();
        self.session.accept_invite(&preview.code);
    }

    // ---- New channel ------------------------------------------------------------------

    /// Opens the "New channel" popover for the server on screen.
    pub fn open_new_channel(&mut self) {
        if self.server_id().is_none() {
            return;
        }
        self.server_forms.channel.error.clear();
        self.open_popover(Popover::NewChannel);
    }

    /// Creates the channel named in the popover in the server on screen; it opens once the
    /// snapshot lists it.
    pub fn create_channel(&mut self) {
        let Some(server_id) = self.server_id().map(str::to_string) else {
            return;
        };
        let form = &mut self.server_forms.channel;
        if form.busy || form.name.text.trim().is_empty() {
            return;
        }
        form.busy = true;
        form.error.clear();
        self.session.create_channel(&server_id, &form.name.text);
    }

    // ---- Invite people ----------------------------------------------------------------

    /// Opens the "Invite people" popover for the server on screen and asks for its invites.
    pub fn open_invites(&mut self) {
        let Some(server_id) = self.server_id().map(str::to_string) else {
            return;
        };
        let form = &mut self.server_forms.invite;
        if form.server_id != server_id {
            // Another server's link and list are not this one's. The limits chosen and the
            // address typed stay, as they do in the other shells.
            form.server_id.clone_from(&server_id);
            form.made = None;
            form.list.clear();
            form.emailed.clear();
            form.copied = false;
            form.busy = false;
        }
        form.error.clear();
        self.open_popover(Popover::Invite);
        self.session.invites(&server_id);
    }

    /// When a new link stops working: 0 for never, else hours from now.
    pub fn set_invite_expiry(&mut self, hours: u32) {
        self.server_forms.invite.expires_in_hours = hours;
    }

    /// How many times a new link works: 0 for unlimited.
    pub fn set_invite_uses(&mut self, uses: u32) {
        self.server_forms.invite.max_uses = uses;
    }

    /// Makes a link with the limits chosen.
    pub fn make_invite(&mut self) {
        let form = &mut self.server_forms.invite;
        if form.busy || form.server_id.is_empty() {
            return;
        }
        form.busy = true;
        form.error.clear();
        form.copied = false;
        let expires = (form.expires_in_hours != 0).then_some(form.expires_in_hours);
        self.session
            .create_invite(&form.server_id, form.max_uses, expires);
    }

    /// Mails a one-use invite, good for a week, to the address in the form
    /// (architecture 6.2).
    pub fn email_invite(&mut self) {
        let form = &mut self.server_forms.invite;
        if form.busy || form.server_id.is_empty() || form.email.text.trim().is_empty() {
            return;
        }
        form.busy = true;
        form.error.clear();
        form.emailed.clear();
        self.session.email_invite(&form.server_id, &form.email.text);
    }

    pub fn revoke_invite(&mut self, code: &str) {
        let form = &mut self.server_forms.invite;
        if form.busy || form.server_id.is_empty() {
            return;
        }
        form.busy = true;
        form.error.clear();
        self.session.revoke_invite(&form.server_id, code);
    }

    /// Puts the link just made on the clipboard.
    pub fn copy_invite(&mut self, tx: &mut Tx) {
        let form = &mut self.server_forms.invite;
        if let Some(invite) = &form.made {
            tx.copy(invite.url.clone());
            form.copied = true;
        }
    }

    // ---- What the session reports -----------------------------------------------------

    pub(super) fn react_server(&mut self, update: &ServerUpdate) {
        match update {
            ServerUpdate::SettingSaved {
                server_id,
                setting,
                result,
            } => self.react_setting(server_id, *setting, result),
            ServerUpdate::ServerCreated(answer) => {
                let form = &mut self.server_forms.add;
                form.creating = false;
                match answer {
                    Ok(server) => {
                        form.name.clear();
                        self.server_forms.opening = Some(Opening::Server(server.server_id.clone()));
                        self.open_made();
                    }
                    Err(failure) => form.create_error.clone_from(&failure.message),
                }
            }
            ServerUpdate::InvitePreview(answer) => {
                let form = &mut self.server_forms.add;
                form.busy = false;
                match answer {
                    Ok(preview) => form.preview = Some(preview.clone()),
                    Err(failure) => {
                        form.error = match failure.code.as_str() {
                            "NOT_FOUND" => "That invite is unknown, expired or used up.".into(),
                            _ => failure.message.clone(),
                        };
                    }
                }
            }
            ServerUpdate::InviteAccepted(answer) => {
                let form = &mut self.server_forms.add;
                form.busy = false;
                match answer {
                    Ok(server_id) => {
                        form.invite.clear();
                        form.preview = None;
                        self.server_forms.opening = Some(Opening::Server(server_id.clone()));
                        self.open_made();
                    }
                    Err(failure) => form.error.clone_from(&failure.message),
                }
            }
            ServerUpdate::ChannelCreated { server_id, channel } => {
                let form = &mut self.server_forms.channel;
                form.busy = false;
                match channel {
                    Ok(channel) => {
                        form.name.clear();
                        if self.popover == Some(Popover::NewChannel) {
                            self.close_popover();
                        }
                        self.server_forms.opening = Some(Opening::Channel {
                            server_id: server_id.clone(),
                            channel_id: channel.channel_id.clone(),
                        });
                        self.open_made();
                    }
                    Err(failure) => {
                        form.error = match failure.code.as_str() {
                            "PERMISSION_DENIED" => "You cannot add channels to this server.".into(),
                            _ => failure.message.clone(),
                        };
                    }
                }
            }
            ServerUpdate::Invites { server_id, invites } => {
                let form = &mut self.server_forms.invite;
                if form.server_id != *server_id {
                    return;
                }
                match invites {
                    Ok(invites) => form.list.clone_from(invites),
                    Err(failure) => form.error = invite_refusal(failure),
                }
            }
            ServerUpdate::InviteCreated { server_id, invite } => {
                let form = &mut self.server_forms.invite;
                if form.server_id != *server_id {
                    return;
                }
                form.busy = false;
                match invite {
                    Ok(invite) => {
                        form.made = Some(invite.clone());
                        form.list.insert(0, invite.clone());
                    }
                    Err(failure) => form.error = invite_refusal(failure),
                }
            }
            ServerUpdate::InviteEmailed { server_id, invite } => {
                let form = &mut self.server_forms.invite;
                if form.server_id != *server_id {
                    return;
                }
                form.busy = false;
                match invite {
                    Ok(invite) => {
                        form.list.insert(0, invite.clone());
                        form.emailed.clone_from(&invite.email);
                        form.email.clear();
                    }
                    Err(failure) => form.error = email_refusal(failure),
                }
            }
            ServerUpdate::InviteRevoked {
                server_id,
                code,
                result,
            } => {
                let form = &mut self.server_forms.invite;
                if form.server_id != *server_id {
                    return;
                }
                form.busy = false;
                match result {
                    Ok(()) => {
                        form.list.retain(|invite| invite.code != *code);
                        if form.made.as_ref().is_some_and(|made| made.code == *code) {
                            form.made = None;
                            form.copied = false;
                        }
                    }
                    Err(failure) => form.error.clone_from(&failure.message),
                }
            }
        }
    }

    /// Opens the server or the channel just made, once the snapshot lists it: called when a
    /// call answers and after every snapshot. The person lands there only if they are still
    /// where they asked; someone who has gone elsewhere is left there.
    pub(super) fn open_made(&mut self) {
        match self.server_forms.opening.clone() {
            Some(Opening::Server(server_id)) => {
                let listed = self
                    .session
                    .servers()
                    .iter()
                    .any(|s| s.server_id == server_id);
                if !listed {
                    return;
                }
                self.server_forms.opening = None;
                if self.place == Place::Home && self.home_view == HomeView::AddServer {
                    self.home_view = HomeView::default();
                    self.select_server(&server_id);
                }
            }
            Some(Opening::Channel {
                server_id,
                channel_id,
            }) => {
                let listed = self
                    .session
                    .channels_of(&server_id)
                    .any(|c| c.channel_id == channel_id);
                if !listed {
                    return;
                }
                self.server_forms.opening = None;
                if self.server_id() == Some(server_id.as_str()) {
                    self.select_channel(&channel_id);
                }
            }
            None => {}
        }
    }
}
