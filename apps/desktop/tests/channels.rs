//! Being in channels, and server DMs (architecture 6.8 and 6.4), over a state no instance is
//! behind: the sidebar's sections and their pluses, Browse and a channel read before
//! joining, invitations, notices, a channel's members with "Add people", the profile card's
//! two ways to talk, this account's settings in a server and the default channels card.
//! What the instance would answer is applied with `State::receive`.

#![allow(clippy::expect_used, clippy::unwrap_used)]

mod common;

use common::*;
use lumia_client::app::{
    ChannelAddSummary, ChannelInvitationSummary, ChannelSummary, SnapshotSummary,
};
use lumia_shell::app::State;
use lumia_shell::app::channels::ServerPage;
use lumia_shell::session::Update;
use lumia_shell::session::channels::ChannelUpdate;
use rune::Runtime;

fn channel(id: &str, name: &str) -> ChannelSummary {
    ChannelSummary {
        channel_id: id.into(),
        server_id: "acme".into(),
        name: name.into(),
        kind: "TEXT".into(),
        last_seq: 0,
        participant_account_ids: Vec::new(),
        host: String::new(),
        private: false,
        is_default: false,
        member_count: 0,
        topic: String::new(),
    }
}

/// The fixture's snapshot with `change` made to it.
fn changed(change: impl Fn(&mut SnapshotSummary)) -> SnapshotSummary {
    let mut snapshot = snapshot();
    change(&mut snapshot);
    snapshot
}

fn take(rt: &mut Runtime<State>, snapshot: SnapshotSummary) {
    rt.update(move |s, _| s.receive(&Update::Snapshot(Ok(snapshot))));
    settle(rt);
}

fn answer(rt: &mut Runtime<State>, update: ChannelUpdate) {
    rt.update(move |s, _| s.receive(&Update::Channel(update)));
    settle(rt);
}

#[test]
fn the_channels_plus_offers_creating_to_who_may_and_browsing_to_all() {
    let mut rt = runtime(signed_in());
    press(&mut rt, "channels-plus");
    assert!(rt.find("menu-create-channel").is_some());
    assert!(rt.find("menu-browse-channels").is_some());
    press(&mut rt, "menu-create-channel");
    assert!(rt.find("new-channel-popover").is_some());
    // Made private, the popover says who sees it.
    press(&mut rt, "channel-private");
    assert!(rt.state.server_forms.channel.private);
    assert!(words(&rt).contains("A text channel only the people in it can see."));

    // Garden is one this account may not make channels in.
    rt.update(|s, _| {
        s.close_popover();
        s.select_server("garden");
    });
    settle(&mut rt);
    press(&mut rt, "channels-plus");
    assert!(rt.find("menu-create-channel").is_none());
    assert!(rt.find("menu-browse-channels").is_some());
}

#[test]
fn browse_lists_a_channel_read_before_joining_and_joined() {
    let mut rt = runtime(signed_in());
    press(&mut rt, "channels-plus");
    press(&mut rt, "menu-browse-channels");
    assert_eq!(rt.state.server_page(), Some(ServerPage::Browse));
    assert!(words(&rt).contains("Looking…"));

    let random = ChannelSummary {
        topic: "Off topic".into(),
        member_count: 3,
        ..channel("random", "random")
    };
    answer(
        &mut rt,
        ChannelUpdate::Browse {
            server_id: "acme".into(),
            channels: Ok(vec![random.clone()]),
        },
    );
    assert!(words(&rt).contains("Off topic · 3 members"));

    // A row opens the channel to read: "Join" stands where the composer would.
    press(&mut rt, "browse-random");
    assert_eq!(rt.state.session.channel(), Some("random"));
    assert!(words(&rt).contains("You’re not in #random"));
    assert!(rt.find("composer").is_none());
    assert!(
        rt.find("leave-channel").is_none(),
        "not in it, so nothing to leave"
    );
    press(&mut rt, "join-channel");
    assert_eq!(rt.state.channel_forms.joining.as_deref(), Some("random"));

    let joined = changed(|s| s.channels.push(random.clone()));
    answer(
        &mut rt,
        ChannelUpdate::Joined {
            channel_id: "random".into(),
            result: Ok(joined),
        },
    );
    assert!(rt.find("join-bar").is_none());
    assert!(rt.find("composer").is_some());
    assert!(rt.find("channel-random").is_some());
    assert!(rt.state.session.previewing.is_none());
}

