//! Being in channels, and server DMs (architecture 6.8 and 6.4): Browse, joining and
//! leaving, a channel's members and adding or removing them, invitations, this account's
//! two switches in a server, a server's default channels, and the 1:1 server DM with a
//! member.
//!
//! Every call reports as a [`ChannelUpdate`]. A call the instance answers with the account's
//! snapshot as it then is (a join, a leave, an invitation dismissed, a switch, the default
//! channels) has it taken at once, so the sidebar follows before the account log says so; a
//! channel's members and Browse's list are kept here for the screens that show them.

use lumia_client::app::{ChannelAddSummary, ChannelSummary, SnapshotSummary};

use super::{Conversation, Failure, Session, Update};

/// What a finished call about channels reports.
#[derive(Debug, Clone)]
pub enum ChannelUpdate {
    /// The public channels of `server_id` this account can see and is not in.
    Browse {
        server_id: String,
        channels: Result<Vec<ChannelSummary>, Failure>,
    },
    /// `channel_id` joined, with the snapshot as it then is.
    Joined {
        channel_id: String,
        result: Result<SnapshotSummary, Failure>,
    },
    /// `channel_id` left.
    Left {
        channel_id: String,
        result: Result<SnapshotSummary, Failure>,
    },
    /// Who is in `channel_id`, as account ids.
    Members {
        channel_id: String,
        members: Result<Vec<String>, Failure>,
    },
    /// People added to `channel_id`, or invited.
    Added {
        channel_id: String,
        result: Result<ChannelAddSummary, Failure>,
    },
    /// `account_id` taken out of `channel_id`.
    Removed {
        channel_id: String,
        account_id: String,
        result: Result<(), Failure>,
    },
    /// The invitation into `channel_id` dismissed.
    Dismissed {
        channel_id: String,
        result: Result<SnapshotSummary, Failure>,
    },
    /// This account's switches in `server_id` saved.
    SwitchesSaved {
        server_id: String,
        result: Result<SnapshotSummary, Failure>,
    },
    /// `server_id`'s default channels saved.
    DefaultsSaved {
        server_id: String,
        result: Result<SnapshotSummary, Failure>,
    },
    /// The server DM with `account_id` in `server_id`, found or started.
    DmOpened {
        server_id: String,
        account_id: String,
        result: Result<ChannelSummary, Failure>,
    },
}

impl Session {
    /// Asks for what Browse lists in a server ([`ChannelUpdate::Browse`]).
    pub fn browse_channels(&self, server_id: &str) {
        let Some(app) = self.client() else { return };
        let server_id = server_id.to_string();
        self.task(async move {
            let channels = app.browse_channels(&server_id).await;
            Update::Channel(ChannelUpdate::Browse {
                server_id,
                channels: channels.map_err(Failure::from),
            })
        });
    }

    /// Puts a channel this account is not in on screen to read (architecture 6.8): it is
    /// subscribed to, as any channel, and drawn with "Join" where the composer would be.
    pub fn preview(&mut self, channel: &ChannelSummary) {
        if let Some(app) = self.client()
            && self.subscribed.insert(channel.channel_id.clone())
        {
            let _ = app.subscribe(&channel.channel_id);
        }
        self.previewing = Some(channel.clone());
        self.open_conversation(Conversation::Channel(channel.channel_id.clone()));
        self.channel_members(&channel.channel_id);
    }

    /// Joins a channel ([`ChannelUpdate::Joined`]).
    pub fn join_channel(&self, channel_id: &str) {
        let Some(app) = self.client() else { return };
        let channel_id = channel_id.to_string();
        self.task(async move {
            let result = app.join_channel(&channel_id).await;
            Update::Channel(ChannelUpdate::Joined {
                channel_id,
                result: result.map_err(Failure::from),
            })
        });
    }

    /// Leaves a channel ([`ChannelUpdate::Left`]).
    pub fn leave_channel(&self, channel_id: &str) {
        let Some(app) = self.client() else { return };
        let channel_id = channel_id.to_string();
        self.task(async move {
            let result = app.leave_channel(&channel_id).await;
            Update::Channel(ChannelUpdate::Left {
                channel_id,
                result: result.map_err(Failure::from),
            })
        });
    }

    /// Asks who is in a channel ([`ChannelUpdate::Members`]).
    pub fn channel_members(&self, channel_id: &str) {
        let Some(app) = self.client() else { return };
        let channel_id = channel_id.to_string();
        self.task(async move {
            let members = app.channel_members(&channel_id).await;
            Update::Channel(ChannelUpdate::Members {
                channel_id,
                members: members.map_err(Failure::from),
            })
        });
    }

    /// Adds members of the server to a channel, or invites them ([`ChannelUpdate::Added`]).
    pub fn add_to_channel(&self, channel_id: &str, account_ids: &[String]) {
        let Some(app) = self.client() else { return };
        let (channel_id, account_ids) = (channel_id.to_string(), account_ids.to_vec());
        self.task(async move {
            let result = app.add_channel_members(&channel_id, &account_ids).await;
            Update::Channel(ChannelUpdate::Added {
                channel_id,
                result: result.map_err(Failure::from),
            })
        });
    }

