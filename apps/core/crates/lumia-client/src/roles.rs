//! Roles and titles as a shell shows them (architecture 7.5): a server's roles in order with
//! what this account may do to each, the permissions a role's editor offers, and the tag
//! beside a member's name. Values in and values out, so the four shells offer the same and
//! tag a member the same. They say what to offer; the instance decides every write
//! (specs/authorization.md 3.3 and 3.5), and the rules here are its rules read the same way.

use lumia_core::proto::{Member, MemberTag, Permission, Role, Server};
use lumia_core::wire;
use serde::{Deserialize, Serialize};

/// What every shell calls the role every member holds.
pub const EVERYONE: &str = "Everyone";

/// The most characters a role's name or a member's title keeps, once trimmed.
pub const LABEL_MAX: usize = 32;

/// The colours a role may have: the tones a rail folder may, or none (empty).
pub use crate::rail::TONES;

/// A role of a server, as the snapshot gives the server's roles: highest first, `Everyone`
/// last.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RoleSummary {
    pub role_id: String,
    /// [`EVERYONE`] for the role every member holds.
    pub name: String,
    /// One of [`TONES`], or empty for none.
    pub color: String,
    /// Lower is higher. The instance's; a shell orders by the list it is given.
    pub position: u32,
    pub is_everyone: bool,
    /// Never a member's tag, whatever its place ("Show as tag" off).
    pub hide_tag: bool,
    /// Every permission it grants by its wire name (`MANAGE_ROLES`), those no editor offers
    /// yet among them.
    pub permissions: Vec<String>,
    /// Whether this account may change it, delete it, give it and take it away; of
    /// `Everyone`, whether it may change its permissions, which is all that changes.
    pub can_manage: bool,
}

/// The tag beside a member's name: a role in its colour, or a title in none.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TagSummary {
    pub label: String,
    /// One of [`TONES`], or empty for the plain chip.
    pub color: String,
}

/// A heading of a role's editor and the permissions under it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PermissionGroup {
    pub title: String,
    pub permissions: Vec<PermissionChoice>,
}

/// A permission a role's editor offers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PermissionChoice {
    /// Its wire name, as [`RoleSummary::permissions`] has it.
    pub name: String,
    pub label: String,
    /// What it allows, in a line.
    pub detail: String,
}

/// A permission offered: itself, its label, and what it allows in a line.
type Offered = (Permission, &'static str, &'static str);

/// The permissions an editor offers, the ones that do something today (architecture 7.5),
/// under their headings.
const OFFERED: [(&str, &[Offered]); 3] = [
    (
        "Server",
        &[
            (
                Permission::Administrator,
                "Administrator",
                "Everything below, in every channel. A channel’s own permissions do not limit it.",
            ),
            (
                Permission::ManageServer,
                "Manage server",
                "Change the server’s name, picture and settings.",
            ),
            (
                Permission::ManageRoles,
                "Manage roles",
                "Create, change and delete roles below their own highest, and give them to members.",
            ),
            (
                Permission::ManageChannels,
                "Manage channels",
                "Create channels, and add people to them or remove them.",
            ),
            (
                Permission::ManageMembers,
                "Manage members",
                "Kick members below them and change those members’ titles.",
            ),
            (
                Permission::CreateInvites,
                "Create invites",
                "Make invite links and send invitations to join the server.",
            ),
        ],
    ),
    (
        "Channels",
        &[
            (
                Permission::ViewChannel,
                "See channels",
                "See the server’s public channels and join them.",
            ),
            (
                Permission::ReadHistory,
                "Read message history",
                "Read the messages already in a channel.",
            ),
        ],
    ),
    (
        "Messages",
        &[
            (
                Permission::SendMessages,
                "Send messages",
                "Write in the channels they are in.",
            ),
            (
                Permission::AttachFiles,
                "Attach files",
                "Add pictures, videos and files to a message.",
            ),
            (
                Permission::AddReactions,
                "Add reactions",
                "React to messages with emoji.",
            ),
            (
                Permission::MentionEveryone,
                "Mention everyone",
                "Notify a whole channel with @everyone.",
            ),
            (
                Permission::ManageMessages,
                "Manage messages",
                "Delete anyone’s messages.",
            ),
            (
                Permission::StartServerDms,
                "Start direct messages",
                "Start a direct message with another member of the server.",
            ),
        ],
    ),
];

