// Inline video and audio, and embeds (specs/media.md 5 and 6.6), as the
// phone decides them before anything is drawn or loaded, as values the
// tests cover: what a conversation holds and which of it plays, with the
// press that came last (`PlaybackHeld`; what starting one stops and what
// stops off screen are the core's, `playbackStops` and
// `playbackStopsOffScreen`); how large an embed is drawn (`EmbedLayout`);
// the address it loads, the page that holds it and which navigations its
// web view lets through (`EmbedPage`, `EmbedNavigation`); and the words
// said of it (`EmbedWords`). Which links have an embed, the embed's
// address and the height a post reports are the core's (`embedFor`, asked
// once a link through `EmbedLookup`, and `reportedEmbedHeight`), from the
// link in the text, never the card.
import CoreGraphics
import Foundation
import LumiaFFI

// MARK: Playing

/// Something a conversation plays or shows on a press: an attached video
/// or recording, or a link's embed in its card's place.
public struct PlaybackSlot: Hashable, Sendable {
    public enum Kind: Sendable {
        /// An attached video: pauses when its row scrolls away.
        case video
        /// An attached recording: plays on while scrolling.
        case audio
        /// A site's player (`player`): back to its card when its row scrolls away.
        case embedPlayer
        /// A site's audio (`audio`): plays on while scrolling.
        case embedAudio
        /// A site's post (`post`): stays shown while scrolling and when an attachment starts.
        case embedPost

        public var isEmbed: Bool { self == .embedPlayer || self == .embedAudio || self == .embedPost }

        /// An embed's kind from the core's `kind`.
        public init(embed kind: String) {
            switch kind {
            case "player": self = .embedPlayer
            case "audio": self = .embedAudio
            default: self = .embedPost
            }
        }
    }

    public let key: String
    public let kind: Kind

    public init(key: String, kind: Kind) {
        self.key = key; self.kind = kind
    }

    /// What it is as the core's rules take it: an embed's player is a video.
    public var summary: PlayingSummary {
        switch kind {
        case .video: PlayingSummary(embed: false, kind: "video")
        case .audio: PlayingSummary(embed: false, kind: "audio")
        case .embedPlayer: PlayingSummary(embed: true, kind: "video")
        case .embedAudio: PlayingSummary(embed: true, kind: "audio")
        case .embedPost: PlayingSummary(embed: true, kind: "post")
        }
    }
}

/// What becomes of one when something else happens: paused where it is
/// (an attached video or recording keeps its player and its place), or
/// closed (an embed back to its card, since no site's player can be
/// paused from outside it; a player let go).
public enum PlaybackStop: Equatable, Sendable {
    case pause
    case close
}

/// What a conversation holds that plays or is shown, which of it plays,
/// and the press that came last (specs/media.md 5 and 6.6). The rules are
/// the core's and are only carried out here: `playbackStops` says what
/// starting one stops (a card pressed, everything else; an attachment
/// started, every other attachment and an embed that plays, a post left
/// shown) and `playbackStopsOffScreen` what stops when its row scrolls
/// away (a video; audio plays on and a post stays). An attachment that
/// stops is paused where it is; an embed goes back to its card. Leaving
/// the conversation closes everything.
public struct PlaybackHeld: Equatable, Sendable {
    /// What has a player or is shown, in the order it started.
    public private(set) var held: [PlaybackSlot] = []
    /// The one that started last and has not stopped, if any.
    public private(set) var playing: String?
    /// The press that came last, while what it asked for is on its way (a
    /// file fetched, an embed's address made): only that one starts when it
    /// arrives, so a slow fetch never takes over from a later press.
    public private(set) var wanted: String?

    public init() {}

    public func holds(_ key: String) -> Bool { held.contains { $0.key == key } }

    /// The embed shown, if any.
    public var embed: PlaybackSlot? { held.first { $0.kind.isEmbed } }

