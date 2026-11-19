//! Roles, titles and tags (architecture 7.5): the core's calls that manage a server's roles
//! and its members, and what the session keeps of them.
//!
//! The session computes nothing here. A server's roles, what this account may do to each and
//! what it may grant are in the snapshot; a member's roles, title and tag are in the server's
//! member list ([`Session::members`]), which the core dresses when it is read. So a change is
//! followed by reading again: a role call answers with the account's snapshot as it then is,
//! which the session takes at once, and the members are read again from what the core keeps
//! ([`Session::read_members`], no request) whenever what dresses them changed: after a call
//! of this account's own, when a snapshot says a server's roles or this account's standing
//! there changed, and on the core's `AppEvent::TagsChanged`, which is how a change someone
//! else made arrives while a screen is open.
//!
//! A role moved is drawn moved at once ([`RolesState::pending`]), as a rail tile is, and put
//! back if the instance refuses.
//!
//! What is sent is what was changed, never what a screen happened to hold (the core's
//! `change_role` and `change_member`): a role's editor says what the role was when it opened
//! and what it holds now, and the core sends the difference; a member is sent the roles to
//! give and to take and the title, in one request the instance does whole or not at all. So a
//! save made from a screen left open undoes nothing someone else did meanwhile. Each change
//! of a member carries a number ([`Session::change_member`] answers it), which its answer
//! names: two forms open on one member each take their own.

use lumia_client::app::{MemberSummary, ServerSummary, SnapshotSummary};
use lumia_client::roles::{RoleDraft, RoleSummary, TagSummary};

use super::{Failure, Session, Update};

/// A role order on its way to the instance.
#[derive(Debug, Default)]
pub struct RolesState {
    /// The server and the order last sent (every role but Everyone), until its answer.
    pub pending: Option<(String, Vec<String>)>,
    /// How many orders were sent; an answer to any but the last is not the order.
    sent: u64,
    /// How many changes of a member were asked for: the last one's number.
    asked: u64,
}

impl RolesState {
    /// Leaves an account: an answer still on its way belongs to it, so it is not taken.
    pub(super) fn forget(&mut self) {
        self.pending = None;
        self.sent += 1;
    }

    /// How many orders were sent: what the last one's answer carries.
    pub fn sent(&self) -> u64 {
        self.sent
    }

    /// The number of the change of a member asked for last, which its answer carries.
    pub fn asked(&self) -> u64 {
        self.asked
    }
}

/// What a finished role or member call reports.
#[derive(Debug, Clone)]
pub enum RoleUpdate {
    /// A role created (`role_id` none) or changed: the account's snapshot as it then is.
    Saved {
        server_id: String,
        role_id: Option<String>,
        result: Result<SnapshotSummary, Failure>,
    },
    Deleted {
        server_id: String,
        role_id: String,
        result: Result<SnapshotSummary, Failure>,
    },
    /// The `sent`th order of a server's roles kept, or why not.
    Ordered {
        server_id: String,
        sent: u64,
        result: Result<SnapshotSummary, Failure>,
    },
    /// A member's roles, title or both changed, by the change numbered `request`
    /// ([`Session::change_member`]): the member as the server now shows them. Refused,
    /// nothing of them was changed.
    Member {
        server_id: String,
        account_id: String,
        request: u64,
        result: Result<MemberSummary, Failure>,
    },
    /// A member removed from the server.
    Kicked {
        server_id: String,
        account_id: String,
        result: Result<(), Failure>,
    },
    /// A role given to the members chosen in its editor. `left` are those who do not have
    /// it, in the order chosen: whoever the instance refused it to, and with a failure that
    /// is no refusal (the instance not reached) everyone after, who were not asked about.
    /// `why` is the first of those answers. Everyone else chosen has the role.
    Given {
        server_id: String,
        role_id: String,
        left: Vec<String>,
        why: Option<Failure>,
    },
}

/// Whether what dresses a server's members differs between two snapshots of it: its roles,
/// which of the two a tag shows, and this account's standing there.
fn dresses_otherwise(old: &ServerSummary, new: &ServerSummary) -> bool {
    old.roles != new.roles
        || old.tag_titles != new.tag_titles
        || old.owner_account_id != new.owner_account_id
        || old.is_owner != new.is_owner
        || old.can_manage_members != new.can_manage_members
        || old.role_ids != new.role_ids
}

impl Session {
    /// A server of the snapshot.
    pub fn server(&self, server_id: &str) -> Option<&ServerSummary> {
        self.servers().iter().find(|s| s.server_id == server_id)
    }

