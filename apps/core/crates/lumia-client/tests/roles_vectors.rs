//! Cross-implementation roles vectors from `proto/vectors/roles/cases.json` (architecture
//! 7.5): what every client's roles screens offer and say. Every client agrees on every case;
//! the web's `client/roles.ts` is held to the same file.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::too_many_lines)]

use std::collections::BTreeMap;

use lumia_client::roles::{
    self, MemberChanges, MemberForm, MemberOffers, RoleDraft, RoleSummary, TagSummary,
};
use serde::Deserialize;
use serde_json::{json, Value};

const PATH: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../../../proto/vectors/roles/cases.json"
);

#[derive(Deserialize)]
struct File {
    roles: BTreeMap<String, Vec<Value>>,
    words: BTreeMap<String, String>,
    cases: Vec<Case>,
}

#[derive(Deserialize)]
struct Case {
    rule: String,
    args: Value,
    answer: Value,
    why: String,
}

fn file() -> File {
    let text = std::fs::read_to_string(PATH).unwrap_or_else(|e| panic!("{PATH}: {e}"));
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("{PATH}: {e}"))
}

fn role(v: &Value) -> RoleSummary {
    RoleSummary {
        role_id: v["roleId"].as_str().unwrap().to_string(),
        name: v["name"].as_str().unwrap().to_string(),
        color: v["color"].as_str().unwrap().to_string(),
        position: 0,
        is_everyone: v["isEveryone"].as_bool().unwrap(),
        hide_tag: v["hideTag"].as_bool().unwrap(),
        permissions: serde_json::from_value(v["permissions"].clone()).unwrap(),
        can_manage: v["canManage"].as_bool().unwrap(),
    }
}

/// A case's roles: a list named in the file's `roles`, or written out.
fn roles_of(file: &File, v: &Value) -> Vec<RoleSummary> {
    match v {
        Value::String(name) => file.roles[name].iter().map(role).collect(),
        Value::Array(list) => list.iter().map(role).collect(),
        other => panic!("no roles in {other}"),
    }
}

fn of<T: for<'de> Deserialize<'de>>(v: &Value) -> T {
    serde_json::from_value(v.clone()).unwrap_or_else(|e| panic!("{v}: {e}"))
}

fn s(v: &Value) -> &str {
    v.as_str().unwrap_or_else(|| panic!("{v} is no string"))
}

fn held(v: &Value) -> Option<u64> {
    v.as_u64()
}

