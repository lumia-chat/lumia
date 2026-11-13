//! Home: the sidebar with the personal conversations and Home's tiles, and a main pane per
//! [`HomeView`]. Each view is a module of its own. The counterpart of the web's
//! `routes/home.tsx`.
//!
//! Personal conversations need this computer to hold an identity; without one the
//! conversation, requests and new-message panes say so and point to Devices, where it is
//! linked.

mod add_server;
mod conversations;
mod devices;
mod new_message;
mod preferences;
mod profile;
mod requests;

/// A page's frame, for the server's settings too.
pub(crate) use devices::{action, page as titled_page, page_column};

use lumia_client::app::ChannelSummary;
use rune::{Cursor, El, Icon, ScrollSpec, col, el, icon, row, text};

use super::components::{
    Look, action_button, avatar_of, empty_state, hover_layer, icon_button, link_button, panel,
    section_label, shadowed,
};
use super::conversation;
use super::shell::{SIDEBAR_WIDTH, sidebar_frame};
use crate::app::{HomeView, State};
use crate::tokens::{self as t, icons};
use crate::ui::scroll::WithScrollBar;

/// Home's sidebar: the title with New message, the tiles, the conversations grouped by the
/// instance that hosts them, and the way to add a server.
pub fn sidebar(s: &State, look: Look) -> El {
    let c = look.c;
    let waiting = s
        .session
        .snapshot
        .as_ref()
        .map_or(0, |snap| snap.pending_requests.len());
    let tiles = row()
        .key("tiles")
        .w_full()
        .gap(t::SPACE_6)
        .pb(t::SPACE_12)
        .child(tile(
            s,
            HomeView::Requests,
            "Requests",
            &icons::MENTION,
            waiting,
            look,
        ))
        .child(tile(
            s,
            HomeView::Devices,
            "Devices",
            &icons::SHIELD,
            0,
            look,
        ))
        .child(tile(
            s,
            HomeView::NewMessage,
            "New",
            &icons::COMPOSE,
            0,
            look,
        ));

    // Grouped by host in the order the snapshot lists them.
    let mut groups: Vec<(&str, Vec<&ChannelSummary>)> = Vec::new();
    for channel in s.session.personal_channels() {
        let host = if channel.host.is_empty() {
            "this instance"
        } else {
            channel.host.as_str()
        };
        match groups.iter_mut().find(|(name, _)| *name == host) {
            Some((_, channels)) => channels.push(channel),
            None => groups.push((host, vec![channel])),
        }
    }
    let mentions = s.session.badge() - s.session.personal_unread();
    let mut list =
        col()
            .w_full()
            .child(el().w_full().pb(t::SPACE_8).child(super::server::nav_row(
                "home-mentions".into(),
                "Mentions",
                &super::server::with_unread("Mentions", (mentions, mentions)),
                super::server::NavLead::Icon(&icons::MENTION),
                s.home_view == HomeView::Mentions,
                (0, mentions),
                State::open_home_mentions,
                look,
            )));
    if groups.is_empty() {
        list = list.child(
            text("No conversations yet.")
                .font(t::TEXT_META)
                .color(c.ink_quiet)
                .padding(t::SPACE_10),
        );
    }
    for (n, (host, channels)) in groups.into_iter().enumerate() {
        let mut group = col()
            .key(format!("conversations-{host}"))
            .w_full()
            .gap(t::SPACE_2)
            .role("list")
            .label(format!("Conversations on {host}"));
        for channel in channels {
            group = group.child(conversation_row(s, channel, look));
        }
        // Sections sit 12 apart, each under a label with 4 above it.
        let label = section_label(host, t::SPACE_4, look);
        list = list
            .child(if n == 0 { label } else { label.mt(t::SPACE_12) })
            .child(group);
    }
    list = list.child(el().px(t::SPACE_10).pt(t::SPACE_8).child(link_button(
        "home-add-server",
        "Add a server",
        |s| s.go_home(HomeView::AddServer),
        look,
    )));
    let body = col().w_full().grow(1.0).min_h(0.0).child(tiles).child(
        col()
            .key("conversations")
            .w_full()
            .grow(1.0)
            .min_h(0.0)
            .scroll(ScrollSpec {
                stick_bottom: false,
            })
            .child(list)
            .with_scroll_bar("conversations", look),
    );
    let new_message = icon_button(
        "new-message",
        &icons::COMPOSE,
        "New message",
        |s| s.go_home(HomeView::NewMessage),
        look,
    );
    sidebar_frame(s, "Home", None, Some(new_message), body, look)
}

