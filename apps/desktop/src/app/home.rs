//! Home's own state: the personal conversations, requests and a new message.
//!
//! Not built yet: this is the seam for what `session::dms` reports.

use super::State;
use crate::session::dms::DmUpdate;

/// What the Home screens hold between frames.
#[derive(Debug, Default)]
pub struct Home {}

impl State {
    pub(super) fn react_dm(&mut self, update: &DmUpdate) {
        match *update {}
    }
}
