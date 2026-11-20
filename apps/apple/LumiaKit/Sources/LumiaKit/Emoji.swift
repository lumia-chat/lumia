// A server's own emoji (specs/emoji.md, plan step 12) as the phone holds
// and draws them, as values the tests cover: the sizes the boards give
// them, the `:word` the composer's list completes, what an emoji choice is,
// "Frequently used" kept by server, a picture's frames decoded and held by
// its address, and the words of the Emoji section of Server settings and
// of its refusals. Every rule of what an emoji is (a name, a match, a ref)
// is the core's (`isEmojiName`, `emojiNameFromFile`, `emojiMatches`,
// `parseBodyMarked`); the views are `EmojiComponents.swift`.
import CoreGraphics
import Foundation
import ImageIO
import LumiaFFI
import Observation
import SwiftUI

// MARK: Sizes

/// The boards' sizes of an emoji, which the tokens do not name.
public enum EmojiMetrics {
    /// In a message's text: 1.3 times the text's size tall, at its shape.
    public static let inText: CGFloat = 1.3
    /// In a reaction chip: 18.
    public static let chip = LumiaTokens.space18
    /// In the `:` list's rows: 24.
    public static let list = LumiaTokens.space24
    /// In the picker's cells: 28, in a cell of 44.
    public static let picker = LumiaTokens.space28
    public static let pickerCell = LumiaTokens.space40 + LumiaTokens.space4
    /// In a settings row: 32.
    public static let row = LumiaTokens.space16 * 2
    /// At the head of a row's sheet: 48.
    public static let sheet = LumiaTokens.space40 + LumiaTokens.space8
    /// In the upload form's frame: 64.
    public static let upload = LumiaTokens.space16 * 4
}

// MARK: What a choice is

/// What the picker hands back: one of Unicode's, or one of the server's.
public enum PickedEmoji: Equatable, Sendable {
    case unicode(String)
    case custom(EmojiSummary)

    /// What the composer puts in the field: the emoji itself, or `:name:`
    /// and a space after it (specs/emoji.md 10).
    public var typed: String {
        switch self {
        case let .unicode(e): e
        case let .custom(e): ":\(e.name): "
        }
    }

    /// The choice as the server's emoji now are (`held`): Unicode's as it
    /// is; one of the server's by its id, under its present name, or nil
    /// when the server no longer has it.
    public func current(in held: [EmojiSummary]) -> PickedEmoji? {
        switch self {
        case .unicode: self
        case let .custom(e): held.first { $0.emojiId == e.emojiId }.map(PickedEmoji.custom)
        }
    }

    /// How "Frequently used" keeps it (`FrequentEmoji`).
    public func kept(serverID: String) -> String {
        switch self {
        case let .unicode(e): e
        case let .custom(e): FrequentEmoji.custom(serverID: serverID, emojiID: e.emojiId)
        }
    }
}

/// A server's emoji as a view draws it: its present name, its shape, and
/// its picture once fetched (nil while it is on its way). An emoji the
/// phone does not hold, or whose picture could not be had, has none of
/// this: it is its `:name:` as text.
public struct EmojiDrawing: Equatable {
    public let emojiID: String
    public let name: String
    public let width: CGFloat
    public let height: CGFloat
    public let animated: Bool
    public let picture: EmojiPicture?

    public init(emojiID: String, name: String, width: CGFloat, height: CGFloat, animated: Bool, picture: EmojiPicture?) {
        self.emojiID = emojiID; self.name = name; self.width = max(1, width); self.height = max(1, height); self.animated = animated; self.picture = picture
    }

    /// `:name:`, its tooltip and what assistive technology reads.
    public var label: String { ":\(name):" }

    /// Its width for a height, by its shape.
    public func width(forHeight h: CGFloat) -> CGFloat { h * width / height }

