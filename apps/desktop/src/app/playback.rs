//! Attached videos and recordings played in the conversation (#21, specs/media.md 5): Rune's
//! own player (`rune::Player`, AVFoundation on macOS and Media Foundation with WASAPI on
//! Windows) in the message's row, with the shell's controls (`ui::media`).
//!
//! Nothing plays before a press. A press writes the attachment under the temporary directory
//! by its id and its media type's extension (`session::media::player_file`), and the clip
//! plays from that file once it is there, unless another was started meanwhile. Starting one
//! pauses every other: one clip plays at a time in the window. A video pauses when its
//! message's row leaves the list's view ([`State::rows_visible`]); a recording plays on
//! while the list scrolls. Both stop and go, with their files, when another conversation
//! opens or the shell is left: a clip belongs to the conversation it was started in.
//!
//! Where Rune has no player (Linux, [`INLINE`]), or for an attachment the server let go
//! (`expired`, which has nothing to fetch), nothing here holds a clip: Play opens the
//! system's player as before, and an expired file does nothing.
//!
//! Full screen is the video over the whole window, as the viewer is, with the window taken
//! full screen with it and brought back when it closes. Rune does not say whether the
//! window was full screen already, so one that was leaves full screen when the video does.

use std::collections::HashMap;
use std::ops::Range;
use std::path::PathBuf;

use lumia_client::app::AttachmentSummary;
use rune::{Key, KeyEvent, Player, PlayerState, Tx, Waker};

use super::State;
use crate::session::Conversation;
use crate::session::media::{MediaUpdate, player_file};

/// Whether Rune plays media on this platform: macOS and Windows. Elsewhere Play opens the
/// system's player.
pub const INLINE: bool = cfg!(any(target_os = "macos", target_os = "windows"));

/// How far the arrow keys move a clip, in seconds.
pub const STEP_SECONDS: f64 = 5.0;

/// Where a clip pressed stands.
pub enum Stage {
    /// Its file is being fetched and written.
    Fetching,
    Ready(Player),
    /// Why it could not be fetched, in words.
    Failed(String),
}

/// A video or a recording started in the conversation on screen.
pub struct Clip {
    pub attachment: AttachmentSummary,
    pub stage: Stage,
    /// The file it plays from, removed when it goes.
    file: Option<PathBuf>,
}

impl Clip {
    pub fn player(&self) -> Option<&Player> {
        match &self.stage {
            Stage::Ready(player) => Some(player),
            _ => None,
        }
    }

    pub fn is_video(&self) -> bool {
        self.attachment.kind == "video"
    }

    /// Whether it plays, or will as soon as it has loaded.
    pub fn playing(&self) -> bool {
        self.player().is_some_and(Player::is_playing)
    }

    /// Why it cannot play, in words: not fetched, or a file Rune's player cannot open.
    pub fn failure(&self) -> Option<String> {
        match &self.stage {
            Stage::Failed(why) => Some(why.clone()),
            Stage::Ready(player) => match player.state() {
                PlayerState::Failed(_) => Some(format!(
                    "This {} can’t be played here. Save it to open it in another app.",
                    if self.is_video() {
                        "video"
                    } else {
                        "recording"
                    }
                )),
                _ => None,
            },
            Stage::Fetching => None,
        }
    }
}

impl Drop for Clip {
    /// Silent at once, whatever still draws it, then its file is removed (best effort: on
    /// Windows a file still open stays until the next start of the same clip writes it again).
    fn drop(&mut self) {
        if let Stage::Ready(player) = &self.stage {
            player.pause();
        }
        self.stage = Stage::Fetching;
        if let Some(file) = self.file.take() {
            let _ = std::fs::remove_file(file);
        }
    }
}

/// The window's clips.
pub struct Playback {
    /// Whether this window plays clips itself; [`INLINE`], which a test may change.
    pub inline: bool,
    clips: HashMap<String, Clip>,
    /// The clip pressed last: it plays once its file is written, unless another was pressed.
    wanted: Option<String>,
    /// The conversation the clips were started in.
    conversation: Option<Conversation>,
    /// The video over the whole window.
    pub full_screen: Option<String>,
    /// The window is owed a turn into or out of full screen, which the next poll pays: a
    /// change of the clips has no `Tx` (a modal's Escape, a conversation's end).
    toggle_owed: bool,
    waker: Option<Waker>,
    /// The quarter second of the playing clip the view was last run for.
    shown: Option<(String, i64)>,
}

impl Default for Playback {
    fn default() -> Self {
        Self {
            inline: INLINE,
            clips: HashMap::new(),
            wanted: None,
            conversation: None,
            full_screen: None,
            toggle_owed: false,
            waker: None,
            shown: None,
        }
    }
}

impl std::fmt::Debug for Playback {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Playback")
            .field("inline", &self.inline)
            .field("clips", &self.clips.len())
            .field("playing", &self.playing())
            .finish_non_exhaustive()
    }
}