    /// Takes someone out of a channel ([`ChannelUpdate::Removed`]).
    pub fn remove_from_channel(&self, channel_id: &str, account_id: &str) {
        let Some(app) = self.client() else { return };
        let (channel_id, account_id) = (channel_id.to_string(), account_id.to_string());
        self.task(async move {
            let result = app.remove_channel_member(&channel_id, &account_id).await;
            Update::Channel(ChannelUpdate::Removed {
                channel_id,
                account_id,
                result: result.map_err(Failure::from),
            })
        });
    }

    /// Says no to an invitation ([`ChannelUpdate::Dismissed`]).
    pub fn dismiss_invitation(&self, channel_id: &str) {
        let Some(app) = self.client() else { return };
        let channel_id = channel_id.to_string();
        self.task(async move {
            let result = app.dismiss_channel_invitation(&channel_id).await;
            Update::Channel(ChannelUpdate::Dismissed {
                channel_id,
                result: result.map_err(Failure::from),
            })
        });
    }

    /// Saves this account's switches in a server; `None` leaves one as it is
    /// ([`ChannelUpdate::SwitchesSaved`]).
    pub fn save_switches(
        &self,
        server_id: &str,
        channel_adds_by_invitation: Option<bool>,
        server_dms_off: Option<bool>,
    ) {
        let Some(app) = self.client() else { return };
        let server_id = server_id.to_string();
        self.task(async move {
            let result = app
                .update_membership(&server_id, channel_adds_by_invitation, server_dms_off)
                .await;
            Update::Channel(ChannelUpdate::SwitchesSaved {
                server_id,
                result: result.map_err(Failure::from),
            })
        });
    }

    /// Saves a server's default channels ([`ChannelUpdate::DefaultsSaved`]).
    pub fn save_default_channels(&self, server_id: &str, channel_ids: &[String]) {
        let Some(app) = self.client() else { return };
        let (server_id, channel_ids) = (server_id.to_string(), channel_ids.to_vec());
        self.task(async move {
            let result = app.set_default_channels(&server_id, &channel_ids).await;
            Update::Channel(ChannelUpdate::DefaultsSaved {
                server_id,
                result: result.map_err(Failure::from),
            })
        });
    }

    /// Finds or starts the server DM with a member ([`ChannelUpdate::DmOpened`]).
    pub fn open_server_dm(&self, server_id: &str, account_id: &str) {
        let Some(app) = self.client() else { return };
        let (server_id, account_id) = (server_id.to_string(), account_id.to_string());
        self.task(async move {
            let result = app.open_server_dm(&server_id, &account_id).await;
            Update::Channel(ChannelUpdate::DmOpened {
                server_id,
                account_id,
                result: result.map_err(Failure::from),
            })
        });
    }

    pub(super) fn apply_channel(&mut self, update: &ChannelUpdate) {
        match update {
            ChannelUpdate::Browse {
                server_id,
                channels: Ok(channels),
            } => {
                self.browse.insert(server_id.clone(), channels.clone());
            }
            ChannelUpdate::Joined {
                channel_id,
                result: Ok(snapshot),
            } => {
                if self
                    .previewing
                    .as_ref()
                    .is_some_and(|c| c.channel_id == *channel_id)
                {
                    self.previewing = None;
                }
                for channels in self.browse.values_mut() {
                    channels.retain(|c| c.channel_id != *channel_id);
                }
                self.take_snapshot(snapshot);
                self.channel_members(channel_id);
            }
            ChannelUpdate::Left {
                channel_id,
                result: Ok(snapshot),
            } => {
                self.channel_members.remove(channel_id);
                self.take_snapshot(snapshot);
            }
            ChannelUpdate::Members {
                channel_id,
                members: Ok(members),
            } => {
                self.channel_members
                    .insert(channel_id.clone(), members.clone());
            }
            ChannelUpdate::Added {
                channel_id,
                result: Ok(_),
            }
            | ChannelUpdate::Removed {
                channel_id,
                result: Ok(()),
                ..
            } => self.channel_members(channel_id),
            ChannelUpdate::Dismissed {
                result: Ok(snapshot),
                ..
            }
            | ChannelUpdate::SwitchesSaved {
                result: Ok(snapshot),
                ..
            }
            | ChannelUpdate::DefaultsSaved {
                result: Ok(snapshot),
                ..
            } => self.take_snapshot(snapshot),
            // The conversation is in the next snapshot, which the account log asks for;
            // asked here too, for an instance whose log is slow to say so.
            ChannelUpdate::DmOpened { result: Ok(_), .. } => self.load(),
            ChannelUpdate::Browse { .. }
            | ChannelUpdate::Joined { .. }
            | ChannelUpdate::Left { .. }
            | ChannelUpdate::Members { .. }
            | ChannelUpdate::Added { .. }
            | ChannelUpdate::Removed { .. }
            | ChannelUpdate::Dismissed { .. }
            | ChannelUpdate::SwitchesSaved { .. }
            | ChannelUpdate::DefaultsSaved { .. }
            | ChannelUpdate::DmOpened { .. } => {}
        }
    }

    /// Takes a snapshot a call answered with, as a fresh one is taken.
    fn take_snapshot(&mut self, snapshot: &SnapshotSummary) {
        self.apply(&Update::Snapshot(Ok(snapshot.clone())));
    }
}
