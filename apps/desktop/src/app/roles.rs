//! Roles and titles (architecture 7.5): what a server's settings, a role's editor, a member's
//! row and the profile card share that needs no window. The core says what this account may
//! do (each role's and member's `can_manage`, the server's `grantable` and its `can_manage…`
//! switches) and the instance decides every write; this reads those into what a screen
//! offers, and holds the words every screen says the same way.
//!
//! It is the web's `client/roles.ts` carried over as it stands: the same functions' answers
//! and the same words, character for character, with the cases of `roles.test.ts` under it.
//! Nothing here computes a permission, a tag or an order of its own, and no toolkit type
//! appears: `role_settings` holds the forms and the window's side.

use lumia_client::app::{MemberSummary, ServerSummary};
use lumia_client::roles::{RoleSummary, TagSummary};

use crate::session::Failure;

/// The nine tones a role may have, as the design system names them.
pub use lumia_client::roles::TONES;

/// The most characters a role's name or a title keeps.
pub const LABEL_MAX: usize = lumia_client::roles::LABEL_MAX;

/// A role's or a tag's colour as a tone; none for no colour, and for anything that is no
/// tone.
pub fn tone_of(color: &str) -> Option<&'static str> {
    TONES.iter().copied().find(|tone| *tone == color)
}

/// A section of a server's settings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Section {
    #[default]
    Overview,
    Roles,
    Members,
}

impl Section {
    /// Its name in the list beside the page.
    pub fn label(self) -> &'static str {
        match self {
            Self::Overview => "Overview",
            Self::Roles => "Roles",
            Self::Members => "Members",
        }
    }
}

/// The sections of a server's settings this account may use, in order: Overview with Manage
/// server, Roles with Manage roles, and Members with Manage members or Manage roles, since a
/// member's roles are changed there too. None, and the settings do not open.
pub fn settings_sections(server: &ServerSummary) -> Vec<Section> {
    let mut sections = Vec::new();
    if server.can_manage {
        sections.push(Section::Overview);
    }
    if server.can_manage_roles {
        sections.push(Section::Roles);
    }
    if server.can_manage_members || server.can_manage_roles {
        sections.push(Section::Members);
    }
    sections
}

/// A count as people write it in English: 1,204.
fn grouped(n: usize) -> String {
    let digits = n.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (i, digit) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(digit);
    }
    out
}

/// "5 roles": a server's roles counted without Everyone.
pub fn roles_count(roles: &[RoleSummary]) -> String {
    match roles.iter().filter(|r| !r.is_everyone).count() {
        1 => "1 role".into(),
        n => format!("{n} roles"),
    }
}

/// "1 member", "1,204 members".
pub fn members_count(n: usize) -> String {
    match n {
        1 => "1 member".into(),
        n => format!("{} members", grouped(n)),
    }
}

/// How many of `members` hold a role; Everyone is held by all of them. None until the
/// members are known.
pub fn holders(role: &RoleSummary, members: Option<&[MemberSummary]>) -> Option<usize> {
    let members = members?;
    Some(if role.is_everyone {
        members.len()
    } else {
        members
            .iter()
            .filter(|m| m.role_ids.contains(&role.role_id))
            .count()
    })
}

/// A role at or above this account's own highest: listed with a lock, and it does not open.
/// Everyone always opens.
pub fn locked_role(role: &RoleSummary) -> bool {
    !role.is_everyone && !role.can_manage
}

/// Where a role may land when it is moved: before `before_id`, a role this account manages,
/// or before Everyone, which is last. A role is never put before one that is locked, since
/// those are at or above this account's own highest, and landing where it already is moves
/// nothing.
pub fn lands_before(roles: &[RoleSummary], role_id: &str, before_id: &str) -> bool {
    let from = roles.iter().position(|r| r.role_id == role_id);
    let to = roles.iter().position(|r| r.role_id == before_id);
    let (Some(from), Some(to)) = (from, to) else {
        return false;
    };
    let (moved, before) = (&roles[from], &roles[to]);
    if locked_role(moved) || moved.is_everyone {
        return false;
    }
    if to == from || to == from + 1 {
        return false;
    }
    before.is_everyone || !locked_role(before)
}

