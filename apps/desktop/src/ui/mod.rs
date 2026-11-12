//! The views: what the window draws for the state in `app`. They read the state and call
//! its methods; nothing here talks to the session's runtime or the network.

pub mod components;
mod conversation;
mod home;
mod server;
mod server_popovers;
mod shell;
mod sign_in;
mod timeline;

use rune::{Cx, El};

use crate::app::{Popover, Route, State};
use components::Look;

/// `AppSpec::view`.
pub fn view(s: &State, cx: &mut Cx<State>) -> El {
    let look = Look::of(cx.system_dark());
    let screen = match s.route {
        Route::SignIn => sign_in::view(s, look),
        Route::Shell => shell::view(s, look),
    };
    // A popover is a child of the root after everything else, so it draws above the shell
    // and escapes its anchor's clipping.
    match s.popover.and_then(|popover| overlay(s, popover, look)) {
        Some(popover) => screen.child(popover),
        None => screen,
    }
}

/// The surface for the open popover. Each belongs to the part of the product that opens it.
fn overlay(s: &State, popover: Popover, look: Look) -> Option<El> {
    match popover {
        Popover::NewChannel => server_popovers::new_channel(s, look),
        Popover::Invite => server_popovers::invite(s, look),
    }
}
