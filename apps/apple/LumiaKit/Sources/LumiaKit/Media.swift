// Attachments and link cards (specs/media.md) as the phone decides them
// before anything is drawn or sent, as values the tests cover: what the
// composer's strip holds and when a send may go (`ComposerAttachments`),
// which links' cards are previewed while composing and which are kept
// (`LinkPreviews`), how pictures sit in a message's column
// (`MediaLayout`), the words a person reads and hears of each
// (`MediaWords`), how each attachment is drawn, with Images as links too
// (`AttachmentDrawn`), and the file a player or the share sheet is handed
// (`MediaFiles`). The processing, the sealing and the fetching are the
// core's; `MediaInput` hands it what the platform decodes.
import CoreGraphics
import Foundation
import LumiaFFI

/// What an attachment is, from the core's `kind`.
public enum AttachmentKind: String, Sendable {
    case image, gif, video, audio, file

    public init(_ kind: String) { self = AttachmentKind(rawValue: kind) ?? .file }

    /// Drawn as a picture in a message's column (a video by its poster),
    /// when Preferences shows images.
    public var visual: Bool { self == .image || self == .gif || self == .video }
}

/// How a message's attachment is drawn (specs/media.md 5), by what it is,
/// whether its server let it go and whether Images are drawn as links.
public enum AttachmentDrawn: Equatable, Sendable {
    /// A picture, a GIF or a video's poster, in the message's pictures.
    case picture
    /// A recording's row, which plays it in place: as links too, where a
    /// recording's row plays it on a press as it always does.
    case recording
    /// Images as links: a video's row, which plays it under itself.
    case videoRow
    /// A file's row, which opens the share sheet; Images as links, a
    /// picture's row, which opens the viewer.
    case fileRow
    /// One its server let go: its name and "No longer available", opening nothing.
    case expired

    public static func of(_ kind: AttachmentKind, expired: Bool, asLinks: Bool) -> AttachmentDrawn {
        if expired { return .expired }
        switch kind {
        case .audio: return .recording
        case .video: return asLinks ? .videoRow : .picture
        case .image, .gif: return asLinks ? .fileRow : .picture
        case .file: return .fileRow
        }
    }

    public static func of(_ a: AttachmentSummary, asLinks: Bool) -> AttachmentDrawn {
        of(AttachmentKind(a.kind), expired: a.expired, asLinks: asLinks)
    }
}

/// An attachment's bytes as a file of its name, for the share sheet and
/// the players. Every write has a folder of its own under `lumia-media`
/// in the temporary directory, so two uses of one attachment (a recording
/// playing while it is shared, the viewer's share beside a row's) never
/// write over or remove each other's file.
public enum MediaFiles {
    static let folder = "lumia-media"

    /// The file's name as one path component.
    static func fileName(_ name: String) -> String {
        let name = name.replacingOccurrences(of: "/", with: "-")
        return name.isEmpty || name == "." || name == ".." ? "attachment" : name
    }

    public static func write(_ data: Data, named name: String, under root: URL = FileManager.default.temporaryDirectory) throws -> URL {
        let dir = root.appendingPathComponent(folder, isDirectory: true).appendingPathComponent(UUID().uuidString, isDirectory: true)
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        let url = dir.appendingPathComponent(fileName(name))
        try data.write(to: url, options: .atomic)
        return url
    }

    public static func write(_ data: Data, for a: AttachmentSummary) throws -> URL {
        try write(data, named: a.name)
    }

    /// Removes a file `write` made, with its folder; nothing else.
    public static func remove(_ url: URL?) {
        guard let url else { return }
        let dir = url.deletingLastPathComponent()
        guard dir.deletingLastPathComponent().lastPathComponent == folder else { return }
        try? FileManager.default.removeItem(at: dir)
    }

    /// Removes every file written, at launch: what a run that was ended
    /// with something playing or being shared left behind.
    public static func clear(under root: URL = FileManager.default.temporaryDirectory) {
        try? FileManager.default.removeItem(at: root.appendingPathComponent(folder, isDirectory: true))
    }
}

// MARK: The composer's strip

/// One file in the composer's strip: added, being processed and uploaded,
/// waiting to be sent, or refused in words.
public struct ComposerAttachment: Identifiable, Equatable, Sendable {
    public enum State: Equatable, Sendable {
        case uploading
        case ready(AttachmentDraft)
        case failed(String)
    }

    /// This phone's own id for it, from the moment it is added.
    public let id: String
    public let name: String
    public var state: State

    public init(id: String, name: String, state: State = .uploading) {
        self.id = id; self.name = name; self.state = state
    }