/// One of the three tiles under Home's title: a glyph over a word, with a count when
/// something waits there.
fn tile(
    s: &State,
    view: HomeView,
    label: &'static str,
    glyph: &'static Icon,
    count: usize,
    look: Look,
) -> El {
    let name = match count {
        0 => label.to_string(),
        n => format!("{label}, {n} waiting"),
    };
    quick_tile(
        label,
        &name,
        glyph,
        count,
        s.home_view == view,
        move |s: &mut State| s.go_home(view),
        look,
    )
}

/// A quick tile of a sidebar, three to a row: a glyph over a word, with a count when there
/// is one. Home's, and a server's Mentions (the boards' Threads, Mentions and Saved).
pub(crate) fn quick_tile(
    label: &'static str,
    name: &str,
    glyph: &'static Icon,
    count: usize,
    current: bool,
    press: impl Fn(&mut State) + 'static,
    look: Look,
) -> El {
    let c = look.c;
    let ink = if current { c.accent_ink } else { c.ink_body };
    // Three across the sidebar's column.
    let width = (SIDEBAR_WIDTH - t::SPACE_12 - t::SPACE_10 - 2.0 * t::SPACE_6) / 3.0;
    let mut tile = col()
        .key(format!("tile-{}", label.to_lowercase()))
        .w(width)
        .h(t::QUICK_TILE_HEIGHT)
        .center()
        .gap(t::SPACE_4)
        .radius(t::RADIUS_14)
        .fill(c.surface)
        .hover_group()
        .role("button")
        .label(name.to_string())
        .cursor(Cursor::Pointer)
        .focusable(true)
        .focus_ring(2.0, 2.0, c.focus)
        .on_press(press);
    if current {
        tile = tile.border(1.0, c.accent);
    } else {
        // A ring at rest, and the darker ring with its shadow under the pointer: the tile
        // again, under its glyph and word, drawn only then.
        tile = shadowed(tile, look.shadows.quick).child(shadowed(
            hover_layer(t::RADIUS_14).fill(c.surface),
            look.shadows.quick_hover,
        ));
    }
    tile = tile
        .child(icon(glyph, t::SPACE_18).color(ink))
        .child(text(label).font(t::TEXT_SECTION).color(ink));
    if count > 0 {
        tile = tile.child(
            el().pin(None, Some(t::SPACE_6), Some(t::SPACE_6), None)
                .h(t::BADGE_SMALL)
                .min_w(t::BADGE_SMALL)
                .px(t::SPACE_4)
                .radius(t::RADIUS_PILL)
                .fill(c.accent)
                .center()
                .a11y_hidden()
                .child(
                    text(count.to_string())
                        .font(t::TEXT_COUNT)
                        .color(c.on_accent),
                ),
        );
    }
    tile
}

/// A personal conversation in the sidebar: the other side's avatar and name.
fn conversation_row(s: &State, channel: &ChannelSummary, look: Look) -> El {
    let c = look.c;
    let name = s.session.conversation_name(channel);
    let tone = s
        .session
        .others_in(channel)
        .next()
        .unwrap_or(channel.channel_id.as_str());
    let active = s.home_view == HomeView::Conversations
        && s.session.dm() == Some(channel.channel_id.as_str());
    let (unread, mentions) = s.session.unread(&channel.channel_id);
    let bold = !active && unread > 0;
    let id = channel.channel_id.clone();
    let mut button = row()
        .key(format!("dm-{}", channel.channel_id))
        .w_full()
        .h(t::ROW_HEIGHT_PERSON)
        .px(t::SPACE_10)
        .gap(t::SPACE_10)
        .items_center()
        .radius(t::RADIUS_8)
        .role("button")
        .label(super::server::with_unread(
            &format!("Conversation with {name}"),
            (unread, mentions),
        ))
        .cursor(Cursor::Pointer)
        .focusable(true)
        .focus_ring(2.0, 0.0, c.focus)
        .on_press(move |s: &mut State| s.open_dm(&id))
        .child(avatar_of(s, &name, tone, t::AVATAR_XS, look))
        .child(
            text(name)
                .font(if active {
                    t::TEXT_NAV_ACTIVE
                } else if bold {
                    t::TEXT_NAV_UNREAD
                } else {
                    t::TEXT_NAV
                })
                .color(if active {
                    c.ink_selected
                } else if bold {
                    c.ink
                } else {
                    c.ink_quiet
                })
                .truncate()
                .grow(1.0)
                .min_w(0.0),
        );
    if mentions > 0 {
        button = button.child(super::components::count_badge(mentions, false, look));
    }
    if active {
        button = button.fill(c.raised);
        for layer in look.shadows.selected {
            button = button.box_shadow(*layer);
        }
    } else {
        button = button.hover(c.hover_row);
    }
    el().w_full().role("listitem").child(button)
}

