//! Spike: Lumia's desktop shell on GPUI and GPUI Kit, over the real client core.

pub use lumia_spike_data as data;
pub mod shell;
pub mod theme;

use gpui_kit::{App, Window, WindowAppearance};

/// Dark or light: `LUMIA_SPIKE_THEME`, else the window's appearance.
pub fn dark(window: &Window) -> bool {
    match std::env::var("LUMIA_SPIKE_THEME").as_deref() {
        Ok("dark") => true,
        Ok("light") => false,
        _ => matches!(window.appearance(), WindowAppearance::Dark | WindowAppearance::VibrantDark),
    }
}

/// The kit, then the tokens over its theme.
pub fn init(dark: bool, cx: &mut App) {
    use gpui_kit::component::{Theme, ThemeMode};
    Theme::change(if dark { ThemeMode::Dark } else { ThemeMode::Light }, None, cx);
    theme::install(dark, cx);
}