    public var draft: AttachmentDraft? {
        if case let .ready(draft) = state { return draft }
        return nil
    }

    public var failure: String? {
        if case let .failed(words) = state { return words }
        return nil
    }

    public var uploading: Bool { state == .uploading }

    /// Its name as the strip shows it: the core's once it is made (the
    /// file's own, where the picker gave a stand-in at first, and a
    /// picture's new extension).
    public var shownName: String { draft?.name ?? name }
}

/// The composer's strip: each file added starts processing and uploading
/// at once (`attach`), and a send waits until none is still on its way.
/// One refused stays, saying why, until its × takes it out; a send leaves
/// it behind. A message carries at most 10.
public struct ComposerAttachments: Equatable, Sendable {
    public static let most = 10
    public private(set) var items: [ComposerAttachment] = []

    public init() {}

    public var isEmpty: Bool { items.isEmpty }
    /// How many more can be added.
    public var room: Int { max(0, Self.most - items.count) }
    /// Some file is still being processed or uploaded: a send waits.
    public var uploading: Bool { items.contains(where: \.uploading) }
    /// The drafts a send carries, in the order they were added.
    public var draftIDs: [String] { items.compactMap(\.draft?.draftId) }

    /// Adds a file by name, its upload about to start; nil when the strip is full.
    public mutating func add(id: String = UUID().uuidString, name: String) -> String? {
        guard room > 0 else { return nil }
        items.append(ComposerAttachment(id: id, name: name))
        return id
    }

    /// Its upload came back: the draft a send names. Nothing for one taken out meanwhile.
    public mutating func uploaded(_ id: String, _ draft: AttachmentDraft) {
        guard let n = items.firstIndex(where: { $0.id == id }) else { return }
        items[n].state = .ready(draft)
    }

    /// It was refused: why, in words, on the file.
    public mutating func failed(_ id: String, _ words: String) {
        guard let n = items.firstIndex(where: { $0.id == id }) else { return }
        items[n].state = .failed(words)
    }

    /// Its alt text changed (the core's answer, clipped as it keeps it).
    public mutating func updated(_ draft: AttachmentDraft) {
        guard let n = items.firstIndex(where: { $0.draft?.draftId == draft.draftId }) else { return }
        items[n].state = .ready(draft)
    }

    /// The × : the file goes; the draft it was, if any, to be discarded.
    @discardableResult
    public mutating func remove(_ id: String) -> String? {
        guard let n = items.firstIndex(where: { $0.id == id }) else { return nil }
        return items.remove(at: n).draft?.draftId
    }

    /// Whether Send does anything: nothing on its way, and words or a file to send.
    public func canSend(body: String) -> Bool {
        !uploading && (!body.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty || !draftIDs.isEmpty)
    }

    /// Sent: the drafts it carried go, and a refused file stays.
    public mutating func sent() {
        items.removeAll { $0.draft != nil }
    }

    /// Every draft still held, to discard when the conversation is left.
    public mutating func clear() -> [String] {
        defer { items = [] }
        return draftIDs
    }
}

// MARK: Link previews while composing

/// The cards the composer shows over the field (specs/media.md 6.4, 6.5):
/// once typing pauses for `pause`, the body's links that may have cards
/// (the core's `cardLinks`, at most three, in order) are each asked for
/// once; a card × is not shown again for that link while the draft is
/// written; the links whose cards are shown when it is sent are the ones
/// the message keeps.
public struct LinkPreviews: Equatable, Sendable {
    /// How long typing must pause before the links are read.
    public static let pause: Duration = .milliseconds(500)
    public static let most = 3

    /// The links the draft has now.
    public private(set) var links: [String] = []
    /// What each link asked for came back with; nil for no card.
    public private(set) var answers: [String: CardSummary?] = [:]
    public private(set) var asking: Set<String> = []
    public private(set) var dismissed: Set<String> = []

    public init() {}

    /// The draft's links after a pause: the ones still to ask for, each once.
    public mutating func update(links found: [String]) -> [String] {
        links = Array(found.prefix(Self.most))
        let wanted = links.filter { answers[$0] == nil && !asking.contains($0) && !dismissed.contains($0) }
        asking.formUnion(wanted)
        return wanted
    }

    /// A link's card came back, or none (a page with none, a failure).
    public mutating func arrived(_ url: String, card: CardSummary?) {
        asking.remove(url)
        answers[url] = .some(card)
    }

    /// The card's ×.
    public mutating func dismiss(_ url: String) {
        dismissed.insert(url)
    }