    /// `key` was pressed and is on its way.
    public mutating func want(_ key: String) {
        wanted = key
    }

    /// What `key`'s press asked for is here: true when it is still the one
    /// wanted, and so starts; false when another press came after it.
    public mutating func arrived(_ key: String) -> Bool {
        guard wanted == key else { return false }
        wanted = nil
        return true
    }

    /// What `key`'s press asked for could not be had.
    public mutating func failed(_ key: String) {
        if wanted == key { wanted = nil }
    }

    /// `slot` starts, pressed or from its own controls, which makes it the
    /// last press: what becomes of each other one held, by the core's rule.
    public mutating func start(_ slot: PlaybackSlot) -> [(PlaybackSlot, PlaybackStop)] {
        var effects: [(PlaybackSlot, PlaybackStop)] = []
        for other in held where other.key != slot.key && playbackStops(starting: slot.summary, other: other.summary) {
            if other.kind.isEmbed {
                effects.append((other, .close))
            } else if playing == other.key {
                effects.append((other, .pause))
            }
        }
        held.removeAll { h in effects.contains { $0.0.key == h.key && $0.1 == .close } }
        if !holds(slot.key) { held.append(slot) }
        playing = slot.key
        wanted = nil
        return effects
    }

    /// It paused by its own controls, or played to its end; it keeps its place.
    public mutating func paused(_ key: String) {
        if playing == key { playing = nil }
    }

    /// Its row scrolled away: by the core's rule a video pauses and a
    /// site's player goes back to its card; audio plays on and a post
    /// stays shown. Nil when nothing changes.
    public mutating func away(_ key: String) -> PlaybackStop? {
        guard let slot = held.first(where: { $0.key == key }), playbackStopsOffScreen(playing: slot.summary) else { return nil }
        if slot.kind.isEmbed {
            close(key)
            return .close
        }
        guard playing == key else { return nil }
        playing = nil
        return .pause
    }

    /// Closed by hand, or its player could not be made.
    public mutating func close(_ key: String) {
        held.removeAll { $0.key == key }
        if playing == key { playing = nil }
        if wanted == key { wanted = nil }
    }

    /// The conversation was left: everything closed, and nothing wanted.
    public mutating func leave() -> [PlaybackSlot] {
        let all = held
        held = []
        playing = nil
        wanted = nil
        return all
    }
}

// MARK: Embeds

/// A link's embed, asked of the core once a link: a card is drawn again
/// whenever anything it reads changes, and asks here each time.
public final class EmbedLookup {
    private var found: [String: EmbedSummary?] = [:]
    private let find: (String) -> EmbedSummary?

    public init(find: @escaping (String) -> EmbedSummary? = { embedFor(url: $0) }) {
        self.find = find
    }

    /// The embed `url` has, if any.
    public func embed(for url: String) -> EmbedSummary? {
        if let known = found[url] { return known }
        let embed = find(url)
        found[url] = .some(embed)
        return embed
    }
}

/// How large an embed is drawn in a column (specs/media.md 6.6): a
/// player at the column's width at its shape and at most 400 tall, audio
/// at the column's width and the site's height, a post at most 550 wide
/// and its fixed height, or the height it reports when it grows (the
/// core's `reportedEmbedHeight`, which holds it in its bounds).
public enum EmbedLayout {
    public static let tallest: CGFloat = 400

    public static func size(_ embed: EmbedSummary, column: CGFloat, reported: UInt32? = nil) -> CGSize {
        guard column > 0 else { return .zero }
        let widest = embed.maxWidth > 0 ? min(column, CGFloat(embed.maxWidth)) : column
        if embed.aspectWidth > 0, embed.aspectHeight > 0 {
            let aspect = CGFloat(embed.aspectWidth) / CGFloat(embed.aspectHeight)
            var w = widest
            var h = w / aspect
            if h > tallest {
                h = tallest
                w = h * aspect
            }
            return CGSize(width: w.rounded(.down), height: h.rounded(.down))
        }
        let height = embed.resizes ? reported ?? embed.fixedHeight : embed.fixedHeight
        return CGSize(width: widest.rounded(.down), height: CGFloat(height))
    }
}

