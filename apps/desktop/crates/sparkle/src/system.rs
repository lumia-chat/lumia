//! Two of the system's settings, made for this app alone: its appearance and when its
//! scroll bars show. Both are AppKit's and Foundation's own, public for as long as the
//! systems the app runs on have existed (`NSApplication.appearance` since macOS 10.14; the
//! shell's deployment target is 14).
//!
//! - **Appearance.** `NSApp.appearance` set to Aqua or Dark Aqua, or to nil to follow the
//!   system again. Every window's effective appearance follows, which is what the toolkit
//!   watches, and so do the things AppKit draws for the app: tooltips, sheets, menus.
//! - **Scroll bars.** `AppleShowScrollBars` in the app's own defaults, where a value takes
//!   precedence over the one System Settings writes for everyone (the same thing
//!   `defaults write <app> AppleShowScrollBars Always` does), or removed to follow the
//!   system again; then the notification AppKit posts when that setting changes, which
//!   is what makes the toolkit read it again.
//!
//! Every `unsafe` block is one message send or one call, on the main thread
//! (`MainThreadMarker`, taken at the top of each function), to a receiver the runtime or
//! Foundation just handed over.

use objc2::rc::Retained;
use objc2::runtime::{AnyClass, AnyObject};
use objc2::{MainThreadMarker, msg_send};
use objc2_foundation::{NSNotificationCenter, NSString, NSUserDefaults, ns_string};

use crate::{Appearance, ScrollBars};

pub(crate) fn set_appearance(appearance: Appearance) -> bool {
    if MainThreadMarker::new().is_none() {
        return false;
    }
    // Looked up by name: this crate links no AppKit of its own. A process without it (a
    // test, a tool) has no application to dress.
    let (Some(application), Some(appearances)) = (
        AnyClass::get(c"NSApplication"),
        AnyClass::get(c"NSAppearance"),
    ) else {
        return false;
    };
    let name: Option<&NSString> = match appearance {
        Appearance::System => None,
        // The values of AppKit's `NSAppearanceNameAqua` and `NSAppearanceNameDarkAqua`.
        Appearance::Light => Some(ns_string!("NSAppearanceNameAqua")),
        Appearance::Dark => Some(ns_string!("NSAppearanceNameDarkAqua")),
    };
    let wanted: Option<Retained<AnyObject>> = match name {
        None => None,
        Some(name) => {
            // SAFETY: `+[NSAppearance appearanceNamed:]` takes an `NSString` and returns an
            // autoreleased `NSAppearance` or nil for a name it does not know, which is what
            // `Option<Retained<_>>` reads. Main thread, checked above.
            let named: Option<Retained<AnyObject>> =
                unsafe { msg_send![appearances, appearanceNamed: name] };
            let Some(named) = named else {
                return false;
            };
            Some(named)
        }
    };
    // SAFETY: `+[NSApplication sharedApplication]` takes nothing and returns the
    // application object, which the toolkit made before any window. Main thread, checked
    // above.
    let app: Option<Retained<AnyObject>> = unsafe { msg_send![application, sharedApplication] };
    let Some(app) = app else {
        return false;
    };
    // SAFETY: `-[NSApplication setAppearance:]` takes a nullable `NSAppearance` (nil is
    // "as the system is") and returns nothing; `wanted` is an `NSAppearance` from the send
    // above, or nil. Main thread, checked above.
    let _: () = unsafe { msg_send![&*app, setAppearance: wanted.as_deref()] };
    true
}

pub(crate) fn set_scroll_bars(scroll_bars: ScrollBars) -> bool {
    if MainThreadMarker::new().is_none() {
        return false;
    }
    let defaults = NSUserDefaults::standardUserDefaults();
    let key = ns_string!("AppleShowScrollBars");
    // The values System Settings writes for its three choices.
    let value: Option<&NSString> = match scroll_bars {
        ScrollBars::System => None,
        ScrollBars::WhenScrolling => Some(ns_string!("WhenScrolling")),
        ScrollBars::Always => Some(ns_string!("Always")),
    };
    match value {
        None => defaults.removeObjectForKey(key),
        Some(value) => {
            // SAFETY: `-[NSUserDefaults setObject:forKey:]` takes a property-list object,
            // which an `NSString` is, and a key; it writes the app's own domain only.
            unsafe { defaults.setObject_forKey(Some(value), key) }
        }
    }
    let changed = ns_string!("NSPreferredScrollerStyleDidChangeNotification");
    // SAFETY: `-[NSNotificationCenter postNotificationName:object:]` takes a name and a
    // nullable sender; observers run here, on the main thread (checked above), which is
    // where AppKit itself posts this one.
    unsafe { NSNotificationCenter::defaultCenter().postNotificationName_object(changed, None) };
    true
}