/// What `lumia_client::roles` answers for a case, as the file writes an answer.
fn answer(file: &File, case: &Case) -> Value {
    let a = &case.args;
    match case.rule.as_str() {
        "settingsSections" => json!(roles::settings_sections(
            a["canManage"].as_bool().unwrap(),
            a["canManageRoles"].as_bool().unwrap(),
            a["canManageMembers"].as_bool().unwrap(),
        )),
        "sectionLabel" => json!(roles::section_label(of(&a["section"]))),
        "untouched" => json!(roles::untouched(&of(&a["opened"]), &of(&a["draft"]))),
        "givenTo" => {
            let people: Vec<Value> = of(&a["members"]);
            let members: Vec<(bool, Vec<String>)> = people
                .iter()
                .map(|p| (p["guest"].as_bool().unwrap(), of(&p["roleIds"])))
                .collect();
            json!(roles::given_to(s(&a["roleId"]), &members))
        }
        "mayBeGiven" => json!(roles::may_be_given(
            s(&a["roleId"]),
            a["guest"].as_bool().unwrap(),
            &of::<Vec<String>>(&a["roleIds"])
        )),
        "holderCounts" => json!(roles::holder_counts(
            &roles_of(file, &a["roles"]),
            &of::<Vec<Vec<String>>>(&a["members"])
        )),
        "searched" => {
            let people: Vec<Value> = of(&a["members"]);
            let fields: Vec<[&str; 4]> = people
                .iter()
                .map(|p| {
                    [
                        s(&p["displayName"]),
                        s(&p["nickname"]),
                        s(&p["handle"]),
                        s(&p["title"]),
                    ]
                })
                .collect();
            json!(roles::searched(s(&a["query"]), &fields))
        }
        "toneOf" => json!(roles::tone_of(s(&a["color"]))),
        "rolesCount" => json!(roles::roles_count(&roles_of(file, &a["roles"]))),
        "membersCount" => json!(roles::members_count(a["n"].as_u64().unwrap())),
        "grouped" => json!(roles::grouped(a["n"].as_u64().unwrap())),
        "holders" => {
            let list = roles_of(file, &a["roles"]);
            let role = list.iter().find(|r| r.role_id == s(&a["roleId"])).unwrap();
            let members: Option<Vec<Vec<String>>> = of(&a["members"]);
            json!(roles::holders(role, members.as_deref()))
        }
        "lockedRole" => {
            let list = roles_of(file, &a["roles"]);
            json!(roles::locked_role(
                list.iter().find(|r| r.role_id == s(&a["roleId"])).unwrap()
            ))
        }
        "landsBefore" => json!(roles::lands_before(
            &roles_of(file, &a["roles"]),
            s(&a["roleId"]),
            s(&a["beforeId"])
        )),
        "movedBefore" => json!(roles::moved_before(
            &roles_of(file, &a["roles"]),
            s(&a["roleId"]),
            s(&a["beforeId"])
        )),
        "stepBefore" => json!(roles::step_before(
            &roles_of(file, &a["roles"]),
            s(&a["roleId"]),
            a["up"].as_bool().unwrap()
        )),
        "movedWords" => json!(roles::moved_words(
            s(&a["roleName"]),
            a["position"].as_u64().unwrap()
        )),
        "savable" => {
            let opened: Option<RoleDraft> = of(&a["opened"]);
            json!(roles::savable(&of(&a["draft"]), opened.as_ref()))
        }
        "followed" => json!(roles::followed(
            &of(&a["opened"]),
            &of(&a["draft"]),
            &of(&a["now"])
        )),
        "withheld" => json!(roles::withheld(
            &of::<Vec<String>>(&a["grantable"]),
            s(&a["permission"])
        )),
        "rolesHeld" => json!(roles::roles_held(
            &roles_of(file, &a["roles"]),
            &of::<Vec<String>>(&a["roleIds"])
        )),
        "roleChoices" => json!(roles::role_choices(
            &roles_of(file, &a["roles"]),
            &of::<Vec<String>>(&a["held"])
        )),
        "toggledRole" => json!(roles::toggled_role(
            &roles_of(file, &a["roles"]),
            &of::<Vec<String>>(&a["held"]),
            s(&a["roleId"])
        )),
        "memberChanges" => json!(roles::member_changes(
            &of::<MemberForm>(&a["opened"]),
            &of::<MemberForm>(&a["form"])
        )),
        "changesAny" => json!(roles::changes_any(&of::<MemberChanges>(&a["changes"]))),
        "memberFollowed" => json!(roles::member_followed(
            &of(&a["opened"]),
            &of(&a["form"]),
            &of(&a["now"])
        )),
        "memberOffers" => json!(roles::member_offers(
            a["canManageRoles"].as_bool().unwrap(),
            a["guest"].as_bool().unwrap(),
            a["canManage"].as_bool().unwrap(),
            a["me"].as_bool().unwrap(),
        )),
        "offersAny" => json!(roles::offers_any(&of::<MemberOffers>(&a["offers"]))),
        "memberMark" => json!(roles::member_mark(
            a["owner"].as_bool().unwrap(),
            a["guest"].as_bool().unwrap()
        )),
        "cardMarks" => {
            let member = a["member"]
                .as_object()
                .map(|m| (m["owner"].as_bool().unwrap(), m["guest"].as_bool().unwrap()));
            json!(roles::card_marks(a["me"].as_bool().unwrap(), member))
        }
        "tagOf" => {
            let tag: Option<TagSummary> = of(&a["tag"]);
            json!(roles::tag_of(tag.as_ref())
                .map(|(label, tone)| json!({"label": label, "tone": tone})))
        }
        "found" => json!(roles::found(
            s(&a["query"]),
            s(&a["displayName"]),
            s(&a["nickname"]),
            s(&a["handle"]),
            s(&a["title"])
        )),
        "roleNameHint" => json!(roles::role_name_hint(s(&a["serverName"]))),
        "tagSwitchCaption" => json!(roles::tag_switch_caption(a["shown"].as_bool().unwrap())),
        "withheldCaption" => json!(roles::withheld_caption(s(&a["detail"]))),
        "deleteTitle" => json!(roles::delete_title(s(&a["roleName"]))),
        "administratorQuestion" => json!(roles::administrator_question(
            s(&a["serverName"]),
            s(&a["roleName"]),
            held(&a["held"])
        )),
        "deleteQuestion" => json!(roles::delete_question(held(&a["held"]))),
        "roleRefusal" => json!(roles::role_refusal(
            s(&a["code"]),
            s(&a["field"]),
            s(&a["message"]),
            s(&a["serverName"])
        )),
        "memberRefusal" => json!(roles::member_refusal(
            s(&a["code"]),
            s(&a["field"]),
            s(&a["message"])
        )),
        other => panic!("no rule {other}"),
    }
}

