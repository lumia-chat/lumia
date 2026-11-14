//! The updater the window shows, where the shell carries its own (architecture section 13,
//! "How the desktop shell updates itself"): today the Linux AppImage. On macOS Sparkle has
//! its own windows and the menu bar's two items (`menu`), and none of this is there; a deb
//! or rpm is updated by the package manager, and a bare binary has no updater at all.
//!
//! What the person can do is here, and what it costs is the platform's
//! (`platform::appimage`): the state says what it wants in `owed`, the platform's poll
//! carries it out on a thread of its own, and what that finds comes back as a [`Report`].
//! Nothing is downloaded or put in place without the person's say: the daily check only
//! finds out, as Sparkle's does, and a release it finds waits for "Update" on Preferences
//! or at the sidebar's foot.

use super::State;
use crate::settings::UpdateChecks;

/// How long after a check the next daily one is due, in seconds.
pub const DAY: i64 = 24 * 60 * 60;

/// The on-screen updater, present only where the platform has one.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Updater {
    pub phase: Phase,
    /// What the window asked for and the platform has not taken yet.
    pub owed: Option<Ask>,
    /// Whether the window asked for the check under way, and so wants to hear "up to date".
    asked: bool,
}

/// Where the updater stands.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum Phase {
    #[default]
    Idle,
    Checking,
    /// The feed was read and holds nothing newer than this build.
    UpToDate,
    /// A newer release, by the version a person is shown.
    Available(String),
    Downloading(String),
    /// The release is in place of this one and starts at the next launch.
    Ready(String),
    /// Why the last check or download failed, as a person is told it.
    Failed(String),
}

/// What the window asks of the platform.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ask {
    Check,
    Install,
    Restart,
}

/// What the platform found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Report {
    /// The feed was read: the newer release's version, or None.
    Checked(Option<String>),
    /// The release was downloaded, checked and put in place of this one.
    Installed(String),
    Failed(String),
}

impl Updater {
    /// Whether a check or a download is under way, so another cannot be asked for.
    pub fn busy(&self) -> bool {
        self.owed.is_some() || matches!(self.phase, Phase::Checking | Phase::Downloading(_))
    }

    /// What the sidebar's foot shows: a release to take, or one waiting for a restart.
    pub fn waiting(&self) -> Option<&Phase> {
        matches!(self.phase, Phase::Available(_) | Phase::Ready(_)).then_some(&self.phase)
    }
}

impl State {
    /// Preferences › Check for updates: a check the person asked for, whose answer is
    /// shown whatever it is.
    pub fn check_for_updates(&mut self) {
        if let Some(updater) = self.updater.as_mut().filter(|u| !u.busy()) {
            updater.owed = Some(Ask::Check);
            updater.asked = true;
            updater.phase = Phase::Checking;
        }
    }

    /// The daily check, when it is on and a day has passed since the last one (`now` in
    /// seconds since 1970). It says nothing unless it finds a release. A clock that went
    /// back counts as due.
    pub fn check_if_due(&mut self, now: i64) {
        if self.settings.update_checks != UpdateChecks::Daily {
            return;
        }
        let due = self
            .settings
            .last_update_check
            .is_none_or(|last| now - last >= DAY || now < last);
        let Some(updater) = self.updater.as_mut() else {
            return;
        };
        // A release found, or put in place, waits for the person; another check adds nothing.
        if !due || updater.busy() || updater.waiting().is_some() {
            return;
        }
        updater.owed = Some(Ask::Check);
        updater.asked = false;
        updater.phase = Phase::Checking;
        self.settings.last_update_check = Some(now);
        self.settings.save(&self.data_dir);
    }

    /// "Update": downloads the release found, checks it and puts it in place.
    pub fn install_update(&mut self) {
        if let Some(updater) = self.updater.as_mut()
            && let Phase::Available(version) = &updater.phase
            && updater.owed.is_none()
        {
            updater.phase = Phase::Downloading(version.clone());
            updater.owed = Some(Ask::Install);
        }
    }

    /// "Restart": quits and starts the release now in place.
    pub fn restart_to_update(&mut self) {
        if let Some(updater) = self.updater.as_mut()
            && matches!(updater.phase, Phase::Ready(_))
        {
            updater.owed = Some(Ask::Restart);
        }
    }

    /// Preferences › Check for updates automatically.
    pub fn set_update_checks(&mut self, checks: UpdateChecks) {
        if self.settings.update_checks != checks {
            self.settings.update_checks = checks;
            self.settings.save(&self.data_dir);
        }
    }

    /// What the platform found. A daily check that found nothing, or failed, leaves the
    /// window as it was: only a check the person asked for answers either way.
    pub fn react_update(&mut self, report: Report) {
        let Some(updater) = self.updater.as_mut() else {
            return;
        };
        let asked = std::mem::take(&mut updater.asked);
        updater.phase = match report {
            Report::Checked(Some(version)) => Phase::Available(version),
            Report::Checked(None) if asked => Phase::UpToDate,
            Report::Installed(version) => Phase::Ready(version),
            Report::Failed(why) if asked || matches!(updater.phase, Phase::Downloading(_)) => {
                Phase::Failed(why)
            }
            Report::Checked(None) | Report::Failed(_) => Phase::Idle,
        };
    }
}
