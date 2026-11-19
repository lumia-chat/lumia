//! Roles and titles as a shell shows them (architecture 7.5): a server's roles in order with
//! what this account may do to each, the permissions a role's editor offers, and the tag
//! beside a member's name. Values in and values out, so the four shells offer the same and
//! tag a member the same. They say what to offer; the instance decides every write
//! (specs/authorization.md 3.3 and 3.5), and the rules here are its rules read the same way.

use lumia_core::proto::{Member, MemberTag, Permission, Role, Server, UpdateRoleRequest};
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

/// A role as its editor holds it: what the editor opened on, and what it holds at Save
/// ([`changes`] sends the difference).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RoleDraft {
    pub name: String,
    /// One of [`TONES`], or empty for none.
    pub color: String,
    /// Never a member's tag ("Show as tag" off).
    pub hide_tag: bool,
    /// The offered permissions switched on, by their wire names. One no editor offers
    /// counts for nothing here, in either draft.
    pub permissions: Vec<String>,
}

/// What an editor changed of a role (architecture 7.5): `was` is the role as the editor
/// opened on it and `now` what it holds at Save. The name when it differs once trimmed, the
/// colour and the tag switch when they differ, and the offered permissions switched on
/// (`grant`) and off (`revoke`). Whatever the editor did not touch is not named, so the
/// instance leaves it as it is, however it has changed since the editor opened; a permission
/// no editor offers is never named.
#[must_use]
pub fn changes(was: &RoleDraft, now: &RoleDraft) -> UpdateRoleRequest {
    let on = |draft: &RoleDraft| -> Vec<i32> {
        let mut on: Vec<i32> = draft
            .permissions
            .iter()
            .filter_map(|name| Permission::from_str_name(name))
            .map(|p| p as i32)
            .filter(|p| is_offered(*p))
            .collect();
        on.sort_unstable();
        on.dedup();
        on
    };
    let (before, after) = (on(was), on(now));
    let name = now.name.trim();
    UpdateRoleRequest {
        name: (name != was.name.trim()).then(|| name.to_string()),
        color: (now.color != was.color).then(|| now.color.clone()),
        hide_tag: (now.hide_tag != was.hide_tag).then_some(now.hide_tag),
        grant: after
            .iter()
            .copied()
            .filter(|p| !before.contains(p))
            .collect(),
        revoke: before
            .iter()
            .copied()
            .filter(|p| !after.contains(p))
            .collect(),
    }
}