#[test]
fn every_case_holds() {
    let file = file();
    let mut failed = Vec::new();
    for case in &file.cases {
        let got = answer(&file, case);
        if got != case.answer {
            failed.push(format!(
                "{} ({}): {} where the file says {}",
                case.rule, case.why, got, case.answer
            ));
        }
    }
    assert!(failed.is_empty(), "{}", failed.join("\n"));
}

#[test]
fn every_rule_has_cases() {
    let file = file();
    for rule in [
        "settingsSections",
        "landsBefore",
        "stepBefore",
        "savable",
        "followed",
        "memberChanges",
        "memberFollowed",
        "memberOffers",
        "cardMarks",
        "deleteQuestion",
        "roleRefusal",
        "memberRefusal",
    ] {
        assert!(
            file.cases.iter().any(|c| c.rule == rule),
            "no case of {rule}"
        );
    }
}

#[test]
fn the_words_are_the_cores() {
    let file = file();
    let core: BTreeMap<&str, &str> = [
        ("ROLES_HINT", roles::ROLES_HINT),
        ("ROLES_HINT_MENU", roles::ROLES_HINT_MENU),
        ("EVERYONE_SUBTITLE", roles::EVERYONE_SUBTITLE),
        ("TITLE_HINT", roles::TITLE_HINT),
        ("GUEST_ROLES_NOTE", roles::GUEST_ROLES_NOTE),
        ("DELETE_ROLE_HINT", roles::DELETE_ROLE_HINT),
        ("TAG_HEADING", roles::TAG_HEADING),
        ("TAG_SUBTITLE", roles::TAG_SUBTITLE),
        ("TAG_NOTE", roles::TAG_NOTE),
        ("TAG_ROLES_LABEL", roles::TAG_ROLES_LABEL),
        ("TAG_ROLES_CAPTION", roles::TAG_ROLES_CAPTION),
        ("TAG_TITLES_LABEL", roles::TAG_TITLES_LABEL),
        ("TAG_TITLES_CAPTION", roles::TAG_TITLES_CAPTION),
        ("NOBODY_HOLDS", roles::NOBODY_HOLDS),
        ("EVERYONE_HOLDS", roles::EVERYONE_HOLDS),
        ("NO_MEMBER_MATCHES", roles::NO_MEMBER_MATCHES),
        ("READING_MEMBERS", roles::READING_MEMBERS),
        ("NO_ROLES", roles::NO_ROLES),
        ("ADMINISTRATOR_TITLE", roles::ADMINISTRATOR_TITLE),
        ("NOT_SHOWN_AS_TAG", roles::NOT_SHOWN_AS_TAG),
        ("ADMINISTRATOR", roles::ADMINISTRATOR),
    ]
    .into_iter()
    .collect();
    let file_words: BTreeMap<&str, &str> = file
        .words
        .iter()
        .map(|(k, v)| (k.as_str(), v.as_str()))
        .collect();
    assert_eq!(core, file_words);
}
