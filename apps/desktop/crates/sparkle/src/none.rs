//! Everywhere but macOS: no Sparkle, so no updater can exist. The type has no values, and
//! the functions on one are there only so the shell compiles the same code on every
//! platform.

use std::convert::Infallible;

use crate::Unavailable;

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
