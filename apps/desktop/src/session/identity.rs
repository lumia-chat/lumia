//! This device's identity (architecture 5): the roster and its devices, linking this
//! computer to an identity, the signed profile, and the status this person chose.
//!
//! Not built yet: this is the seam.

use super::Session;

/// What a finished identity call reports.
#[derive(Debug, Clone)]
pub enum IdentityUpdate {}

/// What this device knows of its identity.
#[derive(Debug, Default)]
pub struct IdentityState {}

impl Session {
    pub(super) fn apply_identity(&mut self, update: &IdentityUpdate) {
        match *update {}
    }
}
