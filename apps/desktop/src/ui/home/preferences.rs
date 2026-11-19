//! Preferences: how Lumia looks on this computer. Each choice takes effect as it is made and
//! is kept in `settings.json` for the next launch: light or dark (or as the system is), how
//! a conversation's messages are drawn, and when a list shows its scroll bar. They are this
//! computer's, not the account's: nothing here is sent to the instance.
//!
//! The boards' account menu has Preferences, and the Preferences boards draw the page: a
//! Home page in the vocabulary of Profile and Devices, its sections a heading over their
//! choices. The choices of one or a few are pictures (the themes, as small windows; the
//! presets, as sketches of their rows); the rest are rows of an outlined group, a name and
//! a line on what it does beside a control as wide as its words, and a switch for each
//! choice that is on or off.
//!
//! Appearance is Light, Dark or System, then the scroll bars' row on macOS only: there the
//! choice is carried out by the system's own setting, made for this app alone
//! (`platform::system`). Rune gives an app no say in it anywhere else, and a control that does
//! nothing is worse than none.
//!
//! Messages starts from a preset (Comfortable, Compact, Minimal), which writes the settings
//! under it and is not kept itself: the picture lit is the preset whose values all match, and
//! when none does, none is lit and a line says so. Then Layout, Spacing, Avatars (not in one
//! line, which has none) and the font size, and the switches: Images (shown in their
//! messages, or off: every attachment a file row, a picture's opening the viewer) and Embeds
//! (links' cards drawn, or off: none), which Minimal turns off (specs/media.md 5), and
//! Reactions (Minimal hides them).
//!
//! The preview is a channel's messages drawn by the timeline's own rows
//! (`timeline::samples`) under the settings as they are, so each choice shows as it is made:
//! beside the settings and held in view while they scroll where the pane is wide enough for
//! both, under the message settings otherwise. Nothing in it presses, and assistive
//! technology reads it as one picture.
//!
//! The Updates section is drawn where the shell carries its own updater (`app::updates`: the
//! Linux AppImage). On macOS the same choices are Sparkle's, in the application menu; a
//! package from a repository is updated by the package manager, and the section is not drawn.

use chrono::{Local, NaiveTime};
use rune::{El, ScrollSpec, col, el, icon, row, text};

use crate::app::State;
use crate::app::updates::{Phase, Updater};
use crate::session::media::size_words;
use crate::settings::{
    Appearance, Avatars, FontSize, Layout, Media, Preset, Reactions, ScrollBars, Spacing,
    UpdateChecks,
};
use crate::tokens::{self as t, Colors, icons};
use crate::ui::components::{
    Look, action_button, check_row, error_text, panel, picture_choice, secondary_button,
    segmented_compact, setting_row, settings_group, switch,
};
use crate::ui::scroll::WithScrollBar;
use crate::ui::{conversation, media, replies, timeline};

/// The key of the page's scroll viewport, whose width says whether the preview fits beside
/// the settings.
const PAGE: &str = "preferences";

/// How wide the page must be for the preview to stand beside the settings: both columns at
/// their narrowest, the room between them and the page's sides.
const SIDE_BY_SIDE_FROM: f32 =
    2.0 * t::SPACE_24 + t::PREFS_COLUMN_MIN + t::PREFS_GAP + t::PREFS_PREVIEW;

/// The preview's share of the room the two columns grow into, beside the settings' 1: next to
/// nothing, so the settings reach their widest before the preview grows past its narrowest.
const PREVIEW_SHARE: f32 = 0.001;

