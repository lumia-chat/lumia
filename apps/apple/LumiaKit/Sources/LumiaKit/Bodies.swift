// A message body as the phone draws it (protocol 14, issue #18), as
// values the tests cover: the core's blocks (`parseBody`, the one grammar
// every client shares) turned into attributed words, cut where their look
// changes kind (words, a mention's pill, an inline code chip); the body as
// heard, each mention by name; when a long one collapses and how much of
// it shows; and the composer's counter under the instance's limit. The
// views are `BodyComponents.swift`.
import Foundation
import LumiaFFI

// MARK: Blocks into words

/// A run of a paragraph drawn one way: plain words (with their emphasis,
/// strike and links), a mention (the index into the message's mentions;
/// its text is the mention as typed, which the view replaces with the
/// person's name), or inline code.
public struct BodyPiece: Equatable {
    public enum Kind: Equatable, Sendable {
        case words
        case mention(Int)
        case code
    }

    public let kind: Kind
    public let text: AttributedString
}

public enum BodyText {
    /// `inline`'s text with its styles as Foundation's attributes: Emphasis,
    /// Strong, Strike and Code as `inlinePresentationIntent`, which add up
    /// where they overlap, and each web link as `link`. The core's ranges
    /// are UTF-16 code units, as `NSRange` counts; one that does not fall
    /// between characters is left out.
    public static func attributed(_ inline: BodyInline) -> AttributedString {
        var all = AttributedString(inline.text)
        for run in inline.runs {
            let intent: InlinePresentationIntent
            switch run.style {
            case .emphasis: intent = .emphasized
            case .strong: intent = .stronglyEmphasized
            case .strike: intent = .strikethrough
            case .code: intent = .code
            case .mention: continue
            }
            guard let range = range(run.start, run.end, in: all) else { continue }
            let parts = all[range].runs[\.inlinePresentationIntent].map { ($0.0, $0.1) }
            for (had, part) in parts { all[part].inlinePresentationIntent = (had ?? []).union(intent) }
        }
        for link in inline.links {
            guard let range = range(link.start, link.end, in: all), isWeb(link.url), let url = URL(string: link.url) else { continue }
            all[range].link = url
        }
        return all
    }

    /// `inline` cut where its look changes kind: at each mention's ends
    /// and each inline code run's, in order. A mention inside code is code
    /// (the core marks none there).
    public static func pieces(_ inline: BodyInline) -> [BodyPiece] {
        let all = attributed(inline)
        let units = UInt32(inline.text.utf16.count)
        var cuts: Set<UInt32> = [0, units]
        var mentions: [(start: UInt32, end: UInt32, index: Int)] = []
        var codes: [(start: UInt32, end: UInt32)] = []
        for run in inline.runs where run.start < run.end && run.end <= units {
            switch run.style {
            case let .mention(index):
                mentions.append((run.start, run.end, Int(index)))
            case .code:
                codes.append((run.start, run.end))
            default:
                continue
            }
            cuts.insert(run.start)
            cuts.insert(run.end)
        }
        let at = cuts.sorted()
        var out: [BodyPiece] = []
        for (from, to) in zip(at, at.dropFirst()) {
            guard let range = range(from, to, in: all) else { continue }
            let kind: BodyPiece.Kind
            if codes.contains(where: { $0.start <= from && to <= $0.end }) {
                kind = .code
            } else if let m = mentions.first(where: { $0.start <= from && to <= $0.end }) {
                kind = .mention(m.index)
            } else {
                kind = .words
            }
            out.append(BodyPiece(kind: kind, text: AttributedString(all[range])))
        }
        return out
    }

    /// The body as it is heard: each paragraph's words with every mention
    /// by name (`name` gives it, empty for one not known, whose mention
    /// stays as typed), a code block's lines, a list's items after their
    /// markers, one block to a line.
    public static func plain(_ blocks: [BodyBlock], mentions: [MentionSummary], name: (String) -> String) -> String {
        blocks.map { block -> String in
            switch block {
            case let .para(inline):
                return pieces(inline).map { piece in
                    let typed = String(piece.text.characters)
                    guard case let .mention(index) = piece.kind, mentions.indices.contains(index) else { return typed }
                    return BodySegment(text: typed, mention: mentions[index]).drawn(name: name)
                }.joined()
            case let .code(_, text):
                return text
            case let .quote(inner):
                return plain(inner, mentions: mentions, name: name)
            case let .list(start, items):
                return items.enumerated().map { k, item in
                    "\(BodyBlocks.marker(start: start, at: k)) \(plain(item.blocks, mentions: mentions, name: name))"
                }.joined(separator: "\n")
            }
        }.joined(separator: "\n")
    }

    /// The protocol's links are web addresses only; the core makes no
    /// other, and this is the same rule where the tap lands.
    public static func isWeb(_ url: String) -> Bool {
        guard let scheme = URL(string: url)?.scheme?.lowercased() else { return false }
        return scheme == "http" || scheme == "https"
    }

    /// The UTF-16 range `start..<end` of `text`, when it falls between characters.
    static func range(_ start: UInt32, _ end: UInt32, in text: AttributedString) -> Range<AttributedString.Index>? {
        guard start < end else { return nil }
        return Range(NSRange(location: Int(start), length: Int(end - start)), in: text)
    }
}

