//! The menu bar's own items: what the SwiftUI app's menus have beyond the ones every Mac
//! app has (`LumiaApp.swift`), which is the Account menu. The app menu, Edit and Window
//! are Rune's standard ones. "Check for updates" waits for an updater.
//!
//! Only macOS has a menu bar to put these in; elsewhere the same actions are on screen
//! (Sign out at the sidebar's foot, Change instance on the sign-in card).

use rune::MenuItem;

use super::{Route, State};

/// The id of Account › Sign out.
pub const SIGN_OUT: &str = "sign-out";
/// The id of Account › Change instance.
pub const CHANGE_INSTANCE: &str = "change-instance";

/// What decides which items can be chosen: signed in, and an instance open.
pub fn standing(state: &State) -> (bool, bool) {
    (state.route == Route::Shell, state.session.is_open())
}

/// The items for a [`standing`]. An item that would do nothing is disabled, as in the
/// SwiftUI app.
pub fn items((signed_in, open): (bool, bool)) -> Vec<MenuItem<State>> {
    vec![
        MenuItem::new("Sign out", |state: &mut State, _| state.sign_out())
            .id(SIGN_OUT)
            .menu("Account")
            .enabled(signed_in),
        MenuItem::new("Change instance", |state: &mut State, _| {
            state.forget_instance();
        })
        .id(CHANGE_INSTANCE)
        .menu("Account")
        .enabled(open),
    ]
}

impl State {
    /// Leaves the instance without signing out of it and asks for one again; the saved
    /// instance is forgotten, so the next launch asks too. Its stores, and the session in
    /// them, stay on disk: opening the same instance again is signed in.
    pub fn forget_instance(&mut self) {
        self.change_instance();
        self.leave_shell();
        self.route = Route::SignIn;
        self.sign_in.reaching = false;
        self.settings.instance = None;
        self.settings.save(&self.data_dir);
    }
}
