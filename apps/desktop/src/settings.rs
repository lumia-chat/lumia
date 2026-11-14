//! Where the shell keeps its state on this machine, and the little it remembers between
//! launches outside the core's stores.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// The directory under the platform's place for application data, named as the app is
/// (`platform::APP_ID`).
const DIRECTORY: &str = "chat.lumia";

/// The shell's data directory: `LUMIA_DATA_DIR` when set (the smoke script, a second
/// profile), else under Application Support on macOS, `%APPDATA%` on Windows, and
/// `$XDG_DATA_HOME` or `~/.local/share` elsewhere.
pub fn data_dir() -> PathBuf {
    if let Some(dir) = std::env::var_os("LUMIA_DATA_DIR").filter(|d| !d.is_empty()) {
        return PathBuf::from(dir);
    }
    let home = || PathBuf::from(std::env::var_os("HOME").unwrap_or_default());
    let base = if cfg!(target_os = "macos") {
        home().join("Library/Application Support")
    } else if cfg!(windows) {
        std::env::var_os("APPDATA").map_or_else(home, PathBuf::from)
    } else {
        std::env::var_os("XDG_DATA_HOME")
            .filter(|d| !d.is_empty())
            .map_or_else(|| home().join(".local/share"), PathBuf::from)
    };
    base.join(DIRECTORY)
}

/// What the shell remembers between launches.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// The instance last opened, reopened at launch.
    pub instance: Option<String>,
    /// The channel last on screen, which a launch returns to (architecture 17's cold start
    /// is to the last-viewed channel).
    pub last_channel: Option<LastChannel>,
    /// What the Preferences page holds, which is this computer's and not the account's:
    /// another computer, or another person at this one, chooses its own. Each is read on
    /// its own, as its default when it is missing or not understood, so one value this
    /// version does not know costs that one setting and not the file.
    #[serde(deserialize_with = "or_default")]
    pub appearance: Appearance,
    /// How a conversation's messages are drawn. Preferences' presets ([`Preset`]) write
    /// all but the font size together, and are not kept themselves.
    #[serde(deserialize_with = "or_default")]
    pub layout: Layout,
    #[serde(deserialize_with = "or_default")]
    pub spacing: Spacing,
    #[serde(deserialize_with = "or_default")]
    pub avatars: Avatars,
    #[serde(deserialize_with = "or_default")]
    pub font_size: FontSize,
    /// Whether a message's pictures and its links' previews are drawn in it or left as a
    /// link. Kept and set by the presets, with no control yet: the shell draws neither
    /// (plan step 9), and a control that does nothing is worse than none.
    #[serde(deserialize_with = "or_default")]
    pub images: Media,
    #[serde(deserialize_with = "or_default")]
    pub embeds: Media,
    #[serde(deserialize_with = "or_default")]
    pub scroll_bars: ScrollBars,
    /// The member list (or a conversation's details) put away from a wide window's row by
    /// its toggle in the conversation's header. A narrow window never has it in the row.
    pub aside_hidden: bool,
    /// The emoji this computer reacted with, most recent first: the emoji picker's
    /// "Frequently used" row.
    pub recent_emoji: Vec<String>,
    /// The rail's folders open on this computer, by id (architecture 6.9: open or closed is
    /// each device's own). A folder no longer on the rail is dropped as the rail is drawn.
    #[serde(deserialize_with = "or_default")]
    pub open_folders: Vec<String>,
}

/// Light, dark, or whichever the system is in.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Appearance {
    Light,
    Dark,
    #[default]
    System,
}

impl Appearance {
    /// Whether the window is dark, given whether the system is.
    pub fn dark(self, system_dark: bool) -> bool {
        match self {
            Self::Light => false,
            Self::Dark => true,
            Self::System => system_dark,
        }
    }
}

/// How a conversation's messages are laid out.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Layout {
    /// One person's quick messages as a block under one avatar and name, as the boards
    /// draw them.
    #[default]
    Grouped,
    /// Every message on a line of its own after its time and its sender's name, the two
    /// repeated on each, and no avatar.
    OneLine,
}

/// How much room there is around and between messages.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Spacing {
    /// The boards' measures.
    #[default]
    Roomy,
    /// Half the room between blocks, inside one and around a day's divider.
    Cozy,
    /// Cozy, with less again over and under a block.
    Tight,
}

/// The picture beside a block of messages.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Avatars {
    #[default]
    Large,
    Small,
    /// None, and no room kept for one: the name starts where it would have been.
    Hidden,
}

/// Whether something a message carries is drawn in it or left as a link.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Media {
    #[default]
    Shown,
    Link,
}

/// The size of a message's text in points, one of [`FontSize::ALL`]. Every text of a
/// message's row, and the composer's, is drawn at this over 15 of its own size.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "u8", into = "u8")]
pub struct FontSize(u8);

