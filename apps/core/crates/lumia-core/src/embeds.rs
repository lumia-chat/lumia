//! Embeds (docs/specs/media.md 6.6): which links in a message play or show inside the
//! conversation, and what each loads on a press.
//!
//! The embed comes from the link, never from the card: [`embed_for`] reads a link as the
//! text holds it, matches it against one pattern per site, and builds the address of the
//! site's own embed from the parts it validated. Nothing from the link is copied into the
//! address unchecked, so an odd link gives no embed rather than an odd embed. A forged card
//! cannot swap in another player, since the card is not consulted at all.
//!
//! No I/O. A Bluesky link that names its account by handle needs the account's DID first
//! ([`Source::BlueskyHandle`]); the client resolves it on the press and builds the address
//! with [`bluesky_embed_url`]. `proto/vectors/embeds/` holds the cases every client agrees on.

use url::Url;

/// The tallest a player is drawn, as pictures are (media spec 5).
pub const MAX_PLAYER_HEIGHT: u32 = 400;
/// The widest a post is drawn.
pub const MAX_POST_WIDTH: u32 = 550;
/// The height a post that grows to its content holds until its embed reports one.
pub const POST_START_HEIGHT: u32 = 250;
/// The shortest and the tallest a post that grows is drawn, whatever its embed reports
/// ([`reported_height`]).
pub const REPORTED_MIN: u32 = 50;
pub const REPORTED_MAX: u32 = 5000;
/// The longest message from an embed read for a height: a resize is a few dozen bytes.
pub const REPORTED_MESSAGE_MAX: usize = 8192;

/// The origins an embed is loaded from: the web's `frame-src`, and what the phones' web
/// views may load. Every address [`embed_for`] and [`bluesky_embed_url`] give is on one.
pub const EMBED_ORIGINS: &[&str] = &[
    "https://www.youtube-nocookie.com",
    "https://player.vimeo.com",
    "https://player.twitch.tv",
    "https://clips.twitch.tv",
    "https://w.soundcloud.com",
    "https://open.spotify.com",
    "https://platform.twitter.com",
    "https://embed.bsky.app",
    "https://www.facebook.com",
    "https://www.instagram.com",
    "https://www.tiktok.com",
    "https://embed.reddit.com",
];

/// [`EMBED_ORIGINS`].
#[must_use]
pub fn embed_origins() -> &'static [&'static str] {
    EMBED_ORIGINS
}

/// The site an embed is from.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Provider {
    YouTube,
    Vimeo,
    Twitch,
    SoundCloud,
    Spotify,
    X,
    Bluesky,
    Facebook,
    Instagram,
    TikTok,
    Reddit,
}

impl Provider {
    /// Every site, in the spec's order.
    pub const ALL: [Self; 11] = [
        Self::YouTube,
        Self::Vimeo,
        Self::Twitch,
        Self::SoundCloud,
        Self::Spotify,
        Self::X,
        Self::Bluesky,
        Self::Facebook,
        Self::Instagram,
        Self::TikTok,
        Self::Reddit,
    ];

    /// A stable lowercase name for code: `youtube`, `soundcloud`, `x`.
    #[must_use]
    pub fn id(self) -> &'static str {
        match self {
            Self::YouTube => "youtube",
            Self::Vimeo => "vimeo",
            Self::Twitch => "twitch",
            Self::SoundCloud => "soundcloud",
            Self::Spotify => "spotify",
            Self::X => "x",
            Self::Bluesky => "bluesky",
            Self::Facebook => "facebook",
            Self::Instagram => "instagram",
            Self::TikTok => "tiktok",
            Self::Reddit => "reddit",
        }
    }

    /// The site [`Provider::id`] names.
    #[must_use]
    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|p| p.id() == id)
    }

    /// The site's name as a person reads it: `YouTube`, `SoundCloud`, `X`.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::YouTube => "YouTube",
            Self::Vimeo => "Vimeo",
            Self::Twitch => "Twitch",
            Self::SoundCloud => "SoundCloud",
            Self::Spotify => "Spotify",
            Self::X => "X",
            Self::Bluesky => "Bluesky",
            Self::Facebook => "Facebook",
            Self::Instagram => "Instagram",
            Self::TikTok => "TikTok",
            Self::Reddit => "Reddit",
        }
    }
}

/// What pressing the card does.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Kind {
    /// Plays a video: put back to its card when its row scrolls away or the conversation
    /// changes.
    Player,
    /// Plays audio: goes on until another starts or the conversation is left.
    Audio,
    /// Shows a post.
    Post,
}

