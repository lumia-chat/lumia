//! Preferences: how Lumia looks on this computer. Each choice takes effect as it is made and
//! is kept in `settings.json` for the next launch: light or dark (or as the system is), how
//! a conversation's messages are drawn, and when a list shows its scroll bar. They are this
//! computer's, not the account's: nothing here is sent to the instance.
//!
//! The boards' account menu has Preferences and no board draws the page, so it is a Home
//! page in the vocabulary of Profile and Devices.
//!
//! The Messages card starts from a preset (Comfortable, Compact, Minimal), which writes the
//! settings under it and is not kept itself: the segment lit is the preset whose values all
//! match, and when none does, none is lit and a line says so. Images and embeds are set by
//! the presets with no control of their own, since the shell draws neither yet; the avatars'
//! control is not drawn in one line, which has none.
//!
//! The scroll bar's card is drawn on macOS only: there the choice is carried out by the
//! system's own setting, made for this app alone (`platform::system`). Rune gives an app
//! no say in it anywhere else, and a control that does nothing is worse than none.

use rune::{El, col, text};

use super::devices::{page, page_column};
use crate::app::State;
use crate::settings::{Appearance, Avatars, FontSize, Layout, Preset, ScrollBars, Spacing};
use crate::tokens as t;
use crate::ui::components::{Look, card, segmented};

pub fn view(s: &State, look: Look) -> El {
    let mut content = page_column()
        .child(appearance(s, look))
        .child(messages(s, look));
    if scroll_bars_can_be_chosen() {
        content = content.child(scroll_bars(s, look));
    }
    page(
        "preferences",
        "Preferences",
        "How Lumia looks on this computer",
        content,
        look,
    )
}

/// Whether this platform lets the app say when its scroll bars show.
pub fn scroll_bars_can_be_chosen() -> bool {
    cfg!(target_os = "macos")
}

fn appearance(s: &State, look: Look) -> El {
    card(
        "Appearance",
        "Light, dark, or whichever this computer is in.",
        look,
    )
    .key("appearance-card")
    .w_full()
    .child(segmented(
        "appearance",
        "Appearance",
        &[
            (Appearance::Light, "Light"),
            (Appearance::Dark, "Dark"),
            (Appearance::System, "System"),
        ],
        s.settings.appearance,
        |s: &mut State, choice| s.set_appearance(choice),
        look,
    ))
}

/// A choice on the Messages card under its name, as a form's field has its label over it.
/// The control says its name to assistive technology itself.
fn labelled(label: &str, control: El, look: Look) -> El {
    col()
        .w_full()
        .gap(t::SPACE_6)
        .child(
            text(label)
                .font(t::TEXT_LABEL)
                .color(look.c.ink_body)
                .a11y_hidden(),
        )
        .child(control)
}

fn messages(s: &State, look: Look) -> El {
    let c = look.c;
    let matching = Preset::matching(s.settings.display());
    let mut preset = labelled(
        "Preset",
        segmented(
            "preset",
            "Preset",
            &[
                (Some(Preset::Comfortable), "Comfortable"),
                (Some(Preset::Compact), "Compact"),
                (Some(Preset::Minimal), "Minimal"),
            ],
            matching,
            |s: &mut State, choice| {
                if let Some(preset) = choice {
                    s.apply_preset(preset);
                }
            },
            look,
        ),
        look,
    );
    if matching.is_none() {
        preset = preset.child(
            text("Custom: these settings match no preset.")
                .key("preset-custom")
                .font(t::TEXT_META)
                .color(c.ink_muted)
                .role("status"),
        );
    }
    let mut card = card(
        "Messages",
        "Start from a preset, then change anything below.",
        look,
    )
    .key("messages-card")
    .w_full()
    .child(preset)
    .child(labelled(
        "Layout",
        segmented(
            "layout",
            "Layout",
            &[(Layout::Grouped, "Grouped"), (Layout::OneLine, "One line")],
            s.settings.layout,
            |s: &mut State, choice| s.set_layout(choice),
            look,
        ),
        look,
    ))
    .child(labelled(
        "Spacing",
        segmented(
            "spacing",
            "Spacing",
            &[
                (Spacing::Roomy, "Roomy"),
                (Spacing::Cozy, "Cozy"),
                (Spacing::Tight, "Tight"),
            ],
            s.settings.spacing,
            |s: &mut State, choice| s.set_spacing(choice),
            look,
        ),
        look,
    ));
    if s.settings.layout == Layout::Grouped {
        card = card.child(labelled(
            "Avatars",
            segmented(
                "avatars",
                "Avatars",
                &[
                    (Avatars::Large, "Large"),
                    (Avatars::Small, "Small"),
                    (Avatars::Hidden, "Hidden"),
                ],
                s.settings.avatars,
                |s: &mut State, choice| s.set_avatars(choice),
                look,
            ),
            look,
        ));
    }
    let sizes: Vec<(FontSize, String)> = FontSize::ALL
        .into_iter()
        .map(|size| (size, size.points().to_string()))
        .collect();
    let sizes: Vec<(FontSize, &str)> = sizes
        .iter()
        .map(|(size, name)| (*size, name.as_str()))
        .collect();
    card.child(labelled(
        "Font size",
        segmented(
            "font-size",
            "Font size",
            &sizes,
            s.settings.font_size,
            |s: &mut State, choice| s.set_font_size(choice),
            look,
        ),
        look,
    ))
}

fn scroll_bars(s: &State, look: Look) -> El {
    card(
        "Scroll bars",
        "When a list shows its scroll bar. Automatic follows this Mac's own setting.",
        look,
    )
    .key("scroll-bars-card")
    .w_full()
    .child(segmented(
        "scroll-bars",
        "Show scroll bars",
        &[
            (ScrollBars::Automatic, "Automatic"),
            (ScrollBars::WhenScrolling, "When scrolling"),
            (ScrollBars::Always, "Always"),
        ],
        s.settings.scroll_bars,
        |s: &mut State, choice| s.set_scroll_bars(choice),
        look,
    ))
}
