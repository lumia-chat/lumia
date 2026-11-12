//! Sparkle on macOS: the framework loaded from the bundle, Sparkle's standard updater
//! (`SPUStandardUpdaterController`, with its own windows) made over the app's main bundle,
//! and the four messages the menu needs.
//!
//! Every `unsafe` block here is one message send (or the load), and rests on the same three
//! facts, which its comment names:
//!
//! - **The thread.** An [`Updater`] is made with a `MainThreadMarker` in hand and holds it,
//!   so it is neither `Send` nor `Sync`: a `&Updater` exists on the main thread only, which
//!   is where Sparkle's classes must be used.
//! - **The receiver.** The controller is an instance of the class looked up by name after
//!   the load, the updater is the object that controller hands over, and both are kept
//!   alive by the `Retained`s in the struct.
//! - **The signature.** [`Updater::start`] refuses a framework in which the class of either
//!   receiver lacks a selector used here, and the types written at each send are the ones
//!   in Sparkle 2's public headers (`SPUStandardUpdaterController.h`, `SPUUpdater.h`), of
//!   the release `scripts/bundle-macos.sh` pins by version and SHA-256. A debug build also
//!   has `objc2` check each send's types against the method the runtime found.

use objc2::rc::{Allocated, Retained};
use objc2::runtime::{AnyClass, AnyObject, Sel};
use objc2::{MainThreadMarker, msg_send, sel};
use objc2_foundation::{NSBundle, NSError, NSString, ns_string};

use crate::Unavailable;

pub(crate) struct Updater {
    /// The `SPUStandardUpdaterController`: it owns the updater and Sparkle's standard
    /// windows, and is kept so they live as long as this does.
    _controller: Retained<AnyObject>,
    /// Its `SPUUpdater`.
    updater: Retained<AnyObject>,
    /// What makes this main-thread-only in the type system; see the module comment.
    _main: MainThreadMarker,
}

impl Updater {
    pub(crate) fn start() -> Result<Self, Unavailable> {
        let main = MainThreadMarker::new().ok_or(Unavailable::OffMainThread)?;
        let bundle = NSBundle::mainBundle();
        // Before anything is loaded: only a bundle that names a feed has an updater. A bare
        // binary's "main bundle" is the directory it sits in, with no Info.plist.
        if bundle
            .objectForInfoDictionaryKey(ns_string!("SUFeedURL"))
            .is_none()
        {
            return Err(Unavailable::NoFeed);
        }
        let frameworks = bundle
            .privateFrameworksPath()
            .ok_or(Unavailable::NoFramework)?;
        let path = NSString::from_str(&format!("{frameworks}/Sparkle.framework"));
        let framework = NSBundle::bundleWithPath(&path).ok_or(Unavailable::NoFramework)?;
        // SAFETY: loading a framework runs its initializers in this process, so this trusts
        // the code at that path: it is inside the app's own bundle, put there by
        // `scripts/bundle-macos.sh` from a release checked against a pinned SHA-256 and
        // sealed by the bundle's signature (with a Developer ID, the hardened runtime
        // refuses a framework signed by anyone else). Nothing of Sparkle is in use yet, and
        // a failure is returned as an error, not left half-done.
        let loaded: Result<(), Retained<NSError>> = unsafe { framework.loadAndReturnError() };
        loaded.map_err(|error| Unavailable::Load(error.localizedDescription().to_string()))?;

        let controller_class = AnyClass::get(c"SPUStandardUpdaterController")
            .ok_or(Unavailable::Mismatch("SPUStandardUpdaterController"))?;
        answers(
            controller_class,
            &[
                (
                    sel!(initWithStartingUpdater:updaterDelegate:userDriverDelegate:),
                    "SPUStandardUpdaterController's initializer",
                ),
                (sel!(updater), "SPUStandardUpdaterController.updater"),
            ],
        )?;
        // SAFETY: `+alloc` on a class the runtime just handed over (an `NSObject` subclass,
        // as every Sparkle class is), on the main thread (`main`). It returns an allocated,
        // uninitialized instance, which is what `Allocated` is.
        let allocated: Allocated<AnyObject> = unsafe { msg_send![controller_class, alloc] };
        // SAFETY: the initializer exists on this class (checked above) and takes a `BOOL`
        // and two nullable delegates, which Sparkle holds weakly and allows to be nil. It
        // consumes the allocation and returns the instance, or nil. Not starting the
        // updater here keeps its failure an error to return (`startUpdater:` below) instead
        // of the alert the controller would put on screen. Main thread: `main`.
        let controller: Option<Retained<AnyObject>> = unsafe {
            msg_send![
                allocated,
                initWithStartingUpdater: false,
                updaterDelegate: None::<&AnyObject>,
                userDriverDelegate: None::<&AnyObject>,
            ]
        };
        let controller = controller.ok_or(Unavailable::Mismatch(
            "SPUStandardUpdaterController instance",
        ))?;
        // SAFETY: `updater` exists on the controller's class (checked above), takes nothing
        // and returns the controller's `SPUUpdater`, a property declared nonnull that is
        // read as nullable here all the same. Main thread: `main`.
        let updater: Option<Retained<AnyObject>> = unsafe { msg_send![&*controller, updater] };
        let updater = updater.ok_or(Unavailable::Mismatch("SPUUpdater instance"))?;
        // Every message this crate will ever send the updater, asked of the class of the
        // object in hand, before the first of them is sent.
        answers(
            updater.class(),
            &[
                (sel!(startUpdater:), "SPUUpdater.startUpdater"),
                (sel!(checkForUpdates), "SPUUpdater.checkForUpdates"),
                (sel!(canCheckForUpdates), "SPUUpdater.canCheckForUpdates"),
                (
                    sel!(automaticallyChecksForUpdates),
                    "SPUUpdater.automaticallyChecksForUpdates",
                ),
                (
                    sel!(setAutomaticallyChecksForUpdates:),
                    "SPUUpdater.setAutomaticallyChecksForUpdates",
                ),
            ],
        )?;
        // SAFETY: `startUpdater:` exists on the updater's class (checked above). It takes an
        // `NSError **` and returns `BOOL`, the convention `_` stands for: on NO the error it
        // wrote is read back, retained. Main thread: `main`.
        let started: Result<(), Retained<NSError>> =
            unsafe { msg_send![&*updater, startUpdater: _] };
        started.map_err(|error| Unavailable::Start(error.localizedDescription().to_string()))?;

        Ok(Self {
            _controller: controller,
            updater,
            _main: main,
        })
    }