impl Kind {
    /// `player`, `audio` or `post`.
    #[must_use]
    pub fn id(self) -> &'static str {
        match self {
            Self::Player => "player",
            Self::Audio => "audio",
            Self::Post => "post",
        }
    }
}

/// How big an embed is drawn.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Shape {
    /// At the column's width at most, at this shape, at most [`MAX_PLAYER_HEIGHT`] tall.
    Ratio { width: u32, height: u32 },
    /// This many points tall at the column's width (a post's at most [`MAX_POST_WIDTH`]);
    /// what does not fit scrolls inside the embed.
    Fixed { height: u32 },
    /// Starts this tall and grows to the height the embed reports by `postMessage`.
    Resizes { height: u32 },
}

/// What a press loads.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Source {
    /// The embed's address.
    Url(String),
    /// A Bluesky post named by its account's handle: the client resolves the handle to a
    /// DID on the press (`com.atproto.identity.resolveHandle`) and loads
    /// [`bluesky_embed_url`] of the two. The handle is lowercase.
    BlueskyHandle { handle: String, rkey: String },
}

/// A link's embed.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Embed {
    pub provider: Provider,
    pub kind: Kind,
    pub source: Source,
    pub shape: Shape,
    /// Twitch refuses an embed whose address does not name the page holding it: the client
    /// appends `&parent=<its host>` (the instance's host on the web and the phones) to the
    /// address before loading it. Only Twitch's are marked.
    pub needs_parent: bool,
}

impl Embed {
    /// The press's accessible name, to which a client adds the card's title after a
    /// comma: `Play video from YouTube`, `Play audio from SoundCloud`, `Show post from X`.
    #[must_use]
    pub fn label(&self) -> String {
        let verb = match self.kind {
            Kind::Player => "Play video",
            Kind::Audio => "Play audio",
            Kind::Post => "Show post",
        };
        format!("{verb} from {}", self.provider.name())
    }

    /// The widest the embed is drawn: [`MAX_POST_WIDTH`] for a post, none (the column's
    /// width) otherwise.
    #[must_use]
    pub fn max_width(&self) -> Option<u32> {
        (self.kind == Kind::Post).then_some(MAX_POST_WIDTH)
    }
}

/// The embed of a link, or none. `link` is a link as the message text holds it (one of
/// [`crate::cards::card_links`], or any link the text draws); only `http` and `https`
/// addresses on a site's own hosts, at the default port and with no user name, match.
/// Anything a pattern does not name exactly (a profile, a search, a short link that only
/// the site can follow, a path with more after it than the pattern allows) gives none.
#[must_use]
pub fn embed_for(link: &str) -> Option<Embed> {
    let url = Url::parse(link).ok()?;
    if !matches!(url.scheme(), "http" | "https")
        || !url.username().is_empty()
        || url.password().is_some()
        || url.port().is_some()
    {
        return None;
    }
    let host = match url.host()? {
        url::Host::Domain(d) => d.to_string(),
        _ => return None,
    };
    let segs: Vec<&str> = url
        .path_segments()
        .map(|s| s.filter(|p| !p.is_empty()).collect())
        .unwrap_or_default();
    match host.as_str() {
        "youtube.com" | "www.youtube.com" | "m.youtube.com" => youtube(&url, &segs, true),
        "music.youtube.com" => match segs.as_slice() {
            ["watch"] => youtube(&url, &segs, false),
            _ => None,
        },
        "youtube-nocookie.com" | "www.youtube-nocookie.com" => match segs.as_slice() {
            ["embed", _] => youtube(&url, &segs, true),
            _ => None,
        },
        "youtu.be" => youtu_be(&url, &segs),
        "vimeo.com" | "www.vimeo.com" => vimeo(&segs),
        "player.vimeo.com" => vimeo_player(&url, &segs),
        "twitch.tv" | "www.twitch.tv" | "m.twitch.tv" => twitch(&segs),
        "clips.twitch.tv" => match segs.as_slice() {
            [slug] if *slug != "embed" => twitch_clip(slug),
            _ => None,
        },
        "soundcloud.com" | "www.soundcloud.com" | "m.soundcloud.com" => soundcloud(&segs),
        "open.spotify.com" => spotify(&segs),
        "x.com" | "www.x.com" | "mobile.x.com" | "twitter.com" | "www.twitter.com"
        | "mobile.twitter.com" => x(&segs),
        "bsky.app" => bluesky(&segs),
        "facebook.com" | "www.facebook.com" | "m.facebook.com" | "mobile.facebook.com" => {
            facebook(&url, &segs)
        }
        "fb.watch" => match segs.as_slice() {
            [code] if word(code, 1, 32) => {
                Some(facebook_video(&format!("https://fb.watch/{code}/")))
            }
            _ => None,
        },
        "instagram.com" | "www.instagram.com" => instagram(&segs),
        "tiktok.com" | "www.tiktok.com" | "m.tiktok.com" => tiktok(&segs),
        "reddit.com" | "www.reddit.com" | "old.reddit.com" | "new.reddit.com" | "m.reddit.com" => {
            reddit(&segs)
        }
        _ => None,
    }
}

