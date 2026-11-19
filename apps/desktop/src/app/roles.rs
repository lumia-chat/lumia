//! Roles and titles (architecture 7.5): the core's rules and words for every shell's roles
//! screens (`lumia_client::roles`, "What a shell offers", held to `proto/vectors/roles`),
//! asked with what this shell holds. The views and `role_settings` call the core directly
//! for its words and for anything asked of a role or a form; what is here hands it the
//! session's types (a server's and a member's summary, a refusal as the session reports it)
//! and keeps what is only this shell's: a tag's tone as the views draw it, a role menu's step,
//! and the sections' names in the list beside the page.

use lumia_client::app::{MemberSummary, ServerSummary};
use lumia_client::roles::{self as core, MemberForm, MemberOffers, RoleSummary};

use crate::session::Failure;

/// A section of a server's settings: the core's.
pub use lumia_client::roles::SettingsSection as Section;

/// What a section of a server's settings is called ([`core::section_label`]).
pub fn section_label(section: Section) -> &'static str {
    core::section_label(section)
}

/// The sections of `server`'s settings this account may use, in order
/// ([`core::settings_sections`]). None, and the settings do not open.
pub fn settings_sections(server: &ServerSummary) -> Vec<Section> {
    core::settings_sections(
        server.can_manage,
        server.can_manage_roles,
        server.can_manage_members,
    )
}

/// How many of `members` hold a role ([`core::holders`]); none until the members are known.
pub fn holders(role: &RoleSummary, members: Option<&[MemberSummary]>) -> Option<u64> {
    let held: Option<Vec<Vec<String>>> =
        members.map(|all| all.iter().map(|m| m.role_ids.clone()).collect());
    core::holders(role, held.as_deref())
}

/// A member's form as the server's list has them.
pub fn member_form(member: &MemberSummary) -> MemberForm {
    MemberForm {
        role_ids: member.role_ids.clone(),
        title: member.title.clone(),
    }
}

/// What this account may change of `member` in `server` ([`core::member_offers`]); `me`
/// when the member is this account.
pub fn member_offers(server: &ServerSummary, member: &MemberSummary, me: bool) -> MemberOffers {
    core::member_offers(server.can_manage_roles, member.guest, member.can_manage, me)
}

/// The word beside a member's name: Owner or Guest ([`core::member_mark`]).
pub fn member_mark(member: &MemberSummary) -> Option<&'static str> {
    core::member_mark(member.owner, member.guest)
}

/// The members `query` finds in Members and in "Add members" ([`core::found`]), in the
/// list's order; all of them for nothing typed.
pub fn searched<'a>(members: &'a [MemberSummary], query: &str) -> Vec<&'a MemberSummary> {
    members
        .iter()
        .filter(|m| core::found(query, &m.display_name, &m.nickname, &m.handle, &m.title))
        .collect()
}

fn field(failure: &Failure) -> &str {
    failure.details.get("field").map_or("", String::as_str)
}

/// Why a role was not saved, moved or deleted ([`core::role_refusal`]).
pub fn role_refusal(failure: &Failure, server_name: &str) -> String {
    core::role_refusal(&failure.code, field(failure), &failure.message, server_name)
}

/// Why a member's roles or title were not saved, or they were not kicked
/// ([`core::member_refusal`]).
pub fn member_refusal(failure: &Failure) -> String {
    core::member_refusal(&failure.code, field(failure), &failure.message)
}

/// What a tag is drawn on: a role's tone, or the plain chip.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TagTone {
    Tone(&'static str),
    Neutral,
}

impl TagTone {
    /// The tone the core named (one of the nine, or none), as the views draw it.
    pub fn of(tone: Option<&str>) -> Self {
        tone.and_then(core::tone_of)
            .map_or(Self::Neutral, Self::Tone)
    }
}

/// A member's tag as a message and a member row draw it ([`core::tag_of`]): its words, on
/// its tone or the plain chip.
pub fn tag_of(tag: Option<&core::TagSummary>) -> Option<(String, TagTone)> {
    core::tag_of(tag).map(|(words, tone)| (words, TagTone::of(tone.as_deref())))
}

/// A step a role's menu offers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    Up,
    Down,
}

