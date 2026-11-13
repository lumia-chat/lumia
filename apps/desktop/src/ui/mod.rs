//! The views: what the window draws for the state in `app`. They read the state and call
//! its methods; nothing here talks to the session's runtime or the network.

pub mod body;
mod channels;
pub mod components;
mod conversation;
mod emoji;
mod home;
mod link;
mod mentions;
mod menus;
mod replies;
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

/// "1 other", "3 others": the rest of a list of people after the names given.
pub fn others(n: usize) -> String {
    if n == 1 {
        "1 other".into()
    } else {
        format!("{n} others")
    }
}

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
    // The toolbar over the message under the pointer, under a popover it opened.
    let toolbar = (s.route == Route::Shell)
        .then(|| s.toolbar_for())
        .flatten()
        .filter(|id| s.session.entries.iter().any(|e| e.id == *id))
        .map(|id| replies::toolbar(id, look));
    let screen = match toolbar {
        Some(toolbar) => screen.child(toolbar),
        None => screen,
    };
    let screen = match s.popover.and_then(|popover| overlay(s, popover, look)) {
        Some(popover) => screen.child(popover),
        None => screen,
    };
    // What floats over the conversation without taking the focus: the @ list over the
    // composer, and "Quote reply" over a selection inside one message.
    let mut screen = screen;
    if s.route == Route::Shell && s.popover.is_none() {
        if let Some(list) = replies::mention_list(s, look) {
            screen = screen.child(list);
        }
        if let Some((id, words, backward)) = &s.replies.selection
            && let Some(at) = replies::quote_button_at(&cx.selection_rects(), *backward)
        {
            screen = screen.child(replies::quote_button(id, words, at, look));
        }
    }
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
        Popover::NewChannel => server_popovers::new_channel(s, menus::SERVER_TITLE, look),
        Popover::NewChannelHere => {
            server_popovers::new_channel(s, server_popovers::CHANNELS_PLUS, look)
        }
        Popover::Invite => server_popovers::invite(s, look),
        Popover::ChannelsMenu => server_popovers::channels_menu(s, look),
        Popover::AddPeople => server_popovers::add_people(s, look),
        Popover::NewDm => server_popovers::new_dm(s, look),
        Popover::ServerMenu => menus::server(s, look),
        Popover::Account => menus::account(s, look),
        Popover::Person => menus::person(s, look),
        Popover::EmojiPicker => emoji::picker(s, look),
    }
}