/// The address of a Bluesky post's embed, from its account's DID and its record key; none
/// when either is not one (a DID from a handle's resolution is checked here too).
#[must_use]
pub fn bluesky_embed_url(did: &str, rkey: &str) -> Option<String> {
    (is_did(did) && is_tid(rkey))
        .then(|| format!("https://embed.bsky.app/embed/{did}/app.bsky.feed.post/{rkey}"))
}

/// Whether `handle` is a Bluesky handle as the AT Protocol defines one: a domain name of at
/// least two labels, its last not starting with a digit.
#[must_use]
pub fn is_bluesky_handle(handle: &str) -> bool {
    if handle.len() > 253 {
        return false;
    }
    let labels: Vec<&str> = handle.split('.').collect();
    labels.len() >= 2
        && labels.iter().all(|l| {
            (1..=63).contains(&l.len())
                && l.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
                && !l.starts_with('-')
                && !l.ends_with('-')
        })
        && labels
            .last()
            .is_some_and(|l| l.as_bytes()[0].is_ascii_alphabetic())
}

/// Whether `did` is a DID a Bluesky embed accepts: `did:plc:` and its 24 base32 letters,
/// or `did:web:` and a lowercase host name.
#[must_use]
pub fn is_did(did: &str) -> bool {
    if let Some(id) = did.strip_prefix("did:plc:") {
        id.len() == 24 && id.bytes().all(|b| matches!(b, b'a'..=b'z' | b'2'..=b'7'))
    } else if let Some(host) = did.strip_prefix("did:web:") {
        is_bluesky_handle(host) && !host.bytes().any(|b| b.is_ascii_uppercase())
    } else {
        false
    }
}

// ---- Sites ------------------------------------------------------------------------------

const VIDEO: Shape = Shape::Ratio {
    width: 16,
    height: 9,
};
const VERTICAL: Shape = Shape::Ratio {
    width: 9,
    height: 16,
};

fn embed(provider: Provider, kind: Kind, url: String, shape: Shape) -> Embed {
    Embed {
        provider,
        kind,
        source: Source::Url(url),
        shape,
        needs_parent: false,
    }
}

/// `youtube.com/watch?v=`, `/shorts/`, `/live/`, `/embed/`; the start from `t` or `start`.
fn youtube(url: &Url, segs: &[&str], paths: bool) -> Option<Embed> {
    let (id, shape) = match segs {
        ["watch"] => (one(url, "v")?, VIDEO),
        ["shorts", id] if paths => ((*id).to_string(), VERTICAL),
        ["live" | "embed", id] if paths => ((*id).to_string(), VIDEO),
        _ => return None,
    };
    youtube_embed(url, &id, shape)
}

fn youtu_be(url: &Url, segs: &[&str]) -> Option<Embed> {
    match segs {
        [id] => youtube_embed(url, id, VIDEO),
        _ => None,
    }
}

fn youtube_embed(url: &Url, id: &str, shape: Shape) -> Option<Embed> {
    if !(id.len() == 11 && word(id, 11, 11)) {
        return None;
    }
    let start = one(url, "t")
        .and_then(|t| seconds(&t))
        .or_else(|| one(url, "start").and_then(|s| digits(&s, 6).then(|| s.parse().ok())?))
        .filter(|s: &u32| *s > 0);
    let mut address = format!("https://www.youtube-nocookie.com/embed/{id}?autoplay=1");
    if let Some(s) = start {
        address.push_str(&format!("&start={s}"));
    }
    Some(embed(Provider::YouTube, Kind::Player, address, shape))
}

