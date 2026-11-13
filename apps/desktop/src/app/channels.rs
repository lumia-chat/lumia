//! Being in channels, and server DMs (architecture 6.8 and 6.4), as the window holds them:
//! the pages a server's main pane shows in place of a channel (Browse, and this account's own
//! settings in the server), the "Add people" and "Message someone" pickers, a channel read
//! before joining, invitations, the default channels card of the server's settings, and the
//! profile card's way to a server DM. Worded as the web's and the phones'.

use rune::TextEdit;

use super::server_settings::Saving;
use super::{Popover, State};
use crate::session::Failure;
use crate::session::channels::ChannelUpdate;

/// What a server's main pane shows in place of a channel, besides its settings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServerPage {
    /// The public channels this account is not in.
    Browse,
    /// "Your settings in" the server: this account's two switches there.
    YourSettings,
}

/// What the channel forms hold between frames.
#[derive(Debug, Default)]
pub struct ChannelForms {
    /// The page open in a server's main pane, and the server it is open for.
    pub page: Option<(String, ServerPage)>,
    pub browse_error: String,
    /// The channel a join is on its way for.
    pub joining: Option<String>,
    pub join_error: String,
    pub add: AddPeople,
    pub new_dm: NewDm,
    /// The switches as the page shows them: adds wait as invitations; no server DMs.
    pub adds_by_invitation: bool,
    pub dms_off: bool,
    pub switches: Saving,
    /// The default channels chosen on the server's settings page, and their save.
    pub defaults: Vec<String>,
    pub defaults_saving: Saving,
    /// A leave or a removal on its way, and why one was refused.
    pub member_busy: bool,
    pub member_error: String,
    pub invitation_error: String,
    /// The profile card's "Message" waiting for its conversation, and why it was refused.
    pub dm_busy: bool,
    pub dm_error: String,
}

/// The "Add people" picker of the channel on screen.
#[derive(Debug, Default)]
pub struct AddPeople {
    pub channel_id: String,
    pub filter: TextEdit,
    /// The account ids ticked.
    pub chosen: Vec<String>,
    pub busy: bool,
    pub error: String,
}

/// The "Message someone" picker of the server on screen.
#[derive(Debug, Default)]
pub struct NewDm {
    pub filter: TextEdit,
    pub error: String,
}

/// Why something about a channel was refused.
fn refusal(failure: &Failure) -> String {
    match failure.code.as_str() {
        "PERMISSION_DENIED" => "You cannot do that in this channel.".into(),
        "NOT_FOUND" => "That channel is gone, or you cannot see it.".into(),
        _ => failure.message.clone(),
    }
}

/// Why a server DM was not opened.
pub fn dm_refusal(failure: &Failure) -> String {
    match failure.code.as_str() {
        "FAILED_PRECONDITION" => "They don’t take messages here.".into(),
        "PERMISSION_DENIED" => "You cannot message people in this server.".into(),
        "NOT_FOUND" => "You cannot message them here.".into(),
        _ => failure.message.clone(),
    }
}

impl State {
    // ---- Pages ------------------------------------------------------------------------

    /// The page open in the server on screen, if any.
    pub fn server_page(&self) -> Option<ServerPage> {
        let (server_id, page) = self.channel_forms.page.as_ref()?;
        (Some(server_id.as_str()) == self.server_id()).then_some(*page)
    }

    fn open_page(&mut self, page: ServerPage) {
        let Some(server_id) = self.server_id().map(str::to_string) else {
            return;
        };
        self.close_popover();
        self.close_server_settings();
        self.channel_forms.page = Some((server_id, page));
    }

    /// Takes a page off the server's main pane.
    pub fn close_page(&mut self) {
        self.channel_forms.page = None;
    }

    /// Browse: the public channels of the server on screen this account is not in.
    pub fn open_browse(&mut self) {
        let Some(server_id) = self.server_id().map(str::to_string) else {
            return;
        };
        self.open_page(ServerPage::Browse);
        self.channel_forms.browse_error.clear();
        self.channel_forms.join_error.clear();
        self.session.browse_channels(&server_id);
    }

