//! This device's identity (architecture 5): the roster and its devices, linking this
//! computer to an identity, the signed profile, and the status this person chose.
//!
//! Built so far: linking (identity spec 4.2) and the roster. Not built yet: revoking a
//! device, the profile, the status, passkeys.

use lumia_client::app::RosterSummary;

use super::{Failure, Session, Update};

/// What a finished identity call reports.
#[derive(Debug, Clone)]
pub enum IdentityUpdate {
    /// Step 1 of linking: the request to show for a device that holds the identity to
    /// approve.
    LinkRequested(Result<String, Failure>),
    /// The code was claimed: this device is in the roster and signed in by identity proof.
    Linked(Result<RosterSummary, Failure>),
    /// The roster this device holds.
    Roster(Result<RosterSummary, Failure>),
}

/// What this device knows of its identity.
#[derive(Debug, Default)]
pub struct IdentityState {
    /// The roster this device holds, once asked for.
    pub roster: Option<RosterSummary>,
}

impl Session {
    /// Whether this device holds an identity (which personal conversations need).
    pub fn has_identity(&self) -> bool {
        self.client().is_some_and(|app| app.has_identity())
    }

    /// Starts linking this computer to an identity; the request to show arrives as
    /// [`IdentityUpdate::LinkRequested`].
    pub fn request_link(&self, label: &str) {
        let Some(app) = self.client() else { return };
        let label = label.to_string();
        self.task_local(move || async move {
            let request = app.identity_link_request(&label).await;
            Update::Identity(IdentityUpdate::LinkRequested(
                request.map_err(Failure::from),
            ))
        });
    }

    /// Claims the code an approving device gave and signs in by identity proof; the answer
    /// is [`IdentityUpdate::Linked`].
    pub fn claim_link(&self, code: &str) {
        let Some(app) = self.client() else { return };
        let code = code.trim().to_string();
        self.task_local(move || async move {
            let linked = async {
                let roster = app.identity_claim(&code).await?;
                app.sign_in_identity().await?;
                Ok::<_, lumia_client::ClientError>(roster)
            };
            Update::Identity(IdentityUpdate::Linked(linked.await.map_err(Failure::from)))
        });
    }

    /// Asks for the roster this device holds ([`IdentityUpdate::Roster`]).
    pub fn load_roster(&self) {
        let Some(app) = self.client() else { return };
        if !app.has_identity() {
            return;
        }
        self.task_local(move || async move {
            Update::Identity(IdentityUpdate::Roster(
                app.identity_show().await.map_err(Failure::from),
            ))
        });
    }

    pub(super) fn apply_identity(&mut self, update: &IdentityUpdate) {
        match update {
            IdentityUpdate::Linked(Ok(roster)) => {
                self.identity.roster = Some(roster.clone());
                self.signed_in = true;
                self.load();
            }
            IdentityUpdate::Roster(Ok(roster)) => self.identity.roster = Some(roster.clone()),
            IdentityUpdate::LinkRequested(_)
            | IdentityUpdate::Linked(Err(_))
            | IdentityUpdate::Roster(Err(_)) => {}
        }
    }
}