/// A YouTube time: plain seconds (`90`, `90s`) or hours, minutes and seconds in that order,
/// each at most once (`1h2m3s`, `2m`).
fn seconds(t: &str) -> Option<u32> {
    if digits(t, 6) {
        return t.parse().ok();
    }
    if t.is_empty() {
        return None;
    }
    let mut total: u32 = 0;
    let mut rest = t;
    let mut units = "hms";
    while !rest.is_empty() {
        let n = rest.bytes().take_while(u8::is_ascii_digit).count();
        if n == 0 || n > 6 {
            return None;
        }
        let value: u32 = rest[..n].parse().ok()?;
        let unit = rest[n..].chars().next()?;
        let at = units.find(unit)?;
        units = &units[at + 1..];
        let scale = match unit {
            'h' => 3600,
            'm' => 60,
            _ => 1,
        };
        total = total.checked_add(value.checked_mul(scale)?)?;
        rest = &rest[n + 1..];
    }
    Some(total)
}

/// `vimeo.com/<id>`, `/<id>/<hash>` (unlisted), `/channels/<name>/<id>`,
/// `/groups/<name>/videos/<id>`.
fn vimeo(segs: &[&str]) -> Option<Embed> {
    let (id, hash) = match segs {
        [id] => (*id, None),
        [id, hash] => (*id, Some(*hash)),
        ["channels", name, id] if word(name, 1, 64) => (*id, None),
        ["groups", name, "videos", id] if word(name, 1, 64) => (*id, None),
        _ => return None,
    };
    vimeo_embed(id, hash)
}

/// `player.vimeo.com/video/<id>`, with `h` when unlisted.
fn vimeo_player(url: &Url, segs: &[&str]) -> Option<Embed> {
    match segs {
        ["video", id] => vimeo_embed(id, one(url, "h").as_deref()),
        _ => None,
    }
}

