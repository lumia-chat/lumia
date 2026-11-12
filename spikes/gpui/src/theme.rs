//! Lumia's design tokens (`assets/design/tokens.json`) as GPUI values: colours read from the
//! file at start, type and sizes written out from its tables.

use std::borrow::Cow;
use std::sync::OnceLock;

use gpui_kit::component::Theme;
use gpui_kit::{App, Hsla, Rgba, rgb};

const TOKENS: &str = include_str!("../../../assets/design/tokens.json");

/// Albert Sans and Bricolage Grotesque, unpacked from `assets/fonts` by `spikes/data/scripts/fonts.sh`.
const FONTS: &[&[u8]] = &[
    include_bytes!("../../data/fonts/AlbertSans-Regular.ttf"),
    include_bytes!("../../data/fonts/AlbertSans-Medium.ttf"),
    include_bytes!("../../data/fonts/AlbertSans-SemiBold.ttf"),
    include_bytes!("../../data/fonts/AlbertSans-Bold.ttf"),
    include_bytes!("../../data/fonts/AlbertSans-ExtraBold.ttf"),
    include_bytes!("../../data/fonts/BricolageGrotesque-SemiBold.ttf"),
    include_bytes!("../../data/fonts/BricolageGrotesque-Bold.ttf"),
    include_bytes!("../../data/fonts/BricolageGrotesque-ExtraBold.ttf"),
];

pub const SANS: &str = "Albert Sans";
pub const DISPLAY: &str = "Bricolage Grotesque";

pub mod size {
    pub const RAIL: f32 = 64.0;
    pub const TILE: f32 = 44.0;
    pub const SIDEBAR: f32 = 248.0;
    pub const ROW: f32 = 32.0;
    pub const HEADER: f32 = 64.0;
    pub const AVATAR: f32 = 40.0;
}

#[derive(Clone, Copy, Debug)]
pub struct Palette {
    pub bg: Hsla,
    pub rail: Hsla,
    pub surface: Hsla,
    pub tile: Hsla,
    pub raised: Hsla,
    pub field: Hsla,
    pub inset: Hsla,
    pub line: Hsla,
    pub panel_edge: Hsla,
    pub border_composer: Hsla,
    pub hover_row: Hsla,
    pub hover_message: Hsla,
    pub ink: Hsla,
    pub ink_selected: Hsla,
    pub ink_field: Hsla,
    pub ink_body: Hsla,
    pub ink_secondary: Hsla,
    pub ink_muted: Hsla,
    pub on_accent: Hsla,
    pub accent: Hsla,
    pub link: Hsla,
    /// Avatar grounds and their inks.
    pub tones: [(Hsla, Hsla); 6],
}

fn read(tokens: &serde_json::Value, theme: &str, name: &str) -> Hsla {
    let value = tokens["color"]["tokens"]
        .as_array()
        .and_then(|list| list.iter().find(|t| t["name"] == name))
        .map(|t| &t["value"]);
    let hex = match value {
        Some(serde_json::Value::String(s)) => Some(s.as_str()),
        Some(by_theme) => by_theme[theme].as_str(),
        None => None,
    };
    // A token the file lacks shows up magenta rather than failing the launch.
    let rgba: Rgba = hex
        .and_then(|h| h.strip_prefix('#'))
        .filter(|h| h.len() == 6)
        .and_then(|h| u32::from_str_radix(h, 16).ok())
        .map_or(rgb(0xff00ff), rgb);
    rgba.into()
}

fn load(theme: &str) -> Palette {
    let tokens: serde_json::Value = serde_json::from_str(TOKENS).unwrap_or_default();
    let c = |name: &str| read(&tokens, theme, name);
    let tone = |name: &str| (c(&format!("tone-{name}")), c(&format!("tone-{name}-ink")));
    Palette {
        bg: c("bg"),
        rail: c("rail"),
        surface: c("surface"),
        tile: c("tile"),
        raised: c("raised"),
        field: c("field"),
        inset: c("inset"),
        line: c("line"),
        panel_edge: c("panel-edge"),
        border_composer: c("border-composer"),
        hover_row: c("hover-row"),
        hover_message: c("hover-message"),
        ink: c("ink"),
        ink_selected: c("ink-selected"),
        ink_field: c("ink-field"),
        ink_body: c("ink-body"),
        ink_secondary: c("ink-secondary"),
        ink_muted: c("ink-muted"),
        on_accent: c("on-accent"),
        accent: c("accent"),
        link: c("link"),
        tones: [tone("peach"), tone("blue"), tone("purple"), tone("mint"), tone("amber"), tone("sky")],
    }
}

pub fn palette(dark: bool) -> &'static Palette {
    static PALETTES: OnceLock<[Palette; 2]> = OnceLock::new();
    &PALETTES.get_or_init(|| [load("light"), load("dark")])[dark as usize]
}

/// Registers the brand fonts and points the kit's theme at them and at the tokens the kit's
/// own components read (the composer's field, message text, links, selection).
pub fn install(dark: bool, cx: &mut App) {
    let fonts = FONTS.iter().map(|bytes| Cow::Borrowed(*bytes)).collect();
    if let Err(error) = cx.text_system().add_fonts(fonts) {
        eprintln!("lumia-spike: loading fonts: {error}");
    }
    let p = palette(dark);
    let theme = Theme::global_mut(cx);
    theme.font_family = SANS.into();
    theme.colors.background = p.surface;
    theme.colors.foreground = p.ink_body;
    theme.colors.muted_foreground = p.ink_muted;
    theme.colors.border = p.line;
    theme.colors.input = p.border_composer;
    theme.colors.link = p.link;
    theme.colors.caret = p.accent;
    theme.colors.selection = p.accent.opacity(0.28);
}