impl Playback {
    /// `waker` is the window's, called when a clip loads, ends or fails.
    pub fn start(&mut self, waker: Waker) {
        for player in self.clips.values().filter_map(Clip::player) {
            player.set_waker(std::sync::Arc::clone(&waker));
        }
        self.waker = Some(waker);
    }

    pub fn clip(&self, attachment_id: &str) -> Option<&Clip> {
        self.clips.get(attachment_id)
    }

    /// The clip playing, if one is.
    pub fn playing(&self) -> Option<&str> {
        self.clips
            .iter()
            .find(|(_, clip)| clip.playing())
            .map(|(id, _)| id.as_str())
    }

    fn pause_others(&mut self, attachment_id: &str) {
        for (id, clip) in &self.clips {
            if id != attachment_id
                && let Some(player) = clip.player()
            {
                player.pause();
            }
        }
    }

    /// Every clip stops and goes, with its file. True when there was one.
    fn forget(&mut self) -> bool {
        let had = !self.clips.is_empty();
        self.clips.clear();
        self.wanted = None;
        self.shown = None;
        if self.full_screen.take().is_some() {
            self.toggle_owed ^= true;
        }
        had
    }
}

impl State {
    /// A video's poster or a video's or a recording's Play pressed: it plays in the window
    /// where Rune has a player, the system's player opens it elsewhere. One the server let
    /// go has nothing to play.
    pub fn play_attachment(&mut self, attachment: &AttachmentSummary) {
        if attachment.expired || player_file(attachment).is_none() {
            return;
        }
        if !self.playback.inline {
            self.session.open_attachment(attachment);
            return;
        }
        self.follow_conversation();
        let id = attachment.attachment_id.clone();
        self.playback.pause_others(&id);
        self.playback.wanted = Some(id.clone());
        match self.playback.clips.get(&id).map(|clip| &clip.stage) {
            Some(Stage::Ready(player)) => player.play(),
            Some(Stage::Fetching) => {}
            Some(Stage::Failed(_)) | None => {
                self.playback.clips.insert(
                    id,
                    Clip {
                        attachment: attachment.clone(),
                        stage: Stage::Fetching,
                        file: None,
                    },
                );
                self.session.write_for_inline(attachment);
            }
        }
        self.session.revision += 1;
    }

    /// The play button of a clip's controls: it pauses while it plays, and plays (pausing
    /// any other) while it does not.
    pub fn toggle_clip(&mut self, attachment_id: &str) {
        let Some(clip) = self.playback.clips.get(attachment_id) else {
            return;
        };
        if let Some(player) = clip.player().filter(|p| p.is_playing()) {
            player.pause();
            self.session.revision += 1;
            return;
        }
        let attachment = clip.attachment.clone();
        self.play_attachment(&attachment);
    }

    /// The seek bar pressed or dragged at `fraction` of its length.
    pub fn seek_clip(&mut self, attachment_id: &str, fraction: f32) {
        let player = self
            .playback
            .clips
            .get(attachment_id)
            .and_then(Clip::player);
        if let Some((player, duration)) = player.and_then(|p| Some((p, p.duration()?))) {
            player.seek(duration * f64::from(fraction.clamp(0.0, 1.0)));
            self.session.revision += 1;
        }
    }

    /// The seek bar's keys: `steps` of [`STEP_SECONDS`] on, or back when below 0.
    pub fn step_clip(&mut self, attachment_id: &str, steps: i32) {
        let player = self
            .playback
            .clips
            .get(attachment_id)
            .and_then(Clip::player);
        if let Some(player) = player {
            player.seek((player.time() + f64::from(steps) * STEP_SECONDS).max(0.0));
            self.session.revision += 1;
        }
    }

    /// The seek bar's Home and End.
    pub fn clip_to_end(&mut self, attachment_id: &str, end: bool) {
        self.seek_clip(attachment_id, if end { 1.0 } else { 0.0 });
    }

    /// The sound button: muted, or not.
    pub fn toggle_clip_sound(&mut self, attachment_id: &str) {
        if let Some(player) = self
            .playback
            .clips
            .get(attachment_id)
            .and_then(Clip::player)
        {
            player.set_muted(!player.muted());
            self.session.revision += 1;
        }
    }

    /// A video's full-screen button: the video over the whole window, and the window full
    /// screen.
    pub fn enter_full_screen(&mut self, attachment_id: &str) {
        let ready = self
            .playback
            .clips
            .get(attachment_id)
            .is_some_and(|clip| clip.is_video() && clip.player().is_some());
        if self.playback.full_screen.is_some() || !ready {
            return;
        }
        self.playback.full_screen = Some(attachment_id.to_string());
        self.playback.toggle_owed ^= true;
        self.session.revision += 1;
    }

    /// Escape or the full-screen video's button: back in its message, the window as it was.
    pub fn exit_full_screen(&mut self) {
        if self.playback.full_screen.take().is_some() {
            self.playback.toggle_owed ^= true;
            self.session.revision += 1;
        }
    }