/// The whole order a move sends: every role but Everyone, with `role_id` now before
/// `before_id`.
pub fn moved_before(roles: &[RoleSummary], role_id: &str, before_id: &str) -> Vec<String> {
    let mut order = Vec::with_capacity(roles.len());
    for role in roles {
        if role.role_id == role_id {
            continue;
        }
        if role.role_id == before_id {
            order.push(role_id.to_string());
        }
        if !role.is_everyone {
            order.push(role.role_id.clone());
        }
    }
    order
}

/// A step a role's menu offers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    Up,
    Down,
}

/// The role a step up or down puts `role_id` before; none where there is no such step (the
/// highest it may go, or last above Everyone).
pub fn step_before(roles: &[RoleSummary], role_id: &str, step: Step) -> Option<String> {
    let at = roles.iter().position(|r| r.role_id == role_id)?;
    let before = match step {
        Step::Up => roles.get(at.checked_sub(1)?),
        Step::Down => roles.get(at + 2),
    }?;
    lands_before(roles, role_id, &before.role_id).then(|| before.role_id.clone())
}

/// What a role's editor holds while it is open.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RoleDraft {
    pub name: String,
    /// A tone, or empty for none.
    pub color: String,
    pub hide_tag: bool,
    /// The permissions switched on, by their wire names.
    pub permissions: Vec<String>,
}

/// The editor's fields for a role, or for a new one: no name, no colour, shown as a tag,
/// nothing granted.
pub fn draft_of(role: Option<&RoleSummary>) -> RoleDraft {
    role.map_or_else(RoleDraft::default, |role| RoleDraft {
        name: role.name.clone(),
        color: role.color.clone(),
        hide_tag: role.hide_tag,
        permissions: role.permissions.clone(),
    })
}

fn same_set(a: &[String], b: &[String]) -> bool {
    a.len() == b.len() && a.iter().all(|x| b.contains(x))
}

/// Whether Save is offered: a new role once it has a name, a role once something of it
/// differs and it still has one. Of Everyone only the permissions are compared, since
/// nothing else of it changes.
pub fn savable(draft: &RoleDraft, role: Option<&RoleSummary>) -> bool {
    let name = draft.name.trim();
    let Some(role) = role else {
        return !name.is_empty();
    };
    if role.is_everyone {
        return !same_set(&draft.permissions, &role.permissions);
    }
    if name.is_empty() {
        return false;
    }
    name != role.name
        || draft.color != role.color
        || draft.hide_tag != role.hide_tag
        || !same_set(&draft.permissions, &role.permissions)
}

/// A permission this account does not hold, and so cannot give or take: its switch is shown
/// and not turned.
pub fn withheld(server: &ServerSummary, permission: &str) -> bool {
    !server.grantable.iter().any(|p| p == permission)
}

/// The permission whose switch asks before it is turned on.
pub const ADMINISTRATOR: &str = "ADMINISTRATOR";

/// A role a member holds, as a card and a settings row list it: its name, on its tone or the
/// plain chip.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeldRole {
    pub role_id: String,
    pub label: String,
    pub tone: Option<&'static str>,
}

/// Every role a member holds, in the server's order, without Everyone.
pub fn roles_held(roles: &[RoleSummary], role_ids: &[String]) -> Vec<HeldRole> {
    roles
        .iter()
        .filter(|r| !r.is_everyone && role_ids.contains(&r.role_id))
        .map(|r| HeldRole {
            role_id: r.role_id.clone(),
            label: r.name.clone(),
            tone: tone_of(&r.color),
        })
        .collect()
}

/// One of a member's roles to tick.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoleChoice {
    pub id: String,
    pub name: String,
    pub tone: Option<&'static str>,
    pub checked: bool,
    /// Not this account's to give or take: shown, not changeable.
    pub locked: bool,
}

