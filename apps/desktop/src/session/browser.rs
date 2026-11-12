//! The system browser (architecture 6.3). A passkey and the instance's OIDC provider are
//! signed in with on the instance's own page, which comes back to the app on the `lumia:`
//! scheme with a one-time token that completes as a magic-link token does. The other way,
//! a signed-in app mints such a token and opens the web page as itself (a passkey is made
//! there). The counterpart of `signInThroughBrowser` and `openWebSignedIn` in LumiaKit.
//!
//! Nothing here opens a browser or hears a link: the shell does both and calls in.

use super::{Failure, Session, Update};

/// Where a sign-in made in the browser returns to, as the other native shells name it. The
/// instance appends `magic={token}`, or `error=…` when its provider refused.
pub const SIGN_IN_RETURN: &str = "lumia://sign-in";

/// What a finished browser call reports.
#[derive(Debug, Clone)]
pub enum BrowserUpdate {
    /// The token a sign-in in the browser came back with was completed.
    SignedIn(Result<(), Failure>),
    /// The page that opens the instance's web app signed in as this account.
    Handoff(Result<String, Failure>),
}

impl Session {
    /// Where to send the system browser to sign in: the instance's OIDC provider, or its
    /// web sign-in page (a passkey there, or any method the page offers). The browser comes
    /// back to `return_to`. `None` with no instance open.
    pub fn browser_sign_in_url(&self, oidc: bool, return_to: &str) -> Option<String> {
        let app = self.client()?;
        Some(if oidc {
            app.oidc_start_url(return_to)
        } else {
            app.web_sign_in_url(return_to)
        })
    }

    /// Completes a sign-in with the one-time token the browser came back with; the answer
    /// is [`BrowserUpdate::SignedIn`].
    pub fn sign_in_with_token(&self, token: &str) {
        let Some(app) = self.client() else { return };
        let token = token.to_string();
        self.task(async move {
            Update::Browser(BrowserUpdate::SignedIn(
                app.sign_in_magic_link(&token).await.map_err(Failure::from),
            ))
        });
    }

    /// Mints a one-time token for this account and reports the page that opens the
    /// instance's web app signed in with it, landing on `next` (`devices` for the passkeys
    /// there), as [`BrowserUpdate::Handoff`].
    pub fn web_handoff(&self, next: &str) {
        let Some(app) = self.client() else { return };
        let next = next.to_string();
        self.task(async move {
            let page = app
                .handoff_token()
                .await
                .map(|token| app.web_handoff_url(&token, &next));
            Update::Browser(BrowserUpdate::Handoff(page.map_err(Failure::from)))
        });
    }

    pub(super) fn apply_browser(&mut self, update: &BrowserUpdate) {
        match update {
            BrowserUpdate::SignedIn(Ok(())) => {
                self.signed_in = true;
                self.load();
            }
            BrowserUpdate::SignedIn(Err(_)) | BrowserUpdate::Handoff(_) => {}
        }
    }
}