pub fn view(s: &State, look: Look) -> El {
    // Read as laid out, so the page is drawn again when the window is resized.
    let wide = rune::memo::layout_with(PAGE, |rect| rect[2].round())
        .is_some_and(|width| width >= SIDE_BY_SIDE_FROM);
    let mut settings = col()
        .w_full()
        .max_w(t::PREFS_COLUMN)
        .gap(t::PREFS_GAP)
        .child(appearance(s, look))
        .child(messages(s, !wide, look));
    if let Some(updater) = &s.updater {
        settings = settings.child(updates(s, updater, look));
    }
    let mut columns = row()
        .w_full()
        .px(t::SPACE_24)
        .pt(t::SPACE_24)
        .pb(t::SPACE_40)
        .gap(t::PREFS_GAP)
        .items_stretch()
        .child(settings.grow(1.0).min_w(0.0));
    if wide {
        // The column is as tall as the settings, so the preview stays in view down all of it.
        columns = columns.child(
            // The settings take the width first, up to theirs; the preview what is left.
            col()
                .grow(PREVIEW_SHARE)
                .min_w(t::PREFS_PREVIEW)
                .max_w(t::PREFS_PREVIEW_MAX)
                .self_stretch()
                .child(
                    preview(s, look)
                        .fill(look.c.surface)
                        .sticky_top(t::SPACE_16),
                ),
        );
    }
    let header = conversation::header_with(
        "Preferences",
        None,
        Some("How Lumia looks on this computer"),
        look,
    );
    panel("panel", look).w_full().child(header).child(
        col()
            .key(PAGE)
            .w_full()
            .grow(1.0)
            .min_h(0.0)
            .scroll(ScrollSpec {
                stick_bottom: false,
            })
            .child(columns)
            .with_scroll_bar(PAGE, look),
    )
}

/// Whether this platform lets the app say when its scroll bars show.
pub fn scroll_bars_can_be_chosen() -> bool {
    cfg!(target_os = "macos")
}

/// A section of the page: its heading and a line on what it holds, over its choices.
fn section(key: &'static str, title: &str, caption: &str, look: Look) -> El {
    let c = look.c;
    col().key(key).w_full().gap(t::SPACE_16).child(
        col()
            .w_full()
            .gap(t::SPACE_4)
            .child(
                text(title)
                    .font(t::TITLE_SECTION)
                    .color(c.ink)
                    .role("heading"),
            )
            .child(
                text(caption)
                    .font(t::TEXT_COMPACT)
                    .color(c.ink_muted)
                    .wrap(),
            ),
    )
}

/// Settings' rows in an outlined box (`components::settings_group`).
fn group(key: &'static str, rows: Vec<El>, look: Look) -> El {
    settings_group(key, rows, look)
}

/// A setting's row (`components::setting_row`).
fn setting(key: impl Into<String>, name: &str, caption: &str, control: El, look: Look) -> El {
    setting_row(key, name, caption, control, look)
}

/// A switch's row, its line saying what the setting does as it is now.
fn toggle(
    key: &'static str,
    name: &str,
    on: bool,
    (when_on, when_off): (&str, &str),
    flip: impl Fn(&mut State, bool) + 'static,
    look: Look,
) -> El {
    setting(
        format!("{key}-row"),
        name,
        if on { when_on } else { when_off },
        switch(key, name, on, move |s: &mut State| flip(s, !on), look),
        look,
    )
}

fn appearance(s: &State, look: Look) -> El {
    let mut themes = row()
        .key("appearance")
        .w_full()
        .gap(t::SPACE_16)
        .role("radiogroup")
        .label("Appearance");
    for (i, (value, name)) in [
        (Appearance::Light, "Light"),
        (Appearance::Dark, "Dark"),
        (Appearance::System, "System"),
    ]
    .into_iter()
    .enumerate()
    {
        themes = themes.child(picture_choice(
            format!("appearance-{i}"),
            name,
            None,
            window_sketch(value),
            s.settings.appearance == value,
            move |s: &mut State| s.set_appearance(value),
            look,
        ));
    }
    let mut section = section(
        "appearance-card",
        "Appearance",
        "Light, dark, or whichever this computer is in.",
        look,
    )
    .child(themes);
    if scroll_bars_can_be_chosen() {
        section = section.child(group(
            "scroll-bars-card",
            vec![setting(
                "scroll-bars-row",
                "Scroll bars",
                "Automatic follows this Mac's own setting.",
                segmented_compact(
                    "scroll-bars",
                    "Show scroll bars",
                    &[
                        (ScrollBars::Automatic, "Automatic"),
                        (ScrollBars::WhenScrolling, "When scrolling"),
                        (ScrollBars::Always, "Always"),
                    ],
                    s.settings.scroll_bars,
                    |s: &mut State, choice| s.set_scroll_bars(choice),
                    None,
                    look,
                ),
                look,
            )],
            look,
        ));
    }
    section
}