fn vimeo_embed(id: &str, hash: Option<&str>) -> Option<Embed> {
    if !digits(id, 12) {
        return None;
    }
    let mut address = format!("https://player.vimeo.com/video/{id}?autoplay=1");
    if let Some(h) = hash {
        if !((6..=32).contains(&h.len())
            && h.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f')))
        {
            return None;
        }
        address.push_str(&format!("&h={h}"));
    }
    Some(embed(Provider::Vimeo, Kind::Player, address, VIDEO))
}

/// `twitch.tv/videos/<id>` and `twitch.tv/<channel>/clip/<slug>`.
fn twitch(segs: &[&str]) -> Option<Embed> {
    match segs {
        ["videos", id] if digits(id, 12) => Some(Embed {
            needs_parent: true,
            ..embed(
                Provider::Twitch,
                Kind::Player,
                format!("https://player.twitch.tv/?video={id}"),
                VIDEO,
            )
        }),
        [channel, "clip", slug] if is_twitch_channel(channel) => twitch_clip(slug),
        _ => None,
    }
}

fn is_twitch_channel(name: &str) -> bool {
    (1..=25).contains(&name.len()) && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
}

fn twitch_clip(slug: &str) -> Option<Embed> {
    word(slug, 1, 100).then(|| Embed {
        needs_parent: true,
        ..embed(
            Provider::Twitch,
            Kind::Player,
            format!("https://clips.twitch.tv/embed?clip={slug}"),
            VIDEO,
        )
    })
}

/// First path segments of soundcloud.com that are not an account.
const SOUNDCLOUD_RESERVED: &[&str] = &[
    "discover",
    "search",
    "stream",
    "upload",
    "you",
    "charts",
    "settings",
    "messages",
    "notifications",
    "pages",
    "terms-of-use",
    "mobile",
    "pro",
    "jobs",
    "imprint",
    "people",
    "tags",
    "popular",
    "home",
    "logout",
    "signin",
    "signup",
    "connect",
    "feed",
    "player",
    "oembed",
];

/// Second path segments of an account's page that are not a track.
const SOUNDCLOUD_TABS: &[&str] = &[
    "tracks",
    "albums",
    "sets",
    "reposts",
    "likes",
    "followers",
    "following",
    "popular-tracks",
    "comments",
    "spotlight",
];

/// `soundcloud.com/<account>/<track>` and `/<account>/sets/<playlist>`, either with a
/// private link's `s-…` token after it.
fn soundcloud(segs: &[&str]) -> Option<Embed> {
    let account = *segs.first()?;
    if !word(account, 1, 64) || SOUNDCLOUD_RESERVED.contains(&account) {
        return None;
    }
    let (path, height) = match &segs[1..] {
        ["sets", set] if word(set, 1, 255) => (format!("{account}/sets/{set}"), 450),
        ["sets", set, secret] if word(set, 1, 255) && is_soundcloud_secret(secret) => {
            (format!("{account}/sets/{set}/{secret}"), 450)
        }
        [track] if word(track, 1, 255) && !SOUNDCLOUD_TABS.contains(track) => {
            (format!("{account}/{track}"), 166)
        }
        [track, secret]
            if word(track, 1, 255)
                && !SOUNDCLOUD_TABS.contains(track)
                && is_soundcloud_secret(secret) =>
        {
            (format!("{account}/{track}/{secret}"), 166)
        }
        _ => return None,
    };
    let track = encode(&format!("https://soundcloud.com/{path}"));
    Some(embed(
        Provider::SoundCloud,
        Kind::Audio,
        format!("https://w.soundcloud.com/player/?url={track}&auto_play=true"),
        Shape::Fixed { height },
    ))
}

fn is_soundcloud_secret(s: &str) -> bool {
    s.strip_prefix("s-").is_some_and(|t| {
        (1..=32).contains(&t.len()) && t.bytes().all(|b| b.is_ascii_alphanumeric())
    })
}

/// `open.spotify.com/<type>/<id>`, optionally after `intl-<locale>/` or `embed/`.
fn spotify(segs: &[&str]) -> Option<Embed> {
    let segs = match segs {
        [first, rest @ ..] if *first == "embed" || is_spotify_locale(first) => rest,
        _ => segs,
    };
    let [kind, id] = segs else {
        return None;
    };
    let height = match *kind {
        "track" | "episode" => 152,
        "album" | "playlist" | "show" | "artist" => 352,
        _ => return None,
    };
    if !(id.len() == 22 && id.bytes().all(|b| b.is_ascii_alphanumeric())) {
        return None;
    }
    Some(embed(
        Provider::Spotify,
        Kind::Audio,
        format!("https://open.spotify.com/embed/{kind}/{id}"),
        Shape::Fixed { height },
    ))
}

/// `intl-de`, `intl-pt-br`.
fn is_spotify_locale(s: &str) -> bool {
    s.strip_prefix("intl-").is_some_and(|l| {
        let parts: Vec<&str> = l.split('-').collect();
        (1..=2).contains(&parts.len())
            && parts
                .iter()
                .all(|p| p.len() == 2 && p.bytes().all(|b| b.is_ascii_alphabetic()))
    })
}

/// `x.com/<account>/status/<id>` (twitter.com alike, `statuses` too), optionally followed
/// by `photo/<n>` or `video/<n>`; `x.com/i/status/<id>` and `/i/web/status/<id>`.
fn x(segs: &[&str]) -> Option<Embed> {
    let id = match segs {
        ["i", "status", id] | ["i", "web", "status", id] => *id,
        [account, "status" | "statuses", id, rest @ ..] if is_x_account(account) => match rest {
            [] => *id,
            ["photo" | "video", n] if digits(n, 1) => *id,
            _ => return None,
        },
        _ => return None,
    };
    digits(id, 20).then(|| {
        embed(
            Provider::X,
            Kind::Post,
            format!("https://platform.twitter.com/embed/Tweet.html?id={id}&dnt=true"),
            Shape::Resizes {
                height: POST_START_HEIGHT,
            },
        )
    })
}

fn is_x_account(name: &str) -> bool {
    name != "i"
        && (1..=15).contains(&name.len())
        && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
}

/// `bsky.app/profile/<did or handle>/post/<rkey>`.
fn bluesky(segs: &[&str]) -> Option<Embed> {
    let ["profile", actor, "post", rkey] = segs else {
        return None;
    };
    let shape = Shape::Resizes {
        height: POST_START_HEIGHT,
    };
    let source = if actor.starts_with("did:") {
        Source::Url(bluesky_embed_url(actor, rkey)?)
    } else if is_bluesky_handle(actor) && is_tid(rkey) {
        Source::BlueskyHandle {
            handle: actor.to_ascii_lowercase(),
            rkey: (*rkey).to_string(),
        }
    } else {
        return None;
    };
    Some(Embed {
        provider: Provider::Bluesky,
        kind: Kind::Post,
        source,
        shape,
        needs_parent: false,
    })
}

/// A record key that is a TID, as every post's is: 13 base32 characters, the first at most
/// `j`.
fn is_tid(s: &str) -> bool {
    let b = s.as_bytes();
    b.len() == 13
        && matches!(b[0], b'2'..=b'7' | b'a'..=b'j')
        && b.iter().all(|c| matches!(c, b'2'..=b'7' | b'a'..=b'z'))
}

/// First path segments of facebook.com that are not a page or a person.
const FACEBOOK_RESERVED: &[&str] = &[
    "watch",
    "groups",
    "events",
    "share",
    "reel",
    "reels",
    "stories",
    "story.php",
    "permalink.php",
    "profile.php",
    "photo",
    "photo.php",
    "photos",
    "video",
    "video.php",
    "marketplace",
    "gaming",
    "pages",
    "login",
    "help",
    "settings",
    "hashtag",
    "notifications",
    "messages",
    "friends",
    "people",
    "search",
    "plugins",
    "sharer",
    "sharer.php",
    "dialog",
    "l.php",
];

/// `facebook.com/<page>/videos/[<slug>/]<id>`, `/watch?v=<id>`, `/<page>/posts/<id>`,
/// `/permalink.php?story_fbid=<id>&id=<page id>`. The plugin is handed the link rebuilt from
/// the parts checked, never the link as written.
fn facebook(url: &Url, segs: &[&str]) -> Option<Embed> {
    match segs {
        ["watch"] => {
            let v = one(url, "v")?;
            digits(&v, 20)
                .then(|| facebook_video(&format!("https://www.facebook.com/watch/?v={v}")))
        }
        [page, "videos", id] | [page, "videos", _, id]
            if is_facebook_page(page) && digits(id, 20) =>
        {
            Some(facebook_video(&format!(
                "https://www.facebook.com/{page}/videos/{id}/"
            )))
        }
        [page, "posts", id] if is_facebook_page(page) && is_facebook_post_id(id) => Some(
            facebook_post(&format!("https://www.facebook.com/{page}/posts/{id}/")),
        ),
        ["permalink.php"] => {
            let story = one(url, "story_fbid")?;
            let page = one(url, "id")?;
            (is_facebook_post_id(&story) && digits(&page, 20)).then(|| {
                facebook_post(&format!(
                    "https://www.facebook.com/permalink.php?story_fbid={story}&id={page}"
                ))
            })
        }
        _ => None,
    }
}

fn is_facebook_page(name: &str) -> bool {
    (1..=80).contains(&name.len())
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'.' || b == b'-')
        && !FACEBOOK_RESERVED.contains(&name)
}