#[test]
fn an_invitation_waits_in_the_sidebar_with_join_and_dismiss() {
    let mut rt = runtime(signed_in());
    take(
        &mut rt,
        changed(|s| {
            s.channel_invitations.push(ChannelInvitationSummary {
                channel: ChannelSummary {
                    private: true,
                    ..channel("secret", "secret")
                },
                invited_by_account_id: "grace".into(),
                created_at_micros: 0,
            });
        }),
    );
    assert!(words(&rt).contains("#secret · Grace Hopper invited you"));
    press(&mut rt, "join-secret");
    assert_eq!(rt.state.channel_forms.joining.as_deref(), Some("secret"));
    assert!(rt.find("dismiss-secret").is_some());
}

#[test]
fn a_server_dm_is_listed_by_the_other_person_and_says_whose_it_is() {
    let mut rt = runtime(signed_in());
    take(
        &mut rt,
        changed(|s| {
            s.channels.push(ChannelSummary {
                kind: "SERVER_DM".into(),
                participant_account_ids: vec!["me".into(), "grace".into()],
                ..channel("dm1", "")
            });
        }),
    );
    let row = rt
        .elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some("dm-dm1"))
        .expect("the DM is in the sidebar");
    assert_eq!(row.label.as_deref(), Some("Conversation with Grace Hopper"));
    press(&mut rt, "dm-dm1");
    let shown = words(&rt);
    assert!(shown.contains(
        "Messages here are kept by Acme Works and follow its message history settings. For an encrypted conversation, message privately."
    ));
    assert!(
        shown.contains("Message Grace Hopper"),
        "the composer names them"
    );
    assert!(rt.find("toggle-aside").is_none(), "no members beside a DM");
}

#[test]
fn messaging_someone_in_the_server_lists_who_takes_it() {
    let mut rt = runtime(signed_in());
    let mut bo = member("bo", "bo", "Bo Diddley");
    bo.server_dms_off = true;
    rt.update(move |s, _| {
        s.receive(&Update::Members {
            server_id: "acme".into(),
            members: vec![
                member("me", "ada", "Ada Lovelace"),
                member("grace", "grace", "Grace Hopper"),
                bo.clone(),
            ],
        });
    });
    settle(&mut rt);
    press(&mut rt, "dms-plus");
    assert!(rt.find("dm-grace").is_some());
    assert!(rt.find("dm-me").is_none(), "not oneself");
    assert!(words(&rt).contains("Bo Diddley, unavailable"));
    press(&mut rt, "dm-grace");
    assert!(rt.state.channel_forms.dm_busy);

    // Refused: said in the picker.
    answer(
        &mut rt,
        ChannelUpdate::DmOpened {
            server_id: "acme".into(),
            account_id: "grace".into(),
            result: Err(lumia_shell::session::Failure {
                code: "FAILED_PRECONDITION".into(),
                ..Default::default()
            }),
        },
    );
    assert!(words(&rt).contains("They don’t take messages here."));
}

#[test]
fn notices_read_as_what_happened() {
    let mut rt = runtime(signed_in());
    rt.update(|s, _| {
        let mut added = message("n1", "me", "", chrono::Utc::now().timestamp_micros());
        added.notice = "added".into();
        added.notice_about = "grace".into();
        let mut left = message("n2", "grace", "", chrono::Utc::now().timestamp_micros());
        left.notice = "left".into();
        s.session.entries.push(added);
        s.session.entries.push(left);
        s.session.revision += 1;
    });
    settle(&mut rt);
    let shown = words(&rt);
    assert!(shown.contains("Ada Lovelace added Grace Hopper"), "{shown}");
    assert!(shown.contains("Grace Hopper left"), "{shown}");
}

#[test]
fn add_people_lists_who_is_not_in_the_channel_and_marks_invitations() {
    let mut rt = runtime(signed_in());
    let mut bo = member("bo", "bo", "Bo Diddley");
    bo.channel_adds_by_invitation = true;
    let cy = member("cy", "cy", "Cy Twombly");
    rt.update(move |s, _| {
        s.receive(&Update::Members {
            server_id: "acme".into(),
            members: vec![
                member("me", "ada", "Ada Lovelace"),
                member("grace", "grace", "Grace Hopper"),
                bo.clone(),
                cy.clone(),
            ],
        });
    });
    settle(&mut rt);
    press(&mut rt, "add-people");
    assert!(rt.find("add-grace").is_none(), "already in it");
    assert!(words(&rt).contains("Bo Diddley, will be invited"));
    press(&mut rt, "add-bo");
    press(&mut rt, "add-cy");
    assert!(words(&rt).contains("Add 2 people"));
    press(&mut rt, "add-chosen");
    assert!(rt.state.channel_forms.add.busy);
    answer(
        &mut rt,
        ChannelUpdate::Added {
            channel_id: "general".into(),
            result: Ok(ChannelAddSummary {
                added_account_ids: vec!["cy".into()],
                invited_account_ids: vec!["bo".into()],
            }),
        },
    );
    assert!(rt.find("add-people-popover").is_none());
}