    /// Its size fitted in a square box of `side`, at its shape.
    public func fitted(in side: CGFloat) -> CGSize {
        width >= height ? CGSize(width: side, height: side * height / width) : CGSize(width: side * width / height, height: side)
    }

    public static func == (a: EmojiDrawing, b: EmojiDrawing) -> Bool {
        a.emojiID == b.emojiID && a.name == b.name && a.width == b.width && a.height == b.height && a.animated == b.animated && a.picture === b.picture
    }
}

// MARK: The composer's list

/// The `:word` at the end of the composer's draft, which the `:` list
/// completes: a `:` at the start or after whitespace (so `10:30` and a
/// `:)` are left alone), then what a name may hold. As the @ list, the
/// word is the one at the draft's end. How many letters list anything is
/// the core's (`emojiMatches`, two at least).
public struct EmojiQuery: Equatable, Sendable {
    /// The draft before the `:`.
    public let before: String
    /// What follows the `:`.
    public let query: String

    public static func at(_ draft: String) -> EmojiQuery? {
        guard let colon = draft.lastIndex(of: ":") else { return nil }
        let query = draft[draft.index(after: colon)...]
        let name = query.unicodeScalars.allSatisfy { $0.isASCII && (CharacterSet.alphanumerics.contains($0) || $0 == "_") }
        let opens = colon == draft.startIndex || draft[draft.index(before: colon)].isWhitespace
        guard name, opens, !query.isEmpty else { return nil }
        return EmojiQuery(before: String(draft[..<colon]), query: String(query))
    }

    /// The draft once `name` is chosen: `:name:` and a space.
    public func picking(_ name: String) -> String { "\(before):\(name): " }
}

/// What a list of a server's emoji (the `:` list, the picker's search)
/// was asked for: the server, the words typed, and the server's emoji as
/// held when it was asked. A rename, a removal or an upload since is another
/// question, as are other words and another server's conversation.
public struct EmojiAsked: Hashable, Sendable {
    public let serverID: String
    public let words: String
    public let held: [EmojiSummary]

    public init(serverID: String, words: String, held: [EmojiSummary]) {
        self.serverID = serverID; self.words = words; self.held = held
    }
}

/// What the core answered for a list of a server's emoji, and what it was
/// asked: shown only while that is what is asked now, so a list asked
/// before an emoji was renamed or removed offers nothing it no longer has.
public struct EmojiFound: Equatable, Sendable {
    public var asked: EmojiAsked?
    public var rows: [EmojiSummary]

    public init(asked: EmojiAsked? = nil, rows: [EmojiSummary] = []) { self.asked = asked; self.rows = rows }

    public func rows(for now: EmojiAsked) -> [EmojiSummary] { asked == now ? rows : [] }
}

/// What the picker's search asks the core for of a server's emoji: the
/// words typed, a colon either side of them let go (`:par` finds what
/// `par` does).
public func emojiSearchPrefix(_ typed: String) -> String {
    var s = typed.trimmingCharacters(in: .whitespaces)
    while s.hasPrefix(":") { s.removeFirst() }
    while s.hasSuffix(":") { s.removeLast() }
    return s
}

// MARK: Frequently used

public extension FrequentEmoji {
    /// One of a server's emoji as "Frequently used" keeps it: its server
    /// with it, so it is offered in that server only (specs/emoji.md 10).
    static func custom(serverID: String, emojiID: String) -> String { "emoji:\(serverID):\(emojiID)" }

    /// A kept entry that is one of a server's emoji: its server and id.
    static func customParts(_ kept: String) -> (serverID: String, emojiID: String)? {
        let parts = kept.split(separator: ":", omittingEmptySubsequences: false)
        guard parts.count == 3, parts[0] == "emoji", !parts[1].isEmpty, !parts[2].isEmpty else { return nil }
        return (String(parts[1]), String(parts[2]))
    }