/// Whether `request` names nothing: an editor saved as it opened.
#[must_use]
pub fn changes_nothing(request: &UpdateRoleRequest) -> bool {
    request.name.is_none()
        && request.color.is_none()
        && request.hide_tag.is_none()
        && request.grant.is_empty()
        && request.revoke.is_empty()
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
    /// whether it holds `MANAGE_ROLES` and `MANAGE_MEMBERS`, and its highest role's
    /// position. Enough to say which roles and which members it manages, and nothing else.
    #[must_use]
    pub fn kept(
        owner: bool,
        manage_roles: bool,
        manage_members: bool,
        highest: Option<u32>,
    ) -> Self {
        let held = [
            (manage_roles, Permission::ManageRoles),
            (manage_members, Permission::ManageMembers),
        ];
        Self {
            owner,
            permissions: held
                .into_iter()
                .filter(|(holds, _)| *holds)
                .map(|(_, permission)| permission as i32)
                .collect(),
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
    /// and the member below it, never the owner. `theirs` is the position of the member's
    /// highest role.
    #[must_use]
    pub fn manages_member(
        &self,
        their_account: &[u8],
        server: &Server,
        theirs: Option<u32>,
    ) -> bool {
        self.manages(their_account == server.owner_account_id.as_slice(), theirs)
    }

    /// [`Standing::manages_member`] for a caller that knows whether the member owns the
    /// server (`they_own`) and not the server itself: one that reads a kept snapshot.
    #[must_use]
    pub fn manages(&self, they_own: bool, theirs: Option<u32>) -> bool {
        !they_own && (self.owner || (self.holds(Permission::ManageMembers) && self.above(theirs)))
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

// ── What a shell offers (architecture 7.5) ──────────────────────────────
//
// The rules every shell's roles screens follow, and the words they say: which settings
// sections an account sees, a role's lock and moves, an editor's drafts and when it saves,
// a member's form and what its save asks, what is offered of a member, the marks beside a
// name, the members' search, and every sentence. `proto/vectors/roles/cases.json` holds the
// cases every client agrees on; the desktop and the phones ask these, and the web, whose page
// needs an answer at once and has the core in a worker, keeps a copy held to the same cases
// (as it does the viewer's).

/// A section of a server's settings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SettingsSection {
    Overview,
    Roles,
    Members,
}

/// The sections of a server's settings an account may use, in order: Overview with Manage
/// server, Roles with Manage roles, and Members with Manage members or Manage roles, since a
/// member's roles are changed there too. None, and the settings do not open.
#[must_use]
pub fn settings_sections(
    can_manage: bool,
    can_manage_roles: bool,
    can_manage_members: bool,
) -> Vec<SettingsSection> {
    let mut sections = Vec::new();
    if can_manage {
        sections.push(SettingsSection::Overview);
    }
    if can_manage_roles {
        sections.push(SettingsSection::Roles);
    }
    if can_manage_members || can_manage_roles {
        sections.push(SettingsSection::Members);
    }
    sections
}

/// What a section of a server's settings is called, in its list and as its page's title.
#[must_use]
pub fn section_label(section: SettingsSection) -> &'static str {
    match section {
        SettingsSection::Overview => "Overview",
        SettingsSection::Roles => "Roles",
        SettingsSection::Members => "Members",
    }
}

/// A role's or a tag's colour as one of [`TONES`]; `None` for none, and for anything that
/// is no tone.
#[must_use]
pub fn tone_of(color: &str) -> Option<&'static str> {
    TONES.iter().copied().find(|t| *t == color)
}

/// `n` with its thousands grouped by commas, as every count on these screens is written.
#[must_use]
pub fn grouped(n: u64) -> String {
    let digits = n.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

/// "5 roles": a server's roles counted without `Everyone`.
#[must_use]
pub fn roles_count(roles: &[RoleSummary]) -> String {
    let n = roles.iter().filter(|r| !r.is_everyone).count() as u64;
    if n == 1 {
        "1 role".to_string()
    } else {
        format!("{} roles", grouped(n))
    }
}

/// "1 member", "1,204 members".
#[must_use]
pub fn members_count(n: u64) -> String {
    if n == 1 {
        "1 member".to_string()
    } else {
        format!("{} members", grouped(n))
    }
}

/// How many of `members` (each by the role ids it holds) hold a role; every one of them holds
/// `Everyone`. `None` while the members are not known: a role is never said to be nobody's
/// before they are read.
#[must_use]
pub fn holders(role: &RoleSummary, members: Option<&[Vec<String>]>) -> Option<u64> {
    let members = members?;
    let n = if role.is_everyone {
        members.len()
    } else {
        members
            .iter()
            .filter(|held| held.contains(&role.role_id))
            .count()
    };
    Some(n as u64)
}

/// A role at or above this account's own highest: listed with a lock, and it does not open.
/// `Everyone` always opens.
#[must_use]
pub fn locked_role(role: &RoleSummary) -> bool {
    !role.is_everyone && !role.can_manage
}

/// Where a role may land when it is moved: before `before_id`, a role this account manages,
/// or before `Everyone`, which is last. A role is never put before one that is locked, since
/// those are at or above this account's own highest, and landing where it already is moves
/// nothing.
#[must_use]
pub fn lands_before(roles: &[RoleSummary], role_id: &str, before_id: &str) -> bool {
    let at = |id: &str| roles.iter().position(|r| r.role_id == id);
    let (Some(from), Some(to)) = (at(role_id), at(before_id)) else {
        return false;
    };
    let (moved, before) = (&roles[from], &roles[to]);
    if locked_role(moved) || moved.is_everyone || to == from || to == from + 1 {
        return false;
    }
    before.is_everyone || !locked_role(before)
}

/// The whole order a move sends: every role but `Everyone`, with `role_id` now before
/// `before_id`.
#[must_use]
pub fn moved_before(roles: &[RoleSummary], role_id: &str, before_id: &str) -> Vec<String> {
    let mut order = Vec::new();
    for r in roles {
        if r.role_id == role_id {
            continue;
        }
        if r.role_id == before_id {
            order.push(role_id.to_string());
        }
        if !r.is_everyone {
            order.push(r.role_id.clone());
        }
    }
    order
}

/// The role a step up (`up`) or down puts `role_id` before; `None` where there is no such
/// step (the highest it may go, or last above `Everyone`).
#[must_use]
pub fn step_before(roles: &[RoleSummary], role_id: &str, up: bool) -> Option<String> {
    let at = roles.iter().position(|r| r.role_id == role_id)?;
    let before = if up { at.checked_sub(1)? } else { at + 2 };
    let before = roles.get(before)?;
    lands_before(roles, role_id, &before.role_id).then(|| before.role_id.clone())
}

/// What "Moved" says to assistive technology once a role has moved: "Design moved to
/// position 3", the position counted from 1 among the roles but `Everyone`.
#[must_use]
pub fn moved_words(role_name: &str, position: u64) -> String {
    format!("{role_name} moved to position {position}")
}

/// The editor's fields for a role, or for a new one: no name, no colour, shown as a tag,
/// nothing granted.
#[must_use]
pub fn draft_of(role: Option<&RoleSummary>) -> RoleDraft {
    role.map_or_else(RoleDraft::default, |r| RoleDraft {
        name: r.name.clone(),
        color: r.color.clone(),
        hide_tag: r.hide_tag,
        permissions: r.permissions.clone(),
    })
}

fn same_set(a: &[String], b: &[String]) -> bool {
    a.len() == b.len() && a.iter().all(|x| b.contains(x))
}

/// Whether two drafts hold the same thing, the permissions in any order; `saved` compares the
/// names as a save would keep them, trimmed.
fn same_draft(a: &RoleDraft, b: &RoleDraft, saved: bool) -> bool {
    let name = |d: &RoleDraft| {
        if saved {
            d.name.trim().to_string()
        } else {
            d.name.clone()
        }
    };
    name(a) == name(b)
        && a.color == b.color
        && a.hide_tag == b.hide_tag
        && same_set(&a.permissions, &b.permissions)
}

/// Whether Save is offered. An editor holds two drafts: the role as the editor opened on it
/// (`opened`) and the one being edited. A new role, which has no `opened`, is saved once it
/// has a name; a role once the draft differs from `opened` and still has a name. The draft is
/// never compared with the role as it now is: what someone else changed of it meanwhile is
/// not this editor's to save, and does not light Save. A save sends the difference between
/// the two ([`changes`]) and nothing else. `Everyone`'s editor offers only its permissions,
/// and its name, the role's own, is never empty.
#[must_use]
pub fn savable(draft: &RoleDraft, opened: Option<&RoleDraft>) -> bool {
    if draft.name.trim().is_empty() {
        return false;
    }
    opened.is_none_or(|opened| !same_draft(draft, opened, true))
}

/// Whether an editor's draft still holds exactly what it opened on, its name as typed: what
/// says "Saved." may stay on screen, and an editor follows the role underneath ([`followed`]).
/// A space typed after the name is a touch, though there is nothing to save.
#[must_use]
pub fn untouched(opened: &RoleDraft, draft: &RoleDraft) -> bool {
    same_draft(draft, opened, false)
}

/// An editor's two drafts.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Drafts {
    pub opened: RoleDraft,
    pub draft: RoleDraft,
}

/// An editor's two drafts once the role under it is `now`. An editor nobody has touched
/// follows: while the draft still holds what it opened on, both become the role as it now is.
/// Once something was changed they stay as they are, so the save sends that change and
/// nothing else, whatever became of the rest of the role meanwhile.
#[must_use]
pub fn followed(opened: &RoleDraft, draft: &RoleDraft, now: &RoleDraft) -> Drafts {
    if !same_draft(draft, opened, false) || same_draft(now, opened, false) {
        return Drafts {
            opened: opened.clone(),
            draft: draft.clone(),
        };
    }
    Drafts {
        opened: now.clone(),
        draft: now.clone(),
    }
}

/// A permission this account does not hold, and so cannot give or take: its switch is shown
/// and not turned. `grantable` is the server's.
#[must_use]
pub fn withheld(grantable: &[String], permission: &str) -> bool {
    !grantable.iter().any(|p| p == permission)
}

/// The permission whose switch asks before it is turned on.
pub const ADMINISTRATOR: &str = "ADMINISTRATOR";

/// A role a member holds, as a card and a settings row list it: its name, on its tone or the
/// plain chip.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HeldRole {
    pub role_id: String,
    pub label: String,
    /// One of [`TONES`], or `None` for the plain chip.
    pub tone: Option<String>,
}