public enum BodyBlocks {
    /// The blocks `parseBody` reads from a body and its mentions, read once: a row is drawn
    /// again at every change around it (a hover, a reaction, the typing line), and the same
    /// body need not cross into the core each time. `NSCache` lets them go under pressure.
    public static func parsed(_ body: String, mentions: [MentionSummary]) -> [BodyBlock] {
        let key = (mentions.map { "\($0.start)-\($0.end)" }.joined(separator: ",") + "\u{0}" + body) as NSString
        if let held = cache.object(forKey: key) { return held.blocks }
        let blocks = parseBody(body: body, mentions: mentions)
        cache.setObject(Held(blocks), forKey: key, cost: body.utf8.count)
        return blocks
    }

    private final class Held {
        let blocks: [BodyBlock]
        init(_ blocks: [BodyBlock]) { self.blocks = blocks }
    }

    /// NSCache is safe to use from any thread; Swift does not know it.
    nonisolated(unsafe) private static let cache: NSCache<NSString, Held> = {
        let cache = NSCache<NSString, Held>()
        cache.totalCostLimit = 4 * 1024 * 1024
        return cache
    }()

    /// A list item's marker: "•", or its number counting from `start`.
    public static func marker(start: UInt64?, at k: Int) -> String {
        start.map { "\($0 + UInt64(k))." } ?? "•"
    }

    /// The room over the block at `index` of `blocks`, as the desktop shell
    /// draws it: a paragraph after a paragraph 4, anything else 8; in a list
    /// item, a nested list 4 and paragraphs 8.
    public static func gap(before index: Int, in blocks: [BodyBlock], inItem: Bool = false) -> CGFloat {
        guard index > 0, blocks.indices.contains(index) else { return 0 }
        if case .para = blocks[index - 1], case .para = blocks[index], !inItem { return LumiaTokens.space4 }
        if inItem, case .list = blocks[index] { return LumiaTokens.space4 }
        return LumiaTokens.space8
    }

    /// Whether "(edited)" follows the last paragraph's words: the body ends
    /// in a paragraph, not in code, a quote or a list, after which it
    /// stands on a line of its own.
    public static func editedInline(_ blocks: [BodyBlock]) -> Bool {
        if case .para = blocks.last { return true }
        return false
    }

    /// Every code block's text, in order, quotes and lists included: what
    /// each one's Copy copies.
    public static func codes(_ blocks: [BodyBlock]) -> [String] {
        blocks.flatMap { block -> [String] in
            switch block {
            case .para: []
            case let .code(_, text): [text]
            case let .quote(inner): codes(inner)
            case let .list(_, items): items.flatMap { codes($0.blocks) }
            }
        }
    }
}

// MARK: Code blocks

public enum CodeCopy {
    public static let copy = "Copy"
    public static let copied = "Copied"
    /// What a screen reader calls the button.
    public static let label = "Copy code"
    /// How long the button says "Copied".
    public static let copiedSeconds = 1.5

    /// The row's action for its `n`th code block of `count`: "Copy code",
    /// numbered when there is more than one.
    public static func action(_ n: Int, of count: Int) -> String {
        count > 1 ? "\(label) \(n + 1)" : label
    }
}

// MARK: Collapse

/// A long message drawn short: lines are counted as drawn, at the width
/// and the font size the row has, not as typed. A body taller than 14
/// lines shows its first 10, fading into the row under them, with Show
/// more under it.
public enum BodyCollapse {
    /// More lines than this collapse.
    public static let over = 14
    /// How many lines a collapsed body shows.
    public static let shown = 10

    public static let more = "Show more"
    public static let less = "Show less"

    /// The button's words, and its value for a screen reader.
    public static func words(expanded: Bool) -> String { expanded ? less : more }
    public static func value(expanded: Bool) -> String { expanded ? "expanded" : "collapsed" }

    public static func collapses(lines: Int) -> Bool { lines > over }

    /// How many lines a body `height` tall is, a line being `line` tall
    /// with `spacing` of it between two lines and none after the last.
    public static func lines(height: CGFloat, line: CGFloat, spacing: CGFloat) -> Int {
        guard line > 0, height > 0 else { return 0 }
        return Int(((height + spacing) / line).rounded())
    }

    /// The height of `lines` lines.
    public static func height(lines: Int, line: CGFloat, spacing: CGFloat) -> CGFloat {
        max(0, CGFloat(lines) * line - spacing)
    }

    /// Whether a body `height` tall collapses.
    public static func collapses(height: CGFloat, line: CGFloat, spacing: CGFloat) -> Bool {
        collapses(lines: lines(height: height, line: line, spacing: spacing))
    }
}

// MARK: The composer's counter

/// The composer's counter under the instance's limit (the core's
/// `messageRoom`): shown from 90% of it, "N characters left", and over it
/// "N characters too long", when Send is refused. Characters are what a
/// person sees as one.
public enum MessageCounter {
    public static func over(_ room: MessageRoom?) -> Bool { (room?.left ?? 0) < 0 }

    /// "1,204 characters left", "1 character left", "3 characters too long".
    public static func words(left: Int64) -> String {
        let n = left.magnitude
        let count = grouped(n) + (n == 1 ? " character" : " characters")
        return left < 0 ? "\(count) too long" : "\(count) left"
    }

    /// A count with a comma between each three digits.
    static func grouped(_ n: UInt64) -> String {
        var digits = Array(String(n))
        var out: [Character] = []
        while digits.count > 3 {
            out = [","] + digits.suffix(3) + out
            digits.removeLast(3)
        }
        return String(digits + out)
    }

    /// Whether a draft has just gone over, which a screen reader is told
    /// once, and not again on every key while it stays over.
    public static func wentOver(from old: MessageRoom?, to new: MessageRoom?) -> Bool {
        over(new) && !over(old)
    }
}