/// Home's main pane.
pub fn main(s: &State, look: Look) -> El {
    match s.home_view {
        HomeView::Conversations => conversations::view(s, look),
        HomeView::Requests => requests::view(s, look),
        HomeView::NewMessage => new_message::view(s, look),
        HomeView::Devices => devices::view(s, look),
        HomeView::Profile => profile::view(s, look),
        HomeView::Preferences => preferences::view(s, look),
        HomeView::AddServer => add_server::view(s, look),
        HomeView::Mentions => super::mentions::view(s, None, look),
    }
}

/// Home's aside panel, when the view has one.
pub fn aside(s: &State, look: Look) -> Option<El> {
    match s.home_view {
        HomeView::Conversations => conversations::aside(s, look),
        _ => None,
    }
}

// ---- What Home's panes share --------------------------------------------------------------

/// A pane that is a page and not a conversation: a header with its topic over whatever
/// scrolls under it.
fn page(title: &str, topic: &str, content: El, look: Look) -> El {
    panel("panel", look)
        .w_full()
        .child(conversation::header_with(title, None, Some(topic), look))
        .child(
            col()
                .key("page")
                .w_full()
                .grow(1.0)
                .min_h(0.0)
                .scroll(ScrollSpec {
                    stick_bottom: false,
                })
                .child(
                    content
                        .w_full()
                        .px(t::SPACE_24)
                        .pt(t::SPACE_16)
                        .pb(t::SPACE_24)
                        .gap(t::SPACE_12),
                )
                .with_scroll_bar("page", look),
        )
}

/// The pane of a computer that holds no identity: personal conversations are encrypted to
/// devices, and this one is not among them yet. Devices is where it is linked.
fn no_identity(look: Look) -> El {
    panel("panel", look)
        .w_full()
        .child(conversation::header("Home", None, look))
        .child(
            empty_state(
                &icons::SHIELD,
                "Link this computer to your identity",
                "Personal messages are end-to-end encrypted between your devices and theirs. \
                 This computer has no device key yet; a device you already use approves it.",
                look,
            )
            .key("no-identity")
            .child(el().pt(t::SPACE_8).child(action_button(
                "link-this-computer",
                "Link this computer",
                true,
                |s| s.go_home(HomeView::Devices),
                look,
            ))),
        )
}

/// A person as a row's start, in a member row's metrics: the avatar, the name with `mark`
/// beside it (a tag), and a quieter line under it.
fn person(
    s: &State,
    name: &str,
    subtitle: &str,
    account_id: &str,
    size: f32,
    mark: Option<El>,
    look: Look,
) -> El {
    let c = look.c;
    let named = row()
        .w_full()
        .gap(t::SPACE_6)
        .items_center()
        .child(
            text(name)
                .font(if size >= t::AVATAR_LG {
                    t::TEXT_AUTHOR
                } else {
                    t::TEXT_MEMBER
                })
                .color(c.ink)
                .truncate()
                .shrink()
                .min_w(0.0),
        )
        .children(mark);
    let mut words = col().grow(1.0).min_w(0.0).child(named);
    if !subtitle.is_empty() {
        words = words.child(
            text(subtitle)
                .font(t::TEXT_META)
                .color(c.ink_muted)
                .truncate(),
        );
    }
    row()
        .grow(1.0)
        .min_w(0.0)
        .gap(t::SPACE_12)
        .items_center()
        .child(avatar_of(s, name, account_id, size, look))
        .child(words)
}

/// `@handle`, or nothing for someone whose handle is not known.
fn at(handle: &str) -> String {
    if handle.is_empty() {
        String::new()
    } else {
        format!("@{handle}")
    }
}
