//! Lumia's design tokens (`assets/design/tokens.json`) as Rune values: colours read from the
//! file at start, type and sizes written out from its tables.

use std::sync::OnceLock;

use rune::{Color, Family, TextStyle};
use rune_kit::MarkdownStyle;

const TOKENS: &str = include_str!("../../../assets/design/tokens.json");

/// Albert Sans and Bricolage Grotesque, unpacked from `assets/fonts` by `spikes/data/scripts/fonts.sh`.
pub const FONTS: &[&[u8]] = &[
    include_bytes!("../../data/fonts/AlbertSans-Regular.ttf"),
    include_bytes!("../../data/fonts/AlbertSans-Medium.ttf"),
    include_bytes!("../../data/fonts/AlbertSans-SemiBold.ttf"),
    include_bytes!("../../data/fonts/AlbertSans-Bold.ttf"),
    include_bytes!("../../data/fonts/AlbertSans-ExtraBold.ttf"),
    include_bytes!("../../data/fonts/BricolageGrotesque-SemiBold.ttf"),
    include_bytes!("../../data/fonts/BricolageGrotesque-Bold.ttf"),
    include_bytes!("../../data/fonts/BricolageGrotesque-ExtraBold.ttf"),
];
/// The sans and mono slots; `ui-monospace` is the platform's own (SF Mono on macOS).
pub const FAMILIES: (&str, &str) = ("Albert Sans", "ui-monospace");

const DISPLAY: Family = Family::Named("Bricolage Grotesque");

pub mod type_ {
    use super::*;

    pub const TITLE_CHANNEL: TextStyle = TextStyle::new(21.0, 700.0).family(DISPLAY).tracking(-0.21);
    pub const TITLE_SERVER: TextStyle = TextStyle::new(19.0, 700.0).family(DISPLAY).tracking(-0.19);
    pub const MONOGRAM: TextStyle = TextStyle::new(18.0, 800.0).family(DISPLAY);
    pub const MESSAGE: TextStyle = TextStyle::new(15.0, 400.0).lh(24.0);
    pub const AUTHOR: TextStyle = TextStyle::new(15.0, 700.0);
    pub const BODY: TextStyle = TextStyle::new(15.0, 400.0).lh(22.0);
    pub const NAV: TextStyle = TextStyle::new(14.5, 400.0);
    pub const NAV_ACTIVE: TextStyle = TextStyle::new(14.5, 700.0);
    pub const COMPACT: TextStyle = TextStyle::new(14.0, 400.0);
    pub const META: TextStyle = TextStyle::new(12.5, 400.0);
    pub const SECTION: TextStyle = TextStyle::new(12.5, 600.0);
}

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
    pub bg: Color,
    pub rail: Color,
    pub surface: Color,
    pub tile: Color,
    pub raised: Color,
    pub field: Color,
    pub inset: Color,
    pub line: Color,
    pub panel_edge: Color,
    pub border_composer: Color,
    pub hover_row: Color,
    pub hover_message: Color,
    pub ink: Color,
    pub ink_selected: Color,
    pub ink_field: Color,
    pub ink_body: Color,
    pub ink_secondary: Color,
    pub ink_muted: Color,
    pub on_accent: Color,
    pub accent: Color,
    pub accent_soft: Color,
    pub link: Color,
    /// Avatar grounds and their inks.
    pub tones: [(Color, Color); 6],
}

fn read(tokens: &serde_json::Value, theme: &str, name: &str) -> Color {
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
    hex.and_then(rune::color::hex).unwrap_or(Color { r: 1.0, g: 0.0, b: 1.0, a: 1.0 })
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
        accent_soft: c("accent-soft"),
        link: c("link"),
        tones: [tone("peach"), tone("blue"), tone("purple"), tone("mint"), tone("amber"), tone("sky")],
    }
}

pub fn palette(dark: bool) -> &'static Palette {
    static PALETTES: OnceLock<[Palette; 2]> = OnceLock::new();
    &PALETTES.get_or_init(|| [load("light"), load("dark")])[dark as usize]
}

/// Message bodies: the protocol's Markdown subset in the message type style.
pub fn markdown(dark: bool) -> MarkdownStyle {
    let p = palette(dark);
    MarkdownStyle::neutral(dark)
        .text(p.ink_body)
        .muted(p.ink_secondary)
        .link(p.link)
        .inline_code_bg(p.inset)
        .code_block(p.inset, p.line)
        .quote_rule(p.line)
        .selection(p.accent.alpha(0.28))
        .body_font(type_::MESSAGE)
}