impl FontSize {
    /// The sizes Preferences offers.
    pub const ALL: [FontSize; 6] = [
        FontSize(13),
        FontSize(14),
        FontSize(15),
        FontSize(16),
        FontSize(18),
        FontSize(20),
    ];

    /// `points` when it is one of [`FontSize::ALL`].
    pub fn new(points: u8) -> Option<Self> {
        Self::ALL.into_iter().find(|size| size.0 == points)
    }

    pub fn points(self) -> u8 {
        self.0
    }
}

impl Default for FontSize {
    /// The message text's size in the design system.
    fn default() -> Self {
        FontSize(15)
    }
}

impl TryFrom<u8> for FontSize {
    type Error = String;

    fn try_from(points: u8) -> Result<Self, Self::Error> {
        Self::new(points).ok_or_else(|| format!("no font size {points}"))
    }
}

impl From<FontSize> for u8 {
    fn from(size: FontSize) -> u8 {
        size.0
    }
}

/// A named set of the message settings: what Preferences' preset choice writes. A preset is
/// not kept; the page shows the one whose values all match what is set ([`Preset::matching`]).
/// The font size is in none of them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Preset {
    Comfortable,
    Compact,
    Minimal,
}

/// The settings a preset writes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Display {
    pub layout: Layout,
    pub spacing: Spacing,
    pub avatars: Avatars,
    pub images: Media,
    pub embeds: Media,
}

impl Preset {
    pub const ALL: [Preset; 3] = [Preset::Comfortable, Preset::Compact, Preset::Minimal];

    /// What the preset writes. Comfortable is a new computer's, and what the shell drew
    /// before there were settings; Compact is the smaller avatar with half the room;
    /// Minimal is a line for each message, with nothing drawn in it.
    pub fn values(self) -> Display {
        match self {
            Preset::Comfortable => Display {
                layout: Layout::Grouped,
                spacing: Spacing::Roomy,
                avatars: Avatars::Large,
                images: Media::Shown,
                embeds: Media::Shown,
            },
            Preset::Compact => Display {
                layout: Layout::Grouped,
                spacing: Spacing::Cozy,
                avatars: Avatars::Small,
                images: Media::Shown,
                embeds: Media::Shown,
            },
            Preset::Minimal => Display {
                layout: Layout::OneLine,
                spacing: Spacing::Tight,
                avatars: Avatars::Hidden,
                images: Media::Link,
                embeds: Media::Link,
            },
        }
    }

    /// The preset whose values are all what `display` is, or none ("Custom"). In one line
    /// the avatars are not compared, since none is drawn there.
    pub fn matching(display: Display) -> Option<Preset> {
        Self::ALL.into_iter().find(|preset| {
            let wanted = preset.values();
            let avatars = display.layout == Layout::OneLine || display.avatars == wanted.avatars;
            avatars
                && Display {
                    avatars: wanted.avatars,
                    ..display
                } == wanted
        })
    }
}

/// When a pane that scrolls shows its scroll bar.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ScrollBars {
    /// As the system says: on a Mac, "Show scroll bars" in System Settings.
    #[default]
    Automatic,
    /// Over the content while it scrolls, then gone.
    WhenScrolling,
    /// Whenever there is more than fits.
    Always,
}

/// A server's channel, by the ids the instance gave.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LastChannel {
    pub server_id: String,
    pub channel_id: String,
}

/// A setting as its default when what is kept for it is not one of its values.
fn or_default<'de, D, T>(deserializer: D) -> Result<T, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de> + Default,
{
    let value = serde_json::Value::deserialize(deserializer)?;
    Ok(T::deserialize(value).unwrap_or_default())
}

impl Settings {
    fn path(data_dir: &Path) -> PathBuf {
        data_dir.join("settings.json")
    }

    /// The message settings a preset writes, as they are now.
    pub fn display(&self) -> Display {
        Display {
            layout: self.layout,
            spacing: self.spacing,
            avatars: self.avatars,
            images: self.images,
            embeds: self.embeds,
        }
    }

    /// Writes `preset`'s values over the message settings. The font size stays.
    pub fn apply(&mut self, preset: Preset) {
        let Display {
            layout,
            spacing,
            avatars,
            images,
            embeds,
        } = preset.values();
        self.layout = layout;
        self.spacing = spacing;
        self.avatars = avatars;
        self.images = images;
        self.embeds = embeds;
    }

    /// The saved settings; the defaults when there are none or they do not parse.
    pub fn load(data_dir: &Path) -> Self {
        std::fs::read_to_string(Self::path(data_dir))
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default()
    }

