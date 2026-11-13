//! Everywhere but macOS: no Sparkle, so no updater can exist. The type has no values, and
//! the functions on one are there only so the shell compiles the same code on every
//! platform.

use std::convert::Infallible;

use crate::{Appearance, ScrollBars, Unavailable};

/// The system's settings are the Mac's; elsewhere there is nothing to set.
#[cfg_attr(target_os = "macos", allow(dead_code))]
pub(crate) fn set_appearance(_appearance: Appearance) -> bool {
    false
}

#[cfg_attr(target_os = "macos", allow(dead_code))]
pub(crate) fn set_scroll_bars(_scroll_bars: ScrollBars) -> bool {
    false
}

#[cfg_attr(target_os = "macos", allow(dead_code))]
pub(crate) fn set_badge(_label: Option<&str>) -> bool {
    false
}

#[cfg_attr(target_os = "macos", allow(dead_code))]
pub(crate) fn clock_24_hour() -> Option<bool> {
    None
}

pub(crate) struct Updater(Infallible);

impl Updater {
    pub(crate) fn start() -> Result<Self, Unavailable> {
        Err(Unavailable::Unsupported)
    }

    pub(crate) fn check_for_updates(&self) {
        match self.0 {}
    }

    pub(crate) fn can_check_for_updates(&self) -> bool {
        match self.0 {}
    }

    pub(crate) fn checks_automatically(&self) -> bool {
        match self.0 {}
    }

    pub(crate) fn set_checks_automatically(&self, _on: bool) {
        match self.0 {}
    }
}