/// The role a step up or down puts `role_id` before ([`core::step_before`]).
pub fn step_before(roles: &[RoleSummary], role_id: &str, step: Step) -> Option<String> {
    core::step_before(roles, role_id, step == Step::Up)
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;

    fn refused(code: &str, field: Option<&str>) -> Failure {
        Failure {
            code: code.into(),
            message: "said by the instance".into(),
            details: field
                .map(|f| HashMap::from([("field".to_string(), f.to_string())]))
                .unwrap_or_default(),
        }
    }

    #[test]
    fn a_refusal_hands_the_core_its_code_its_field_and_its_message() {
        assert_eq!(
            role_refusal(&refused("INVALID_ARGUMENT", Some("color")), "Acme"),
            "A role’s colour is one of the nine, or none."
        );
        assert_eq!(
            role_refusal(&refused("ALREADY_EXISTS", None), "Acme"),
            "Another role in Acme has that name."
        );
        assert_eq!(
            member_refusal(&refused("INVALID_ARGUMENT", Some("title"))),
            "A title is up to 32 characters on one line, with no hidden characters."
        );
        // No field: an INVALID_ARGUMENT the core has no words for is the instance's own.
        assert_eq!(
            member_refusal(&refused("INVALID_ARGUMENT", None)),
            "said by the instance"
        );
        let offline = Failure {
            message: "transport: offline".into(),
            ..Failure::default()
        };
        assert_eq!(role_refusal(&offline, "Acme"), "transport: offline");
    }

    #[test]
    fn a_server_and_a_member_are_handed_over_as_the_core_asks() {
        let server = ServerSummary {
            can_manage: false,
            can_manage_roles: false,
            can_manage_members: true,
            ..ServerSummary::default()
        };
        assert_eq!(settings_sections(&server), [Section::Members]);
        assert_eq!(section_label(Section::Members), "Members");
        let roles = ServerSummary {
            can_manage_roles: true,
            ..ServerSummary::default()
        };
        let guest = MemberSummary {
            guest: true,
            can_manage: true,
            ..MemberSummary::default()
        };
        let offers = member_offers(&roles, &guest, false);
        assert!(!offers.roles && offers.roles_note.is_some() && offers.title && offers.kick);
        assert!(!member_offers(&roles, &guest, true).kick);
        let owner = MemberSummary {
            owner: true,
            ..MemberSummary::default()
        };
        assert_eq!(member_mark(&owner), Some("Owner"));
        assert_eq!(member_mark(&guest), Some("Guest"));
    }

    #[test]
    fn holders_are_counted_from_the_members_roles() {
        let role = RoleSummary {
            role_id: "eng".into(),
            name: "Engineering".into(),
            color: String::new(),
            position: 2,
            is_everyone: false,
            hide_tag: false,
            permissions: Vec::new(),
            can_manage: true,
        };
        let holding = |ids: &[&str]| MemberSummary {
            role_ids: ids.iter().map(|id| (*id).to_string()).collect(),
            ..MemberSummary::default()
        };
        let members = [holding(&["eng"]), holding(&["eng", "design"]), holding(&[])];
        assert_eq!(holders(&role, Some(&members)), Some(2));
        assert_eq!(holders(&role, None), None);
        assert_eq!(
            member_form(&members[1]),
            MemberForm {
                role_ids: vec!["eng".into(), "design".into()],
                title: String::new(),
            }
        );
    }

    #[test]
    fn a_search_reads_each_members_name_handle_and_title() {
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
        assert_eq!(handles(""), ["devon", "maya"]);
        assert_eq!(handles("engineer"), ["devon"]);
        assert_eq!(handles("@maya"), ["maya"]);
    }

    #[test]
    fn a_tag_is_drawn_on_its_tone_or_the_plain_chip() {
        let tag = |label: &str, color: &str| core::TagSummary {
            label: label.into(),
            color: color.into(),
        };
        assert_eq!(
            tag_of(Some(&tag("Moderator", "green"))),
            Some(("Moderator".into(), TagTone::Tone("green")))
        );
        assert_eq!(
            tag_of(Some(&tag("Staff Engineer", ""))),
            Some(("Staff Engineer".into(), TagTone::Neutral))
        );
        assert_eq!(tag_of(None), None);
        assert_eq!(TagTone::of(Some("#ff0000")), TagTone::Neutral);
    }
}
