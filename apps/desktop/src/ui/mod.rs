//! The views: what the window draws for the state in `app`. They read the state and call
//! its methods; nothing here talks to the session's runtime or the network.

pub mod body;
pub mod components;
mod conversation;
mod home;
mod link;
mod menus;
mod scroll;
mod server;
mod server_popovers;
mod server_settings;
mod shell;
mod sign_in;
mod timeline;

use rune::{Cx, El};

use crate::app::{Popover, Route, State};
use crate::tokens;
use components::Look;
pub use shell::TRAFFIC_LIGHTS;

/// `AppSpec::view`.
pub fn view(s: &State, cx: &mut Cx<State>) -> El {
    // Light or dark as Preferences says, which may be "as the system is".
    let look = Look::of(s.settings.appearance.dark(cx.system_dark()));
    let screen = match s.route {
        Route::SignIn => sign_in::view(s, look),
        Route::Link => link::view(s, look),
        Route::Shell => shell::view(s, look),
    };
    // A popover is a child of the root after everything else, so it draws above the shell
    // and escapes its anchor's clipping.
    let screen = match s.popover.and_then(|popover| overlay(s, popover, look)) {
        Some(popover) => screen.child(popover),
        None => screen,
    };
    match s.autoscroll.origin() {
        Some(at) => screen.child(autoscroll_mark(at, look)),
        None => screen,
    }
}

/// Where the middle button came down while it autoscrolls a pane (`app::autoscroll`): a
/// ring with a dot, as browsers mark it, that the pointer moves away from.
fn autoscroll_mark(at: [f32; 2], look: Look) -> El {
    let c = look.c;
    let half = tokens::SPACE_24 / 2.0;
    rune::el()
        .key("autoscroll-mark")
        .absolute(at[0] - half, at[1] - half)
        .size(tokens::SPACE_24, tokens::SPACE_24)
        .radius(half)
        .fill(c.surface)
        .border(1.5, c.ink_muted)
        .center()
        .a11y_hidden()
        .child(
            rune::el()
                .size(tokens::SPACE_6, tokens::SPACE_6)
                .radius(tokens::SPACE_6 / 2.0)
                .fill(c.ink_muted),
        )
}

/// The surface for the open popover. Each belongs to the part of the product that opens it.
fn overlay(s: &State, popover: Popover, look: Look) -> Option<El> {
    match popover {
        Popover::NewChannel => server_popovers::new_channel(s, look),
        Popover::Invite => server_popovers::invite(s, look),
        Popover::ServerMenu => menus::server(s, look),
        Popover::Account => menus::account(s, look),
        Popover::Person => menus::person(s, look),
    }
}
