//! Roles and titles (architecture 7.5), with the sample data of the step 11 boards: Acme, its
//! six roles (Admin, Moderator, Design, Engineering, Can invite, Everyone), Ryan who owns it
//! and holds no role, Maya, Devon, Sam, Priya, Leo, and Jordan the guest. No instance is
//! behind any of it.
//!
//! What a viewer may do is not decided here: each scene hands the core's own functions
//! (`lumia_client::roles`) the server's roles and the viewer's membership, as the instance
//! would send them, and holds what they answer, which is what a snapshot and a member list
//! carry. So a scene drawn as Devon, whose highest role is Moderator, has the locks, the
//! withheld permissions and the members above him that the boards draw for him.
//!
//! The boards are 1440 wide: `--size 1440x901` for the lists, taller for the pages drawn at
//! their full length (`roles-overview` 1440x2034, `roles-editor` 1440x1995).

use chrono::{Local, NaiveTime};
use lumia_client::app::{ChannelSummary, MemberSummary, ServerSummary, SnapshotSummary};
use lumia_client::rail::tone_of;
use lumia_client::roles::{self as core, Standing};
use lumia_core::proto::{Member, MemberTag, Permission, Role, Server};
use lumia_core::wire;

use crate::app::State;
use crate::app::role_settings::{CardForm, RoleDrag};
use crate::app::roles::Section;
use crate::session::servers::ServerUpdate;
use crate::session::{Entry, Update};

/// The server's id.
pub const ACME: &str = "acme";

/// A role's id as a snapshot gives it, from the name the scenes call it by: `admin`, `mod`,
/// `design`, `eng`, `invite`, `everyone`.
pub fn role_id(name: &str) -> String {
    wire::display(name.as_bytes())
}

/// The id of one of the seven, which hashes to the avatar tone the boards draw them in.
pub fn person_id(name: &str) -> String {
    let tone = match name {
        "ryan" => "indigo",
        "maya" => "peach",
        "devon" => "purple",
        "sam" => "mint",
        "priya" | "alex" => "amber",
        "leo" => "sky",
        "jordan" => "stone",
        "noor" => "green",
        _ => return name.to_string(),
    };
    (0..)
        .map(|n| {
            if n == 0 {
                name.to_string()
            } else {
                format!("{name}-{n}")
            }
        })
        .find(|id| tone_of(id) == tone)
        .unwrap_or_else(|| name.to_string())
}

/// One of Acme's people: the name the scenes call them by, how they are shown, their title,
/// the roles they hold, and whether they are a guest.
struct Person {
    key: &'static str,
    name: String,
    title: &'static str,
    roles: Vec<&'static str>,
    guest: bool,
}