/// The permissions a role's editor offers, under their headings, the same on every shell.
#[must_use]
pub fn offered() -> Vec<PermissionGroup> {
    OFFERED
        .iter()
        .map(|(title, choices)| PermissionGroup {
            title: (*title).to_string(),
            permissions: choices
                .iter()
                .map(|(permission, label, detail)| PermissionChoice {
                    name: permission.as_str_name().to_string(),
                    label: (*label).to_string(),
                    detail: (*detail).to_string(),
                })
                .collect(),
        })
        .collect()
}

fn is_offered(permission: i32) -> bool {
    OFFERED
        .iter()
        .flat_map(|(_, choices)| choices.iter())
        .any(|(offered, _, _)| *offered as i32 == permission)
}

/// What a save of a role sends: `chosen`, the offered permissions switched on (names no
/// editor offers are dropped), and every permission the role `held` that no editor offers,
/// kept as it was.
#[must_use]
pub fn saved_permissions(chosen: &[String], held: &[i32]) -> Vec<i32> {
    let mut saved: Vec<i32> = chosen
        .iter()
        .filter_map(|name| Permission::from_str_name(name))
        .map(|p| p as i32)
        .filter(|p| is_offered(*p))
        .collect();
    saved.extend(held.iter().copied().filter(|p| !is_offered(*p)));
    saved.sort_unstable();
    saved.dedup();
    saved
}

/// This account's standing in a server: what its own membership and the server's roles say
/// it may do to a role or to a member.
#[derive(Debug, Clone, Default)]
pub struct Standing {
    owner: bool,
    permissions: Vec<i32>,
    /// The position of its highest role; `None` holding none, which is above nothing.
    highest: Option<u32>,
}

impl Standing {
    /// From the account's own membership in `server` (`None` when it has none) and the
    /// server's roles.
    #[must_use]
    pub fn of(server: &Server, own: Option<&Member>, roles: &[Role]) -> Self {
        let Some(own) = own else {
            return Self::default();
        };
        Self {
            owner: !server.owner_account_id.is_empty() && server.owner_account_id == own.account_id,
            permissions: own.permissions.clone(),
            highest: highest(&own.role_ids, roles),
        }
    }

    /// From what a kept snapshot says of the account in a server: whether it owns it,
    /// whether it holds `MANAGE_ROLES`, and its highest role's position. Enough to say which
    /// roles it manages, and nothing else.
    #[must_use]
    pub fn kept(owner: bool, manage_roles: bool, highest: Option<u32>) -> Self {
        Self {
            owner,
            permissions: if manage_roles {
                vec![Permission::ManageRoles as i32]
            } else {
                Vec::new()
            },
            highest,
        }
    }

    #[must_use]
    pub fn is_owner(&self) -> bool {
        self.owner
    }

    /// Whether it holds `wanted`: the owner and an administrator hold everything.
    #[must_use]
    pub fn holds(&self, wanted: Permission) -> bool {
        self.owner
            || self
                .permissions
                .iter()
                .any(|p| *p == wanted as i32 || *p == Permission::Administrator as i32)
    }

    /// The offered permissions it may switch on for a role: those it holds itself.
    #[must_use]
    pub fn grantable(&self) -> Vec<String> {
        OFFERED
            .iter()
            .flat_map(|(_, choices)| choices.iter())
            .filter(|(permission, _, _)| self.holds(*permission))
            .map(|(permission, _, _)| permission.as_str_name().to_string())
            .collect()
    }