/// A member's roles to tick: the server's in order without Everyone, each not this account's
/// to give or take marked.
pub fn role_choices(roles: &[RoleSummary], held: &[String]) -> Vec<RoleChoice> {
    roles
        .iter()
        .filter(|r| !r.is_everyone)
        .map(|r| RoleChoice {
            id: r.role_id.clone(),
            name: r.name.clone(),
            tone: tone_of(&r.color),
            checked: held.contains(&r.role_id),
            locked: !r.can_manage,
        })
        .collect()
}

/// The roles a tick leaves a member with; a locked role is not changed by one.
pub fn toggled_role(roles: &[RoleSummary], held: &[String], role_id: &str) -> Vec<String> {
    let changeable = roles
        .iter()
        .any(|r| r.role_id == role_id && !r.is_everyone && r.can_manage);
    if !changeable {
        return held.to_vec();
    }
    if held.iter().any(|id| id == role_id) {
        held.iter().filter(|id| *id != role_id).cloned().collect()
    } else {
        let mut held = held.to_vec();
        held.push(role_id.to_string());
        held
    }
}

/// What this account may change of a member, on their row in Members and on their card.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MemberOffers {
    /// Their roles to tick: with Manage roles, for anyone but a guest.
    pub roles: bool,
    /// In the roles' place, for a guest.
    pub roles_note: Option<&'static str>,
    /// Their title: this account's own, or that of a member below it with Manage members.
    pub title: bool,
    /// Kick: with Manage members, a member below this account, never itself.
    pub kick: bool,
}

pub fn member_offers(server: &ServerSummary, member: &MemberSummary, me: bool) -> MemberOffers {
    MemberOffers {
        roles: server.can_manage_roles && !member.guest,
        roles_note: (server.can_manage_roles && member.guest).then_some(GUEST_ROLES_NOTE),
        title: me || member.can_manage,
        kick: !me && member.can_manage,
    }
}

/// Whether a member's row in Members opens at all: there is something of theirs to change.
pub fn offers_any(offers: MemberOffers) -> bool {
    offers.roles || offers.roles_note.is_some() || offers.title || offers.kick
}

/// The word beside a name on a card and in Members: the server's owner, or a guest.
pub fn member_mark(member: &MemberSummary) -> Option<&'static str> {
    if member.owner {
        Some("Owner")
    } else if member.guest {
        Some("Guest")
    } else {
        None
    }
}

/// What a tag is drawn on: a role's tone, or the plain chip.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TagTone {
    Tone(&'static str),
    Neutral,
}

/// A member's tag as a message and a member row draw it: its words, on its tone or the plain
/// chip.
pub fn tag_of(tag: Option<&TagSummary>) -> Option<(&str, TagTone)> {
    let tag = tag.filter(|tag| !tag.label.is_empty())?;
    Some((
        tag.label.as_str(),
        tone_of(&tag.color).map_or(TagTone::Neutral, TagTone::Tone),
    ))
}

/// Members whose name, handle or title has `query` in it, whatever its capitals; all of them
/// for nothing typed.
pub fn searched<'a>(members: &'a [MemberSummary], query: &str) -> Vec<&'a MemberSummary> {
    let q = query.trim().to_lowercase();
    let q = q.strip_prefix('@').unwrap_or(&q);
    if q.is_empty() {
        return members.iter().collect();
    }
    members
        .iter()
        .filter(|m| {
            [&m.display_name, &m.nickname, &m.handle, &m.title]
                .into_iter()
                .any(|s| s.to_lowercase().contains(q))
        })
        .collect()
}

// ---- Words --------------------------------------------------------------------------------

pub const ROLES_HINT: &str =
    "In order, highest first. Drag a role to move it. A new role starts last, just above Everyone.";
/// The same where a role is moved from its menu alone (a phone, a narrow window).
pub const ROLES_HINT_MENU: &str = "In order, highest first. A role is moved from its menu. A new role starts last, just above Everyone.";
pub const EVERYONE_SUBTITLE: &str =
    "What every member may do before any role adds to it. Guests are not among them.";
