//! Profile: how people see this person. The status they chose (protocol 6.7: online, away
//! or do not disturb, with a custom line), the signed profile the identity carries
//! (architecture 6.4: display name, pronouns and bio, signed by this computer's device key
//! and sent to the instance as the account's), and, for an account with no identity on
//! this computer, the way to linking it. The same states and wording as `ProfileView` in
//! the SwiftUI app's `HomeViews.swift` and the web's `home.tsx`.
//!
//! Sign out is at the foot of this page. The SwiftUI app keeps it in its Account menu,
//! with "Change instance"; this shell has no menu of its own yet, and a menu bar is macOS
//! only.
//!
//! Not here: the avatar (no shell sets one yet; a save keeps the one the profile names).

use rune::{El, Tx, col, text};

use super::devices::{action, link_notice, page};
use crate::app::State;
use crate::tokens::{self as t, icons};
use crate::ui::components::{
    Look, card, error_text, field, notice, primary_button, secondary_button, segmented,
};

pub fn view(s: &State, look: Look) -> El {
    let mut content = col()
        .w_full()
        .padding(t::SPACE_16)
        .gap(t::SPACE_12)
        .child(status(s, look));
    let form = &s.account.profile;
    content = if form.loaded || s.session.has_identity() {
        content.child(profile(s, look))
    } else {
        content.child(link_notice(
            "profile-link",
            "Your profile travels with your identity, signed by its devices. This computer holds no device key yet; a device you already use approves it.",
            look,
        ))
    };
    content = content.child(account(s, look));
    page("profile", "Profile", "How people see you", content, look)
}

fn quiet(words: &str, look: Look) -> El {
    text(words)
        .font(t::TEXT_META)
        .color(look.c.ink_secondary)
        .wrap()
        .w_full()
}

fn save_status(s: &mut State, _: &mut Tx) {
    s.save_status();
}

fn save_profile(s: &mut State, _: &mut Tx) {
    s.save_profile();
}

/// What people see beside this person's name in a server's member list.
fn status(s: &State, look: Look) -> El {
    let form = &s.account.profile;
    let chosen = match form.status.as_str() {
        "AWAY" => "AWAY",
        "DO_NOT_DISTURB" => "DO_NOT_DISTURB",
        _ => "ONLINE",
    };
    let mut card = card(
        "Status",
        "What people see beside your name in a server's member list.",
        look,
    )
    .key("status-card")
    .w_full()
    .child(segmented(
        "status",
        "Status",
        &[
            ("ONLINE", "Online"),
            ("AWAY", "Away"),
            ("DO_NOT_DISTURB", "Do not disturb"),
        ],
        chosen,
        |s: &mut State, status: &str| s.pick_status(status),
        look,
    ))
    .child(field(
        "custom-status",
        "Custom status",
        &form.custom_status,
        "At lunch",
        |s| &mut s.account.profile.custom_status,
        save_status,
        look,
    ));
    if !form.status_error.is_empty() {
        card = card.child(error_text(&form.status_error, look));
    }
    if form.status_saved {
        card = card.child(quiet("Status saved.", look));
    }
    card.child(action(primary_button(
        "save-status",
        "Save status",
        true,
        State::save_status,
        look,
    )))
}

/// The signed profile's form. It waits for the profile held: fields shown before it lands
/// would be overwritten by it, typing and all.
fn profile(s: &State, look: Look) -> El {
    let form = &s.account.profile;
    let title = match s.session.snapshot.as_ref().map(|snap| snap.handle.as_str()) {
        Some(handle) if !handle.is_empty() => format!("@{handle}"),
        _ => "You".into(),
    };
    let subtitle = match (&s.session.identity.profile, form.loaded) {
        (_, false) => "Looking…".to_string(),
        (Some(held), true) => format!(
            "Version {}, signed by this computer's device key; every instance you sign in to takes it.",
            held.version
        ),
        (None, true) => "Nothing set yet: people see your handle.".to_string(),
    };
    let mut card = card(&title, &subtitle, look).key("profile-card").w_full();
    if !form.loaded {
        return card;
    }
    card = card
        .child(field(
            "display-name",
            "Display name",
            &form.display_name,
            "How people see you",
            |s| &mut s.account.profile.display_name,
            save_profile,
            look,
        ))
        .child(field(
            "pronouns",
            "Pronouns",
            &form.pronouns,
            "they/them",
            |s| &mut s.account.profile.pronouns,
            save_profile,
            look,
        ))
        .child(field(
            "bio",
            "Bio",
            &form.bio,
            "A line about you",
            |s| &mut s.account.profile.bio,
            save_profile,
            look,
        ));
    if !form.error.is_empty() {
        card = card.child(error_text(&form.error, look));
    }
    if form.saved {
        card = card.child(quiet("Saved. This instance shows it now.", look));
    }
    card.child(action(primary_button(
        "save-profile",
        "Save",
        !form.busy,
        State::save_profile,
        look,
    )))
}

/// Who is signed in and where, with the way out.
fn account(s: &State, look: Look) -> El {
    notice(
        &icons::KEY,
        format!(
            "Signed in to {} as {}.",
            s.session.address(),
            s.session.self_name()
        ),
        look,
    )
    .key("account")
    .items_center()
    .child(secondary_button(
        "sign-out",
        "Sign out",
        true,
        State::sign_out,
        look,
    ))
}