    /// "Frequently used" in a conversation of `serverID` (empty for a
    /// personal one): Unicode's as ever, and a server's emoji only in that
    /// server and only while `held` has it, then the quick six.
    static func list(recent: [String], serverID: String, held: (String) -> Bool) -> [String] {
        let here = recent.filter { kept in
            guard let parts = customParts(kept) else { return true }
            return !serverID.isEmpty && parts.serverID == serverID && held(parts.emojiID)
        }
        return list(recent: here)
    }
}

// MARK: Pictures

/// An emoji's picture decoded: its frames and how long each shows; one
/// frame for a still one. Frames are the instance's own pixels, at most
/// 128 each way.
public final class EmojiPicture: @unchecked Sendable {
    public let frames: [CGImage]
    public let delays: [Double]
    /// How long one pass of its frames lasts.
    public let duration: Double

    public init(frames: [CGImage], delays: [Double]) {
        self.frames = frames
        self.delays = delays.count == frames.count ? delays : Array(repeating: EmojiPicture.slowest, count: frames.count)
        duration = self.delays.reduce(0, +)
    }

    /// A delay under 20 ms is drawn as 100 ms, as the instance makes them and browsers draw them.
    static let slowest = 0.1

    public var animated: Bool { frames.count > 1 }

    /// Whether an animated one stands still: the system asks for reduced
    /// motion, or (a debug build's smoke) `LUMIA_DEV_REDUCE_MOTION=1` does.
    public static func still(_ reduceMotion: Bool) -> Bool {
        #if DEBUG
        if ProcessInfo.processInfo.environment["LUMIA_DEV_REDUCE_MOTION"] == "1" { return true }
        #endif
        return reduceMotion
    }

    /// The shortest any frame shows: how often a playing one is drawn again.
    public var shortest: Double { delays.min() ?? Self.slowest }

    /// The frame showing `seconds` into its loop; the first for a still one.
    public func frame(at seconds: Double?) -> CGImage {
        guard let seconds, animated, duration > 0 else { return frames[0] }
        var t = seconds.truncatingRemainder(dividingBy: duration)
        if t < 0 { t += duration }
        for (n, d) in delays.enumerated() {
            if t < d { return frames[n] }
            t -= d
        }
        return frames[frames.count - 1]
    }

    /// The picture in `data`, a PNG or a GIF as the instance makes them; nil
    /// for anything that does not decode. At most 200 frames are read, as
    /// the instance keeps no more.
    public static func decode(_ data: Data) -> EmojiPicture? {
        guard let source = CGImageSourceCreateWithData(data as CFData, [kCGImageSourceShouldCache: true] as CFDictionary) else { return nil }
        let count = min(CGImageSourceGetCount(source), 200)
        var frames: [CGImage] = []
        var delays: [Double] = []
        for n in 0..<count {
            guard let frame = CGImageSourceCreateImageAtIndex(source, n, nil), frame.width <= 1024, frame.height <= 1024 else { continue }
            frames.append(frame)
            let props = CGImageSourceCopyPropertiesAtIndex(source, n, nil) as? [CFString: Any]
            let gif = props?[kCGImagePropertyGIFDictionary] as? [CFString: Any]
            let delay = (gif?[kCGImagePropertyGIFUnclampedDelayTime] as? Double) ?? (gif?[kCGImagePropertyGIFDelayTime] as? Double) ?? slowest
            delays.append(delay < 0.02 ? slowest : delay)
        }
        return frames.isEmpty ? nil : EmojiPicture(frames: frames, delays: delays)
    }

