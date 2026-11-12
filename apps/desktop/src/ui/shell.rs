//! The app shell on the Quiet Split, as the design canvas draws it: the server rail, a
//! sidebar on the ground, the main pane as a floating panel, and an aside panel. This file
//! is the frame and the rail; what fills the frame is [`super::home`] at Home and
//! [`super::server`] in a server.
//!
//! The board draws the window's traffic lights at the head of the rail, with the Home tile
//! under them; [`TRAFFIC_LIGHTS`] is where the real ones are put to match. Where the frame
//! leaves the board: nothing lifts on hover, and the rail has no unread dot or mention
//! count on a server, which the session does not count. The self avatar opens the account
//! menu (`menus::account`), and its dot says whether the gateway is connected.

use lumia_client::app::ServerSummary;
use rune::{Cursor, El, col, el, icon, row, text};

use super::components::{Look, avatar_of, circled, drawn, initials, shadowed, status_dot};
use super::menus::{SELF_AVATAR, SERVER_TITLE};
use super::{home, server};
use crate::app::{HomeView, Place, Popover, State};
use crate::session::Connection;
use crate::tokens::{self as t, icons};

/// Where the window's traffic lights go, from the window's top left corner to the close
/// button's. The board draws three 12-point lights 7 points in and 12 down, filling the
/// rail's width. AppKit's are 14 points, so they start a point higher to share the board's
/// centre line, and at AppKit's own spacing the third ends a few points past the rail,
/// over the sidebar's padding and clear of its title.
pub const TRAFFIC_LIGHTS: (f32, f32) = (7.0, 11.0);
/// The rail's top inset, which puts the Home tile where the board has it under the lights:
/// 12 above them, their 12, 8 under them and the rail's gap of 12.
const TITLEBAR: f32 = t::SPACE_40 + t::SPACE_4;
/// The sidebar's top inset, the board's.
const SIDEBAR_TOP: f32 = t::SPACE_14;
/// The sidebar column and the aside panel. The board gives the rail and the sidebar 304
/// together and the aside's column 300, 8 of it either side of the panel; the design system
/// has no token for either width.
pub const SIDEBAR_WIDTH: f32 = 240.0;
pub const ASIDE_WIDTH: f32 = 284.0;

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

/// A button on the rail, a tile across: what every one of them is, before its shape and fill.
fn rail_button(key: impl Into<String>, label: &str, look: Look) -> El {
    el().key(key)
        .size(t::TILE_SIZE, t::TILE_SIZE)
        .center()
        .role("button")
        .label(label)
        .help(label)
        .cursor(Cursor::Pointer)
        .focusable(true)
        .focus_ring(2.0, 2.0, look.c.focus)
        .no_window_drag()
}