pub const TITLE_HINT: &str = "Up to 32 characters. A title grants nothing.";
pub const GUEST_ROLES_NOTE: &str = "Guests aren’t given roles.";
pub const DELETE_ROLE_HINT: &str = "Its members lose what it allowed. This can’t be undone.";
pub const TAG_NOTE: &str = "A profile shows a member’s title and roles either way.";
/// The two things the tag beside a name can show: a label and what it means.
pub const TAG_ROLES: (&str, &str) = (
    "Roles",
    "Each member’s highest role that is shown as a tag, in the role’s colour.",
);
pub const TAG_TITLES: (&str, &str) = (
    "Titles",
    "Each member’s title, in no colour. A member without a title has no tag.",
);

pub fn role_name_hint(server_name: &str) -> String {
    format!("1 to 32 characters. No other role in {server_name} can have the same name.")
}

/// Under "Show as tag", as it stands.
pub fn tag_switch_caption(shown: bool) -> &'static str {
    if shown {
        "This role can be the tag beside a member’s name."
    } else {
        "This role only carries permissions. It is nobody’s tag."
    }
}

/// A permission's line where its switch is not this account's to turn.
pub fn withheld_caption(detail: &str) -> String {
    format!("{detail} You don’t hold it, so it is not yours to give or take.")
}

/// What is asked before Administrator is turned on: the lines under "Turn on
/// Administrator?". A new role has no holders to count.
pub fn administrator_question(
    server_name: &str,
    role_name: &str,
    held: Option<usize>,
) -> Vec<String> {
    let mut lines = vec![format!(
        "Administrator allows everything in {server_name}, in every channel, whatever a channel’s own permissions say."
    )];
    match held {
        None | Some(0) => {}
        Some(1) => lines.push(format!("1 member holds {role_name}.")),
        Some(n) => lines.push(format!("{} members hold {role_name}.", grouped(n))),
    }
    lines
}

/// What is asked before a role is deleted: the lines under "Delete Moderator?".
pub fn delete_question(held: Option<usize>) -> Vec<String> {
    let first = match held {
        None | Some(0) => "Nobody holds this role.".to_string(),
        Some(1) => "1 member holds this role. They lose what it allowed.".to_string(),
        Some(n) => format!(
            "{} members hold this role. They lose what it allowed.",
            grouped(n)
        ),
    };
    vec![first, "This can’t be undone.".to_string()]
}

fn field(failure: &Failure) -> Option<&str> {
    failure.details.get("field").map(String::as_str)
}

/// Why a role was not saved, moved or deleted.
pub fn role_refusal(failure: &Failure, server_name: &str) -> String {
    match (failure.code.as_str(), field(failure)) {
        ("PERMISSION_DENIED", _) => "That isn’t yours to change. A role at or above your own highest, or a permission you don’t hold, is not yours to give.".into(),
        ("ALREADY_EXISTS", _) => format!("Another role in {server_name} has that name."),
        ("NOT_FOUND", _) => "That role is gone. Someone deleted it.".into(),
        ("INVALID_ARGUMENT", Some("name")) => {
            "A role’s name is 1 to 32 characters, and can’t be “everyone”.".into()
        }
        ("INVALID_ARGUMENT", Some("color")) => "A role’s colour is one of the nine, or none.".into(),
        ("INVALID_ARGUMENT", Some("role_ids")) => {
            "The roles changed while you were moving one. Try again.".into()
        }
        ("FAILED_PRECONDITION", _) => "This server has as many roles as it can hold.".into(),
        _ => failure.message.clone(),
    }
}