    /// The cards shown over the field, in the order their links appear.
    public var shown: [CardSummary] {
        links.compactMap { url in dismissed.contains(url) ? nil : answers[url] ?? nil }
    }

    /// The links whose cards a send keeps: those shown that the body still holds.
    public func kept(in links: [String]) -> [String] {
        shown.map(\.url).filter { links.contains($0) }
    }

    /// Sent, or the conversation changed: everything starts again.
    public mutating func reset() { self = LinkPreviews() }
}

// MARK: Layout

/// How a message's pictures sit in its column (specs/media.md 5): one at
/// its own shape, no wider than the column or its own pixels and at most
/// `tallest` high; several in a grid of square tiles.
public enum MediaLayout {
    public static let tallest: CGFloat = 400
    /// A picture whose size is not known is drawn this shape until it is.
    public static let unknownAspect: CGFloat = 4 / 3

    /// One picture of `width`×`height` pixels in a column `column` wide.
    public static func single(width: UInt32, height: UInt32, column: CGFloat) -> CGSize {
        guard column > 0 else { return .zero }
        let aspect = width > 0 && height > 0 ? CGFloat(width) / CGFloat(height) : unknownAspect
        var w = min(column, width > 0 ? CGFloat(width) : column)
        var h = w / aspect
        if h > tallest {
            h = tallest
            w = h * aspect
        }
        return CGSize(width: w.rounded(.down), height: h.rounded(.down))
    }

    /// Several pictures: how many to a row, and each square tile's side.
    /// Two and four go two to a row, the rest three.
    public static func grid(count: Int, column: CGFloat, gap: CGFloat) -> (columns: Int, rows: Int, side: CGFloat) {
        guard count > 1 else { return (1, count, column) }
        let columns = count == 2 || count == 4 ? 2 : 3
        let rows = (count + columns - 1) / columns
        let side = max(0, (column - gap * CGFloat(columns - 1)) / CGFloat(columns)).rounded(.down)
        return (columns, rows, side)
    }

    /// Where each of `count` tiles goes in a grid.
    public static func tiles(count: Int, column: CGFloat, gap: CGFloat) -> [CGRect] {
        let g = grid(count: count, column: column, gap: gap)
        return (0..<count).map { n in
            CGRect(x: CGFloat(n % g.columns) * (g.side + gap), y: CGFloat(n / g.columns) * (g.side + gap), width: g.side, height: g.side)
        }
    }
}

// MARK: Words

/// What is said of attachments and cards, shown and to assistive technology.
public enum MediaWords {
    /// A size as the Files app says it: "1.2 MB".
    public static func size(_ bytes: UInt64) -> String {
        ByteCountFormatter.string(fromByteCount: Int64(clamping: bytes), countStyle: .file)
    }

    /// A length: "0:07", "12:30", "1:02:03".
    public static func duration(ms: UInt32) -> String {
        let seconds = Int(ms / 1000)
        let (h, m, s) = (seconds / 3600, seconds / 60 % 60, seconds % 60)
        return h > 0 ? String(format: "%d:%02d:%02d", h, m, s) : String(format: "%d:%02d", m, s)
    }

    /// What an attachment is called, first in its label.
    public static func kindName(_ kind: AttachmentKind) -> String {
        switch kind {
        case .image: "Photo"
        case .gif: "GIF"
        case .video: "Video"
        case .audio: "Audio"
        case .file: "File"
        }
    }

    /// An attachment as assistive technology hears it: a picture by its alt
    /// text ("Photo, a dog on a beach"), else its name; a video or audio
    /// with its length; a file with its size ("File, notes.pdf, 1.2 MB").
    /// Drawn as a file row (Images set to link, or a picture that cannot
    /// be drawn), it is a file however it is a picture.
    public static func label(kind: AttachmentKind, name: String, alt: String, sizeBytes: UInt64, durationMs: UInt32, asFile: Bool = false) -> String {
        if asFile || kind == .file || kind == .audio {
            var parts = [asFile ? "File" : kindName(kind), name]
            if kind == .audio, durationMs > 0 { parts.append(duration(ms: durationMs)) }
            parts.append(size(sizeBytes))
            return parts.joined(separator: ", ")
        }
        switch kind {
        case .video:
            return ([kindName(kind), alt.isEmpty ? name : alt] + (durationMs > 0 ? [duration(ms: durationMs)] : [])).joined(separator: ", ")
        default:
            return "\(kindName(kind)), \(alt.isEmpty ? name : alt)"
        }
    }