fn messages(s: &State, with_preview: bool, look: Look) -> El {
    let c = look.c;
    let matching = Preset::matching(s.settings.display());
    let mut presets = row()
        .key("preset")
        .w_full()
        .gap(t::SPACE_16)
        .role("radiogroup")
        .label("Preset");
    for (i, preset) in Preset::ALL.into_iter().enumerate() {
        let (name, caption) = match preset {
            Preset::Comfortable => ("Comfortable", "Large avatars, room to breathe"),
            Preset::Compact => ("Compact", "Small avatars, more on screen"),
            Preset::Minimal => ("Minimal", "A line each, media as links"),
        };
        presets = presets.child(picture_choice(
            format!("preset-{i}"),
            name,
            Some(caption),
            preset_sketch(preset, look),
            matching == Some(preset),
            move |s: &mut State| s.apply_preset(preset),
            look,
        ));
    }
    let mut section = section(
        "messages-card",
        "Messages",
        "Start from a preset, then change anything below.",
        look,
    );
    section = section.child(presets);
    if matching.is_none() {
        section = section.child(
            row()
                .gap(t::SPACE_8)
                .items_center()
                .child(
                    el().size(t::SPACE_6, t::SPACE_6)
                        .radius(t::RADIUS_PILL)
                        .fill(c.accent)
                        .a11y_hidden(),
                )
                .child(
                    text("Custom: these settings match no preset.")
                        .key("preset-custom")
                        .font(t::TEXT_META)
                        .color(c.ink_muted)
                        .role("status"),
                ),
        );
    }

    let mut rows = vec![
        setting(
            "layout-row",
            "Layout",
            "One person's messages as a block, or a line each.",
            segmented_compact(
                "layout",
                "Layout",
                &[(Layout::Grouped, "Grouped"), (Layout::OneLine, "One line")],
                s.settings.layout,
                |s: &mut State, choice| s.set_layout(choice),
                None,
                look,
            ),
            look,
        ),
        setting(
            "spacing-row",
            "Spacing",
            "Room between messages.",
            segmented_compact(
                "spacing",
                "Spacing",
                &[
                    (Spacing::Roomy, "Roomy"),
                    (Spacing::Cozy, "Cozy"),
                    (Spacing::Tight, "Tight"),
                ],
                s.settings.spacing,
                |s: &mut State, choice| s.set_spacing(choice),
                None,
                look,
            ),
            look,
        ),
    ];
    if s.settings.layout == Layout::Grouped {
        rows.push(setting(
            "avatars-row",
            "Avatars",
            "Beside each block of messages.",
            segmented_compact(
                "avatars",
                "Avatars",
                &[
                    (Avatars::Large, "Large"),
                    (Avatars::Small, "Small"),
                    (Avatars::Hidden, "Hidden"),
                ],
                s.settings.avatars,
                |s: &mut State, choice| s.set_avatars(choice),
                None,
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
    // The small and the large letter either side say which way is bigger.
    let letter = |style| text("A").font(style).color(c.ink_muted).a11y_hidden();
    rows.push(setting(
        "font-size-row",
        "Font size",
        "Messages and names, in points.",
        row()
            .gap(t::SPACE_8)
            .items_center()
            .child(letter(t::TEXT_SIZE_SMALL))
            .child(segmented_compact(
                "font-size",
                "Font size",
                &sizes,
                s.settings.font_size,
                |s: &mut State, choice| s.set_font_size(choice),
                Some(t::SEGMENT_NUMBER),
                look,
            ))
            .child(letter(t::TEXT_SIZE_LARGE)),
        look,
    ));

    let switches = vec![
        toggle(
            "images",
            "Images",
            s.settings.images == Media::Shown,
            (
                "Shown in their messages.",
                "As links: each one a file row that opens the viewer.",
            ),
            |s: &mut State, on| s.set_images(if on { Media::Shown } else { Media::Link }),
            look,
        ),
        toggle(
            "embeds",
            "Embeds",
            s.settings.embeds == Media::Shown,
            ("A card under each link.", "As links: no cards."),
            |s: &mut State, on| s.set_embeds(if on { Media::Shown } else { Media::Link }),
            look,
        ),
        toggle(
            "reactions",
            "Reactions",
            s.settings.reactions == Reactions::Shown,
            (
                "Shown under each message.",
                "Hidden. You can still add one from a message's toolbar.",
            ),
            |s: &mut State, on| {
                s.set_reactions(if on {
                    Reactions::Shown
                } else {
                    Reactions::Hidden
                });
            },
            look,
        ),
    ];
    section = section
        .child(group("message-settings", rows, look))
        .child(group("message-switches", switches, look));
    if with_preview {
        section = section.child(preview(s, look));
    }
    section
}

/// A sketch's bar, `fraction` of the width it stands in.
fn bar(fraction: f32, color: rune::Color) -> El {
    el().h(t::SKETCH_BAR)
        .w_frac(fraction)
        .radius(t::SKETCH_BAR / 2.0)
        .fill(color)
}

/// A sketch's bar `width` long.
fn stub(width: f32, color: rune::Color) -> El {
    el().h(t::SKETCH_BAR)
        .w(width)
        .radius(t::SKETCH_BAR / 2.0)
        .fill(color)
}

/// The theme's picture: the app's window drawn small in its colours, the rail, the sidebar
/// and a panel. System is the light window's side and the dark window's panel.
fn window_sketch(appearance: Appearance) -> El {
    let (light, dark) = (t::colors(false), t::colors(true));
    let rail = |c: &Colors| el().w(t::SKETCH_RAIL).h_full().fill(c.rail);
    let sidebar = |c: &Colors| {
        col()
            .w(t::SKETCH_SIDEBAR)
            .py(t::SPACE_10)
            .px(t::SPACE_6)
            .gap(t::SPACE_6)
            .child(bar(0.8, c.rail_divider))
            .child(bar(1.0, c.rail_divider))
            .child(bar(0.64, c.rail_divider))
    };
    let panel = |c: &Colors| {
        col()
            .grow(1.0)
            .min_w(0.0)
            .my(t::SPACE_6)
            .mr(t::SPACE_6)
            .padding(t::SPACE_10)
            .gap(t::SPACE_6)
            .radius(t::RADIUS_7)
            .fill(c.surface)
            .child(bar(0.5, c.accent))
            .child(bar(0.9, c.line_strong))
            .child(bar(0.72, c.line_strong))
    };
    let window = |c: &'static Colors| {
        row()
            .fill(c.bg)
            .child(rail(c))
            .child(sidebar(c))
            .child(panel(c))
    };
    match appearance {
        Appearance::Light => window(light),
        Appearance::Dark => window(dark),
        Appearance::System => row()
            .child(
                row()
                    .grow(1.0)
                    .h_full()
                    .fill(light.bg)
                    .child(rail(light))
                    .child(sidebar(light).grow(1.0)),
            )
            .child(
                row()
                    .grow(1.0)
                    .h_full()
                    .pl(t::SPACE_6)
                    .fill(dark.bg)
                    .child(panel(dark)),
            ),
    }
}

/// The preset's picture: its rows sketched, their avatars in two people's tones.
fn preset_sketch(preset: Preset, look: Look) -> El {
    let c = look.c;
    let (faint, strong) = (c.border_form, c.border_active);
    let tones = [t::tone_named("mint", c).0, t::tone_named("amber", c).0];
    let disc = |size: f32, tone| el().size(size, size).radius(t::RADIUS_PILL).fill(tone);
    let sketch = col().padding(t::SPACE_14).fill(c.track);
    match preset {
        Preset::Comfortable => tones.into_iter().zip([(0.36, 0.88), (0.3, 0.7)]).fold(
            sketch.gap(t::SPACE_12),
            |sketch, (tone, (name, words))| {
                sketch.child(
                    row()
                        .w_full()
                        .gap(t::SPACE_8)
                        .child(disc(t::SKETCH_AVATAR, tone))
                        .child(
                            col()
                                .grow(1.0)
                                .min_w(0.0)
                                .pt(t::SPACE_2)
                                .gap(t::SPACE_6)
                                .child(bar(name, strong))
                                .child(bar(words, faint)),
                        ),
                )
            },
        ),
        Preset::Compact => {
            let under = |fraction| {
                row()
                    .w_full()
                    .pl(t::SKETCH_AVATAR_SMALL + t::SPACE_6)
                    .child(bar(fraction, faint))
            };
            let head = |tone, fraction| {
                row()
                    .w_full()
                    .gap(t::SPACE_6)
                    .items_center()
                    .child(disc(t::SKETCH_AVATAR_SMALL, tone))
                    .child(bar(fraction, strong))
            };
            sketch
                .gap(t::SPACE_6)
                .child(head(tones[0], 0.3))
                .child(under(0.8))
                .child(head(tones[1], 0.26))
                .child(under(0.64))
                .child(under(0.72))
        }
        Preset::Minimal => {
            [0.5, 0.38, 0.6, 0.3, 0.46]
                .into_iter()
                .fold(sketch.gap(t::SPACE_6), |sketch, words| {
                    sketch.child(
                        row()
                            .w_full()
                            .gap(t::SPACE_6)
                            .child(stub(t::SPACE_14, faint))
                            .child(stub(t::SPACE_24, strong))
                            .child(row().grow(1.0).min_w(0.0).child(bar(words, faint))),
                    )
                })
        }
    }
}

/// The preview: a channel's messages as the conversation draws them under these settings,
/// with a line on where the settings are kept.
fn preview(s: &State, look: Look) -> El {
    let c = look.c;
    let ty = timeline::type_of(s);
    let at = |hour, minute| {
        NaiveTime::from_hms_opt(hour, minute, 0)
            .and_then(|time| {
                Local::now()
                    .date_naive()
                    .and_time(time)
                    .and_local_timezone(Local)
                    .single()
            })
            .map_or(0, |time| time.timestamp_micros())
    };
    let picture = if s.settings.images == Media::Shown {
        picture_sketch(look).key("preview-picture")
    } else {
        media::file_box(
            row()
                .w_full()
                .gap(t::SPACE_10)
                .items_center()
                .child(media::file_words(
                    &icons::FILE,
                    "onboarding-first-screen.png",
                    &size_words(412_000),
                    &ty,
                    look,
                ))
                .child(media::glyph_face(&icons::DOWNLOAD, t::FILE_ACTION, look)),
            look,
        )
        .key("preview-file")
    };
    let mut card = Vec::new();
    if s.settings.embeds == Media::Shown {
        card.push(
            media::card_box(
                row()
                    .grow(1.0)
                    .min_w(0.0)
                    .gap(t::SPACE_12)
                    .child(media::card_words(
                        "lumia.chat",
                        "What's new in Lumia 0.13",
                        "Video and audio play in the conversation, and links open as cards on \
                     every client.",
                        &ty,
                        look,
                    )),
                look,
            )
            .key("preview-card"),
        );
    }
    let reactions = row()
        .key("preview-reactions")
        .w_full()
        .flex_wrap()
        .gap(t::SPACE_6)
        .items_center()
        .child(replies::chip("👀", 2, false, &ty, look))
        .child(replies::chip("🎉", 3, true, &ty, look))
        .child(replies::add_reaction_face(look));
    let samples = vec![
        timeline::Sample {
            author: "Priya Nair",
            author_id: "preview-priya",
            at_micros: at(10, 42),
            body: "Pushed the new onboarding flow to staging. Could you look before standup?",
            boxes: Vec::new(),
            reactions: None,
        },
        timeline::Sample {
            author: "Priya Nair",
            author_id: "preview-priya",
            at_micros: at(10, 42),
            body: "The invite step is the one I'm least sure about.",
            boxes: Vec::new(),
            reactions: None,
        },
        timeline::Sample {
            author: "Tomás Reyes",
            author_id: "preview-tomas",
            at_micros: at(10, 44),
            body: "On it. Here's the first screen on my phone:",
            boxes: vec![picture],
            reactions: Some(reactions),
        },
        timeline::Sample {
            author: "Mei Chen",
            author_id: "preview-mei",
            at_micros: at(10, 51),
            body: "Release notes are up for review too: \
                   [lumia.chat/blog/0-13](https://lumia.chat/blog/0-13)",
            boxes: card,
            reactions: None,
        },
    ];
    let channel = row()
        .w_full()
        .px(t::SPACE_16)
        .py(t::SPACE_12)
        .gap(t::SPACE_6)
        .items_center()
        .border_sides([0.0, 0.0, 1.0, 0.0], c.line)
        .child(icon(&icons::HASH, t::SPACE_16).color(c.ink_muted))
        .child(text("design-review").font(t::TITLE_SECTION).color(c.ink));
    col()
        .key("preview")
        .w_full()
        .gap(t::SPACE_12)
        .child(
            text("Preview")
                .font(t::TEXT_SECTION)
                .color(c.ink_muted)
                .a11y_hidden(),
        )
        .child(
            col()
                .key("preview-messages")
                .w_full()
                .radius(t::RADIUS_16)
                .clip_rounded(t::RADIUS_16)
                .fill(c.surface)
                .border(1.0, c.line_strong)
                .role("img")
                .label("Preview of messages with these settings")
                .child(
                    col()
                        .w_full()
                        .a11y_hidden()
                        .child(channel)
                        .child(timeline::samples(s, samples, look)),
                ),
        )
        .child(
            text(
                "Kept on this computer only. Your other devices and your account are not changed.",
            )
            .font(t::TEXT_META)
            .color(c.ink_muted)
            .wrap(),
        )
}

/// The picture in the preview's third message: a phone's sign-in card on a tone, drawn.
fn picture_sketch(look: Look) -> El {
    let c = look.c;
    let field = || {
        el().w_full()
            .h(t::PREVIEW_FIELD)
            .radius(t::RADIUS_6)
            .border(1.0, c.line_strong)
    };
    el().size(t::PREVIEW_PICTURE[0], t::PREVIEW_PICTURE[1])
        .radius(t::RADIUS_12)
        .fill(t::tone_named("blue", c).0)
        .center()
        .child(
            col()
                .size(t::PREVIEW_CARD[0], t::PREVIEW_CARD[1])
                .pt(t::SPACE_14)
                .px(t::SPACE_12)
                .gap(t::SPACE_8)
                .radius(t::RADIUS_12)
                .fill(c.surface)
                .child(bar(0.6, c.border_active))
                .child(field())
                .child(field())
                .child(field().border(0.0, c.accent).fill(c.accent)),
        )
}

fn updates(s: &State, updater: &Updater, look: Look) -> El {
    let c = look.c;
    let current = env!("CARGO_PKG_VERSION");
    let status = match &updater.phase {
        Phase::Idle => format!("This is Lumia {current}."),
        Phase::Checking => "Checking for updates…".to_string(),
        Phase::UpToDate => format!("Lumia {current} is the newest release."),
        Phase::Available(version) => format!("Lumia {version} is available."),
        Phase::Downloading(version) => format!("Downloading Lumia {version}…"),
        Phase::Ready(version) => {
            format!("Lumia {version} is ready. Restart Lumia to start using it.")
        }
        Phase::Failed(_) => format!("This is Lumia {current}."),
    };
    let mut buttons = row()
        .gap(t::SPACE_10)
        .items_center()
        .child(secondary_button(
            "check-for-updates",
            "Check for updates",
            !updater.busy(),
            State::check_for_updates,
            look,
        ));
    match updater.phase {
        Phase::Available(_) => {
            buttons = buttons.child(action_button(
                "install-update",
                "Update",
                true,
                State::install_update,
                look,
            ));
        }
        Phase::Ready(_) => {
            buttons = buttons.child(action_button(
                "restart-to-update",
                "Restart",
                true,
                State::restart_to_update,
                look,
            ));
        }
        _ => {}
    }
    let mut section = section(
        "updates-card",
        "Updates",
        "Lumia updates itself from its releases on GitHub, each one signed.",
        look,
    )
    .child(
        text(status)
            .key("update-status")
            .font(t::TEXT_COMPACT)
            .color(c.ink_secondary)
            .wrap()
            .role("status"),
    );
    if let Phase::Failed(why) = &updater.phase {
        section = section.child(error_text(&format!("Could not update: {why}"), look));
    }
    let daily = s.settings.update_checks == UpdateChecks::Daily;
    section.child(buttons).child(check_row(
        "check-updates-automatically",
        "Check for updates automatically",
        Some("Once a day. Nothing is downloaded until you choose Update."),
        daily,
        move |s: &mut State| {
            s.set_update_checks(if daily {
                UpdateChecks::Off
            } else {
                UpdateChecks::Daily
            });
        },
        look,
    ))
}