/// A post's id: digits, or `pfbid` and letters and digits.
fn is_facebook_post_id(id: &str) -> bool {
    digits(id, 20)
        || id.strip_prefix("pfbid").is_some_and(|t| {
            (1..=100).contains(&t.len()) && t.bytes().all(|b| b.is_ascii_alphanumeric())
        })
}

fn facebook_video(href: &str) -> Embed {
    embed(
        Provider::Facebook,
        Kind::Player,
        format!(
            "https://www.facebook.com/plugins/video.php?href={}",
            encode(href)
        ),
        VIDEO,
    )
}

fn facebook_post(href: &str) -> Embed {
    embed(
        Provider::Facebook,
        Kind::Post,
        format!(
            "https://www.facebook.com/plugins/post.php?href={}",
            encode(href)
        ),
        Shape::Fixed { height: 500 },
    )
}

/// First path segments of instagram.com that are not an account.
const INSTAGRAM_RESERVED: &[&str] = &[
    "p", "reel", "reels", "tv", "stories", "explore", "accounts", "direct", "about", "legal",
];

/// `instagram.com/p/<code>`, `/reel/<code>`, `/reels/<code>` and `/tv/<code>`; and `/p/` and
/// `/reel/` after an account's name (`/<account>/p/<code>`), the two forms Instagram's own
/// links take there.
fn instagram(segs: &[&str]) -> Option<Embed> {
    let (kind, code) = match segs {
        [kind, code] => (*kind, *code),
        [account, kind @ ("p" | "reel"), code]
            if (1..=30).contains(&account.len())
                && account
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'.' || b == b'_')
                && !INSTAGRAM_RESERVED.contains(account) =>
        {
            (*kind, *code)
        }
        _ => return None,
    };
    let path = match kind {
        "p" | "tv" => "p",
        "reel" | "reels" => "reel",
        _ => return None,
    };
    word(code, 5, 64).then(|| {
        embed(
            Provider::Instagram,
            Kind::Post,
            format!("https://www.instagram.com/{path}/{code}/embed"),
            Shape::Fixed { height: 640 },
        )
    })
}