    /// Writes the settings. A failure is reported and otherwise ignored: the app works
    /// without them, it only asks for the instance again.
    pub fn save(&self, data_dir: &Path) {
        let written = std::fs::create_dir_all(data_dir).and_then(|()| {
            std::fs::write(
                Self::path(data_dir),
                serde_json::to_vec_pretty(self).unwrap_or_default(),
            )
        });
        if let Err(error) = written {
            eprintln!("lumia: saving settings in {}: {error}", data_dir.display());
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    #[test]
    fn each_preset_writes_its_values_and_is_the_one_that_matches_them() {
        for preset in Preset::ALL {
            let mut settings = Settings::default();
            settings.apply(preset);
            assert_eq!(settings.display(), preset.values());
            assert_eq!(Preset::matching(settings.display()), Some(preset));
        }
        // A new computer's are Comfortable's.
        assert_eq!(
            Preset::matching(Settings::default().display()),
            Some(Preset::Comfortable)
        );
    }

    #[test]
    fn a_preset_leaves_the_font_size_alone() {
        let mut settings = Settings {
            font_size: FontSize::new(20).unwrap(),
            ..Settings::default()
        };
        settings.apply(Preset::Minimal);
        assert_eq!(settings.font_size.points(), 20);
    }

    #[test]
    fn one_change_from_a_preset_matches_none() {
        let tweaked = Display {
            spacing: Spacing::Tight,
            ..Preset::Comfortable.values()
        };
        assert_eq!(Preset::matching(tweaked), None);
        let tweaked = Display {
            avatars: Avatars::Hidden,
            ..Preset::Compact.values()
        };
        assert_eq!(Preset::matching(tweaked), None);
        let tweaked = Display {
            images: Media::Shown,
            ..Preset::Minimal.values()
        };
        assert_eq!(Preset::matching(tweaked), None);
    }

    #[test]
    fn in_one_line_the_avatars_are_not_compared() {
        for avatars in [Avatars::Large, Avatars::Small, Avatars::Hidden] {
            let display = Display {
                avatars,
                ..Preset::Minimal.values()
            };
            assert_eq!(Preset::matching(display), Some(Preset::Minimal));
        }
        // In grouped they are.
        let display = Display {
            avatars: Avatars::Hidden,
            ..Preset::Comfortable.values()
        };
        assert_eq!(Preset::matching(display), None);
    }

    #[test]
    fn the_values_are_kept_under_their_names() {
        let mut settings = Settings::default();
        settings.apply(Preset::Minimal);
        settings.font_size = FontSize::new(18).unwrap();
        let kept = serde_json::to_value(&settings).unwrap();
        for (key, value) in [
            ("layout", serde_json::json!("one_line")),
            ("spacing", serde_json::json!("tight")),
            ("avatars", serde_json::json!("hidden")),
            ("font_size", serde_json::json!(18)),
            ("images", serde_json::json!("link")),
            ("embeds", serde_json::json!("link")),
        ] {
            assert_eq!(kept[key], value, "{key}");
        }
        assert!(kept.get("density").is_none());
        let read: Settings = serde_json::from_value(kept).unwrap();
        assert_eq!(read, settings);
    }

    #[test]
    fn each_setting_is_its_default_alone_when_it_is_not_understood() {
        let read: Settings = serde_json::from_str(
            r#"{
                "instance": "https://chat.example.com",
                "density": "compact",
                "appearance": "sepia",
                "layout": "one_line",
                "spacing": 3,
                "avatars": "small",
                "font_size": 17,
                "images": "link",
                "embeds": null
            }"#,
        )
        .unwrap();
        assert_eq!(read.instance.as_deref(), Some("https://chat.example.com"));
        assert_eq!(read.appearance, Appearance::System);
        assert_eq!(read.layout, Layout::OneLine);
        assert_eq!(read.spacing, Spacing::Roomy);
        assert_eq!(read.avatars, Avatars::Small);
        assert_eq!(read.font_size, FontSize::default());
        assert_eq!(read.images, Media::Link);
        assert_eq!(read.embeds, Media::Shown);
        // Missing, as in a file from before them: the defaults, Comfortable at 15, whatever
        // the density was.
        let read: Settings = serde_json::from_str(r#"{"density": "compact"}"#).unwrap();
        assert_eq!(Preset::matching(read.display()), Some(Preset::Comfortable));
        assert_eq!(read.font_size.points(), 15);
    }

    #[test]
    fn only_the_offered_font_sizes_are_sizes() {
        let offered: Vec<u8> = FontSize::ALL.iter().map(|s| s.points()).collect();
        assert_eq!(offered, [13, 14, 15, 16, 18, 20]);
        assert_eq!(FontSize::new(17), None);
        assert_eq!(FontSize::new(15), Some(FontSize::default()));
    }
}