/// Every role a member holds, in the server's order, without `Everyone`.
#[must_use]
pub fn roles_held(roles: &[RoleSummary], role_ids: &[String]) -> Vec<HeldRole> {
    roles
        .iter()
        .filter(|r| !r.is_everyone && role_ids.contains(&r.role_id))
        .map(|r| HeldRole {
            role_id: r.role_id.clone(),
            label: r.name.clone(),
            tone: tone_of(&r.color).map(str::to_string),
        })
        .collect()
}

/// A role to tick in a member's form.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RoleChoice {
    pub id: String,
    pub name: String,
    pub tone: Option<String>,
    pub checked: bool,
    /// Not this account's to give or take: shown, not changeable.
    pub locked: bool,
}

/// A member's roles to tick: the server's in order without `Everyone`, each not this
/// account's to give or take marked.
#[must_use]
pub fn role_choices(roles: &[RoleSummary], held: &[String]) -> Vec<RoleChoice> {
    roles
        .iter()
        .filter(|r| !r.is_everyone)
        .map(|r| RoleChoice {
            id: r.role_id.clone(),
            name: r.name.clone(),
            tone: tone_of(&r.color).map(str::to_string),
            checked: held.contains(&r.role_id),
            locked: !r.can_manage,
        })
        .collect()
}

