//! Lumia wire types (`lumia.v1`), generated from `proto/` at build time.
//!
//! Nothing in this crate is hand-written beyond this file. The schemas are
//! the source of truth (docs/protocol.md section 1).

#![allow(clippy::all, missing_debug_implementations)]

pub mod lumia {
    pub mod v1 {
        include!(concat!(env!("OUT_DIR"), "/lumia.v1.rs"));
    }
}

pub use lumia::v1;
pub use prost;
pub use prost_types;