/// The page a phone's web view holds an embed in (specs/media.md 6.6):
/// the embed's iframe alone, with the instance's origin as its base so
/// that a site refusing an embed with no referrer (YouTube) plays, and,
/// for a post that grows, a listener that hands the app what that
/// iframe's own window posts from the embed's origin, as text, for the
/// core to read a height from.
public enum EmbedPage {
    /// The name the page posts the embed's messages to (`window.webkit.messageHandlers`).
    public static let handler = "lumiaEmbed"

    /// The instance's origin, the page's base: `https://chat.acme.dev/`.
    public static func base(_ instance: URL) -> URL? {
        guard let scheme = instance.scheme, let host = instance.host else { return nil }
        var parts = URLComponents()
        parts.scheme = scheme
        parts.host = host
        parts.port = instance.port
        parts.path = "/"
        return parts.url
    }

    /// The address to load: the core's, with Twitch's `parent` (the
    /// instance's host, the page's own) added where it needs one.
    public static func address(_ url: String, needsParent: Bool, instance: URL) -> String? {
        guard !url.isEmpty, URL(string: url) != nil else { return nil }
        guard needsParent else { return url }
        guard let host = instance.host, !host.isEmpty,
              let encoded = host.addingPercentEncoding(withAllowedCharacters: .urlHostAllowed) else { return nil }
        return url + (url.contains("?") ? "&" : "?") + "parent=" + encoded
    }

    /// An origin as a page compares one: `https://www.youtube-nocookie.com`.
    public static func origin(_ url: URL) -> String? {
        guard let scheme = url.scheme?.lowercased(), let host = url.host?.lowercased(), !host.isEmpty else { return nil }
        let port = url.port.map { ":\($0)" } ?? ""
        return "\(scheme)://\(host)\(port)"
    }

    /// The page holding `src`, titled `title`. Only a post of a fixed
    /// height (`scrolls`) scrolls inside itself; any other embed is drawn
    /// whole, so a drag over it scrolls the conversation. For one that
    /// grows (`reports`) the page passes on what the iframe's window posts
    /// from `src`'s origin, and nothing from anywhere else: a string as it
    /// was posted, an object as its JSON, which is how the core's
    /// `reportedEmbedHeight` takes a message. The page reads nothing in it.
    public static func html(src: String, title: String, scrolls: Bool = false, reports: Bool = false) -> String {
        let scrolling = scrolls ? "" : #" scrolling="no""#
        return """
        <!doctype html>
        <html><head>
        <meta charset="utf-8">
        <meta name="viewport" content="width=device-width, initial-scale=1, maximum-scale=1">
        <meta name="referrer" content="strict-origin-when-cross-origin">
        <style>html,body{margin:0;padding:0;width:100%;height:100%;overflow:hidden;background:transparent}iframe{display:block;border:0;width:100%;height:100%}</style>
        </head><body>
        <iframe id="embed" src="\(escape(src))" title="\(escape(title))" allow="autoplay; encrypted-media; fullscreen; picture-in-picture" allowfullscreen referrerpolicy="strict-origin-when-cross-origin"\(scrolling)></iframe>\(reports ? "\n<script>\n\(listener(src: src))\n</script>" : "")
        </body></html>
        """
    }

    /// The page's script for an embed that grows: what the iframe's own
    /// window posts from `src`'s origin, handed on as text.
    static func listener(src: String) -> String {
        let origin = URL(string: src).flatMap(origin) ?? ""
        return """
        (function () {
          var frame = document.getElementById("embed");
          var origin = \(json(origin));
          window.addEventListener("message", function (event) {
            if (event.source !== frame.contentWindow || event.origin !== origin) return;
            var text = event.data;
            if (typeof text !== "string") { try { text = JSON.stringify(text); } catch (e) { return; } }
            if (typeof text === "string") window.webkit.messageHandlers.\(handler).postMessage(text);
          });
        })();
        """
    }