    /// "Your settings in" the server on screen, from what the snapshot says they are.
    pub fn open_your_settings(&mut self) {
        let Some(server) = self.current_server().cloned() else {
            return;
        };
        self.open_page(ServerPage::YourSettings);
        let form = &mut self.channel_forms;
        form.adds_by_invitation = server.channel_adds_by_invitation;
        form.dms_off = server.server_dms_off;
        form.switches = Saving::default();
    }

    fn current_server(&self) -> Option<&lumia_client::app::ServerSummary> {
        let id = self.server_id()?;
        self.session.servers().iter().find(|s| s.server_id == id)
    }

    // ---- Joining and leaving ------------------------------------------------------------

    /// Opens a channel Browse listed, to read before joining.
    pub fn preview_channel(&mut self, channel_id: &str) {
        let Some(server_id) = self.server_id() else {
            return;
        };
        let Some(channel) = self
            .session
            .browse
            .get(server_id)
            .and_then(|list| list.iter().find(|c| c.channel_id == channel_id))
            .cloned()
        else {
            return;
        };
        self.close_page();
        self.close_server_settings();
        self.draft.clear();
        self.channel_forms.join_error.clear();
        self.session.preview(&channel);
    }

    /// Joins a channel: one Browse lists, the one being read, or one this account was
    /// invited to.
    pub fn join_channel(&mut self, channel_id: &str) {
        if self.channel_forms.joining.is_some() {
            return;
        }
        self.channel_forms.joining = Some(channel_id.to_string());
        self.channel_forms.join_error.clear();
        self.channel_forms.invitation_error.clear();
        self.session.join_channel(channel_id);
    }

    /// Leaves the channel on screen.
    pub fn leave_channel(&mut self) {
        let Some(channel_id) = self.session.channel().map(str::to_string) else {
            return;
        };
        if self.channel_forms.member_busy {
            return;
        }
        self.channel_forms.member_busy = true;
        self.channel_forms.member_error.clear();
        self.session.leave_channel(&channel_id);
    }

    /// Takes someone out of the channel on screen (`MANAGE_CHANNELS`).
    pub fn remove_from_channel(&mut self, account_id: &str) {
        let Some(channel_id) = self.session.channel().map(str::to_string) else {
            return;
        };
        if self.channel_forms.member_busy {
            return;
        }
        self.channel_forms.member_busy = true;
        self.channel_forms.member_error.clear();
        self.session.remove_from_channel(&channel_id, account_id);
    }

    /// Says no to an invitation into a channel.
    pub fn dismiss_invitation(&mut self, channel_id: &str) {
        self.channel_forms.invitation_error.clear();
        self.session.dismiss_invitation(channel_id);
    }

    // ---- Add people -----------------------------------------------------------------------

    /// Opens the Channels heading's menu: create a channel, browse them.
    pub fn open_channels_menu(&mut self) {
        if self.server_id().is_some() {
            self.open_popover(Popover::ChannelsMenu);
        }
    }

    /// Opens "Add people" for the channel on screen.
    pub fn open_add_people(&mut self) {
        let Some(channel_id) = self.session.channel().map(str::to_string) else {
            return;
        };
        self.channel_forms.add = AddPeople {
            channel_id,
            ..AddPeople::default()
        };
        self.open_popover(Popover::AddPeople);
    }

    /// Ticks someone in "Add people", or unticks them.
    pub fn toggle_chosen(&mut self, account_id: &str) {
        let chosen = &mut self.channel_forms.add.chosen;
        match chosen.iter().position(|id| id == account_id) {
            Some(at) => {
                chosen.remove(at);
            }
            None => chosen.push(account_id.to_string()),
        }
    }