    /// A picture chosen to upload, drawn in the form before the instance has
    /// made its own of it: any format the system reads, each frame made at
    /// most 256 across and upright, at most 200 of them; nil for one it
    /// cannot read. Nothing of it is sent but its bytes as chosen.
    public static func preview(_ data: Data) -> EmojiPicture? {
        guard let source = CGImageSourceCreateWithData(data as CFData, [kCGImageSourceShouldCache: false] as CFDictionary) else { return nil }
        let count = min(CGImageSourceGetCount(source), 200)
        let options: [CFString: Any] = [kCGImageSourceCreateThumbnailFromImageAlways: true, kCGImageSourceThumbnailMaxPixelSize: 256, kCGImageSourceCreateThumbnailWithTransform: true]
        var frames: [CGImage] = []
        var delays: [Double] = []
        for n in 0..<count {
            guard let frame = CGImageSourceCreateThumbnailAtIndex(source, n, options as CFDictionary) else { continue }
            frames.append(frame)
            let props = CGImageSourceCopyPropertiesAtIndex(source, n, nil) as? [CFString: Any]
            let gif = props?[kCGImagePropertyGIFDictionary] as? [CFString: Any]
            let delay = (gif?[kCGImagePropertyGIFUnclampedDelayTime] as? Double) ?? (gif?[kCGImagePropertyGIFDelayTime] as? Double) ?? slowest
            delays.append(delay < 0.02 ? slowest : delay)
        }
        return frames.isEmpty ? nil : EmojiPicture(frames: frames, delays: delays)
    }

    /// A clear picture of `width` by `height` pixels: what a picture on its
    /// way stands as in a line of text, so the line does not move when it comes.
    static func clear(width: Int, height: Int) -> CGImage? {
        let w = max(1, width), h = max(1, height)
        let key = w << 16 | h
        if let held = clears[key] { return held }
        let made = CGContext(data: nil, width: w, height: h, bitsPerComponent: 8, bytesPerRow: 0, space: CGColorSpace(name: CGColorSpace.sRGB)!, bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue)?.makeImage()
        clears[key] = made
        return made
    }

    nonisolated(unsafe) private static var clears: [Int: CGImage] = [:]
}

/// The servers' emoji pictures, fetched by `image_url` through the
/// system's URL cache, which avatars and server pictures load through too
/// (`URLSession.shared`), decoded off the main thread and held by address
/// while a server lists them. A view asks for one while it draws and is
/// drawn again when it comes. One that could not be fetched or decoded is
/// its `:name:` until it comes, fetched again by itself after a wait, and at
/// once on a reconnect (`retryFailed`).
@MainActor
@Observable
public final class EmojiPictures {
    private var held: [URL: EmojiPicture] = [:]
    private var failed: Set<URL> = []
    /// The fetches on their way: not something a view watches, since a view
    /// asks while drawing itself (the session's `profilesInFlight` has why).
    @ObservationIgnored private var asking: Set<URL> = []
    /// How many times in a row each picture in `failed` could not be had.
    @ObservationIgnored private var failures: [URL: Int] = [:]
    /// How long a picture that could not be had for the nth time in a row
    /// waits before it is fetched again: 15 s, doubling, at most 5 minutes.
    /// A test's own.
    @ObservationIgnored var retryAfter: @Sendable (Int) -> Duration = { n in .seconds(min(300, 15 << min(max(n - 1, 0), 5))) }
    /// How a picture is fetched; a test's own.
    @ObservationIgnored var fetch: @Sendable (URL) async throws -> Data = { url in
        let (data, response) = try await URLSession.shared.data(from: url)
        if let http = response as? HTTPURLResponse, !(200..<300).contains(http.statusCode) { throw URLError(.badServerResponse) }
        return data
    }

    public init() {}

    /// The picture at `url`, once fetched; asked for the first time it is
    /// wanted. nil while on its way and when it could not be had (`failed`).
    public func picture(_ url: URL) -> EmojiPicture? {
        if let picture = held[url] { return picture }
        if !failed.contains(url), asking.insert(url).inserted {
            Task { await load(url) }
        }
        return nil
    }

    /// Whether the picture at `url` could not be had: drawn as its name,
    /// also while it is fetched again.
    public func failed(_ url: URL) -> Bool { failed.contains(url) }