    /// A server's roles as its settings list them: highest first, Everyone last; in the
    /// order just sent while its answer is on its way.
    pub fn roles_of(&self, server_id: &str) -> Vec<RoleSummary> {
        let Some(server) = self.server(server_id) else {
            return Vec::new();
        };
        let Some((_, order)) = self
            .roles
            .pending
            .as_ref()
            .filter(|(pending, _)| pending == server_id)
        else {
            return server.roles.clone();
        };
        let place = |role: &RoleSummary| {
            order
                .iter()
                .position(|id| *id == role.role_id)
                .unwrap_or(usize::MAX)
        };
        let mut roles = server.roles.clone();
        // Stable: Everyone, and a role the order does not name, stay after the rest.
        roles.sort_by_key(|role| (role.is_everyone, place(role)));
        roles
    }

    /// Someone as a server's member list has them: their roles, title and tag there.
    pub fn member_in(&self, server_id: &str, account_id: &str) -> Option<&MemberSummary> {
        self.members
            .get(server_id)?
            .iter()
            .find(|m| m.account_id == account_id)
    }

    /// The tag beside someone's name in a server, as the core worked it out.
    pub fn tag_in(&self, server_id: &str, account_id: &str) -> Option<&TagSummary> {
        self.member_in(server_id, account_id)?.tag.as_ref()
    }

    /// Reads a server's members again from what the core keeps (no request): their roles,
    /// titles and tags as they now are, by the snapshot the core last took. A server never
    /// listed is left as it is held.
    pub(crate) fn read_members(&mut self, server_id: &str) {
        let Some(app) = self.app.clone() else { return };
        let Ok(members) = app.kept_members(server_id) else {
            return;
        };
        if members.is_empty() {
            return;
        }
        self.members.insert(server_id.to_string(), members);
        // Every row that draws a tag draws again.
        self.revision += 1;
    }

    /// The servers of `new` whose members `old` dressed otherwise.
    pub(super) fn redressed(old: Option<&SnapshotSummary>, new: &SnapshotSummary) -> Vec<String> {
        let Some(old) = old else {
            return Vec::new();
        };
        new.servers
            .iter()
            .filter(|now| {
                old.servers
                    .iter()
                    .find(|was| was.server_id == now.server_id)
                    .is_some_and(|was| dresses_otherwise(was, now))
            })
            .map(|server| server.server_id.clone())
            .collect()
    }

    /// Creates a role ([`RoleUpdate::Saved`] with no `role_id`).
    pub fn create_role(&self, server_id: &str, role: RoleDraft) {
        let Some(app) = self.client() else { return };
        let server_id = server_id.to_string();
        self.task(async move {
            let result = app
                .create_role(
                    &server_id,
                    &role.name,
                    &role.color,
                    role.hide_tag,
                    &role.permissions,
                )
                .await;
            Update::Role(RoleUpdate::Saved {
                server_id,
                role_id: None,
                result: result.map_err(Failure::from),
            })
        });
    }

    /// Saves what an editor changed of a role ([`RoleUpdate::Saved`]): `was` is the role as
    /// the editor opened on it and `now` what it holds, and the core sends the difference
    /// and nothing else of the role, so what someone else changed of it meanwhile stays.
    pub fn change_role(&self, server_id: &str, role_id: &str, was: RoleDraft, now: RoleDraft) {
        let Some(app) = self.client() else { return };
        let (server_id, role_id) = (server_id.to_string(), role_id.to_string());
        self.task(async move {
            let result = app.change_role(&server_id, &role_id, &was, &now).await;
            Update::Role(RoleUpdate::Saved {
                server_id,
                role_id: Some(role_id),
                result: result.map_err(Failure::from),
            })
        });
    }

    /// Deletes a role ([`RoleUpdate::Deleted`]).
    pub fn delete_role(&self, server_id: &str, role_id: &str) {
        let Some(app) = self.client() else { return };
        let (server_id, role_id) = (server_id.to_string(), role_id.to_string());
        self.task(async move {
            let result = app.delete_role(&server_id, &role_id).await;
            Update::Role(RoleUpdate::Deleted {
                server_id,
                role_id,
                result: result.map_err(Failure::from),
            })
        });
    }

    /// Sends a server's roles in a new order, every role but Everyone
    /// ([`RoleUpdate::Ordered`]); the list shows it from now.
    pub fn order_roles(&mut self, server_id: &str, order: Vec<String>) {
        self.roles.sent += 1;
        let sent = self.roles.sent;
        self.roles.pending = Some((server_id.to_string(), order.clone()));
        let Some(app) = self.client() else { return };
        let server_id = server_id.to_string();
        self.task(async move {
            let result = app.reorder_roles(&server_id, &order).await;
            Update::Role(RoleUpdate::Ordered {
                server_id,
                sent,
                result: result.map_err(Failure::from),
            })
        });
    }