    /// Adds everyone ticked, or invites those who take adds that way.
    pub fn add_chosen(&mut self) {
        let form = &mut self.channel_forms.add;
        if form.busy || form.chosen.is_empty() {
            return;
        }
        form.busy = true;
        form.error.clear();
        let (channel_id, chosen) = (form.channel_id.clone(), form.chosen.clone());
        self.session.add_to_channel(&channel_id, &chosen);
    }

    // ---- Server DMs -----------------------------------------------------------------------

    /// Opens "Message someone" for the server on screen.
    pub fn open_new_dm(&mut self) {
        if self.server_id().is_none() {
            return;
        }
        self.channel_forms.new_dm = NewDm::default();
        self.open_popover(Popover::NewDm);
    }

    /// Opens the server DM with `account_id` in the server on screen, starting it if need be.
    pub fn message_in_server(&mut self, account_id: &str) {
        let Some(server_id) = self.server_id().map(str::to_string) else {
            return;
        };
        if self.channel_forms.dm_busy {
            return;
        }
        self.channel_forms.dm_busy = true;
        self.channel_forms.dm_error.clear();
        self.channel_forms.new_dm.error.clear();
        self.session.open_server_dm(&server_id, account_id);
    }

    /// The profile card's "Message": the server DM with the person on the card.
    pub fn message_person_here(&mut self) {
        let Some(person) = self.person.clone() else {
            return;
        };
        self.message_in_server(&person.account_id);
    }

    // ---- This account's switches, and the default channels -------------------------------

    pub fn set_adds_by_invitation(&mut self, on: bool) {
        self.channel_forms.adds_by_invitation = on;
        self.channel_forms.switches.saved = false;
    }

    pub fn set_dms_off(&mut self, off: bool) {
        self.channel_forms.dms_off = off;
        self.channel_forms.switches.saved = false;
    }

    pub fn save_switches(&mut self) {
        let Some(server) = self.current_server().cloned() else {
            return;
        };
        let form = &mut self.channel_forms;
        if form.switches.busy {
            return;
        }
        form.switches = Saving {
            busy: true,
            ..Saving::default()
        };
        let adds = (form.adds_by_invitation != server.channel_adds_by_invitation)
            .then_some(form.adds_by_invitation);
        let dms = (form.dms_off != server.server_dms_off).then_some(form.dms_off);
        self.session.save_switches(&server.server_id, adds, dms);
    }

    /// The default channels as the server's settings page opens with them: the joined
    /// channels the snapshot says are defaults, and Browse's.
    pub(super) fn load_defaults(&mut self, server_id: &str) {
        self.channel_forms.defaults = self.default_channels(server_id);
        self.channel_forms.defaults_saving = Saving::default();
        self.session.browse_channels(server_id);
    }

    fn default_channels(&self, server_id: &str) -> Vec<String> {
        self.public_channels(server_id)
            .into_iter()
            .filter(|c| c.is_default)
            .map(|c| c.channel_id)
            .collect()
    }

    /// A server's public channels, as far as this account knows them: those it is in, then
    /// the ones Browse listed.
    pub fn public_channels(&self, server_id: &str) -> Vec<lumia_client::app::ChannelSummary> {
        let mut all: Vec<_> = self
            .session
            .channels_of(server_id)
            .filter(|c| !c.private)
            .cloned()
            .collect();
        for channel in self.session.browse.get(server_id).into_iter().flatten() {
            if all.iter().all(|c| c.channel_id != channel.channel_id) {
                all.push(channel.clone());
            }
        }
        all
    }

    pub fn toggle_default(&mut self, channel_id: &str) {
        let defaults = &mut self.channel_forms.defaults;
        match defaults.iter().position(|id| id == channel_id) {
            Some(at) => {
                defaults.remove(at);
            }
            None => defaults.push(channel_id.to_string()),
        }
        self.channel_forms.defaults_saving.saved = false;
    }

