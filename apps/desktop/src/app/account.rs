//! The account screens' state: devices, linking this computer to an identity, the profile
//! and the status.
//!
//! Not built yet: this is the seam for what `session::identity` reports.

use super::State;
use crate::session::identity::IdentityUpdate;

/// What the account screens hold between frames.
#[derive(Debug, Default)]
pub struct Account {}

impl State {
    pub(super) fn react_identity(&mut self, update: &IdentityUpdate) {
        // No screen shows these yet.
        match update {
            IdentityUpdate::LinkRequested(_)
            | IdentityUpdate::Linked(_)
            | IdentityUpdate::Roster(_) => {}
        }
    }
}