/// The roles a tick leaves a member with; a locked role, and `Everyone`, are not changed by
/// one.
#[must_use]
pub fn toggled_role(roles: &[RoleSummary], held: &[String], role_id: &str) -> Vec<String> {
    match roles.iter().find(|r| r.role_id == role_id) {
        Some(r) if !r.is_everyone && r.can_manage => {
            if held.iter().any(|h| h == role_id) {
                held.iter().filter(|h| *h != role_id).cloned().collect()
            } else {
                let mut next = held.to_vec();
                next.push(role_id.to_string());
                next
            }
        }
        _ => held.to_vec(),
    }
}

/// What a member's form holds, in their row in Members and in a card's two pops: the roles
/// ticked and the title typed.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemberForm {
    pub role_ids: Vec<String>,
    pub title: String,
}

fn same_form(a: &MemberForm, b: &MemberForm) -> bool {
    a.title == b.title && same_set(&a.role_ids, &b.role_ids)
}

/// What a save of a member's form asks, in one request ([`crate::app::App::change_member`]).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemberChanges {
    /// The roles ticked that the form did not open with.
    pub give: Vec<String>,
    /// The roles it opened with that are no longer ticked.
    pub take: Vec<String>,
    /// The title as it would be kept, where it differs from the one the form opened with;
    /// `None` leaves it as it is, and empty clears it.
    pub title: Option<String>,
}

/// What a member's form changed, against the form as it opened (`opened`), never against the
/// member as the list now has them: a role someone else gave them meanwhile is in neither list
/// and so is not taken, and one this form did not touch is not named. Roles and title go in
/// the one request, which the instance does whole or not at all.
#[must_use]
pub fn member_changes(opened: &MemberForm, form: &MemberForm) -> MemberChanges {
    let title = form.title.trim();
    MemberChanges {
        give: form
            .role_ids
            .iter()
            .filter(|id| !opened.role_ids.contains(id))
            .cloned()
            .collect(),
        take: opened
            .role_ids
            .iter()
            .filter(|id| !form.role_ids.contains(id))
            .cloned()
            .collect(),
        title: (title != opened.title.trim()).then(|| title.to_string()),
    }
}

/// Whether there is anything to ask: a form saved as it opened asks nothing.
#[must_use]
pub fn changes_any(changes: &MemberChanges) -> bool {
    !changes.give.is_empty() || !changes.take.is_empty() || changes.title.is_some()
}

/// Whether a member's form still holds exactly what it opened with.
#[must_use]
pub fn form_untouched(opened: &MemberForm, form: &MemberForm) -> bool {
    same_form(form, opened)
}

