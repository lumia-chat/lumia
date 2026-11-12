//! The Lumia desktop shell (architecture section 13): the client core behind a Rune interface.
//!
//! `session` is the client and the state the screens read, with no toolkit types in it;
//! `app` is the window's state over it; `ui` is the views; `tokens` is the design system as
//! Rune values.

pub mod app;
pub mod session;
pub mod settings;
pub mod tokens;
pub mod ui;
