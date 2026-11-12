//! The forms that make and manage servers: add a server (create one, or join by invite),
//! a new channel, invite people.
//!
//! Not built yet: this is the seam for what `session::servers` reports.

use super::State;
use crate::session::servers::ServerUpdate;

/// What the server-management forms hold between frames.
#[derive(Debug, Default)]
pub struct ServerForms {}

impl State {
    pub(super) fn react_server(&mut self, update: &ServerUpdate) {
        match *update {}
    }
}
