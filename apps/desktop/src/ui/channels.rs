//! Being in channels (architecture 6.8, `app::channels`): Browse, this account's settings in
//! a server, the bar that stands for the composer in a channel read before joining, and a
//! channel's members beside it with "Add people" and the way out.
//!
//! No board draws these. They are the Profile page's vocabulary (cards, rows, the side
//! panel's member rows), worded as the web's and the phones'.

use lumia_client::app::{ChannelSummary, MemberSummary, ServerSummary};
use rune::{El, ScrollSpec, col, el, row, text};

use super::components::{
    Look, action_button, card, check_row, error_text, link_button, panel, primary_button,
    secondary_button, section_label,
};
use super::home::{action, page_column, titled_page};
use super::server_popovers::ADD_PEOPLE;
use super::shell::ASIDE_WIDTH;
use crate::app::State;
use crate::session::display_name;
use crate::tokens::{self as t, icons};
use crate::ui::scroll::WithScrollBar;

fn quiet(words: &str, look: Look) -> El {
    text(words)
        .font(t::TEXT_META)
        .color(look.c.ink_secondary)
        .wrap()
        .w_full()
}

/// How many are in a channel, in words.
pub fn member_count(n: u32) -> String {
    if n == 1 {
        "1 member".into()
    } else {
        format!("{n} members")
    }
}

/// Browse: the public channels of the server this account can see and is not in, each with
/// its topic, how many are in it, and the way in. A row opens the channel to read first.
pub fn browse(s: &State, server: &ServerSummary, look: Look) -> El {
    let form = &s.channel_forms;
    let listed = s.session.browse.get(&server.server_id);
    let mut list = col()
        .key("browse-list")
        .w_full()
        .gap(t::SPACE_8)
        .role("list")
        .label("Channels to join");
    for channel in listed.into_iter().flatten() {
        list = list.child(browse_row(s, channel, look));
    }
    let mut content = page_column();
    content = match listed {
        None if form.browse_error.is_empty() => content.child(quiet("Looking…", look)),
        Some(all) if all.is_empty() => content.child(
            super::components::empty_state(
                &icons::HASH,
                "You are in every channel here",
                "Channels you leave, and new ones, show here.",
                look,
            )
            .h(t::SPACE_24 * 10.0),
        ),
        _ => content.child(list),
    };
    if !form.browse_error.is_empty() {
        content = content.child(error_text(&form.browse_error, look));
    }
    if !form.join_error.is_empty() {
        content = content.child(error_text(&form.join_error, look));
    }
    titled_page("browse", "Browse channels", &server.name, content, look)
}

fn browse_row(s: &State, channel: &ChannelSummary, look: Look) -> El {
    let c = look.c;
    let (open, join) = (channel.channel_id.clone(), channel.channel_id.clone());
    let joining = s.channel_forms.joining.as_deref() == Some(channel.channel_id.as_str());
    let mut words = col().grow(1.0).min_w(0.0).gap(t::SPACE_2).child(
        row()
            .gap(t::SPACE_6)
            .items_center()
            .child(text("#").font(t::TEXT_AUTHOR).color(c.ink_faint))
            .child(
                text(channel.name.clone())
                    .font(t::TEXT_AUTHOR)
                    .color(c.ink)
                    .truncate(),
            ),
    );
    let line = [channel.topic.as_str(), &member_count(channel.member_count)]
        .into_iter()
        .filter(|w| !w.is_empty())
        .collect::<Vec<_>>()
        .join(" · ");
    words = words.child(text(line).font(t::TEXT_META).color(c.ink_muted).truncate());
    row()
        .key(format!("browse-{}", channel.channel_id))
        .w_full()
        .px(t::SPACE_14)
        .py(t::SPACE_12)
        .gap(t::SPACE_12)
        .items_center()
        .radius(t::RADIUS_12)
        .fill(c.surface)
        .border(1.0, c.line)
        .hover(c.hover_member)
        .role("listitem")
        .label(format!(
            "{}, {}",
            channel.name,
            member_count(channel.member_count)
        ))
        .cursor(rune::Cursor::Pointer)
        .focusable(true)
        .focus_ring(2.0, 0.0, c.focus)
        .on_press(move |s: &mut State| s.preview_channel(&open))
        .child(words)
        .child(action_button(
            format!("join-{}", channel.channel_id),
            "Join",
            !joining,
            move |s| s.join_channel(&join),
            look,
        ))
}

