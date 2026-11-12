//! The views: what the window draws for the state in `app`. They read the state and call
//! its methods; nothing here talks to the session's runtime or the network.

pub mod components;
mod shell;
mod sign_in;

use rune::{Cx, El};

use crate::app::{Route, State};
use components::Look;

/// `AppSpec::view`.
pub fn view(s: &State, cx: &mut Cx<State>) -> El {
    let look = Look::of(cx.system_dark());
    match s.route {
        Route::SignIn => sign_in::view(s, look),
        Route::Shell => shell::view(s, look),
    }
}