    /// Whether it may change `role`, delete it, give it and take it away: `MANAGE_ROLES`
    /// and the role below its own highest, or the owner. `Everyone`'s permissions are
    /// anyone's with `MANAGE_ROLES` to change.
    #[must_use]
    pub fn manages_role(&self, role: &Role) -> bool {
        self.owner
            || (self.holds(Permission::ManageRoles)
                && (role.is_everyone || self.above(Some(role.position))))
    }

    /// Whether it may remove a member from the server and set their title: `MANAGE_MEMBERS`
    /// and the member below it, never the owner.
    #[must_use]
    pub fn manages_member(
        &self,
        their_account: &[u8],
        server: &Server,
        theirs: Option<u32>,
    ) -> bool {
        if their_account == server.owner_account_id.as_slice() {
            return false;
        }
        self.owner || (self.holds(Permission::ManageMembers) && self.above(theirs))
    }

    /// The instance's `above?`: holding no role is above nothing, and someone holding none
    /// is below everyone who holds one.
    fn above(&self, theirs: Option<u32>) -> bool {
        match (self.highest, theirs) {
            (None, _) => false,
            (Some(_), None) => true,
            (Some(mine), Some(theirs)) => mine < theirs,
        }
    }
}

/// The position of the highest of `role_ids` among `roles`; `None` holding none of them.
#[must_use]
pub fn highest(role_ids: &[Vec<u8>], roles: &[Role]) -> Option<u32> {
    roles
        .iter()
        .filter(|r| !r.is_everyone && role_ids.contains(&r.role_id))
        .map(|r| r.position)
        .min()
}

/// A server's roles as a shell lists them: in order, `Everyone` last and so named, each
/// with whether `standing` manages it.
#[must_use]
pub fn summaries(roles: &[Role], standing: &Standing) -> Vec<RoleSummary> {
    let mut ordered: Vec<&Role> = roles.iter().collect();
    ordered.sort_by_key(|r| (r.is_everyone, r.position));
    ordered
        .into_iter()
        .map(|r| RoleSummary {
            role_id: wire::display(&r.role_id),
            name: if r.is_everyone {
                EVERYONE.to_string()
            } else {
                r.name.clone()
            },
            color: if r.is_everyone {
                String::new()
            } else {
                r.color.clone()
            },
            position: r.position,
            is_everyone: r.is_everyone,
            hide_tag: r.hide_tag,
            permissions: r
                .permissions
                .iter()
                .filter_map(|p| Permission::try_from(*p).ok())
                .map(|p| p.as_str_name().to_string())
                .collect(),
            can_manage: standing.manages_role(r),
        })
        .collect()
}

/// Whether the tag beside a name in `server` is the member's title.
#[must_use]
pub fn tags_are_titles(server: &Server) -> bool {
    server.member_tag == MemberTag::Title as i32
}

/// The tag beside a member's name (architecture 7.5), from the server's `roles` as
/// [`summaries`] orders them. Where the server shows titles it is the member's title, in no
/// colour; where it shows roles, the member's highest role that is shown as a tag, never
/// `Everyone`, in the role's colour. There is no falling back from one to the other.
#[must_use]
pub fn tag(
    titles: bool,
    roles: &[RoleSummary],
    role_ids: &[String],
    title: &str,
) -> Option<TagSummary> {
    if titles {
        return Some(title.trim())
            .filter(|t| !t.is_empty())
            .map(|t| TagSummary {
                label: t.to_string(),
                color: String::new(),
            });
    }
    roles
        .iter()
        .filter(|r| !r.is_everyone && !r.hide_tag && role_ids.contains(&r.role_id))
        .min_by_key(|r| r.position)
        .map(|r| TagSummary {
            label: r.name.clone(),
            color: r.color.clone(),
        })
}