    pub fn save_defaults(&mut self) {
        let Some(server_id) = self.server_forms.settings.server_id.clone() else {
            return;
        };
        let form = &mut self.channel_forms;
        if form.defaults_saving.busy {
            return;
        }
        form.defaults_saving = Saving {
            busy: true,
            ..Saving::default()
        };
        let defaults = form.defaults.clone();
        self.session.save_default_channels(&server_id, &defaults);
    }

    // ---- What the session reports -----------------------------------------------------

    pub(super) fn react_channel(&mut self, update: &ChannelUpdate) {
        let form = &mut self.channel_forms;
        match update {
            ChannelUpdate::Browse { channels, .. } => {
                form.browse_error = channels.as_ref().err().map(refusal).unwrap_or_default();
                // Browse's list fills in what the default channels card knows of; a choice
                // made there already stands.
                if let Some(server_id) = self.server_forms.settings.server_id.clone()
                    && !self.channel_forms.defaults_saving.saved
                    && self.channel_forms.defaults.is_empty()
                {
                    self.channel_forms.defaults = self.default_channels(&server_id);
                }
            }
            ChannelUpdate::Joined { channel_id, result } => {
                form.joining = None;
                match result {
                    Ok(_) => {
                        // Joined from Browse or an invitation: the channel opens.
                        if self.server_page() == Some(ServerPage::Browse)
                            || self.session.channel() != Some(channel_id.as_str())
                        {
                            self.select_channel(channel_id);
                        }
                    }
                    Err(failure) => {
                        let why = refusal(failure);
                        form.join_error.clone_from(&why);
                        form.invitation_error = why;
                    }
                }
            }
            ChannelUpdate::Left { result, .. } => {
                form.member_busy = false;
                if let Err(failure) = result {
                    form.member_error = refusal(failure);
                }
            }
            ChannelUpdate::Removed { result, .. } => {
                form.member_busy = false;
                if let Err(failure) = result {
                    form.member_error = refusal(failure);
                }
            }
            ChannelUpdate::Added { channel_id, result } => {
                let add = &mut form.add;
                if add.channel_id != *channel_id {
                    return;
                }
                add.busy = false;
                match result {
                    Ok(_) => {
                        add.chosen.clear();
                        if self.popover == Some(Popover::AddPeople) {
                            self.close_popover();
                        }
                    }
                    Err(failure) => {
                        add.error = match failure.code.as_str() {
                            "INVALID_ARGUMENT" => "Someone chosen cannot see this channel.".into(),
                            _ => refusal(failure),
                        };
                    }
                }
            }
            ChannelUpdate::Dismissed { result, .. } => {
                if let Err(failure) = result {
                    form.invitation_error = refusal(failure);
                }
            }
            ChannelUpdate::SwitchesSaved { result, .. } => {
                form.switches = Saving::default();
                match result {
                    Ok(_) => form.switches.saved = true,
                    Err(failure) => form.switches.error.clone_from(&failure.message),
                }
            }
            ChannelUpdate::DefaultsSaved { result, .. } => {
                form.defaults_saving = Saving::default();
                match result {
                    Ok(_) => form.defaults_saving.saved = true,
                    Err(failure) => {
                        form.defaults_saving.error = match failure.code.as_str() {
                            "PERMISSION_DENIED" => {
                                "You cannot change this server's settings.".into()
                            }
                            _ => failure.message.clone(),
                        };
                    }
                }
            }
            ChannelUpdate::DmOpened {
                server_id, result, ..
            } => {
                form.dm_busy = false;
                match result {
                    Ok(channel) => {
                        if matches!(self.popover, Some(Popover::Person | Popover::NewDm)) {
                            self.close_popover();
                        }
                        self.open_made_channel(server_id, &channel.channel_id);
                    }
                    Err(failure) => {
                        let why = dm_refusal(failure);
                        form.dm_error.clone_from(&why);
                        form.new_dm.error = why;
                    }
                }
            }
            ChannelUpdate::Members { .. } => {}
        }
    }
}