/// `tiktok.com/@<account>/video/<id>`, and the embeds' own `/embed/v2/<id>` and
/// `/player/v1/<id>`.
fn tiktok(segs: &[&str]) -> Option<Embed> {
    let id = match segs {
        [account, "video", id] if is_tiktok_account(account) => *id,
        ["embed", "v2", id] | ["player", "v1", id] => *id,
        _ => return None,
    };
    digits(id, 20).then(|| {
        embed(
            Provider::TikTok,
            Kind::Player,
            format!("https://www.tiktok.com/player/v1/{id}?autoplay=1"),
            VERTICAL,
        )
    })
}

fn is_tiktok_account(name: &str) -> bool {
    name.strip_prefix('@').is_some_and(|n| {
        (1..=24).contains(&n.len())
            && n.bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'.' || b == b'_')
    })
}

/// `reddit.com/r/<subreddit>/comments/<id>[/<slug>]`. A link to a comment has more after
/// the slug and gives none.
fn reddit(segs: &[&str]) -> Option<Embed> {
    let (sub, id) = match segs {
        ["r", sub, "comments", id] | ["r", sub, "comments", id, _] => (*sub, *id),
        _ => return None,
    };
    let sub_ok = (2..=21).contains(&sub.len())
        && sub.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_');
    let id_ok =
        (1..=12).contains(&id.len()) && id.bytes().all(|b| matches!(b, b'a'..=b'z' | b'0'..=b'9'));
    (sub_ok && id_ok).then(|| {
        embed(
            Provider::Reddit,
            Kind::Post,
            format!("https://embed.reddit.com/r/{sub}/comments/{id}/"),
            Shape::Resizes {
                height: POST_START_HEIGHT,
            },
        )
    })
}

// ---- Pieces -----------------------------------------------------------------------------

/// The value of the query parameter `key` when the link has exactly one.
fn one(url: &Url, key: &str) -> Option<String> {
    let mut values = url.query_pairs().filter(|(k, _)| k == key).map(|(_, v)| v);
    let value = values.next()?;
    values.next().is_none().then(|| value.into_owned())
}

/// 1 to `max` ASCII digits.
fn digits(s: &str, max: usize) -> bool {
    (1..=max).contains(&s.len()) && s.bytes().all(|b| b.is_ascii_digit())
}

/// `min` to `max` of `[A-Za-z0-9_-]`.
fn word(s: &str, min: usize, max: usize) -> bool {
    (min..=max).contains(&s.len())
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}

/// `s` encoded as a query value.
fn encode(s: &str) -> String {
    url::form_urlencoded::byte_serialize(s.as_bytes()).collect()
}

/// The height a post's embed reported to the page holding it (media spec 6.6), in that
/// site's own form, rounded up and held to [`REPORTED_MIN`]..=[`REPORTED_MAX`]; none for
/// any other message or site. `message` is what the embed's window posted, as text: a
/// string as it was posted, an object as its JSON (`JSON.stringify`). The caller passes on
/// only messages from the embed's own window and origin; nothing in one is trusted to have
/// the shape its site gives it. `proto/vectors/embeds/heights.json` holds the cases.
///
/// - **X**: `platform.twitter.com/widgets.js` takes JSON-RPC under `"twttr.embed"`, as a
///   string that starts with it (the JSON after it) or an object with it as a key (its value
///   an object or JSON text), and reads `twttr.private.resize`'s `params[0].height`.
/// - **Bluesky**: `bskyembed`'s `{height, id}`, read by `embed.bsky.app`'s `embed.js` as
///   `data.height`.
/// - **Reddit**: `embed.reddit.com`'s `{"type": "resize.embed", "data": <height>}`.
#[must_use]
pub fn reported_height(provider: Provider, message: &str) -> Option<u32> {
    if message.len() > REPORTED_MESSAGE_MAX {
        return None;
    }
    let height = match provider {
        Provider::X => x_height(message),
        Provider::Bluesky => number(json(message)?.get("height")?),
        Provider::Reddit => {
            let m = json(message)?;
            (m.get("type")?.as_str()? == "resize.embed").then_some(())?;
            number(m.get("data")?)
        }
        _ => None,
    }?;
    (height.is_finite() && height > 0.0).then(|| {
        // Held in range before it becomes a whole number, so no height overflows one.
        let held = height
            .ceil()
            .clamp(f64::from(REPORTED_MIN), f64::from(REPORTED_MAX));
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let whole = held as u32;
        whole
    })
}