/// Whether a member may be offered a role in "Add members", from its editor: a guest is
/// given no roles, and one who holds it already has it. `role_ids` are the member's.
#[must_use]
pub fn may_be_given(role_id: &str, guest: bool, role_ids: &[String]) -> bool {
    !guest && !role_ids.iter().any(|id| id == role_id)
}

/// A member's form's two states.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Forms {
    pub opened: MemberForm,
    pub form: MemberForm,
}

/// A member's form once the list has them as `now`: one nobody has touched follows, as a
/// role's editor does ([`followed`]).
#[must_use]
pub fn member_followed(opened: &MemberForm, form: &MemberForm, now: &MemberForm) -> Forms {
    if !same_form(form, opened) || same_form(now, opened) {
        return Forms {
            opened: opened.clone(),
            form: form.clone(),
        };
    }
    Forms {
        opened: now.clone(),
        form: now.clone(),
    }
}

/// What this account may change of a member, on their row in Members and on their card.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemberOffers {
    /// Their roles to tick: with Manage roles, for anyone but a guest.
    pub roles: bool,
    /// In the roles' place, for a guest, where roles would otherwise be offered.
    pub roles_note: Option<String>,
    /// Their title: this account's own, or that of a member below it with Manage members.
    pub title: bool,
    /// Kick: with Manage members, a member below this account, never itself.
    pub kick: bool,
}

/// What this account may change of a member: `can_manage_roles` is the server's, `guest` and
/// `can_manage` the member's, `me` whether the member is this account.
#[must_use]
pub fn member_offers(
    can_manage_roles: bool,
    guest: bool,
    can_manage: bool,
    me: bool,
) -> MemberOffers {
    MemberOffers {
        roles: can_manage_roles && !guest,
        roles_note: (can_manage_roles && guest).then(|| GUEST_ROLES_NOTE.to_string()),
        title: me || can_manage,
        kick: !me && can_manage,
    }
}

/// Whether a member's row in Members opens at all: there is something of theirs to change.
#[must_use]
pub fn offers_any(offers: &MemberOffers) -> bool {
    offers.roles || offers.roles_note.is_some() || offers.title || offers.kick
}

/// The word beside a name on a card and in Members: the server's owner, or a guest.
#[must_use]
pub fn member_mark(owner: bool, guest: bool) -> Option<&'static str> {
    if owner {
        Some("Owner")
    } else if guest {
        Some("Guest")
    } else {
        None
    }
}

/// The words beside the name on a profile card, in order: "You" on this account's own, then
/// who they are to the server the card was opened in ("You" and "Owner", or "You" and
/// "Guest", where both are so). `member` is `(owner, guest)`, `None` where there is no member
/// (at Home, or before the server's members are read).
#[must_use]
pub fn card_marks(me: bool, member: Option<(bool, bool)>) -> Vec<String> {
    let mut marks = Vec::new();
    if me {
        marks.push("You".to_string());
    }
    if let Some(mark) = member.and_then(|(owner, guest)| member_mark(owner, guest)) {
        marks.push(mark.to_string());
    }
    marks
}

/// A member's tag as a message and a member row draw it: its words, and its tone, `None` for
/// the plain chip. `None` where there is no tag or it has no words.
#[must_use]
pub fn tag_of(tag: Option<&TagSummary>) -> Option<(String, Option<String>)> {
    let tag = tag.filter(|t| !t.label.is_empty())?;
    Some((tag.label.clone(), tone_of(&tag.color).map(str::to_string)))
}

/// Whether a member is found by `query` in Members: their display name, nickname, handle or
/// title has it in it, whatever its capitals, a leading `@` let go. Everyone is found by
/// nothing typed.
#[must_use]
pub fn found(query: &str, display_name: &str, nickname: &str, handle: &str, title: &str) -> bool {
    let q = query.trim().to_lowercase();
    let q = q.strip_prefix('@').unwrap_or(&q);
    q.is_empty()
        || [display_name, nickname, handle, title]
            .iter()
            .any(|s| s.to_lowercase().contains(q))
}

// The words, as every shell says them.

pub const ROLES_HINT: &str =
    "In order, highest first. Drag a role to move it. A new role starts last, just above Everyone.";
/// The same where a role is moved from its menu alone (a phone, a narrow window).
pub const ROLES_HINT_MENU: &str =
    "In order, highest first. A role is moved from its menu. A new role starts last, just above Everyone.";