    /// Keys on the full-screen video: Space plays or pauses, Left and Right move it
    /// [`STEP_SECONDS`], M mutes. Escape is its modal's.
    pub fn full_screen_key(&mut self, key: &KeyEvent) -> bool {
        let Some(id) = self.playback.full_screen.clone() else {
            return false;
        };
        match &key.key {
            Key::Text(t) if t == " " => self.toggle_clip(&id),
            Key::Text(t) if t.eq_ignore_ascii_case("m") => self.toggle_clip_sound(&id),
            Key::Left => self.step_clip(&id, -1),
            Key::Right => self.step_clip(&id, 1),
            _ => return false,
        }
        true
    }

    /// The list's rows `visible` are in its view: a video whose message is not pauses.
    /// True when one did.
    pub fn rows_visible(&mut self, visible: &Range<usize>) -> bool {
        let mut paused = false;
        for clip in self.playback.clips.values() {
            if !clip.is_video() || !clip.playing() {
                continue;
            }
            let id = clip.attachment.attachment_id.as_str();
            let row = self
                .session
                .entries
                .iter()
                .position(|e| e.attachments.iter().any(|a| a.attachment_id == id));
            if row.is_none_or(|row| !visible.contains(&row))
                && self.playback.full_screen.as_deref() != Some(id)
                && let Some(player) = clip.player()
            {
                player.pause();
                paused = true;
            }
        }
        paused
    }

    /// The clips belong to the conversation on screen: another, or none, ends them.
    fn follow_conversation(&mut self) -> bool {
        let now = self.session.conversation.as_ref();
        if self.playback.conversation.as_ref() == now {
            return false;
        }
        self.playback.conversation = now.cloned();
        self.playback.forget()
    }

    /// On the way out of the shell: every clip stops and goes.
    pub(crate) fn forget_clips(&mut self) {
        self.playback.forget();
        self.playback.conversation = None;
    }

    /// What the passing of time does for the clips: they end with their conversation, and
    /// while one plays the view runs again each quarter second for its time and its bar.
    /// True when that changed what is on screen.
    pub(crate) fn tick_playback(&mut self) -> bool {
        let ended = self.follow_conversation();
        let now = self.playback.playing().and_then(|id| {
            let player = self.playback.clips.get(id)?.player()?;
            #[allow(clippy::cast_possible_truncation)]
            let quarter = (player.time() * 4.0).floor() as i64;
            Some((id.to_string(), quarter))
        });
        let moved = now.is_some() && now != self.playback.shown;
        self.playback.shown = now;
        if ended || moved {
            self.session.revision += 1;
        }
        ended || moved
    }

    /// While a clip plays the window is woken for its time; the window is taken into or out
    /// of full screen as the video was.
    pub(crate) fn poll_playback(&mut self, tx: &mut Tx) {
        if self.playback.playing().is_some() {
            tx.wake_after(std::time::Duration::from_millis(250));
        }
        if std::mem::take(&mut self.playback.toggle_owed) {
            tx.toggle_full_screen();
        }
    }

    /// A clip's file was written, or could not be: it is ready, and plays when it is still
    /// the one pressed last. One no longer held (its conversation ended) is removed.
    pub(crate) fn react_playback(&mut self, update: &MediaUpdate) {
        let MediaUpdate::Written {
            attachment_id,
            file,
        } = update
        else {
            return;
        };
        let wanted = self.playback.wanted.as_deref() == Some(attachment_id);
        let waker = self.playback.waker.clone();
        let Some(clip) = self
            .playback
            .clips
            .get_mut(attachment_id)
            .filter(|clip| matches!(clip.stage, Stage::Fetching))
        else {
            if let Ok(file) = file
                && !self.playback.clips.contains_key(attachment_id)
            {
                let _ = std::fs::remove_file(file);
            }
            return;
        };
        match file {
            Ok(file) => {
                let player = if clip.is_video() {
                    let player = Player::file(file.clone());
                    // Rune starts a video muted; one a person pressed plays with its sound.
                    player.set_muted(false);
                    player
                } else {
                    Player::audio(file.clone())
                };
                if let Some(waker) = waker {
                    player.set_waker(waker);
                }
                if wanted {
                    player.play();
                }
                clip.stage = Stage::Ready(player);
                clip.file = Some(file.clone());
            }
            Err(failure) => {
                clip.stage = Stage::Failed(format!("Could not fetch it: {}", failure.message));
            }
        }
        self.session.revision += 1;
    }
}

/// A clip's time as its controls say it: `1:05`, or `1:02:05` past an hour.
pub fn clock(seconds: f64) -> String {
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let whole = seconds.max(0.0).floor() as u64;
    let (h, m, s) = (whole / 3600, whole / 60 % 60, whole % 60);
    if h > 0 {
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{m}:{s:02}")
    }
}

#[cfg(test)]
mod tests {
    use super::clock;

    #[test]
    fn a_clips_time_reads_as_a_player_says_it() {
        assert_eq!(clock(0.0), "0:00");
        assert_eq!(clock(65.9), "1:05");
        assert_eq!(clock(3725.0), "1:02:05");
        assert_eq!(clock(-3.0), "0:00");
    }
}
