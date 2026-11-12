//! Home: the sidebar with the personal conversations and Home's tiles, and a main pane per
//! [`HomeView`]. Each view is a module of its own.

mod add_server;
mod conversations;
mod devices;
mod new_message;
mod profile;
mod requests;

use rune::{El, ScrollSpec, col, text};

use super::components::Look;
use super::shell::sidebar_frame;
use crate::app::{HomeView, State};
use crate::tokens::{self as t};

/// Home's sidebar. Not built yet: the conversations and the tiles that lead to the other
/// views belong here.
pub fn sidebar(s: &State, look: Look) -> El {
    let body = col()
        .w_full()
        .grow(1.0)
        .min_h(0.0)
        .scroll(ScrollSpec {
            stick_bottom: false,
        })
        .child(
            text("No conversations yet.")
                .font(t::TEXT_META)
                .color(look.c.ink_quiet)
                .padding(t::SPACE_10),
        );
    sidebar_frame(s, "Home", None, body, look)
}

/// Home's main pane.
pub fn main(s: &State, look: Look) -> El {
    match s.home_view {
        HomeView::Conversations => conversations::view(s, look),
        HomeView::Requests => requests::view(s, look),
        HomeView::NewMessage => new_message::view(s, look),
        HomeView::Devices => devices::view(s, look),
        HomeView::Profile => profile::view(s, look),
        HomeView::AddServer => add_server::view(s, look),
    }
}

/// Home's aside panel, when the view has one.
pub fn aside(s: &State, look: Look) -> Option<El> {
    match s.home_view {
        HomeView::Conversations => conversations::aside(s, look),
        _ => None,
    }
}