    func load(_ url: URL) async {
        defer { asking.remove(url) }
        var picture: EmojiPicture?
        if let data = try? await fetch(url) {
            picture = await Task.detached(operation: { EmojiPicture.decode(data) }).value
        }
        if let picture {
            held[url] = picture
            failed.remove(url)
            failures[url] = nil
            return
        }
        // Its name meanwhile, and fetched again by itself later, the wait
        // doubling while it still cannot be had.
        failed.insert(url)
        let n = (failures[url] ?? 0) + 1
        failures[url] = n
        let wait = retryAfter(n)
        Task { [weak self] in
            try? await Task.sleep(for: wait)
            guard let self, self.failures[url] == n else { return }
            self.again(url)
        }
    }

    /// A picture that could not be had, fetched again; drawn as its name until it comes.
    private func again(_ url: URL) {
        guard failed.contains(url), asking.insert(url).inserted else { return }
        Task { await load(url) }
    }

    /// Every picture that could not be had, fetched again now: the phone is
    /// back on the instance.
    public func retryFailed() {
        for url in failed { again(url) }
    }

    /// Keeps only the pictures at `listed`, the addresses the servers list
    /// now: one an emoji no longer has (removed, specs/emoji.md 7) goes
    /// from here and from the URL cache. One that failed is asked for again
    /// when it is next drawn.
    public func keep(only listed: Set<URL>) {
        for url in held.keys where !listed.contains(url) {
            held[url] = nil
            URLCache.shared.removeCachedResponse(for: URLRequest(url: url))
        }
        failed = []
        failures = [:]
    }

    /// Everything held: the account left the phone.
    public func forget() {
        held = [:]
        failed = []
        failures = [:]
    }
}

// MARK: Words

/// What the Emoji section of Server settings says (the "Emoji settings"
/// and "Emoji on phones" boards), and what its refusals say.
public enum EmojiWords {
    public static let upload = "Upload emoji"
    public static let photos = "Photos"
    public static let files = "Files"
    public static let rename = "Rename"
    public static let remove = "Remove"
    public static let keep = "Keep it"
    public static let chooseAnother = "Choose another…"

    /// "12 of 50".
    public static func count(_ n: Int, limit: UInt32) -> String { "\(n) of \(limit)" }

    /// The section's line over its list, and at the limit, why Upload is off.
    public static func lead(server: String, count: Int, limit: UInt32) -> String {
        atLimit(count: count, limit: limit) ? full(server: server, limit: limit) : "Used as :name: in \(server)’s channels and server DMs, and as reactions. A PNG, JPEG, WebP or GIF up to 8 MB, made to fit 128 pixels; an animated one plays."
    }

    /// Upload is offered below the limit only (specs/emoji.md 5).
    public static func atLimit(count: Int, limit: UInt32) -> Bool { count >= Int(limit) }

    public static func full(server: String, limit: UInt32) -> String {
        "\(server) has as many emoji as it may (\(limit)). Remove one to upload another."
    }

    /// Under a row's name: "Uploaded by Devon Park · Animated".
    public static func uploaded(by name: String, animated: Bool) -> String {
        "Uploaded by \(name)" + (animated ? " · Animated" : "")
    }

    /// Under the upload form's name.
    public static func nameHint(_ name: String) -> String {
        "2 to 32 letters, digits and _. It is typed as :\(name.isEmpty ? "name" : name): in a message."
    }

    /// The question Remove asks first.
    public static func removeTitle(_ name: String) -> String { "Remove :\(name):?" }
    public static let removeLine = "Messages that used it will show its name instead."

    public static let nameRule = "A name is 2 to 32 of a to z, 0 to 9 and _, with no spaces."
    public static func taken(server: String, name: String) -> String { "\(server) already has :\(name):. Choose another name." }
    public static let tooLarge = "This picture is too large for an emoji. An animated one can have at most 200 frames and come to 512 KB once made."
    public static let unsupported = "Lumia can’t use this file. Choose a PNG, JPEG, WebP or GIF."
    public static let unreadable = "This picture couldn’t be read."
    public static let gone = "That emoji has been removed."
    public static func notAllowed(server: String) -> String { "You can’t manage \(server)’s emoji." }