    /// Changes a member: the roles to give them and to take away, and their title when one
    /// is given, in one request the instance does whole or not at all
    /// ([`RoleUpdate::Member`]). Their other roles stay as the instance has them. Answers
    /// the change's number, which its answer names.
    pub fn change_member(
        &mut self,
        server_id: &str,
        account_id: &str,
        give: Vec<String>,
        take: Vec<String>,
        title: Option<String>,
    ) -> u64 {
        self.roles.asked += 1;
        let request = self.roles.asked;
        let Some(app) = self.client() else {
            return request;
        };
        let (server_id, account_id) = (server_id.to_string(), account_id.to_string());
        self.task(async move {
            let result = app
                .change_member(&server_id, &account_id, &give, &take, title.as_deref())
                .await;
            Update::Role(RoleUpdate::Member {
                server_id,
                account_id,
                request,
                result: result.map_err(Failure::from),
            })
        });
        request
    }

    /// Removes a member from a server ([`RoleUpdate::Kicked`]).
    pub fn kick(&self, server_id: &str, account_id: &str) {
        let Some(app) = self.client() else { return };
        let (server_id, account_id) = (server_id.to_string(), account_id.to_string());
        self.task(async move {
            let result = app.remove_member(&server_id, &account_id).await;
            Update::Role(RoleUpdate::Kicked {
                server_id,
                account_id,
                result: result.map_err(Failure::from),
            })
        });
    }

    /// Gives a role to each of `members`, one change each of that role alone
    /// ([`RoleUpdate::Given`]): what else each holds is the instance's to say, not this
    /// list's. Someone the instance refuses it to does not stop the rest; a failure that is
    /// no refusal does, since nobody after would be reached either.
    pub fn give_role(&self, server_id: &str, role_id: &str, members: &[String]) {
        let Some(app) = self.client() else { return };
        let members = members.to_vec();
        let (server_id, role_id) = (server_id.to_string(), role_id.to_string());
        self.task(async move {
            let give = [role_id.clone()];
            let (mut left, mut why) = (Vec::new(), None);
            for (i, account_id) in members.iter().enumerate() {
                let Err(error) = app
                    .change_member(&server_id, account_id, &give, &[], None)
                    .await
                else {
                    continue;
                };
                let failure = Failure::from(error);
                let refused = !failure.code.is_empty();
                why.get_or_insert(failure);
                if refused {
                    left.push(account_id.clone());
                } else {
                    left.extend(members[i..].iter().cloned());
                    break;
                }
            }
            Update::Role(RoleUpdate::Given {
                server_id,
                role_id,
                left,
                why,
            })
        });
    }

    pub(super) fn apply_role(&mut self, update: &RoleUpdate) {
        match update {
            RoleUpdate::Saved {
                result: Ok(snapshot),
                ..
            }
            | RoleUpdate::Deleted {
                result: Ok(snapshot),
                ..
            } => self.hold_snapshot(snapshot.clone()),
            RoleUpdate::Ordered {
                server_id,
                sent,
                result,
            } => {
                // An answer to an order since replaced changes nothing: the later one's is
                // on its way.
                if *sent != self.roles.sent {
                    return;
                }
                self.roles.pending = None;
                if let Ok(snapshot) = result {
                    self.hold_snapshot(snapshot.clone());
                }
                // The tags may follow the order: read either way, a refusal included.
                self.read_members(server_id);
            }
            RoleUpdate::Member {
                server_id,
                account_id,
                result,
                ..
            } => {
                // Held as answered where the core keeps no list of this server to read.
                if let (Ok(now), Some(list)) = (result, self.members.get_mut(server_id))
                    && let Some(held) = list.iter_mut().find(|m| m.account_id == *account_id)
                {
                    *held = now.clone();
                }
                // And read again either way: someone refused may have changed meanwhile.
                self.read_members(server_id);
                self.revision += 1;
            }
            RoleUpdate::Kicked {
                server_id,
                account_id,
                result: Ok(()),
            } => {
                if let Some(list) = self.members.get_mut(server_id) {
                    list.retain(|m| m.account_id != *account_id);
                }
                // Out of the server is out of its channels.
                let channels: Vec<String> = self
                    .snapshot
                    .iter()
                    .flat_map(|snapshot| &snapshot.channels)
                    .filter(|channel| channel.server_id == *server_id)
                    .map(|channel| channel.channel_id.clone())
                    .collect();
                for channel in channels {
                    if let Some(inside) = self.channel_members.get_mut(&channel) {
                        inside.retain(|id| id != account_id);
                    }
                }
                self.revision += 1;
            }
            RoleUpdate::Given { server_id, .. } => self.read_members(server_id),
            RoleUpdate::Saved { .. } | RoleUpdate::Deleted { .. } | RoleUpdate::Kicked { .. } => {}
        }
    }
}
