//! The app shell on the Quiet Split, as the design canvas draws it and `ShellView.swift`
//! builds it: the server rail, a sidebar on the ground, the main pane as a floating panel,
//! and an aside panel. This file is the frame and the rail; what fills the frame is
//! [`super::home`] at Home and [`super::server`] in a server.

use lumia_client::app::ServerSummary;
use rune::{Cursor, El, col, el, icon, row, text};

use super::components::{Look, avatar, initials};
use super::{home, server};
use crate::app::{HomeView, Place, State};
use crate::session::Connection;
use crate::tokens::{self as t, icons};

/// The rail's top inset, clear of the window's traffic lights under a full-size content view.
const TITLEBAR: f32 = t::SPACE_40 + t::SPACE_4;
pub const SIDEBAR_WIDTH: f32 = 240.0;
pub const ASIDE_WIDTH: f32 = 280.0;

pub fn view(s: &State, look: Look) -> El {
    let current = s
        .server_id()
        .and_then(|id| s.session.servers().iter().find(|x| x.server_id == id));
    let (sidebar, main, aside) = match current {
        Some(server) => (
            server::sidebar(s, server, look),
            server::main(s, server, look),
            Some(server::members(s, server, look)),
        ),
        None => (
            home::sidebar(s, look),
            home::main(s, look),
            home::aside(s, look),
        ),
    };
    let shell = row()
        .key("shell")
        .w_full()
        .h_full()
        .fill(look.c.bg)
        .child(rail(s, current, look))
        .child(sidebar)
        .child(
            col()
                .grow(1.0)
                .min_w(0.0)
                .h_full()
                .py(t::SPACE_8)
                .child(main),
        );
    match aside {
        Some(aside) => shell.child(col().h_full().padding(t::SPACE_8).child(aside)),
        None => shell.child(el().w(t::SPACE_8)),
    }
}

// ---- Rail -------------------------------------------------------------------------------

/// A round button on the rail, for something that is not a server.
fn rail_button(
    key: &'static str,
    label: &str,
    current: bool,
    press: fn(&mut State),
    look: Look,
) -> El {
    let c = look.c;
    let mut tile = el()
        .key(key)
        .size(t::TILE_SIZE, t::TILE_SIZE)
        .radius(t::RADIUS_14)
        .fill(c.tile)
        .center()
        .role("button")
        .label(label)
        .help(label)
        .cursor(Cursor::Pointer)
        .focusable(true)
        .focus_ring(2.0, 2.0, c.focus)
        .no_window_drag()
        .on_press(press);
    if current {
        for ring in look.shadows.ring_current {
            tile = tile.box_shadow(*ring);
        }
    } else {
        for layer in look.shadows.tile {
            tile = tile.box_shadow(*layer);
        }
    }
    tile
}