#[test]
fn a_channel_manager_may_take_someone_out_and_anyone_may_leave() {
    let mut rt = runtime(signed_in());
    assert!(rt.find("remove-grace").is_some());
    assert!(
        rt.find("remove-me").is_none(),
        "leaving is the way out for oneself"
    );
    press(&mut rt, "leave-channel");
    assert!(rt.state.channel_forms.member_busy);

    // In Garden this account manages no channels.
    rt.update(|s, _| {
        s.session
            .channel_members
            .insert("plots".into(), vec!["me".into(), "grace".into()]);
        s.select_server("garden");
    });
    settle(&mut rt);
    assert!(rt.find("remove-grace").is_none());
}

#[test]
fn the_card_offers_a_server_dm_first_and_a_personal_one_beside_it() {
    let mut rt = runtime(signed_in());
    press(&mut rt, "member-grace");
    assert!(rt.find("card-message").is_some());
    assert!(rt.find("card-message-privately").is_some());
    press(&mut rt, "card-message");
    assert!(rt.state.channel_forms.dm_busy);

    // Someone who takes no server DMs here: said, and only the personal way offered.
    let mut rt = runtime(signed_in());
    rt.update(|s, _| {
        let mut grace = member("grace", "grace", "Grace Hopper");
        grace.server_dms_off = true;
        s.receive(&Update::Members {
            server_id: "acme".into(),
            members: vec![member("me", "ada", "Ada Lovelace"), grace],
        });
    });
    settle(&mut rt);
    press(&mut rt, "member-grace");
    assert!(rt.find("card-message").is_none());
    assert!(words(&rt).contains("Doesn’t take messages here"));
    assert!(rt.find("card-message-privately").is_some());
}

#[test]
fn your_settings_in_a_server_are_two_switches_saved_together() {
    let mut rt = runtime(signed_in());
    press(&mut rt, "server-title");
    press(&mut rt, "menu-your-settings");
    assert_eq!(rt.state.server_page(), Some(ServerPage::YourSettings));
    assert!(words(&rt).contains("Let others add me to channels"));
    press(&mut rt, "switch-adds");
    assert!(rt.state.channel_forms.adds_by_invitation);
    press(&mut rt, "save-switches");
    assert!(rt.state.channel_forms.switches.busy);
    let saved = changed(|s| s.servers[0].channel_adds_by_invitation = true);
    answer(
        &mut rt,
        ChannelUpdate::SwitchesSaved {
            server_id: "acme".into(),
            result: Ok(saved),
        },
    );
    assert!(words(&rt).contains("Saved."));
    // A channel chosen goes back to it.
    press(&mut rt, "channel-design");
    assert_eq!(rt.state.server_page(), None);
}

#[test]
fn the_default_channels_are_chosen_in_the_servers_settings() {
    let mut rt = runtime(signed_in());
    take(&mut rt, changed(|s| s.channels[0].is_default = true));
    press(&mut rt, "server-title");
    press(&mut rt, "menu-server-settings");
    assert!(words(&rt).contains("New members join these channels."));
    assert_eq!(rt.state.channel_forms.defaults, ["general"]);
    press(&mut rt, "default-design");
    assert_eq!(rt.state.channel_forms.defaults, ["general", "design"]);
    press(&mut rt, "save-defaults");
    assert!(rt.state.channel_forms.defaults_saving.busy);
}

#[test]
fn private_channels_have_a_section_of_their_own() {
    let mut rt = runtime(signed_in());
    assert!(rt.find("private-channels").is_none(), "a Private heading with nothing under it");
    take(
        &mut rt,
        changed(|s| {
            s.channels.push(ChannelSummary {
                private: true,
                ..channel("leads", "leads")
            });
        }),
    );
    // In Private, between Channels and Direct messages, and not among the public channels.
    let (_, channels) = rt.find("channel-design").expect("design listed");
    let (_, leads) = rt.find("channel-leads").expect("the private channel listed");
    let (_, dms) = rt.find("server-dms").expect("the server DMs listed");
    assert!(channels < leads && leads < dms, "Private is not between the two");
    assert!(rt.find("private-channels").is_some());
    assert!(words(&rt).contains("Private"));
}