    /// Text for an attribute's value.
    static func escape(_ text: String) -> String {
        text.replacingOccurrences(of: "&", with: "&amp;")
            .replacingOccurrences(of: "\"", with: "&quot;")
            .replacingOccurrences(of: "'", with: "&#39;")
            .replacingOccurrences(of: "<", with: "&lt;")
            .replacingOccurrences(of: ">", with: "&gt;")
    }

    /// A string as a script's literal, safe inside a `<script>`.
    static func json(_ text: String) -> String {
        var out = "\""
        for scalar in text.unicodeScalars {
            switch scalar {
            case "\\": out += "\\\\"
            case "\"": out += "\\\""
            case "<", ">", "&", "\u{2028}", "\u{2029}": out += String(format: "\\u%04x", scalar.value)
            default:
                if scalar.value < 0x20 { out += String(format: "\\u%04x", scalar.value) } else { out.unicodeScalars.append(scalar) }
            }
        }
        return out + "\""
    }
}

/// What an embed's web view does with a navigation (specs/media.md 6.6):
/// it loads only the page holding the embed and the embeds' own origins;
/// a link followed away from them opens in the browser, and anything else
/// is not loaded.
public enum EmbedNavigation: Equatable, Sendable {
    case allow
    case cancel
    case openOutside(URL)

    /// `wrapperLoaded`: the page itself has loaded, so the main frame goes nowhere else.
    public static func decide(url: URL?, mainFrame: Bool, wrapperLoaded: Bool, userActivated: Bool, origins: Set<String>) -> EmbedNavigation {
        guard let url else { return .cancel }
        let scheme = url.scheme?.lowercased() ?? ""
        let web = scheme == "https" || scheme == "http"
        if mainFrame {
            if !wrapperLoaded { return .allow }
            return web ? .openOutside(url) : .cancel
        }
        if ["about", "blob", "data"].contains(scheme) { return .allow }
        if web, let origin = EmbedPage.origin(url), origins.contains(origin) { return .allow }
        return web && userActivated ? .openOutside(url) : .cancel
    }
}

/// What is said of an embed, shown and to assistive technology.
public enum EmbedWords {
    /// The press's name, the core's label and the card's title after a
    /// comma: "Play video from YouTube, Never gonna", "Show post from X".
    public static func press(_ embed: EmbedSummary, title: String) -> String {
        let title = title.trimmingCharacters(in: .whitespacesAndNewlines)
        return title.isEmpty ? embed.label : "\(embed.label), \(title)"
    }

    /// The button on its card.
    public static func button(_ embed: EmbedSummary) -> String {
        embed.kind == "post" ? "Show post" : "Play"
    }

    /// The embed's title: the card's, or the site's name when it has none.
    public static func title(_ embed: EmbedSummary, cardTitle: String) -> String {
        let title = cardTitle.trimmingCharacters(in: .whitespacesAndNewlines)
        return title.isEmpty ? embed.providerName : title
    }

    /// While a Bluesky handle is resolved on the press.
    public static let finding = "Finding the post…"

    /// A Bluesky handle that did not resolve: the card stays, and opens the browser.
    public static func notFound(handle: String) -> String {
        "Bluesky couldn’t find @\(handle). The card opens it in the browser."
    }
}

public extension MediaWords {
    /// A recording's place as it plays: "0:12 / 1:30", or the place alone with no length.
    static func position(ms: UInt32, of total: UInt32) -> String {
        total > 0 ? "\(duration(ms: min(ms, total))) / \(duration(ms: total))" : duration(ms: ms)
    }
}