/// What stands where the composer would in a channel read before joining.
pub fn join_bar(s: &State, channel: &ChannelSummary, look: Look) -> El {
    let c = look.c;
    let id = channel.channel_id.clone();
    let joining = s.channel_forms.joining.as_deref() == Some(channel.channel_id.as_str());
    let mut bar = row()
        .key("join-bar")
        .w_full()
        .px(t::SPACE_16)
        .py(t::SPACE_10)
        .gap(t::SPACE_12)
        .items_center()
        .radius(t::RADIUS_16)
        .fill(c.inset)
        .child(
            text(format!("You’re not in #{}", channel.name))
                .font(t::TEXT_COMPACT)
                .color(c.ink_secondary)
                .truncate()
                .grow(1.0)
                .min_w(0.0),
        )
        .child(action_button(
            "join-channel",
            "Join",
            !joining,
            move |s| s.join_channel(&id),
            look,
        ));
    if !s.channel_forms.join_error.is_empty() {
        bar = bar.child(error_text(&s.channel_forms.join_error, look).w(t::AUTH_WIDTH / 2.0));
    }
    col()
        .w_full()
        .px(t::SPACE_24)
        .pt(t::SPACE_4)
        .pb(t::SPACE_14)
        .child(bar)
}

/// "Your settings in" a server: this account's two switches there.
pub fn your_settings(s: &State, server: &ServerSummary, look: Look) -> El {
    let form = &s.channel_forms;
    let adds_on = !form.adds_by_invitation;
    let dms_on = !form.dms_off;
    let mut card = card(
        "In this server",
        "These are yours: nobody else here sees or changes them.",
        look,
    )
    .key("your-settings")
    .w_full()
    .child(check_row(
        "switch-adds",
        "Let others add me to channels",
        Some("When off, being added becomes an invitation you can accept or dismiss."),
        adds_on,
        move |s: &mut State| s.set_adds_by_invitation(adds_on),
        look,
    ))
    .child(check_row(
        "switch-dms",
        "Let members of this server message me",
        Some("When off, people here can still message you privately."),
        dms_on,
        move |s: &mut State| s.set_dms_off(dms_on),
        look,
    ));
    if !form.switches.error.is_empty() {
        card = card.child(error_text(&form.switches.error, look));
    } else if form.switches.saved {
        card = card.child(quiet("Saved.", look));
    }
    let changed = form.adds_by_invitation != server.channel_adds_by_invitation
        || form.dms_off != server.server_dms_off;
    card = card.child(action(primary_button(
        "save-switches",
        "Save",
        !form.switches.busy && changed,
        State::save_switches,
        look,
    )));
    titled_page(
        "your-settings-page",
        &format!("Your settings in {}", server.name),
        "Only you",
        page_column().child(card),
        look,
    )
}

/// The default channels card of a server's settings: its public channels, the ones every
/// newcomer joins ticked.
pub fn defaults_card(s: &State, server: &ServerSummary, look: Look) -> El {
    let form = &s.channel_forms;
    let mut card = card("Default channels", "New members join these channels.", look)
        .key("default-channels")
        .w_full();
    for channel in s.public_channels(&server.server_id) {
        let id = channel.channel_id.clone();
        card = card.child(check_row(
            format!("default-{}", channel.channel_id),
            &format!("#{}", channel.name),
            None,
            form.defaults.contains(&channel.channel_id),
            move |s: &mut State| s.toggle_default(&id),
            look,
        ));
    }
    if !form.defaults_saving.error.is_empty() {
        card = card.child(error_text(&form.defaults_saving.error, look));
    } else if form.defaults_saving.saved {
        card = card.child(quiet("Saved.", look));
    }
    card.child(action(primary_button(
        "save-defaults",
        "Save",
        !form.defaults_saving.busy,
        State::save_defaults,
        look,
    )))
}