/// Why a member's roles or title were not saved, or they were not kicked.
pub fn member_refusal(failure: &Failure) -> String {
    match (failure.code.as_str(), field(failure)) {
        ("PERMISSION_DENIED", _) => {
            "That isn’t yours to change. They may be at or above your own highest role.".into()
        }
        ("NOT_FOUND", _) => "They are no longer a member.".into(),
        ("FAILED_PRECONDITION", _) => "A member can’t hold that many roles.".into(),
        ("INVALID_ARGUMENT", Some("title")) => {
            "A title is up to 32 characters, on one line.".into()
        }
        _ => failure.message.clone(),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;

    fn role(id: &str) -> RoleSummary {
        RoleSummary {
            role_id: id.into(),
            name: id.into(),
            color: String::new(),
            position: 0,
            is_everyone: false,
            hide_tag: false,
            permissions: Vec::new(),
            can_manage: true,
        }
    }

    fn ids(list: &[&str]) -> Vec<String> {
        list.iter().map(|id| (*id).to_string()).collect()
    }

    /// Acme as Devon sees it: Admin and Moderator are at or above Devon's highest.
    fn acme() -> Vec<RoleSummary> {
        vec![
            RoleSummary {
                color: "indigo".into(),
                can_manage: false,
                ..role("admin")
            },
            RoleSummary {
                color: "green".into(),
                can_manage: false,
                ..role("mod")
            },
            RoleSummary {
                color: "peach".into(),
                ..role("design")
            },
            RoleSummary {
                color: "sky".into(),
                ..role("eng")
            },
            RoleSummary {
                hide_tag: true,
                ..role("invite")
            },
            RoleSummary {
                name: "Everyone".into(),
                is_everyone: true,
                ..role("everyone")
            },
        ]
    }

    fn server(manage: bool, roles: bool, members: bool) -> ServerSummary {
        ServerSummary {
            can_manage: manage,
            can_manage_roles: roles,
            can_manage_members: members,
            ..ServerSummary::default()
        }
    }

    fn holding(role_ids: &[&str]) -> MemberSummary {
        MemberSummary {
            role_ids: ids(role_ids),
            ..MemberSummary::default()
        }
    }

    fn refused(code: &str, field: Option<&str>) -> Failure {
        Failure {
            code: code.into(),
            message: String::new(),
            details: field
                .map(|f| HashMap::from([("field".to_string(), f.to_string())]))
                .unwrap_or_default(),
        }
    }

    // ---- sections ----

    #[test]
    fn each_permission_opens_its_own_and_manage_roles_opens_members_too() {
        use Section::{Members, Overview, Roles};
        assert_eq!(
            settings_sections(&server(true, true, true)),
            [Overview, Roles, Members]
        );
        assert_eq!(settings_sections(&server(true, false, false)), [Overview]);
        assert_eq!(
            settings_sections(&server(false, true, false)),
            [Roles, Members]
        );
        assert_eq!(settings_sections(&server(false, false, true)), [Members]);
        assert_eq!(settings_sections(&server(false, false, false)), []);
    }

    // ---- the list ----

    #[test]
    fn counts_roles_without_everyone() {
        assert_eq!(roles_count(&acme()), "5 roles");
        let one = [
            role("a"),
            RoleSummary {
                is_everyone: true,
                ..role("everyone")
            },
        ];
        assert_eq!(roles_count(&one), "1 role");
        assert_eq!(members_count(1), "1 member");
        assert_eq!(members_count(1204), "1,204 members");
    }

    #[test]
    fn counts_who_holds_a_role_and_everyone_holds_everyone() {
        let acme = acme();
        let members = [holding(&["eng"]), holding(&["eng", "design"]), holding(&[])];
        assert_eq!(holders(&acme[3], Some(&members)), Some(2));
        assert_eq!(holders(&acme[5], Some(&members)), Some(3));
        assert_eq!(holders(&acme[3], None), None);
    }

    #[test]
    fn a_role_not_managed_is_locked_and_everyone_never_is() {
        let acme = acme();
        assert!(locked_role(&acme[0]));
        assert!(!locked_role(&acme[2]));
        assert!(!locked_role(&RoleSummary {
            is_everyone: true,
            can_manage: false,
            ..role("everyone")
        }));
    }

    #[test]
    fn a_tone_is_one_of_the_nine() {
        assert_eq!(tone_of("sky"), Some("sky"));
        assert_eq!(tone_of(""), None);
        assert_eq!(tone_of("#ff0000"), None);
    }

    // ---- moving a role ----

    #[test]
    fn lands_before_a_role_it_manages_or_before_everyone() {
        let acme = acme();
        assert!(lands_before(&acme, "eng", "design"));
        assert!(lands_before(&acme, "design", "everyone"));
        assert!(lands_before(&acme, "design", "invite"));
    }

    #[test]
    fn never_before_a_locked_role_never_where_it_is_and_a_locked_role_and_everyone_do_not_move() {
        let acme = acme();
        assert!(!lands_before(&acme, "eng", "mod"));
        assert!(!lands_before(&acme, "eng", "admin"));
        assert!(!lands_before(&acme, "eng", "eng"));
        assert!(!lands_before(&acme, "eng", "invite"));
        assert!(!lands_before(&acme, "mod", "everyone"));
        assert!(!lands_before(&acme, "everyone", "design"));
        assert!(!lands_before(&acme, "gone", "design"));
    }

    #[test]
    fn the_order_sent_is_every_role_but_everyone() {
        let acme = acme();
        assert_eq!(
            moved_before(&acme, "eng", "design"),
            ["admin", "mod", "eng", "design", "invite"]
        );
        assert_eq!(
            moved_before(&acme, "design", "everyone"),
            ["admin", "mod", "eng", "invite", "design"]
        );
    }

    #[test]
    fn a_step_up_or_down_where_there_is_one() {
        let acme = acme();
        assert_eq!(
            step_before(&acme, "eng", Step::Up).as_deref(),
            Some("design")
        );
        assert_eq!(
            step_before(&acme, "eng", Step::Down).as_deref(),
            Some("everyone")
        );
        assert_eq!(
            step_before(&acme, "design", Step::Down).as_deref(),
            Some("invite")
        );
        // Design is the highest Devon manages, and Can invite the last above Everyone.
        assert_eq!(step_before(&acme, "design", Step::Up), None);
        assert_eq!(step_before(&acme, "invite", Step::Down), None);
        assert_eq!(step_before(&acme, "admin", Step::Down), None);
    }

    // ---- the editor ----

    fn moderator() -> RoleSummary {
        RoleSummary {
            name: "Moderator".into(),
            color: "green".into(),
            permissions: ids(&["MANAGE_ROLES", "SEND_IN_THREADS"]),
            ..role("mod")
        }
    }

    #[test]
    fn a_new_role_saves_once_it_has_a_name() {
        assert!(!savable(&draft_of(None), None));
        let named = |name: &str| RoleDraft {
            name: name.into(),
            ..draft_of(None)
        };
        assert!(!savable(&named("  "), None));
        assert!(savable(&named("Design"), None));
    }

    #[test]
    fn a_role_saves_once_something_differs() {
        let moderator = moderator();
        let draft = || draft_of(Some(&moderator));
        let named = |name: &str| RoleDraft {
            name: name.into(),
            ..draft()
        };
        let granting = |permissions: &[&str]| RoleDraft {
            permissions: ids(permissions),
            ..draft()
        };
        assert!(!savable(&draft(), Some(&moderator)));
        assert!(!savable(&named(" Moderator "), Some(&moderator)));
        assert!(savable(&named("Mods"), Some(&moderator)));
        assert!(!savable(&named(""), Some(&moderator)));
        assert!(savable(
            &RoleDraft {
                color: String::new(),
                ..draft()
            },
            Some(&moderator)
        ));
        assert!(savable(
            &RoleDraft {
                hide_tag: true,
                ..draft()
            },
            Some(&moderator)
        ));
        assert!(!savable(
            &granting(&["SEND_IN_THREADS", "MANAGE_ROLES"]),
            Some(&moderator)
        ));
        assert!(savable(&granting(&["MANAGE_ROLES"]), Some(&moderator)));
    }

    #[test]
    fn of_everyone_only_the_permissions_count() {
        let everyone = RoleSummary {
            name: "Everyone".into(),
            is_everyone: true,
            permissions: ids(&["VIEW_CHANNEL"]),
            ..role("everyone")
        };
        assert!(!savable(&draft_of(Some(&everyone)), Some(&everyone)));
        assert!(savable(
            &RoleDraft {
                permissions: Vec::new(),
                ..draft_of(Some(&everyone))
            },
            Some(&everyone)
        ));
    }

    #[test]
    fn a_permission_not_held_is_withheld() {
        let server = ServerSummary {
            grantable: ids(&["MANAGE_ROLES"]),
            ..ServerSummary::default()
        };
        assert!(withheld(&server, "ADMINISTRATOR"));
        assert!(!withheld(&server, "MANAGE_ROLES"));
    }

    #[test]
    fn the_questions_count_who_holds_the_role() {
        assert_eq!(
            administrator_question("Acme", "Moderator", Some(3)),
            [
                "Administrator allows everything in Acme, in every channel, whatever a channel’s own permissions say.",
                "3 members hold Moderator."
            ]
        );
        assert_eq!(
            administrator_question("Acme", "Moderator", Some(1))[1],
            "1 member holds Moderator."
        );
        assert_eq!(administrator_question("Acme", "", None).len(), 1);
        assert_eq!(
            administrator_question("Acme", "Moderator", Some(0)).len(),
            1
        );
        assert_eq!(
            delete_question(Some(3)),
            [
                "3 members hold this role. They lose what it allowed.",
                "This can’t be undone."
            ]
        );
        assert_eq!(
            delete_question(Some(1))[0],
            "1 member holds this role. They lose what it allowed."
        );
        assert_eq!(delete_question(Some(0))[0], "Nobody holds this role.");
    }

    // ---- a member ----

    #[test]
    fn their_roles_in_the_servers_order_without_everyone() {
        let held = |id: &str, tone| HeldRole {
            role_id: id.into(),
            label: id.into(),
            tone,
        };
        assert_eq!(
            roles_held(&acme(), &ids(&["invite", "everyone", "design"])),
            [held("design", Some("peach")), held("invite", None)]
        );
    }

    #[test]
    fn the_roles_to_tick_mark_what_is_not_this_accounts_to_give() {
        let choices = role_choices(&acme(), &ids(&["mod", "eng"]));
        let named = |keep: fn(&RoleChoice) -> bool| -> Vec<&str> {
            choices
                .iter()
                .filter(|c| keep(c))
                .map(|c| c.id.as_str())
                .collect()
        };
        assert_eq!(named(|_| true), ["admin", "mod", "design", "eng", "invite"]);
        assert_eq!(named(|c| c.checked), ["mod", "eng"]);
        assert_eq!(named(|c| c.locked), ["admin", "mod"]);
    }

    #[test]
    fn a_tick_gives_and_takes_and_leaves_a_locked_role_as_it_was() {
        let acme = acme();
        assert_eq!(
            toggled_role(&acme, &ids(&["eng"]), "design"),
            ["eng", "design"]
        );
        assert_eq!(
            toggled_role(&acme, &ids(&["eng", "design"]), "eng"),
            ["design"]
        );
        assert_eq!(toggled_role(&acme, &ids(&["mod"]), "mod"), ["mod"]);
        assert!(toggled_role(&acme, &[], "admin").is_empty());
        assert!(toggled_role(&acme, &[], "everyone").is_empty());
    }

    #[test]
    fn what_is_offered_of_a_member() {
        let roles = server(false, true, false);
        let none = server(false, false, false);
        let member = |guest, can_manage| MemberSummary {
            guest,
            can_manage,
            ..MemberSummary::default()
        };
        let offers = |roles, roles_note, title, kick| MemberOffers {
            roles,
            roles_note,
            title,
            kick,
        };
        assert_eq!(
            member_offers(&roles, &member(false, true), false),
            offers(true, None, true, true)
        );
        // Above this account: their roles below its own can still be given and taken.
        assert_eq!(
            member_offers(&roles, &member(false, false), false),
            offers(true, None, false, false)
        );
        assert_eq!(
            member_offers(&roles, &member(true, true), false),
            offers(false, Some("Guests aren’t given roles."), true, true)
        );
        assert_eq!(
            member_offers(&none, &member(false, true), false),
            offers(false, None, true, true)
        );
        // This account itself: its title is its own, and it is never offered Kick.
        assert_eq!(
            member_offers(&none, &member(false, false), true),
            offers(false, None, true, false)
        );
        assert!(!offers_any(member_offers(
            &none,
            &member(false, false),
            false
        )));
        assert!(offers_any(member_offers(
            &roles,
            &member(true, false),
            false
        )));
    }

    #[test]
    fn the_owner_and_a_guest_are_marked() {
        let member = |owner, guest| MemberSummary {
            owner,
            guest,
            ..MemberSummary::default()
        };
        assert_eq!(member_mark(&member(true, false)), Some("Owner"));
        assert_eq!(member_mark(&member(false, true)), Some("Guest"));
        assert_eq!(member_mark(&member(false, false)), None);
    }

    #[test]
    fn a_tag_on_its_tone_or_the_plain_chip() {
        let tag = |label: &str, color: &str| TagSummary {
            label: label.into(),
            color: color.into(),
        };
        assert_eq!(
            tag_of(Some(&tag("Moderator", "green"))),
            Some(("Moderator", TagTone::Tone("green")))
        );
        assert_eq!(
            tag_of(Some(&tag("Staff Engineer", ""))),
            Some(("Staff Engineer", TagTone::Neutral))
        );
        assert_eq!(tag_of(None), None);
        assert_eq!(tag_of(Some(&tag("", "green"))), None);
    }

    #[test]
    fn a_search_reads_the_name_the_handle_and_the_title() {
        let person = |name: &str, handle: &str, title: &str| MemberSummary {
            display_name: name.into(),
            handle: handle.into(),
            title: title.into(),
            ..MemberSummary::default()
        };
        let people = [
            person("Devon Park", "devon", "Staff Engineer"),
            person("Maya Chen", "maya", ""),
        ];
        let handles = |query: &str| -> Vec<&str> {
            searched(&people, query)
                .into_iter()
                .map(|p| p.handle.as_str())
                .collect()
        };
        assert_eq!(searched(&people, "").len(), 2);
        assert_eq!(handles("ENGINEER"), ["devon"]);
        assert_eq!(handles("@maya"), ["maya"]);
        assert_eq!(handles("chen"), ["maya"]);
        assert!(searched(&people, "nobody").is_empty());
    }

    // ---- refusals ----

    #[test]
    fn a_roles_refusal() {
        assert_eq!(
            role_refusal(&refused("ALREADY_EXISTS", None), "Acme"),
            "Another role in Acme has that name."
        );
        assert!(
            role_refusal(&refused("INVALID_ARGUMENT", Some("name")), "Acme")
                .contains("1 to 32 characters")
        );
        assert!(
            role_refusal(&refused("INVALID_ARGUMENT", Some("role_ids")), "Acme")
                .contains("Try again")
        );
        assert!(
            role_refusal(&refused("PERMISSION_DENIED", None), "Acme")
                .contains("isn’t yours to change")
        );
        let offline = Failure {
            message: "transport: offline".into(),
            ..Failure::default()
        };
        assert_eq!(role_refusal(&offline, "Acme"), "transport: offline");
    }

    #[test]
    fn a_members_refusal() {
        assert_eq!(
            member_refusal(&refused("NOT_FOUND", None)),
            "They are no longer a member."
        );
        assert!(
            member_refusal(&refused("INVALID_ARGUMENT", Some("title"))).contains("32 characters")
        );
        let nope = Failure {
            message: "nope".into(),
            ..Failure::default()
        };
        assert_eq!(member_refusal(&nope), "nope");
    }
}
