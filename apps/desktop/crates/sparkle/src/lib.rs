//! The shell's bridge to [Sparkle](https://sparkle-project.org), which updates the macOS app
//! from the release feed named in its `Info.plist` (architecture section 13, "Distribution").
//!
//! **This is the one crate of the shell's workspace where `unsafe` is allowed** (the shell's
//! manifest denies it; this one's lints refuse a block without its reason). Sparkle is
//! Objective-C, so talking to it is message sends, and it is **loaded at run time** from the
//! bundle's `Contents/Frameworks` instead of being linked: the binary has no load command
//! for it. A bare `cargo run` binary, the tests, a headless run and the Linux and Windows
//! builds therefore start with no updater and no dependency on the framework, and
//! [`Updater::start`] says which of those it was.
//!
//! The surface is what the shell's menu needs and nothing more: start the updater, ask for
//! a check the person started (Sparkle's own windows show what it finds), whether a check
//! can be made now, and "check automatically" read and set. What is checked, how often and
//! against which key are the bundle's `Info.plist` keys (`scripts/bundle-macos.sh`), read by
//! Sparkle itself; nothing here can point it at another feed or key.
//!
//! Sparkle's objects belong to the main thread. An [`Updater`] can only be made there
//! (`MainThreadMarker`) and is neither `Send` nor `Sync`, so every call on one is made
//! there too:
//!
//! ```compile_fail,E0277
//! fn crosses_threads<T: Send>() {}
//! crosses_threads::<lumia_sparkle::Updater>();
//! ```
//!
//! ```compile_fail,E0277
//! fn shared_between_threads<T: Sync>() {}
//! shared_between_threads::<lumia_sparkle::Updater>();
//! ```
//!
//! Off macOS the same functions exist and [`Updater::start`] always answers
//! [`Unavailable::Unsupported`].

use std::fmt;
use std::marker::PhantomData;

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
use macos as platform;

#[cfg(not(target_os = "macos"))]
mod none;
#[cfg(not(target_os = "macos"))]
use none as platform;

/// On a Mac the other platforms' side is compiled into the tests as well, so a change to
/// the surface that forgets it fails here and not first on Linux.
#[cfg(all(test, target_os = "macos"))]
#[allow(dead_code)]
mod none;

/// Why there is no updater. The first three are how most launches go and are nothing to
/// report; the rest mean the bundle was put together wrong ([`Unavailable::is_fault`]).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Unavailable {
    /// Not macOS: the other platforms' updaters are not Sparkle.
    Unsupported,
    /// Asked for off the main thread, where Sparkle's objects may not be made.
    OffMainThread,
    /// The program is not a bundle that names an update feed (`SUFeedURL`): a bare
    /// `cargo run` binary, a test, or a bundle built with no feed.
    NoFeed,
    /// The bundle names a feed and carries no `Sparkle.framework`.
    NoFramework,
    /// The framework is there and would not load (its architecture, its signature).
    Load(String),
    /// The framework loaded and is not the Sparkle this was written against: it lacks the
    /// named class or method.
    Mismatch(&'static str),
    /// Sparkle refused to start, which is its word on the bundle's update keys (no valid
    /// public key, a feed that is not HTTPS without one, signed feeds without verification
    /// before extraction).
    Start(String),
}

impl Unavailable {
    /// Whether this is a bundle that should have had an updater and does not.
    pub fn is_fault(&self) -> bool {
        matches!(
            self,
            Self::NoFramework | Self::Load(_) | Self::Mismatch(_) | Self::Start(_)
        )
    }
}

impl fmt::Display for Unavailable {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unsupported => f.write_str("no updater on this platform"),
            Self::OffMainThread => f.write_str("the updater starts on the main thread only"),
            Self::NoFeed => f.write_str("not a bundle with an update feed"),
            Self::NoFramework => f.write_str("the bundle has no Sparkle.framework"),
            Self::Load(why) => write!(f, "Sparkle.framework did not load: {why}"),
            Self::Mismatch(what) => write!(f, "Sparkle.framework has no {what}"),
            Self::Start(why) => write!(f, "Sparkle did not start: {why}"),
        }
    }
}

impl std::error::Error for Unavailable {}

/// The running updater: Sparkle's standard one, with its own windows, over the app's main
/// bundle. Made once, on the main thread, and kept for the life of the process: Sparkle's
/// schedule lives in it.
pub struct Updater {
    inner: platform::Updater,
    /// Neither `Send` nor `Sync` on any platform, so code that holds one compiles the same
    /// way everywhere.
    _main_thread: PhantomData<*const ()>,
}

impl Updater {
    /// Loads Sparkle from the bundle and starts it: from here it checks the feed on its
    /// schedule (once a day, and at launch when a day has passed) unless the person turned
    /// that off. Call on the main thread, once the application object exists.
    pub fn start() -> Result<Self, Unavailable> {
        Ok(Self {
            inner: platform::Updater::start()?,
            _main_thread: PhantomData,
        })
    }

    /// A check the person asked for. Sparkle shows what it finds in its own windows: the
    /// update and its notes, "you're up to date", or the error.
    pub fn check_for_updates(&self) {
        self.inner.check_for_updates();
    }

    /// Whether a check can be asked for now. False while one is under way.
    pub fn can_check_for_updates(&self) -> bool {
        self.inner.can_check_for_updates()
    }

    /// Whether Sparkle checks the feed on its own. The person's choice once they have made
    /// one, else the bundle's `SUEnableAutomaticChecks`.
    pub fn checks_automatically(&self) -> bool {
        self.inner.checks_automatically()
    }

    /// Turns the scheduled check on or off; Sparkle keeps the choice in the app's
    /// preferences.
    pub fn set_checks_automatically(&self, on: bool) {
        self.inner.set_checks_automatically(on);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A test binary is no bundle and its tests run off the main thread: there is never an
    /// updater here, and asking for one is harmless.
    #[test]
    fn a_test_has_no_updater_and_says_why() {
        let Err(why) = Updater::start() else {
            panic!("an updater started inside a test");
        };
        assert!(!why.is_fault(), "{why}");
        if cfg!(target_os = "macos") {
            assert!(
                matches!(why, Unavailable::OffMainThread | Unavailable::NoFeed),
                "{why}"
            );
        } else {
            assert_eq!(why, Unavailable::Unsupported);
        }
    }

    /// What Windows and Linux compile, run here too: there is nothing to start.
    #[cfg(target_os = "macos")]
    #[test]
    fn the_other_platforms_never_have_one() {
        assert!(matches!(
            crate::none::Updater::start(),
            Err(Unavailable::Unsupported)
        ));
    }

    #[test]
    fn only_a_bundle_put_together_wrong_is_a_fault() {
        for quiet in [
            Unavailable::Unsupported,
            Unavailable::OffMainThread,
            Unavailable::NoFeed,
        ] {
            assert!(!quiet.is_fault(), "{quiet}");
        }
        for fault in [
            Unavailable::NoFramework,
            Unavailable::Load("wrong architecture".into()),
            Unavailable::Mismatch("SPUUpdater"),
            Unavailable::Start("no public key".into()),
        ] {
            assert!(fault.is_fault(), "{fault}");
        }
    }
}