fn person(key: &'static str, name: &str, title: &'static str, roles: &[&'static str]) -> Person {
    Person {
        key,
        name: name.into(),
        title,
        roles: roles.to_vec(),
        guest: false,
    }
}

/// The seven the boards draw.
fn seven() -> Vec<Person> {
    vec![
        person("ryan", "Ryan Winchester", "Founder", &[]),
        person("maya", "Maya Chen", "Head of Design", &["admin", "design"]),
        person("devon", "Devon Park", "Staff Engineer", &["mod", "eng"]),
        person("sam", "Sam Okafor", "Backend Engineer", &["eng", "invite"]),
        person("priya", "Priya Natarajan", "", &["design"]),
        person("leo", "Leo Martins", "Mobile Lead", &["invite"]),
        Person {
            guest: true,
            ..person("jordan", "Jordan Lee", "Contractor", &[])
        },
    ]
}

/// The rest of Acme's 128: the two other moderators the editor's board lists, and enough
/// others for the boards' counts (2 hold Admin, 3 Moderator, 12 Design, 24 Engineering, 41
/// Can invite).
fn crowd() -> Vec<Person> {
    const FIRST: [&str; 12] = [
        "Ana", "Ben", "Chidi", "Dara", "Emil", "Farah", "Goran", "Hana", "Ines", "Jun", "Kofi",
        "Lena",
    ];
    const LAST: [&str; 10] = [
        "Abbasi", "Berg", "Castro", "Dube", "Eze", "Fontaine", "Greco", "Hale", "Ito", "Jensen",
    ];
    let mut people = vec![
        person("noor", "Noor Haddad", "", &["mod"]),
        person("alex", "Alex Rivera", "", &["mod"]),
    ];
    for i in 0..119 {
        let roles: &[&'static str] = match i {
            0 => &["admin"],
            1..=10 => &["design"],
            11..=32 => &["eng"],
            33..=71 => &["invite"],
            _ => &[],
        };
        people.push(Person {
            key: "",
            name: format!("{} {}", FIRST[i % FIRST.len()], LAST[i / FIRST.len()]),
            title: "",
            roles: roles.to_vec(),
            guest: false,
        });
    }
    people
}

/// Acme's roles as the instance has them, highest first.
fn acme_roles() -> Vec<Role> {
    use Permission::{
        AddReactions, Administrator, AttachFiles, CreateInvites, ManageChannels, ManageMembers,
        ManageMessages, ManageRoles, MentionEveryone, ReadHistory, SendMessages, StartServerDms,
        ViewChannel,
    };
    let role = |key: &str, name: &str, position, color: &str, permissions: &[Permission]| Role {
        role_id: key.as_bytes().to_vec(),
        name: name.into(),
        position,
        color: color.into(),
        permissions: permissions.iter().map(|p| *p as i32).collect(),
        ..Role::default()
    };
    vec![
        role("admin", "Admin", 1, "indigo", &[Administrator]),
        role(
            "mod",
            "Moderator",
            2,
            "green",
            &[
                ManageRoles,
                ManageChannels,
                ManageMembers,
                MentionEveryone,
                ManageMessages,
            ],
        ),
        role("design", "Design", 3, "peach", &[]),
        role("eng", "Engineering", 4, "sky", &[]),
        Role {
            hide_tag: true,
            ..role("invite", "Can invite", 5, "", &[CreateInvites])
        },
        Role {
            is_everyone: true,
            ..role(
                "everyone",
                "@everyone",
                1_000_000,
                "",
                &[
                    CreateInvites,
                    ViewChannel,
                    ReadHistory,
                    SendMessages,
                    AttachFiles,
                    AddReactions,
                    StartServerDms,
                ],
            )
        },
    ]
}

/// What a scene is drawn with.
pub struct Acme {
    /// Whose window it is: `ryan` (the owner), `devon`, `sam` or `priya`.
    pub viewer: &'static str,
    /// The server shows titles as tags, not roles.
    pub titles: bool,
    /// All 128 members, for the settings' counts; the seven alone otherwise.
    pub crowd: bool,
    /// The roles as the instance has them, for a scene that changes them first.
    pub roles: Vec<Role>,
    /// A change to the seven before they are dressed.
    pub change: fn(&mut Vec<(String, MemberSummary)>),
}

impl Acme {
    pub fn seen_by(viewer: &'static str) -> Self {
        Self {
            viewer,
            titles: false,
            crowd: false,
            roles: acme_roles(),
            change: |_| {},
        }
    }
}

/// Signs the state in as the scene's viewer, in Acme, with its roles and its members as the
/// core dresses them for that viewer.
pub fn acme(state: &mut State, with: &Acme) {
    let mut people = seven();
    if with.crowd {
        people.extend(crowd());
    }
    let ids: Vec<String> = people
        .iter()
        .enumerate()
        .map(|(i, p)| {
            if p.key.is_empty() {
                format!("member-{i}")
            } else {
                person_id(p.key)
            }
        })
        .collect();
    let id_of = |key: &str| person_id(key);
    let server = Server {
        owner_account_id: id_of("ryan").into_bytes(),
        member_tag: if with.titles {
            MemberTag::Title as i32
        } else {
            MemberTag::Role as i32
        },
        ..Server::default()
    };
    // The viewer's own membership, as the instance resolves it: every permission its roles
    // and Everyone's grant.
    let me = id_of(with.viewer);
    let mine = people
        .iter()
        .find(|p| p.key == with.viewer)
        .map(|p| p.roles.clone())
        .unwrap_or_default();
    let granted: Vec<i32> = with
        .roles
        .iter()
        .filter(|r| r.is_everyone || mine.iter().any(|key| key.as_bytes() == r.role_id))
        .flat_map(|r| r.permissions.clone())
        .collect();
    let own = Member {
        account_id: me.clone().into_bytes(),
        role_ids: mine.iter().map(|key| key.as_bytes().to_vec()).collect(),
        permissions: granted,
        ..Member::default()
    };
    let standing = Standing::of(&server, Some(&own), &with.roles);
    let roles = core::summaries(&with.roles, &standing);

    let mut members: Vec<(String, MemberSummary)> = people
        .iter()
        .zip(&ids)
        .map(|(p, id)| {
            (
                p.key.to_string(),
                MemberSummary {
                    account_id: id.clone(),
                    handle: if p.key.is_empty() {
                        p.name.to_lowercase().replace(' ', ".")
                    } else if p.key == "alex" {
                        "arivera".into()
                    } else {
                        p.key.into()
                    },
                    display_name: p.name.clone(),
                    role_ids: p.roles.iter().map(|key| role_id(key)).collect(),
                    title: p.title.into(),
                    guest: p.guest,
                    ..MemberSummary::default()
                },
            )
        })
        .collect();
    (with.change)(&mut members);
    // Dressed as the core dresses a member list: the tag, the owner, who the viewer manages.
    let members: Vec<MemberSummary> = members
        .into_iter()
        .map(|(_, mut m)| {
            m.tag = core::tag(with.titles, &roles, &m.role_ids, &m.title);
            m.owner = m.account_id == id_of("ryan");
            m.can_manage = standing.manages_member(
                m.account_id.as_bytes(),
                &server,
                core::highest_of(&m.role_ids, &roles),
            );
            m
        })
        .collect();
    let own_title = members
        .iter()
        .find(|m| m.account_id == me)
        .map(|m| m.title.clone())
        .unwrap_or_default();
    let own_name = members
        .iter()
        .find(|m| m.account_id == me)
        .map(|m| m.display_name.clone())
        .unwrap_or_default();

    let channel = |id: &str, server: &str, name: &str| ChannelSummary {
        channel_id: id.into(),
        server_id: server.into(),
        name: name.into(),
        kind: "TEXT".into(),
        last_seq: 0,
        participant_account_ids: Vec::new(),
        host: String::new(),
        private: false,
        is_default: false,
        member_count: 0,
        topic: String::new(),
    };
    let other = |id: &str, name: &str| ServerSummary {
        server_id: id.into(),
        name: name.into(),
        ..ServerSummary::default()
    };
    let dm_with = |key: &str| ChannelSummary {
        kind: "SERVER_DM".into(),
        participant_account_ids: vec![me.clone(), id_of(key)],
        ..channel(&format!("dm-{key}"), ACME, "")
    };
    let mut channels = vec![
        ChannelSummary {
            is_default: true,
            topic: "Company-wide announcements and water-cooler chat".into(),
            ..channel("general", ACME, "general")
        },
        channel("design", ACME, "design"),
        channel("engineering", ACME, "engineering"),
        ChannelSummary {
            is_default: true,
            ..channel("random", ACME, "random")
        },
        ChannelSummary {
            private: true,
            ..channel("leadership", ACME, "leadership")
        },
        ChannelSummary {
            private: true,
            ..channel("northstar", ACME, "northstar-launch")
        },
    ];
    for key in ["maya", "devon"] {
        if key != with.viewer {
            channels.push(dm_with(key));
        }
    }
    use Permission::{
        ManageChannels, ManageMembers, ManageMessages, ManageRoles, ManageServer, MentionEveryone,
        StartServerDms,
    };
    state.receive(&Update::SignedIn(Ok(())));
    state.receive(&Update::Snapshot(Ok(SnapshotSummary {
        account_id: me.clone(),
        handle: with.viewer.into(),
        display_name: own_name,
        avatar_url: String::new(),
        servers: vec![
            ServerSummary {
                server_id: ACME.into(),
                name: "Acme".into(),
                can_manage: standing.holds(ManageServer),
                can_manage_channels: standing.holds(ManageChannels),
                can_start_dms: standing.holds(StartServerDms),
                can_mention_everyone: standing.holds(MentionEveryone),
                can_manage_messages: standing.holds(ManageMessages),
                owner_account_id: id_of("ryan"),
                is_owner: standing.is_owner(),
                can_manage_roles: standing.holds(ManageRoles),
                can_manage_members: standing.holds(ManageMembers),
                tag_titles: with.titles,
                roles,
                grantable: standing.grantable(),
                role_ids: mine.iter().map(|key| role_id(key)).collect(),
                title: own_title,
                ..ServerSummary::default()
            },
            other("guild", "Design Guild"),
            other("builders", "Lumia Builders"),
            other("friends", "Friends"),
        ],
        channels,
        pending_requests: Vec::new(),
        channel_invitations: Vec::new(),
        read_states: Vec::new(),
        rail: Vec::new(),
    })));
    state.receive(&Update::Members {
        server_id: ACME.into(),
        members,
    });
    // Who is here, as the boards have them: Ryan, Maya and Sam online, Devon away.
    let here = state.session.presence.entry(ACME.into()).or_default();
    for (key, status) in [
        ("ryan", "ONLINE"),
        ("maya", "ONLINE"),
        ("devon", "AWAY"),
        ("sam", "ONLINE"),
    ] {
        here.insert(id_of(key), (status.into(), String::new()));
    }
    let inside: Vec<String> = seven().iter().map(|p| id_of(p.key)).collect();
    for channel in ["general", "design", "engineering", "random"] {
        state
            .session
            .channel_members
            .insert(channel.into(), inside.clone());
    }
    state.select_server(ACME);
    state.select_channel("general");
}

/// The morning in #general the "Tags" boards draw.
fn talk(state: &mut State, said: &[(&str, (u32, u32), &str)]) {
    let today = Local::now().date_naive();
    state.session.entries = said
        .iter()
        .enumerate()
        .map(|(i, (who, (hour, minute), body))| Entry {
            id: format!("t{i}"),
            seq: i as u64,
            author_account_id: person_id(who),
            body: (*body).into(),
            at_micros: NaiveTime::from_hms_opt(*hour, *minute, 0)
                .and_then(|time| today.and_time(time).and_local_timezone(Local).single())
                .map_or(0, |at| at.timestamp_micros()),
            deleted: false,
            notice: String::new(),
            notice_about: String::new(),
            reply: None,
            mentions: Vec::new(),
            reactions: Vec::new(),
            mentions_me: false,
            edited: false,
            attachments: Vec::new(),
            cards: Vec::new(),
        })
        .collect();
    state.session.revision += 1;
}

const MORNING: [(&str, (u32, u32), &str); 7] = [
    (
        "maya",
        (9, 4),
        "Morning all. The new onboarding flow is in the design channel if anyone wants a look before standup.",
    ),
    (
        "devon",
        (9, 11),
        "Looks great. I’ll check the copy on the second screen against the tone guide.",
    ),
    (
        "sam",
        (9, 20),
        "The importer keeps its old path for one more release, so nothing blocks it.",
    ),
    (
        "priya",
        (9, 24),
        "Two screens changed since Friday, both in the sign-in card.",
    ),
    ("leo", (9, 31), "Mobile gets the same composer this week."),
    ("ryan", (9, 38), "Thanks, everyone. Thursday still stands."),
    ("jordan", (9, 40), "I’ll have the audit notes in by then."),
];

/// The settings of Acme with all 128 members, open on `section`.
fn settings(state: &mut State, viewer: &'static str, section: Section) {
    acme(
        state,
        &Acme {
            crowd: true,
            ..Acme::seen_by(viewer)
        },
    );
    state.open_server_settings();
    state.choose_section(section);
    // The boards' seven, then "Show more members".
    state.roles.shown = 7;
}

/// Server settings, Overview: today's cards and "Tag beside names".
pub fn overview(state: &mut State) {
    settings(state, "ryan", Section::Overview);
    state.receive(&Update::Server(ServerUpdate::Storage {
        server_id: ACME.into(),
        result: Ok(lumia_client::app::StorageSummary {
            used_bytes: 412_000_000,
            max_bytes: 2_000_000_000,
        }),
    }));
}

/// Roles, as the owner sees them.
pub fn list(state: &mut State) {
    settings(state, "ryan", Section::Roles);
}

/// The same list to Devon: Admin and Moderator are at or above his highest.
pub fn list_moderator(state: &mut State) {
    settings(state, "devon", Section::Roles);
}

/// A drag, held: Engineering lifted, and the line where it would land, above Design. The
/// rows' places are the list's at `--size 1440x901`.
pub fn drag(state: &mut State) {
    list(state);
    // Where the rows are laid out, which a drag reads from the frame before it.
    for (i, name) in ["admin", "mod", "design", "eng", "invite", "everyone"]
        .into_iter()
        .enumerate()
    {
        let mut rect = LIST_ROW;
        rect[1] += LIST_PITCH * i as f32;
        state.place_role(&role_id(name), rect);
    }
    let mut origin = LIST_ROW;
    origin[1] += LIST_PITCH * 3.0;
    state.roles.drag = Some(RoleDrag {
        id: role_id("eng"),
        origin,
        by: [24.0, -58.0],
        before: Some(role_id("design")),
    });
}

/// The first row of the `roles-list` scene at 1440 wide, and how far apart the rows are.
const LIST_ROW: [f32; 4] = [524.0, 175.0, 640.0, 62.0];
const LIST_PITCH: f32 = 68.0;

/// A role's menu, Design's.
pub fn menu(state: &mut State) {
    list(state);
    state.open_role_menu(&role_id("design"), [700.0, 340.0]);
}

/// The highest role's menu: no way up.
pub fn menu_first(state: &mut State) {
    list(state);
    state.open_role_menu(&role_id("admin"), [700.0, 205.0]);
}

/// Moderator's editor.
pub fn editor(state: &mut State) {
    list(state);
    state.open_role(&role_id("mod"));
}

/// Everyone's editor: permissions only.
pub fn editor_everyone(state: &mut State) {
    list(state);
    state.open_role(&role_id("everyone"));
}

/// "Turn on Administrator?", asked against its switch.
pub fn ask_administrator(state: &mut State) {
    editor(state);
    state.toggle_permission("ADMINISTRATOR");
}

/// "Delete Moderator?", asked against Delete role.
pub fn ask_delete(state: &mut State) {
    editor(state);
    state.ask_delete_role(&role_id("mod"), "delete-role");
}

/// "New role": the editor with nothing in it yet.
pub fn editor_new(state: &mut State) {
    list(state);
    state.new_role();
}

/// Can invite: "Show as tag" off and no colour.
pub fn editor_no_tag(state: &mut State) {
    list(state);
    state.open_role(&role_id("invite"));
}

/// To Devon, editing Engineering: Administrator and Manage server are not his to give.
pub fn editor_moderator(state: &mut State) {
    list_moderator(state);
    state.open_role(&role_id("eng"));
}

/// "Add members" to Moderator.
pub fn add_holders(state: &mut State) {
    editor(state);
    state.open_add_holders();
    state.toggle_holder(&person_id("sam"));
}

/// Members, Devon's row open, as the owner sees it.
pub fn members(state: &mut State) {
    settings(state, "ryan", Section::Members);
    state.toggle_member(&person_id("devon"));
}

/// Members in a window too narrow for the aside: a member's roles under their name, and the
/// editing state's parts one under the other. Drawn at `--size 900x900`.
pub fn members_narrow(state: &mut State) {
    members(state);
    state.set_window_width(900.0);
}

/// To Devon, Sam's row: the roles at or above his own are shown and not changeable.
pub fn members_below(state: &mut State) {
    settings(state, "devon", Section::Members);
    state.toggle_member(&person_id("sam"));
}

/// To Devon, Maya's row: she is above him, so no title and no Kick.
pub fn members_above(state: &mut State) {
    settings(state, "devon", Section::Members);
    state.toggle_member(&person_id("maya"));
}

/// A guest's row: a title and Kick, and no roles.
pub fn members_guest(state: &mut State) {
    settings(state, "ryan", Section::Members);
    state.toggle_member(&person_id("jordan"));
}

/// #general with Acme showing roles.
pub fn tags_roles(state: &mut State) {
    acme(state, &Acme::seen_by("ryan"));
    talk(state, &MORNING);
}

/// The same conversation with Acme showing titles.
pub fn tags_titles(state: &mut State) {
    acme(
        state,
        &Acme {
            titles: true,
            ..Acme::seen_by("ryan")
        },
    );
    talk(state, &MORNING);
}

/// A role's tag on each of the nine tones and with no colour, in a message and in the member
/// list: the seven each given a role of a tone of its own, and two more.
pub fn tags_tones(state: &mut State) {
    let mut roles = acme_roles();
    let tones = lumia_client::rail::TONES;
    let mut extra: Vec<Role> = tones
        .iter()
        .enumerate()
        .map(|(i, tone)| Role {
            role_id: tone.as_bytes().to_vec(),
            name: {
                let mut name = (*tone).to_string();
                if let Some(first) = name.get_mut(0..1) {
                    first.make_ascii_uppercase();
                }
                name
            },
            position: 10 + i as u32,
            color: (*tone).into(),
            ..Role::default()
        })
        .collect();
    extra.push(Role {
        role_id: b"plain".to_vec(),
        name: "No colour".into(),
        position: 30,
        ..Role::default()
    });
    roles.splice(0..5, extra);
    acme(
        state,
        &Acme {
            roles,
            change: |members| {
                let tones = lumia_client::rail::TONES;
                for (i, (_, member)) in members.iter_mut().enumerate() {
                    member.guest = false;
                    member.role_ids = vec![role_id(tones.get(i).copied().unwrap_or("plain"))];
                }
            },
            ..Acme::seen_by("ryan")
        },
    );
    talk(
        state,
        &[
            ("ryan", (9, 1), "Peach."),
            ("maya", (9, 2), "Blue."),
            ("devon", (9, 3), "Purple."),
            ("sam", (9, 4), "Mint."),
            ("priya", (9, 5), "Amber."),
            ("leo", (9, 6), "Sky."),
            ("jordan", (9, 7), "Stone."),
        ],
    );
}

/// The longest a tag can be, 32 characters: whole in a message, and cut in a member's row,
/// the tag giving way before the name does.
pub fn tags_longest(state: &mut State) {
    let mut roles = acme_roles();
    roles.insert(
        0,
        Role {
            role_id: b"events".to_vec(),
            name: "Community and Events Coordinator".into(),
            position: 0,
            color: "mint".into(),
            ..Role::default()
        },
    );
    acme(
        state,
        &Acme {
            roles,
            titles: false,
            change: |members| {
                for (key, member) in members.iter_mut() {
                    if key == "priya" {
                        member.role_ids = vec![role_id("events")];
                    }
                }
            },
            ..Acme::seen_by("ryan")
        },
    );
    talk(
        state,
        &[
            ("devon", (9, 11), "Looks great."),
            ("priya", (9, 12), "A role’s name can be as long."),
        ],
    );
}

/// The longest title, where the server shows titles.
pub fn tags_longest_title(state: &mut State) {
    acme(
        state,
        &Acme {
            titles: true,
            change: |members| {
                for (key, member) in members.iter_mut() {
                    if key == "devon" {
                        member.title = "Principal Reliability Engineer I".into();
                    }
                }
            },
            ..Acme::seen_by("ryan")
        },
    );
    talk(state, &[("devon", (9, 11), "Looks great.")]);
}

/// A profile card in Acme, as `viewer` sees `whose`.
fn card(state: &mut State, viewer: &'static str, whose: &str) {
    acme(state, &Acme::seen_by(viewer));
    talk(state, &MORNING);
    let id = person_id(whose);
    state.open_person(&id, &format!("member-{id}"));
}

/// Devon's card as others see it.
pub fn card_others(state: &mut State) {
    card(state, "sam", "devon");
}

/// Devon's card as someone who may edit sees it (the owner).
pub fn card_editor(state: &mut State) {
    card(state, "ryan", "devon");
}

/// What Edit beside Roles opens.
pub fn card_edit_roles(state: &mut State) {
    card_editor(state);
    state.edit_on_card(CardForm::Roles);
}

/// What Edit beside the title opens.
pub fn card_edit_title(state: &mut State) {
    card_editor(state);
    state.edit_on_card(CardForm::Title);
}

/// A member's own card: their title is theirs to edit.
pub fn card_own(state: &mut State) {
    card(state, "sam", "sam");
}

/// Their own card with no title yet.
pub fn card_own_no_title(state: &mut State) {
    card(state, "priya", "priya");
}

/// The owner, marked on the card.
pub fn card_owner(state: &mut State) {
    card(state, "sam", "ryan");
}

/// A guest, to someone who may edit.
pub fn card_guest(state: &mut State) {
    card(state, "ryan", "jordan");
}

/// A role that is not shown as a tag is still listed on the card.
pub fn card_untagged(state: &mut State) {
    card(state, "sam", "leo");
}
