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
    /// The Preferences page's three choices, which are this computer's and not the
    /// account's: another computer, or another person at this one, chooses its own.
    pub appearance: Appearance,
    pub density: Density,
    pub scroll_bars: ScrollBars,
    /// The member list (or a conversation's details) put away from a wide window's row by
    /// its toggle in the conversation's header. A narrow window never has it in the row.
    pub aside_hidden: bool,
    /// The emoji this computer reacted with, most recent first: the emoji picker's
    /// "Frequently used" row.
    pub recent_emoji: Vec<String>,
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

/// How much room a conversation's messages take.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Density {
    /// The boards' measures.
    #[default]
    Comfortable,
    /// Smaller avatars and less room around each message and between blocks: more of a
    /// conversation in the same window.
    Compact,
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

impl Settings {
    fn path(data_dir: &Path) -> PathBuf {
        data_dir.join("settings.json")
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
