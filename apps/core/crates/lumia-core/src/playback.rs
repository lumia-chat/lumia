//! One thing playing at a time (docs/specs/media.md 5 and 6.6): what starting an attached
//! video or audio, or pressing a card's embed, stops; and what stops when its row scrolls
//! away. Rules only, with no player: each client keeps what is playing and carries out a
//! stop as it can (an attachment pauses; an embed goes back to its card, since no site's
//! player can be paused from outside it). Leaving the conversation stops everything.
//! `proto/vectors/playback/cases.json` holds the cases every client agrees on.

/// What is playing or shown.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Playing {
    /// A site's embed in its card's place, rather than an attachment.
    pub embed: bool,
    pub kind: Kind,
}

/// What it is: an attachment is a video or audio; an embed a player (video), audio or a post.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Kind {
    Video,
    Audio,
    Post,
}

impl Kind {
    /// `video`, `audio` or `post`; an embed's `player` is a video.
    #[must_use]
    pub fn from_id(id: &str) -> Option<Self> {
        match id {
            "video" | "player" => Some(Self::Video),
            "audio" => Some(Self::Audio),
            "post" => Some(Self::Post),
            _ => None,
        }
    }
}

/// Whether `starting` stops `other`, something else already playing or shown. A card
/// pressed puts back every other embed (one is shown at a time) and pauses attached video and
/// audio. An attachment started pauses every other attachment and puts back an embed that
/// plays (a player or audio), and leaves a post being read where it is.
#[must_use]
pub fn stops(starting: Playing, other: Playing) -> bool {
    starting.embed || !other.embed || other.kind != Kind::Post
}

/// Whether `p` stops when its row scrolls away: a video does (an attachment pauses, an
/// embed's player goes back to its card); audio plays on until another starts or the
/// conversation is left, and a post stays shown.
#[must_use]
pub fn stops_off_screen(p: Playing) -> bool {
    p.kind == Kind::Video
}