pub const EVERYONE_SUBTITLE: &str =
    "What every member may do before any role adds to it. Guests are not among them.";
pub const TITLE_HINT: &str = "Up to 32 characters. A title grants nothing.";
pub const GUEST_ROLES_NOTE: &str = "Guests aren’t given roles.";
pub const DELETE_ROLE_HINT: &str = "Its members lose what it allowed. This can’t be undone.";
pub const TAG_HEADING: &str = "Tag beside names";
pub const TAG_SUBTITLE: &str =
    "What shows beside a member’s name in messages and in the member list.";
pub const TAG_NOTE: &str = "A profile shows a member’s title and roles either way.";
pub const TAG_ROLES_LABEL: &str = "Roles";
pub const TAG_ROLES_CAPTION: &str =
    "Each member’s highest role that is shown as a tag, in the role’s colour.";
pub const TAG_TITLES_LABEL: &str = "Titles";
pub const TAG_TITLES_CAPTION: &str =
    "Each member’s title, in no colour. A member without a title has no tag.";
/// Where a role's holders would be listed.
pub const NOBODY_HOLDS: &str = "Nobody holds this role yet.";
/// Where "Add members" would list who may be given a role.
pub const EVERYONE_HOLDS: &str = "Everyone who can hold this role already does.";
/// A search that found nobody, in Members and in "Add members".
pub const NO_MEMBER_MATCHES: &str = "No member matches that.";
pub const READING_MEMBERS: &str = "Reading the members…";
/// On a profile card, under Roles, for a member who holds none where they may be given some.
pub const NO_ROLES: &str = "No roles";
pub const ADMINISTRATOR_TITLE: &str = "Turn on Administrator?";

/// The title of what is asked before a role is deleted: "Delete Moderator?".
#[must_use]
pub fn delete_title(role_name: &str) -> String {
    format!("Delete {role_name}?")
}

/// Under a role's name field.
#[must_use]
pub fn role_name_hint(server_name: &str) -> String {
    format!("1 to 32 characters. No other role in {server_name} can have the same name.")
}

/// Under "Show as tag", as it stands.
#[must_use]
pub fn tag_switch_caption(shown: bool) -> &'static str {
    if shown {
        "This role can be the tag beside a member’s name."
    } else {
        "This role only carries permissions. It is nobody’s tag."
    }
}

/// A permission's line where its switch is not this account's to turn.
#[must_use]
pub fn withheld_caption(detail: &str) -> String {
    format!("{detail} You don’t hold it, so it is not yours to give or take.")
}

/// What is asked before Administrator is turned on: the lines under "Turn on
/// Administrator?". How many hold the role is said when there are some; a new role has none.
#[must_use]
pub fn administrator_question(
    server_name: &str,
    role_name: &str,
    held: Option<u64>,
) -> Vec<String> {
    let mut lines = vec![format!(
        "Administrator allows everything in {server_name}, in every channel, whatever a channel’s own permissions say."
    )];
    match held {
        Some(1) => lines.push(format!("1 member holds {role_name}.")),
        Some(n) if n > 1 => lines.push(format!("{} members hold {role_name}.", grouped(n))),
        _ => {}
    }
    lines
}

/// What is asked before a role is deleted: the lines under "Delete Moderator?". How many hold
/// it is said only when it is known: while the members are being read, or could not be,
/// nothing is said of them, since "Nobody holds this role." would be said of one forty hold.
#[must_use]
pub fn delete_question(held: Option<u64>) -> Vec<String> {
    let last = "This can’t be undone.".to_string();
    let first = match held {
        None => return vec![last],
        Some(0) => "Nobody holds this role.".to_string(),
        Some(1) => "1 member holds this role. They lose what it allowed.".to_string(),
        Some(n) => format!(
            "{} members hold this role. They lose what it allowed.",
            grouped(n)
        ),
    };
    vec![first, last]
}