/// The open channel's members beside it: "Add people" at the top for those in it, the
/// people grouped by presence as the server's are, a way to take someone out for those who
/// manage channels, and "Leave channel" at the end.
pub fn channel_members(
    s: &State,
    server: &ServerSummary,
    channel: &ChannelSummary,
    look: Look,
) -> El {
    let c = look.c;
    let ids = s
        .session
        .channel_members
        .get(&channel.channel_id)
        .cloned()
        .unwrap_or_default();
    let mut people: Vec<MemberSummary> = ids
        .iter()
        .map(|id| {
            s.session
                .person(id)
                .cloned()
                .unwrap_or_else(|| MemberSummary {
                    account_id: id.clone(),
                    handle: String::new(),
                    display_name: s.session.name_of(id),
                    nickname: String::new(),
                    pronouns: String::new(),
                    bio: String::new(),
                    avatar_url: String::new(),
                    channel_adds_by_invitation: false,
                    server_dms_off: false,
                })
        })
        .collect();
    people.sort_by_cached_key(|m| display_name(m).to_lowercase());
    let total = people.len();
    let wanted = s.member_filter.text.trim().to_lowercase();
    if !wanted.is_empty() {
        people.retain(|m| {
            display_name(m).to_lowercase().contains(&wanted)
                || m.handle.to_lowercase().contains(&wanted)
        });
    }
    let joined = s.session.joined(&channel.channel_id);
    let manages = server.can_manage_channels;

    let heading = row()
        .w_full()
        .h(t::SEGMENT_HEIGHT + t::SPACE_16 * 2.0)
        .px(t::SPACE_18)
        .gap(t::SPACE_6)
        .items_center()
        .border_sides([0.0, 0.0, 1.0, 0.0], c.line)
        .child(
            text("Members")
                .font(t::TEXT_TAB)
                .color(c.ink)
                .role("heading"),
        )
        .child(
            text(total.to_string())
                .font(t::TEXT_SECTION)
                .color(c.ink_muted)
                .label(format!("{total} in this channel")),
        );
    let mut top = col()
        .w_full()
        .px(t::SPACE_14)
        .pt(t::SPACE_12)
        .pb(t::SPACE_6)
        .gap(t::SPACE_8);
    if joined {
        top = top.child(
            secondary_button(ADD_PEOPLE, "Add people", true, State::open_add_people, look).w_full(),
        );
    }
    top = top.child(super::server::find_member(s, look));
    let (here, away): (Vec<_>, Vec<_>) = people.iter().partition(|m| {
        s.session
            .presence_of(&server.server_id, &m.account_id)
            .is_some()
    });
    let mut list = col()
        .key("members")
        .w_full()
        .px(t::SPACE_8)
        .pt(t::SPACE_4)
        .pb(t::SPACE_14)
        .role("list")
        .label("Members");
    let add_group = |list: El, title: Option<String>, top: f32, group: &[&MemberSummary]| {
        let mut list = list;
        if let Some(title) = title {
            list = list.child(section_label(title, top, look));
        }
        for person in group {
            let removable = (manages && joined).then_some(channel.name.as_str());
            list = list.child(member(s, server, person, removable, look));
        }
        list
    };
    if !here.is_empty() {
        list = add_group(
            list,
            Some(format!("Online · {}", here.len())),
            t::SPACE_8,
            &here,
        );
    }
    if !away.is_empty() {
        let title = (!here.is_empty()).then(|| format!("Offline · {}", away.len()));
        list = add_group(list, title, t::SPACE_14, &away);
    }
    if people.is_empty() && !wanted.is_empty() {
        list = list.child(
            text("No member matches that.")
                .font(t::TEXT_META)
                .color(c.ink_quiet)
                .wrap()
                .padding(t::SPACE_10),
        );
    }
    let mut foot = col()
        .w_full()
        .px(t::SPACE_14)
        .pb(t::SPACE_14)
        .gap(t::SPACE_6);
    if !s.channel_forms.member_error.is_empty() {
        foot = foot.child(error_text(&s.channel_forms.member_error, look));
    }
    if joined {
        foot = foot.child(
            secondary_button(
                "leave-channel",
                "Leave channel",
                !s.channel_forms.member_busy,
                State::leave_channel,
                look,
            )
            .w_full(),
        );
    }
    panel("aside", look)
        .w(ASIDE_WIDTH)
        .child(heading)
        .child(top)
        .child(
            col()
                .key("members")
                .w_full()
                .grow(1.0)
                .min_h(0.0)
                .scroll(ScrollSpec {
                    stick_bottom: false,
                })
                .child(list)
                .with_scroll_bar("members", look),
        )
        .child(foot)
}

/// A member's row, with "Remove" at its end for one who manages channels (not on their own):
/// `removable` names the channel they would be taken out of.
fn member(
    s: &State,
    server: &ServerSummary,
    person: &MemberSummary,
    removable: Option<&str>,
    look: Look,
) -> El {
    let row = super::server::member_row(s, server, person, look);
    let Some(channel) = removable.filter(|_| person.account_id != s.session.me()) else {
        return row;
    };
    let id = person.account_id.clone();
    let name = display_name(person);
    el().w_full().child(row).child(
        el().pin(None, Some(0.0), Some(t::SPACE_10), Some(0.0))
            .center()
            .child(
                link_button(
                    format!("remove-{}", person.account_id),
                    "Remove",
                    move |s| s.remove_from_channel(&id),
                    look,
                )
                .label(format!("Remove {name} from #{channel}")),
            ),
    )
}