    /// The name a form sends: as typed, trimmed, in lower case, as the instance keeps it.
    public static func sent(_ typed: String) -> String { typed.trimmingCharacters(in: .whitespaces).lowercased() }
}

/// Where a refusal of an upload, a rename or a removal is said: under the
/// name, under the picture, or over the whole.
public enum EmojiRefusalPlace: Equatable, Sendable { case name, picture, form }

/// What the instance refused of an emoji, in the boards' words, and where
/// it is said: `code` and `details` as the error carries them.
public enum EmojiRefusal {
    public static func words(code: String?, details: [String: String], message: String, server: String, name: String) -> (place: EmojiRefusalPlace, words: String) {
        switch code {
        case "ALREADY_EXISTS": return (.name, EmojiWords.taken(server: server, name: name))
        case "INVALID_ARGUMENT" where details["field"] == "picture":
            switch details["reason"] {
            case "too_large": return (.picture, EmojiWords.tooLarge)
            case "unsupported": return (.picture, EmojiWords.unsupported)
            default: return (.picture, EmojiWords.unreadable)
            }
        case "INVALID_ARGUMENT": return (.name, EmojiWords.nameRule)
        case "RESOURCE_EXHAUSTED" where details["state"] == "emoji":
            return (.form, EmojiWords.full(server: server, limit: UInt32(details["limit"] ?? "") ?? 0))
        case "PERMISSION_DENIED": return (.form, EmojiWords.notAllowed(server: server))
        case "NOT_FOUND": return (.form, EmojiWords.gone)
        case "FAILED_PRECONDITION": return (.form, "This instance does not keep pictures.")
        case "RATE_LIMITED": return (.form, "The instance is busy. Try again in a moment.")
        default: return (.form, message)
        }
    }

    /// What is refused here before anything is sent: a name the core's rule
    /// does not allow, a picture over 8 MB.
    public static func before(name: String, bytes: Int) -> (place: EmojiRefusalPlace, words: String)? {
        if bytes > ServerSettingsRefusal.largestPicture { return (.picture, EmojiWords.tooLarge) }
        if !isEmojiName(name: EmojiWords.sent(name)) { return (.name, EmojiWords.nameRule) }
        return nil
    }
}

// MARK: The composer's caret

/// Where the composer's caret is, so what the picker chooses goes in there
/// (iOS 18's `TextSelection`; iOS 17's field says nothing of it, and the
/// choice goes at the end of the draft).
@MainActor
public final class ComposerCaret {
    /// The field's selection as it last said it: a `TextSelection`, held
    /// untyped so the type can be kept on iOS 17.
    private var raw: Any?

    public init() {}

    @available(iOS 18, macOS 15, *)
    var selection: Binding<TextSelection?> {
        Binding(get: { self.raw as? TextSelection }, set: { self.raw = $0 })
    }

    /// `text` with `typed` put in place of what is selected, or at the
    /// caret, or at the end when the caret is not known (or no longer
    /// falls in the text); the caret after it.
    public func insert(_ typed: String, into text: String) -> String {
        guard #available(iOS 18, macOS 15, *), let selection = raw as? TextSelection, case let .selection(range) = selection.indices,
              range.upperBound <= text.endIndex, let lower = String.Index(range.lowerBound, within: text), let upper = String.Index(range.upperBound, within: text), lower <= upper else {
            return text + typed
        }
        var out = text
        out.replaceSubrange(lower..<upper, with: typed)
        let at = out.index(out.startIndex, offsetBy: text.distance(from: text.startIndex, to: lower) + typed.count)
        raw = TextSelection(insertionPoint: at)
        return out
    }
}