fn rail(s: &State, current: Option<&ServerSummary>, look: Look) -> El {
    let c = look.c;
    let mut rail = col()
        .key("rail")
        .w(t::RAIL_WIDTH)
        .h_full()
        .pt(TITLEBAR)
        .pb(t::SPACE_12)
        .gap(t::SPACE_12)
        .items_center()
        .fill(c.rail)
        .window_drag();
    // Home, with the requests that wait as its badge.
    let waiting = s
        .session
        .snapshot
        .as_ref()
        .map_or(0, |snap| snap.pending_requests.len());
    let mut home = el().child(
        rail_button(
            "home",
            "Home",
            s.place == Place::Home,
            |s| s.go_home(HomeView::Conversations),
            look,
        )
        .child(icon(&icons::LOGO, t::TILE_SIZE - t::SPACE_8)),
    );
    if waiting > 0 {
        home = home.child(
            el().absolute(t::TILE_SIZE - t::SPACE_14, -t::SPACE_6)
                .h(t::SPACE_18 + t::SPACE_2)
                .min_w(t::SPACE_18 + t::SPACE_2)
                .px(t::SPACE_4)
                .radius(t::RADIUS_PILL)
                .fill(c.accent)
                .border(2.0, c.rail)
                .center()
                .label(format!("{waiting} requests waiting"))
                .child(
                    text(waiting.to_string())
                        .font(t::TEXT_COUNT)
                        .color(c.on_accent),
                ),
        );
    }
    rail = rail
        .child(home)
        .child(el().w(t::SPACE_24).h(1.0).fill(c.rail_divider));
    for server in s.session.servers() {
        let active = current.is_some_and(|x| x.server_id == server.server_id);
        let id = server.server_id.clone();
        let mut tile = el()
            .key(format!("server-{}", server.server_id))
            .size(t::TILE_SIZE, t::TILE_SIZE)
            .radius(t::TILE_SIZE / 2.0)
            .fill(if active { c.raised } else { c.tile })
            .center()
            .role("button")
            .label(server.name.clone())
            .help(server.name.clone())
            .cursor(Cursor::Pointer)
            .focusable(true)
            .focus_ring(2.0, 2.0, c.focus)
            .no_window_drag()
            .on_press(move |s: &mut State| s.select_server(&id))
            .child(
                text(initials(&server.name))
                    .font(if active {
                        t::TITLE_MONOGRAM
                    } else {
                        t::TEXT_AUTHOR
                    })
                    .color(if active { c.ink_selected } else { c.ink }),
            );
        if active {
            for ring in look.shadows.ring_current {
                tile = tile.box_shadow(*ring);
            }
        }
        rail = rail.child(tile);
    }
    rail = rail.child(
        rail_button(
            "add-server",
            "Add a server",
            false,
            |s| s.go_home(HomeView::AddServer),
            look,
        )
        .radius(t::TILE_SIZE / 2.0)
        .child(icon(&icons::PLUS, t::SPACE_18).color(c.ink_secondary)),
    );
    let connected = s.session.connection == Connection::Connected;
    let me = el()
        .key("self")
        .role("button")
        .label(format!("{}, open profile", s.session.self_name()))
        .help(format!(
            "{} · {}",
            s.session.self_name(),
            s.session.connection.label()
        ))
        .cursor(Cursor::Pointer)
        .focusable(true)
        .focus_ring(2.0, 2.0, c.focus)
        .radius(t::AVATAR_SELF / 2.0)
        .no_window_drag()
        .on_press(|s: &mut State| s.go_home(HomeView::Profile))
        .child(avatar(
            s.session.self_name(),
            s.session.me(),
            t::AVATAR_SELF,
            look,
        ))
        .child(
            el().absolute(t::AVATAR_SELF - t::SPACE_10, t::AVATAR_SELF - t::SPACE_10)
                .size(t::SPACE_12, t::SPACE_12)
                .radius(t::SPACE_6)
                .fill(if connected { c.online } else { c.ink_muted })
                .border(2.0, c.rail),
        );
    rail.child(el().grow(1.0)).child(me)
}

// ---- Sidebar ----------------------------------------------------------------------------

/// The sidebar column both places share: a title row with an optional control at its end,
/// and whatever scrolls under it. Who is signed in, and Sign out, are on the profile page
/// the rail's self avatar opens.
pub fn sidebar_frame(_s: &State, title: &str, trailing: Option<El>, body: El, look: Look) -> El {
    let c = look.c;
    let mut title_row = row()
        .w_full()
        .h(t::HEADER_HEIGHT - t::SPACE_24)
        .pl(t::SPACE_8)
        .mb(t::SPACE_10)
        .gap(t::SPACE_6)
        .items_center()
        .window_drag()
        .child(
            text(title)
                .font(t::TITLE_SERVER)
                .color(c.ink)
                .truncate()
                .grow(1.0)
                .min_w(0.0)
                .role("heading"),
        );
    if let Some(trailing) = trailing {
        title_row = title_row.child(trailing);
    }
    col()
        .key("sidebar")
        .w(SIDEBAR_WIDTH)
        .h_full()
        .pt(t::SPACE_14 + t::SPACE_14)
        .pl(t::SPACE_12)
        .pr(t::SPACE_10)
        .child(title_row)
        .child(body)
}
