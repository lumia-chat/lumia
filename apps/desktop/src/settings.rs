//! Where the shell keeps its state on this machine, and the little it remembers between
//! launches outside the core's stores.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// The directory under the platform's place for application data. Not the SwiftUI macOS
/// app's `Lumia`: while both shells exist they must not open one MLS store from two
/// processes (architecture 15.5 says why a ratchet has one writer).
const DIRECTORY: &str = "chat.lumia.desktop";

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