/// Why a role was not saved, moved or deleted: the instance's `code` and `details.field`
/// (empty where there are none), its `message` for anything else.
#[must_use]
pub fn role_refusal(code: &str, field: &str, message: &str, server_name: &str) -> String {
    match (code, field) {
        ("PERMISSION_DENIED", _) => "That isn’t yours to change. A role at or above your own highest, or a permission you don’t hold, is not yours to give.".to_string(),
        ("ALREADY_EXISTS", _) => format!("Another role in {server_name} has that name."),
        ("NOT_FOUND", _) => "That role is gone. Someone deleted it.".to_string(),
        ("INVALID_ARGUMENT", "name") => "A role’s name is 1 to 32 characters on one line, with no hidden characters, and can’t be “everyone”.".to_string(),
        ("INVALID_ARGUMENT", "color") => "A role’s colour is one of the nine, or none.".to_string(),
        ("INVALID_ARGUMENT", "role_ids") => "The roles changed while you were moving one. Try again.".to_string(),
        ("FAILED_PRECONDITION", _) => "This server has as many roles as it can hold.".to_string(),
        _ => message.to_string(),
    }
}

/// Why a member's roles or title were not saved, or they were not kicked.
#[must_use]
pub fn member_refusal(code: &str, field: &str, message: &str) -> String {
    match (code, field) {
        ("PERMISSION_DENIED", _) => {
            "That isn’t yours to change. They may be at or above your own highest role.".to_string()
        }
        ("NOT_FOUND", _) => "They are no longer a member.".to_string(),
        ("FAILED_PRECONDITION", _) => "A member can’t hold that many roles.".to_string(),
        ("INVALID_ARGUMENT", "title") => {
            "A title is up to 32 characters on one line, with no hidden characters.".to_string()
        }
        // A member's own title, changed more often than the instance takes in an hour.
        ("RATE_LIMITED", _) => {
            "You’ve changed your title too many times this hour. Try again later.".to_string()
        }
        _ => message.to_string(),
    }
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

        // What a kept snapshot says of the account is enough to say the same.
        let kept = Standing::kept(false, false, true, Some(2));
        assert!(kept.manages(false, Some(3)) && kept.manages(false, None));
        assert!(!kept.manages(false, Some(2)) && !kept.manages(true, None));
        assert!(!Standing::kept(false, true, false, Some(1)).manages(false, None));
        assert!(!Standing::kept(false, false, true, None).manages(false, None));
        assert!(Standing::kept(true, false, false, None).manages(false, Some(1)));
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

    fn draft(name: &str, color: &str, hide_tag: bool, permissions: &[&str]) -> RoleDraft {
        RoleDraft {
            name: name.into(),
            color: color.into(),
            hide_tag,
            permissions: permissions.iter().map(|p| (*p).to_string()).collect(),
        }
    }

    #[test]
    fn an_editor_saves_what_it_changed_and_names_nothing_else() {
        let held = ["SEND_MESSAGES", "ATTACH_FILES"];
        let was = draft("Mods", "sky", false, &held);
        // Saved as it opened, a space typed after the name and the switches in another order.
        let same = draft("Mods ", "sky", false, &["ATTACH_FILES", "SEND_MESSAGES"]);
        assert!(changes_nothing(&changes(&was, &same)));

        // A rename names the name: no permission is sent, so one switched off elsewhere
        // since the editor opened stays off.
        let renamed = changes(&was, &draft(" Moderators ", "sky", false, &held));
        assert_eq!(renamed.name.as_deref(), Some("Moderators"));
        assert!(renamed.color.is_none() && renamed.hide_tag.is_none());
        assert!(renamed.grant.is_empty() && renamed.revoke.is_empty());

        // The colour cleared, the tag hidden, one permission on and one off.
        let request = changes(
            &was,
            &draft("Mods", "", true, &["SEND_MESSAGES", "MANAGE_MESSAGES"]),
        );
        assert_eq!(request.name, None);
        assert_eq!(request.color.as_deref(), Some(""));
        assert_eq!(request.hide_tag, Some(true));
        assert_eq!(request.grant, [Permission::ManageMessages as i32]);
        assert_eq!(request.revoke, [Permission::AttachFiles as i32]);
        assert!(!changes_nothing(&request));
    }

    #[test]
    fn a_permission_no_editor_offers_is_never_named() {
        // Held and not offered: in neither list, whichever draft has it.
        let was = draft("Bots", "", false, &["MANAGE_BOTS", "SEND_MESSAGES"]);
        let now = draft("Bots", "", false, &["START_CALLS", "NOT_A_PERMISSION"]);
        let request = changes(&was, &now);
        assert!(request.grant.is_empty());
        assert_eq!(request.revoke, [Permission::SendMessages as i32]);
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
