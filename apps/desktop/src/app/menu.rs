//! The menu bar's own items, beyond the ones every Mac app has: the Account menu, and in
//! the application menu, under About, the two update items. The app menu, Edit and Window
//! are Rune's standard ones.
//!
//! Only macOS has a menu bar to put these in; elsewhere the same actions are on screen
//! (Sign out at the sidebar's foot, Change instance on the sign-in card). The update items
//! exist only while there is an updater, which is a bundled app on macOS
//! (`platform::updates`): a bare binary's menu does not offer what it cannot do.

use rune::{MenuItem, Tx};

use super::{Route, State};

/// The id of Account › Sign out.
pub const SIGN_OUT: &str = "sign-out";
/// The id of Account › Change instance.
pub const CHANGE_INSTANCE: &str = "change-instance";
/// The id of Lumia › Check for updates….
pub const CHECK_FOR_UPDATES: &str = "check-for-updates";
/// The id of Lumia › Check for updates automatically.
pub const CHECK_AUTOMATICALLY: &str = "check-for-updates-automatically";

/// What decides which items can be chosen: signed in, and an instance open.
pub fn standing(state: &State) -> (bool, bool) {
    (state.route == Route::Shell, state.session.is_open())
}

/// What the updater says about itself, which is what the update items show.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Updates {
    /// A check can be asked for now: none is under way.
    pub can_check: bool,
    /// The feed is checked once a day without being asked.
    pub automatic: bool,
}

/// An update item before it is a menu item: its id and title, whether it can be chosen
/// and whether it is ticked.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UpdateItem {
    pub id: &'static str,
    pub title: &'static str,
    pub enabled: bool,
    pub checked: bool,
}

impl Updates {
    /// The two items, in the order and words Lumia on the Mac has always had: a check the
    /// person starts, which cannot be chosen while one is under way, and the switch for
    /// the daily one (the only time the app contacts anything but the person's instances,
    /// so it is theirs to turn off), ticked while it is on.
    pub fn items(self) -> [UpdateItem; 2] {
        [
            UpdateItem {
                id: CHECK_FOR_UPDATES,
                title: "Check for updates…",
                enabled: self.can_check,
                checked: false,
            },
            UpdateItem {
                id: CHECK_AUTOMATICALLY,
                title: "Check for updates automatically",
                enabled: true,
                checked: self.automatic,
            },
        ]
    }
}

/// What the update items do when chosen. They act on the updater, which belongs to the
/// platform, so the platform supplies them (`platform::updates::ACTIONS`).
#[derive(Clone, Copy)]
pub struct UpdateActions {
    pub check: fn(&mut State, &mut Tx),
    pub toggle_automatic: fn(&mut State, &mut Tx),
}

/// The items for a [`standing`], and for an updater when there is one. An item that would
/// do nothing is disabled; without an updater its items are not
/// there at all. The update items carry no menu name, which puts them in the application
/// menu after About, where a Mac app has them.
pub fn items(
    (signed_in, open): (bool, bool),
    updater: Option<(Updates, UpdateActions)>,
) -> Vec<MenuItem<State>> {
    let mut items = Vec::new();
    if let Some((updates, actions)) = updater {
        for (item, action) in updates
            .items()
            .into_iter()
            .zip([actions.check, actions.toggle_automatic])
        {
            items.push(
                MenuItem::new(item.title, action)
                    .id(item.id)
                    .enabled(item.enabled)
                    .checked(item.checked),
            );
        }
    }
    items.push(
        MenuItem::new("Sign out", |state: &mut State, _| state.sign_out())
            .id(SIGN_OUT)
            .menu("Account")
            .enabled(signed_in),
    );
    items.push(
        MenuItem::new("Change instance", |state: &mut State, _| {
            state.forget_instance();
        })
        .id(CHANGE_INSTANCE)
        .menu("Account")
        .enabled(open),
    );
    items
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
