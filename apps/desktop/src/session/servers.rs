//! Making and joining servers, channels and invites (architecture 6.1, 6.2 and 6.6).
//!
//! Not built yet: this is the seam.

use super::Session;

/// What a finished server-management call reports.
#[derive(Debug, Clone)]
pub enum ServerUpdate {}

impl Session {
    pub(super) fn apply_server(&mut self, update: &ServerUpdate) {
        match *update {}
    }
}
