//! The rail as this account arranged it (architecture 6.9): its servers in an order of their
//! own, some gathered in folders.
//!
//! The snapshot carries the arrangement ready to draw, its servers already in the rail's
//! order. An edit is made by the core's `lumia_client::rail` functions on the rail as it is
//! and sent whole with [`Session::arrange_rail`]; until the instance answers, the rail shows
//! the edit ([`RailState::pending`]), so a tile moves the moment it is dropped. The answer is
//! the account's snapshot as it then is. A failure puts the rail back as the snapshot has it
//! and says why over the composer.

use lumia_client::app::SnapshotSummary;
use lumia_client::rail::{self, RailItem};

use super::{Failure, Session, Update};

/// The edit on its way to the instance.
#[derive(Debug, Default)]
pub struct RailState {
    /// The rail as last sent, until its answer arrives.
    pub pending: Option<Vec<RailItem>>,
    /// How many arrangements were sent; an answer to any but the last is not the rail.
    sent: u64,
}

/// What an arrangement sent reports.
#[derive(Debug, Clone)]
pub enum RailUpdate {
    /// The account's snapshot once the `sent`th arrangement was kept, or why it was not.
    Arranged {
        sent: u64,
        result: Result<SnapshotSummary, Failure>,
    },
}

impl RailState {
    /// Leaves an account: an answer still on its way belongs to it, so it is not taken.
    pub(super) fn forget(&mut self) {
        self.pending = None;
        self.sent += 1;
    }

    /// How many arrangements were sent: what the last one's answer carries.
    pub fn sent(&self) -> u64 {
        self.sent
    }
}

impl Session {
    /// The rail between Home and **+**: the edit being sent, else the snapshot's, with every
    /// server this account is in placed once (a snapshot the core did not tidy, as a test's,
    /// reads as its servers in order).
    pub fn rail(&self) -> Vec<RailItem> {
        let Some(snapshot) = &self.snapshot else {
            return Vec::new();
        };
        let ids: Vec<String> = snapshot
            .servers
            .iter()
            .map(|s| s.server_id.clone())
            .collect();
        rail::tidy(self.rail.pending.as_ref().unwrap_or(&snapshot.rail), &ids)
    }

    /// Keeps `items` as this account's rail ([`RailUpdate::Arranged`]); the rail shows them
    /// from now. The same rail as before sends nothing.
    pub fn arrange_rail(&mut self, items: Vec<RailItem>) {
        if items == self.rail() {
            return;
        }
        self.rail.sent += 1;
        let sent = self.rail.sent;
        self.rail.pending = Some(items.clone());
        let Some(app) = self.client() else { return };
        self.task(async move {
            let result = app.arrange_rail(&items).await.map_err(Failure::from);
            Update::Rail(RailUpdate::Arranged { sent, result })
        });
    }

    /// The servers in the rail's order, a folder's in its place.
    pub fn rail_order(&self) -> Vec<String> {
        rail::server_order(&self.rail())
    }

    pub(super) fn apply_rail(&mut self, update: &RailUpdate) {
        let RailUpdate::Arranged { sent, result } = update;
        // An answer to an arrangement since replaced changes nothing: the later one's is
        // on its way.
        if *sent != self.rail.sent {
            return;
        }
        self.rail.pending = None;
        match result {
            Ok(snapshot) => self.snapshot = Some(snapshot.clone()),
            Err(failure) => {
                self.last_error = Some(format!("The rail was not saved: {}", failure.message));
            }
        }
    }
}
