//! Personal conversations (architecture 6.4 and 11): the end-to-end encrypted DMs the core's
//! engine keeps, the requests that open them, and reading and sending in one.
//!
//! Not built yet: this is the seam. The engine's events are received and dropped, and a
//! personal conversation cannot be opened.

use lumia_client::app::AppEvent;

use super::{Entry, Session};

/// What a finished personal-conversation call reports.
#[derive(Debug, Clone)]
pub enum DmUpdate {}

/// The personal conversations this device knows, and what waits on them.
#[derive(Debug, Default)]
pub struct DmState {}

impl Session {
    pub(super) fn apply_dm(&mut self, update: &DmUpdate) {
        match *update {}
    }

    /// What the DM engine reported through the gateway loop.
    pub(super) fn on_dm_event(&mut self, event: &AppEvent) {
        if let AppEvent::DmError { message } = event {
            self.last_error = Some(message.clone());
        }
    }

    /// A personal conversation's messages as this device can read them.
    pub(super) fn read_dm_entries(&mut self, _channel_id: &str) -> Result<Vec<Entry>, String> {
        Ok(Vec::new())
    }

    /// Sends to a personal conversation; false when it could not be queued.
    pub(super) fn send_dm(&mut self, _channel_id: &str, _body: &str, _now_micros: i64) -> bool {
        false
    }
}