    pub(crate) fn check_for_updates(&self) {
        // SAFETY: `updater` is the started `SPUUpdater`, whose class has `checkForUpdates`
        // (checked in `start`): no arguments, no result. Sparkle allows it at any time and
        // brings its window forward when a check is already under way. Main thread: `self`
        // cannot be anywhere else.
        let _: () = unsafe { msg_send![&*self.updater, checkForUpdates] };
    }

    pub(crate) fn can_check_for_updates(&self) -> bool {
        // SAFETY: `canCheckForUpdates` exists on the updater's class (checked in `start`):
        // a getter with no arguments returning `BOOL`. Main thread: `self` cannot be
        // anywhere else.
        unsafe { msg_send![&*self.updater, canCheckForUpdates] }
    }

    pub(crate) fn checks_automatically(&self) -> bool {
        // SAFETY: `automaticallyChecksForUpdates` exists on the updater's class (checked in
        // `start`): a getter with no arguments returning `BOOL`. Main thread: `self` cannot
        // be anywhere else.
        unsafe { msg_send![&*self.updater, automaticallyChecksForUpdates] }
    }

    pub(crate) fn set_checks_automatically(&self, on: bool) {
        // SAFETY: `setAutomaticallyChecksForUpdates:` exists on the updater's class (checked
        // in `start`): one `BOOL`, no result. Sparkle writes the choice to the app's
        // preferences and reschedules itself. Main thread: `self` cannot be anywhere else.
        let _: () = unsafe { msg_send![&*self.updater, setAutomaticallyChecksForUpdates: on] };
    }
}

/// Whether instances of `class` answer every selector in `needs`; the first one they do not
/// is the error. Asking a class what it responds to is safe at any time, and it is what
/// lets the sends above name a method the binary never linked.
fn answers(class: &AnyClass, needs: &[(Sel, &'static str)]) -> Result<(), Unavailable> {
    match needs
        .iter()
        .find(|(selector, _)| !class.responds_to(*selector))
    {
        Some((_, what)) => Err(Unavailable::Mismatch(what)),
        None => Ok(()),
    }
}