fn rail(s: &State, current: Option<&ServerSummary>, look: Look) -> El {
    let c = look.c;
    let mut rail = col()
        .key("rail")
        .w(t::RAIL_WIDTH)
        .h_full()
        .pt(TITLEBAR)
        .pb(t::SPACE_16)
        .gap(t::SPACE_12)
        .items_center()
        .fill(c.rail)
        .window_drag();
    // Home: the logo on a rounded tile, with the requests that wait as its badge.
    let waiting = s
        .session
        .snapshot
        .as_ref()
        .map_or(0, |snap| snap.pending_requests.len());
    let tile = rail_button("home", "Home", look)
        .radius(t::RADIUS_14)
        .fill(c.tile)
        .on_press(|s: &mut State| s.go_home(HomeView::Conversations))
        .child(icon(&icons::LOGO, t::TILE_SIZE - t::SPACE_8));
    let mut home = el().child(shadowed(
        tile,
        if s.place == Place::Home {
            look.shadows.ring_current
        } else {
            look.shadows.tile
        },
    ));
    if waiting > 0 {
        // Over the tile's corner, ringed in the rail's colour (the board has it 5 out; 4
        // is the nearest step).
        home = home.child(
            el().pin(None, Some(-t::SPACE_4), Some(-t::SPACE_4), None)
                .h(t::BADGE)
                .min_w(t::BADGE)
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
        let mut tile = rail_button(format!("server-{}", server.server_id), &server.name, look)
            .radius(t::TILE_SIZE / 2.0)
            .fill(if active { c.raised } else { c.tile })
            .on_press(move |s: &mut State| s.select_server(&id));
        // The server's picture once this computer holds it; its initials until it is drawn.
        let picture = s.picture_at(&server.icon_url);
        if !picture.is_some_and(drawn) {
            tile = tile.child(
                text(initials(&server.name))
                    .font(if active {
                        t::TITLE_MONOGRAM
                    } else {
                        t::TEXT_MONOGRAM_REST
                    })
                    .color(if active { c.ink_selected } else { c.ink }),
            );
        }
        if let Some(picture) = picture {
            tile = tile.child(circled(picture, t::TILE_SIZE).a11y_hidden());
        }
        if active {
            tile = shadowed(tile, look.shadows.ring_current);
        }
        rail = rail.child(tile);
    }
    // Adding a server is not one yet: a dashed outline on the rail itself.
    rail = rail.child(
        rail_button("add-server", "Add a server", look)
            .radius(t::TILE_SIZE / 2.0)
            .border(1.5, c.border_dashed)
            .border_dash(t::SPACE_4, t::SPACE_4)
            .on_press(|s: &mut State| s.go_home(HomeView::AddServer))
            .child(icon(&icons::PLUS, t::SPACE_18).color(c.ink_quiet)),
    );
    let connected = s.session.connection == Connection::Connected;
    let me = el()
        .key(SELF_AVATAR)
        .role("button")
        .label(format!("{}, account menu", s.session.self_name()))
        .expanded(s.popover == Some(Popover::Account))
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
        .on_press(State::open_account_menu)
        .child(avatar_of(
            s,
            s.session.self_name(),
            s.session.me(),
            t::AVATAR_SELF,
            look,
        ))
        .child(status_dot(
            t::AVATAR_SELF,
            if connected { c.online } else { c.ink_muted },
            c.rail,
        ));
    rail.child(el().grow(1.0)).child(me)
}

// ---- Sidebar ----------------------------------------------------------------------------

/// The sidebar column both places share: a title row with an optional control at its end,
/// and whatever scrolls under it.
///
/// With `menu`, the title is the board's button: the name and a chevron, opening the menu
/// `menu` opens. Without one it is a heading in the button's place and metrics.
pub fn sidebar_frame(
    s: &State,
    title: &str,
    menu: Option<fn(&mut State)>,
    trailing: Option<El>,
    body: El,
    look: Look,
) -> El {
    let c = look.c;
    let name = text(title)
        .font(t::TITLE_SERVER)
        .color(c.ink)
        .truncate()
        .shrink()
        .min_w(0.0)
        .role("heading");
    let mut titled = row()
        .grow(1.0)
        .min_w(0.0)
        .h(t::TITLE_ROW_HEIGHT)
        .px(t::SPACE_8)
        .gap(t::SPACE_6)
        .items_center()
        .radius(t::RADIUS_9)
        .child(name);
    if let Some(open) = menu {
        titled = titled
            .key(SERVER_TITLE)
            .role("button")
            .label(format!("{title}, server menu"))
            .expanded(s.popover == Some(Popover::ServerMenu))
            .hover(c.hover_row)
            .cursor(Cursor::Pointer)
            .focusable(true)
            .focus_ring(2.0, 0.0, c.focus)
            .no_window_drag()
            .on_press(open)
            .child(icon(&icons::CHEVRON_DOWN, t::SPACE_14).color(c.ink_quiet));
    }
    let mut title_row = row()
        .w_full()
        .mb(t::SPACE_10)
        .gap(t::SPACE_6)
        .items_center()
        .window_drag()
        .child(titled);
    if let Some(trailing) = trailing {
        title_row = title_row.child(trailing);
    }
    col()
        .key("sidebar")
        .w(SIDEBAR_WIDTH)
        .h_full()
        .pt(SIDEBAR_TOP)
        .pb(t::SPACE_14)
        .pl(t::SPACE_12)
        .pr(t::SPACE_10)
        .child(title_row)
        .child(body)
}