/// The position of the highest of a member's roles, by their ids as a shell has them.
#[must_use]
pub fn highest_of(role_ids: &[String], roles: &[RoleSummary]) -> Option<u32> {
    roles
        .iter()
        .filter(|r| !r.is_everyone && role_ids.contains(&r.role_id))
        .map(|r| r.position)
        .min()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id(n: u8) -> Vec<u8> {
        vec![n; 16]
    }

    fn role(n: u8, name: &str, position: u32) -> Role {
        Role {
            role_id: id(n),
            name: name.into(),
            position,
            ..Default::default()
        }
    }

    fn everyone() -> Role {
        Role {
            role_id: id(99),
            name: "@everyone".into(),
            position: 1_000_000,
            is_everyone: true,
            ..Default::default()
        }
    }

    fn server(owner: u8) -> Server {
        Server {
            owner_account_id: id(owner),
            ..Default::default()
        }
    }

    fn member(account: u8, roles: &[u8], permissions: &[Permission]) -> Member {
        Member {
            account_id: id(account),
            role_ids: roles.iter().map(|r| id(*r)).collect(),
            permissions: permissions.iter().map(|p| *p as i32).collect(),
            ..Default::default()
        }
    }

    /// admin (1), mods (2), helper (3), then Everyone.
    fn roles() -> Vec<Role> {
        vec![
            everyone(),
            role(3, "helper", 3),
            role(1, "admin", 1),
            role(2, "mods", 2),
        ]
    }

    #[test]
    fn roles_are_listed_in_order_with_everyone_last_and_so_named() {
        let own = member(7, &[2], &[Permission::ManageRoles]);
        let standing = Standing::of(&server(1), Some(&own), &roles());
        let listed = summaries(&roles(), &standing);
        let names: Vec<&str> = listed.iter().map(|r| r.name.as_str()).collect();
        assert_eq!(names, ["admin", "mods", "helper", EVERYONE]);
        // Its own highest and what is above it are not its to manage; what is below is,
        // and Everyone's permissions are.
        let manages: Vec<bool> = listed.iter().map(|r| r.can_manage).collect();
        assert_eq!(manages, [false, false, true, true]);
    }

    #[test]
    fn the_owner_manages_every_role_and_holding_none_manages_only_everyone() {
        let owner = member(1, &[], &[]);
        let standing = Standing::of(&server(1), Some(&owner), &roles());
        assert!(standing.is_owner());
        assert!(summaries(&roles(), &standing).iter().all(|r| r.can_manage));

        // MANAGE_ROLES from Everyone alone: above nothing.
        let plain = member(7, &[], &[Permission::ManageRoles]);
        let standing = Standing::of(&server(1), Some(&plain), &roles());
        let manages: Vec<bool> = summaries(&roles(), &standing)
            .iter()
            .map(|r| r.can_manage)
            .collect();
        assert_eq!(manages, [false, false, false, true]);

        let without = member(7, &[1], &[]);
        let standing = Standing::of(&server(1), Some(&without), &roles());
        assert!(summaries(&roles(), &standing).iter().all(|r| !r.can_manage));
    }

    #[test]
    fn a_member_is_managed_from_above_and_the_owner_by_nobody() {
        let server = server(1);
        let own = member(7, &[2], &[Permission::ManageMembers]);
        let standing = Standing::of(&server, Some(&own), &roles());
        let all = roles();
        // Below, and holding no role at all.
        assert!(standing.manages_member(&id(8), &server, highest(&[id(3)], &all)));
        assert!(standing.manages_member(&id(8), &server, None));
        // At its own height, above it, and the owner.
        assert!(!standing.manages_member(&id(8), &server, highest(&[id(2)], &all)));
        assert!(!standing.manages_member(&id(8), &server, highest(&[id(1)], &all)));
        assert!(!standing.manages_member(&id(1), &server, None));

        // An administrator is held to its place too; the owner is not.
        let admin = member(7, &[3], &[Permission::Administrator]);
        let standing = Standing::of(&server, Some(&admin), &all);
        assert!(!standing.manages_member(&id(8), &server, highest(&[id(2)], &all)));
        let owner = member(1, &[], &[]);
        let standing = Standing::of(&server, Some(&owner), &all);
        assert!(standing.manages_member(&id(8), &server, highest(&[id(1)], &all)));
        assert!(!standing.manages_member(&id(1), &server, None));
    }

    #[test]
    fn what_it_may_grant_is_what_it_holds_of_what_is_offered() {
        let own = member(
            7,
            &[2],
            &[Permission::ManageRoles, Permission::SendInThreads],
        );
        let standing = Standing::of(&server(1), Some(&own), &roles());
        assert_eq!(standing.grantable(), ["MANAGE_ROLES"]);

        let admin = member(7, &[2], &[Permission::Administrator]);
        let standing = Standing::of(&server(1), Some(&admin), &roles());
        assert_eq!(standing.grantable().len(), 14);
    }

    #[test]
    fn fourteen_permissions_are_offered_each_once() {
        let names: Vec<String> = offered()
            .into_iter()
            .flat_map(|g| g.permissions)
            .map(|p| p.name)
            .collect();
        assert_eq!(names.len(), 14);
        let mut unique = names.clone();
        unique.sort();
        unique.dedup();
        assert_eq!(unique.len(), 14);
        for hidden in [
            "SEND_IN_THREADS",
            "START_CALLS",
            "MANAGE_WEBHOOKS",
            "MANAGE_BOTS",
        ] {
            assert!(!names.iter().any(|n| n == hidden));
        }
    }

    #[test]
    fn a_save_keeps_what_no_editor_offers_as_it_was() {
        let held = [
            Permission::SendMessages as i32,
            Permission::SendInThreads as i32,
            Permission::StartCalls as i32,
        ];
        let chosen = [
            "ATTACH_FILES".to_string(),
            // Not offered: a shell cannot switch it on.
            "MANAGE_BOTS".to_string(),
            "NOT_A_PERMISSION".to_string(),
        ];
        assert_eq!(
            saved_permissions(&chosen, &held),
            [
                Permission::SendInThreads as i32,
                Permission::AttachFiles as i32,
                Permission::StartCalls as i32,
            ]
        );
    }

    fn listed() -> Vec<RoleSummary> {
        let mut all = roles();
        all[2].color = "mint".into();
        all[3].color = "sky".into();
        summaries(&all, &Standing::default())
    }

    fn ids(roles: &[u8]) -> Vec<String> {
        roles.iter().map(|r| wire::display(&id(*r))).collect()
    }

    #[test]
    fn a_role_tag_is_the_highest_role_shown_as_one() {
        let roles = listed();
        let tag_of = |held: &[u8]| tag(false, &roles, &ids(held), "Designer");
        assert_eq!(
            tag_of(&[3, 2]),
            Some(TagSummary {
                label: "mods".into(),
                color: "sky".into()
            })
        );
        assert_eq!(tag_of(&[1, 2, 3]).map(|t| t.label), Some("admin".into()));
        // Everyone is no tag, and holding nothing is none: the title does not stand in.
        assert_eq!(tag_of(&[]), None);
        assert_eq!(tag_of(&[99]), None);
    }

    #[test]
    fn a_role_not_shown_as_a_tag_gives_way_to_the_next() {
        let mut roles = listed();
        roles[0].hide_tag = true;
        assert_eq!(
            tag(false, &roles, &ids(&[1, 3]), "").map(|t| t.label),
            Some("helper".into())
        );
        assert_eq!(tag(false, &roles, &ids(&[1]), ""), None);
    }

    #[test]
    fn a_title_tag_has_no_colour_and_no_role_stands_in_for_it() {
        let roles = listed();
        assert_eq!(
            tag(true, &roles, &ids(&[1]), " Staff Engineer "),
            Some(TagSummary {
                label: "Staff Engineer".into(),
                color: String::new()
            })
        );
        assert_eq!(tag(true, &roles, &ids(&[1]), "  "), None);
    }
}