fn json(text: &str) -> Option<serde_json::Value> {
    serde_json::from_str(text).ok()
}

/// A height as a number, or as a string of digits with or without a fraction.
fn number(v: &serde_json::Value) -> Option<f64> {
    match v {
        serde_json::Value::Number(n) => n.as_f64(),
        serde_json::Value::String(s) => {
            let t = s.trim();
            let (whole, fraction) = t.split_once('.').unwrap_or((t, "0"));
            let digits = |part: &str| !part.is_empty() && part.bytes().all(|b| b.is_ascii_digit());
            (digits(whole) && digits(fraction)).then(|| t.parse().ok())?
        }
        _ => None,
    }
}

fn x_height(message: &str) -> Option<f64> {
    const KEY: &str = "twttr.embed";
    let rpc = match message.strip_prefix(KEY) {
        Some(rest) => json(rest)?,
        None => match json(message)?.get(KEY)? {
            serde_json::Value::String(inner) => json(inner)?,
            inner => inner.clone(),
        },
    };
    (rpc.get("method")?.as_str()? == "twttr.private.resize").then_some(())?;
    number(rpc.get("params")?.as_array()?.first()?.get("height")?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn times_read_as_youtube_writes_them() {
        assert_eq!(seconds("90"), Some(90));
        assert_eq!(seconds("90s"), Some(90));
        assert_eq!(seconds("1m30s"), Some(90));
        assert_eq!(seconds("1h2m3s"), Some(3723));
        assert_eq!(seconds("2m"), Some(120));
        assert_eq!(seconds("1s1m"), None);
        assert_eq!(seconds("1m1m"), None);
        assert_eq!(seconds("m"), None);
        assert_eq!(seconds(""), None);
        assert_eq!(seconds("1x"), None);
    }

    #[test]
    fn every_address_is_on_an_origin_listed() {
        for link in [
            "https://youtu.be/dQw4w9WgXcQ",
            "https://vimeo.com/76979871",
            "https://www.twitch.tv/videos/123456",
            "https://clips.twitch.tv/AwkwardHelplessSalamander",
            "https://soundcloud.com/forss/flickermood",
            "https://open.spotify.com/track/4uLU6hMCjMI75M1A2tKUQC",
            "https://x.com/jack/status/20",
            "https://bsky.app/profile/did:plc:z72i7hdynmk6r22z27h6tvur/post/3k2la3rsg5w2y",
            "https://www.facebook.com/watch/?v=123",
            "https://www.instagram.com/p/CxYz123abcD/",
            "https://www.tiktok.com/@scout2015/video/6718335390845095173",
            "https://www.reddit.com/r/rust/comments/1abcde/x/",
        ] {
            let Some(Embed {
                source: Source::Url(address),
                ..
            }) = embed_for(link)
            else {
                panic!("{link} has no embed address");
            };
            assert!(
                EMBED_ORIGINS
                    .iter()
                    .any(|o| address.starts_with(&format!("{o}/"))),
                "{address}"
            );
        }
    }

    #[test]
    fn every_site_is_named_once() {
        let mut ids: Vec<&str> = Provider::ALL.iter().map(|p| p.id()).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), Provider::ALL.len());
    }

    #[test]
    fn dids_and_handles() {
        assert!(is_did("did:plc:z72i7hdynmk6r22z27h6tvur"));
        assert!(is_did("did:web:example.com"));
        assert!(!is_did("did:plc:Z72i7hdynmk6r22z27h6tvur"));
        assert!(!is_did("did:plc:z72i7hdynmk6r22z27h6tvu"));
        assert!(!is_did("did:web:Example.com"));
        assert!(!is_did("did:web:example.com/x"));
        assert!(!is_did("did:key:z6Mk"));
        assert!(is_bluesky_handle("alice.bsky.social"));
        assert!(!is_bluesky_handle("alice"));
        assert!(!is_bluesky_handle("alice.123"));
        assert!(!is_bluesky_handle("-alice.bsky.social"));
        assert_eq!(
            bluesky_embed_url("did:plc:z72i7hdynmk6r22z27h6tvur", "x"),
            None
        );
    }
}