    public static func label(_ a: AttachmentSummary, asFile: Bool = false) -> String {
        if a.expired { return expiredLabel(name: a.name) }
        return label(kind: AttachmentKind(a.kind), name: a.name, alt: a.alt, sizeBytes: a.sizeBytes, durationMs: a.durationMs, asFile: asFile)
    }

    /// What a file row says under the name of an attachment its server let
    /// go (specs/media.md 4.5), in place of its size.
    public static let expired = "No longer available"

    /// Such an attachment as assistive technology hears it: "File, notes.pdf, no longer available".
    public static func expiredLabel(name: String) -> String {
        "File, \(name), no longer available"
    }

    /// A file row's second line: its size, and a video's or audio's length.
    public static func detail(kind: AttachmentKind, sizeBytes: UInt64, durationMs: UInt32) -> String {
        let length = (kind == .video || kind == .audio) && durationMs > 0 ? [duration(ms: durationMs)] : []
        return (length + [size(sizeBytes)]).joined(separator: " · ")
    }

    /// The SF Symbol for a file row, by kind and then by what the name says.
    public static func symbol(kind: AttachmentKind, mediaType: String, name: String) -> String {
        switch kind {
        case .image, .gif: return "photo"
        case .video: return "film"
        case .audio: return "waveform"
        case .file: break
        }
        let ext = (name as NSString).pathExtension.lowercased()
        if mediaType == "application/pdf" || ext == "pdf" { return "doc.richtext" }
        if ["zip", "gz", "tgz", "bz2", "xz", "7z", "rar", "tar"].contains(ext) { return "doc.zipper" }
        if mediaType.hasPrefix("text/") || ["txt", "md", "csv", "json", "log", "rtf"].contains(ext) { return "doc.text" }
        if mediaType.hasPrefix("image/") { return "photo" }
        return "doc"
    }

    /// A strip file's state in words, after its name.
    public static func state(_ item: ComposerAttachment) -> String {
        switch item.state {
        case .uploading: "Uploading…"
        case let .ready(draft): size(draft.sizeBytes)
        case let .failed(words): words
        }
    }

    /// A card as assistive technology hears it: "Link, example.com, Title".
    public static func cardLabel(_ card: CardSummary) -> String {
        (["Link", card.host, card.title] + (card.video ? ["video"] : [])).filter { !$0.isEmpty }.joined(separator: ", ")
    }

    /// A file too large to send before anything is read of it.
    public static func tooLarge(_ bytes: UInt64, max: UInt64) -> String {
        "Too large: \(size(bytes)), and this instance takes up to \(size(max))."
    }

    /// What the instance's refusal of an upload says on the file; the
    /// core's own words (a picture that cannot be made small enough) as they
    /// are. A server out of room for its files (specs/media.md 4.5) is
    /// `RESOURCE_EXHAUSTED` with `state` `storage` in the details.
    public static func refusal(code: String?, details: [String: String] = [:], message: String) -> String {
        switch code {
        case "PERMISSION_DENIED": "You can’t attach files here."
        case "PAYLOAD_TOO_LARGE": "Too large for this instance."
        case "RATE_LIMITED": "Too many uploads just now. Try again in a minute."
        case "RESOURCE_EXHAUSTED" where details["state"] == "storage": outOfSpace
        default: message
        }
    }

    public static let outOfSpace = "This server is out of space for files."

    /// The composer's attach button.
    public static let attach = "Attach"
    public static let photos = "Photos and videos"
    public static let files = "Files"
    public static let altPrompt = "Describe this picture for people who can’t see it."
    public static let gifBadge = "GIF"
}

/// What an edit of one's own message does with what it carries (specs/media.md
/// 4.4): its attachments taken off and its cards dropped by the ×s shown
/// while it is edited. The body may then be empty when something is left.
public struct MediaEdit: Equatable, Sendable {
    public var removedAttachments: Set<String> = []
    public var removedCards: Set<String> = []

    public init() {}

    public var changed: Bool { !removedAttachments.isEmpty || !removedCards.isEmpty }

    /// The attachment ids the edit drops.
    public func removing(_ attachments: [AttachmentSummary]) -> [String] {
        attachments.map(\.attachmentId).filter { removedAttachments.contains($0) }
    }

    /// The links whose cards stay, when any was dropped; nil keeps every
    /// card whose link the body still holds.
    public func cardURLs(_ cards: [CardSummary]) -> [String]? {
        removedCards.isEmpty ? nil : cards.map(\.url).filter { !removedCards.contains($0) }
    }

    /// Whether anything is left once the edit is saved, words aside.
    public func leaves(_ attachments: [AttachmentSummary]) -> Bool {
        attachments.contains { !removedAttachments.contains($0.attachmentId) }
    }
}
