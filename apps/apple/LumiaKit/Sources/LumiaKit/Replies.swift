// Quote replies, reactions, mentions and read state (architecture 6.6), as
// values the tests cover: what a quote holds, how a reaction's chip and the
// bar after a send read, the composer's @ list, where a body's mentions
// fall, the counts the sidebars and the badge show, when a conversation is
// marked read, where its "New messages" line goes, and which delivered
// notifications are read. The rules and words are the desktop shell's
// (apps/desktop/src/app/replies.rs, src/session/reads.rs, src/ui/replies.rs)
// and the web's.
import Foundation
import LumiaFFI

public enum Replies {
    /// The quick reactions: the top of a message's actions, and the start of
    /// the picker's "Frequently used".
    public static let quick = ["👍", "❤️", "😂", "🎉", "😮", "😢"]
    /// The most a quote holds, in characters.
    public static let quoteLimit = 2_000
    /// How long a message a jump reached stays marked, in seconds.
    public static let flashSeconds = 1.0
    /// How long the bar says who was invited before it goes, in seconds.
    public static let saidSeconds = 4.0
    /// How many pages further back a jump to a message reads before it gives up.
    public static let seekPages = 5
    /// A page, as the conversation reads them.
    public static let pageSize: UInt32 = 200

    /// The quote of a message: the whole of its body, or the part of it
    /// chosen, trimmed and kept to `quoteLimit` characters.
    public static func quote(messageID: String, authorID: String, body: String, part: String? = nil) -> QuoteSummary {
        let text = (part ?? body).trimmingCharacters(in: .whitespacesAndNewlines)
        return QuoteSummary(messageId: messageID, authorAccountId: authorID, text: String(text.prefix(quoteLimit)), originalDeleted: false, silent: false)
    }

    /// What a quote above a message says for assistive technology, and its
    /// first line over the composer: "Replying to Ada".
    public static func replyingTo(_ name: String) -> String { "Replying to \(name)" }

    /// A quote as assistive technology hears it; `name` is nil for an author not known.
    public static func quoteLabel(name: String?, text: String) -> String {
        if let name { return "Quote of \(name): \(text)" }
        return "Quote: \(text)"
    }

    /// What a quote of a message that was deleted says.
    public static let originalDeleted = "Original message was deleted"

    /// Who reacted with an emoji, for its chip: "You and Ada reacted with 👍",
    /// "Ada, Bo and 3 others reacted with 👍". `name` gives a person's name,
    /// empty for one not known, who is "Someone".
    public static func reactedWords(_ reaction: ReactionSummary, me: String, name: (String) -> String) -> String {
        var names: [String] = reaction.me ? ["You"] : []
        for id in reaction.accountIds where id != me {
            let n = name(id)
            names.append(n.isEmpty ? "Someone" : n)
        }
        let unnamed = max(0, Int(reaction.count) - names.count)
        let who: String
        switch (names.count, unnamed) {
        case (0, let n): who = "\(n) people"
        case (1, 0): who = names[0]
        case (2, 0): who = "\(names[0]) and \(names[1])"
        case (1, let n): who = "\(names[0]) and \(others(n))"
        case (_, let n): who = "\(names[0]), \(names[1]) and \(others(names.count - 2 + n))"
        }
        return "\(who) reacted with \(reaction.reaction)"
    }

    /// The rest after the names: "1 other", "3 others".
    static func others(_ n: Int) -> String { n == 1 ? "1 other" : "\(n) others" }

    /// Of the people a message's mentions name, those not in its channel,
    /// in the order named, each once: not this account, not everyone, and
    /// nobody when who is in the channel is not known.
    public static func outside(_ mentions: [MentionSummary], inside: Set<String>?, me: String) -> [String] {
        guard let inside else { return [] }
        var out: [String] = []
        for m in mentions where !m.everyone && !inside.contains(m.accountId) && m.accountId != me && !out.contains(m.accountId) {
            out.append(m.accountId)
        }
        return out
    }

    /// The bar after a send that mentioned people not in the channel:
    /// "Ada isn’t in #design, so they won’t be notified."
    public static func outsideWords(names: [String], channel: String) -> String {
        switch names.count {
        case 0: ""
        case 1: "\(names[0]) isn’t in #\(channel), so they won’t be notified."
        case 2: "\(names[0]) and \(names[1]) aren’t in #\(channel), so they won’t be notified."
        default: "\(names[0]), \(names[1]) and \(others(names.count - 2)) aren’t in #\(channel), so they won’t be notified."
        }
    }

    /// What the bar says once the add made invitations of some: "Invited Ada
    /// to #design.", or how many.
    public static func invitedWords(names: [String], channel: String) -> String {
        names.count == 1 ? "Invited \(names[0]) to #\(channel)." : "Invited \(names.count) people to #\(channel)."
    }

    /// A row's name for assistive technology with what is unread in it:
    /// "design channel, unread", "Mentions, 2 mentions".
    public static func unreadLabel(_ label: String, unread: UInt32, mentions: UInt32) -> String {
        let words = unreadWords(unread: unread, mentions: mentions)
        return words.isEmpty ? label : "\(label), \(words)"
    }

    /// What is unread in a row, alone: "", "unread", "1 mention", "2
    /// mentions". The phone says it as the row's value, after its name.
    public static func unreadWords(unread: UInt32, mentions: UInt32) -> String {
        switch (unread, mentions) {
        case (0, 0): ""
        case (_, 0): "unread"
        case (_, 1): "1 mention"
        case (_, let n): "\(n) mentions"
        }
    }

    /// The Mentions page's words.
    public static let noMentions = "No mentions yet"
    public static let noMentionsLine = "When someone mentions you or replies to you, it shows up here."

    /// Where a mention was said, on a Mentions page: `#design`, or across
    /// every server (Home's) `Acme · #design`.
    public static func mentionPlace(channel: String, server: String?) -> String {
        guard let server, !server.isEmpty else { return "#\(channel)" }
        return "\(server) · #\(channel)"
    }
}

// MARK: The @ list

/// The `@word` at the end of the composer's draft, which the @ list
/// completes: an `@` at the start or after whitespace, then nothing or what
/// can be in a handle. The phone's field says nothing of its caret to the
/// app on iOS 17, so the word is the one being typed at the draft's end.
public struct MentionQuery: Equatable, Sendable {
    /// The draft before the `@`.
    public let before: String
    /// What follows the `@`.
    public let query: String

    public static func at(_ draft: String) -> MentionQuery? {
        guard let at = draft.lastIndex(of: "@") else { return nil }
        let query = draft[draft.index(after: at)...]
        let handle = query.unicodeScalars.allSatisfy { $0.isASCII && (CharacterSet.alphanumerics.contains($0) || "_.-".unicodeScalars.contains($0)) }
        let opens = at == draft.startIndex || draft[draft.index(before: at)].isWhitespace
        guard handle, opens else { return nil }
        return MentionQuery(before: String(draft[..<at]), query: String(query))
    }

    /// The draft once `handle` is picked: the `@word` becomes the handle and a space.
    public func picking(_ handle: String) -> String { "\(before)@\(handle) " }
}

/// One row of the @ list.
public enum MentionChoice: Equatable, Sendable {
    case everyone
    case member(MemberSummary, inside: Bool)
}

public enum MentionList {
    /// How many rows the list shows.
    public static let rows = 8

    /// The rows for `query`: everyone first for one who may mention
    /// everyone, then the server's members whose handle, display name or
    /// nickname starts with it, those in the channel first, then by name.
    /// `inside` is who is in the channel, nil while that is not known (all
    /// count as in it). Never this account.
    public static func choices(query: String, members: [MemberSummary], inside: Set<String>?, me: String, everyone: Bool, name: (MemberSummary) -> String) -> [MentionChoice] {
        let wanted = query.lowercased()
        let starts = { (s: String) in s.lowercased().hasPrefix(wanted) }
        let people = members
            .filter { $0.accountId != me && (starts($0.handle) || starts($0.displayName) || starts($0.nickname)) }
            .map { (inside: inside?.contains($0.accountId) ?? true, name: name($0).lowercased(), member: $0) }
            .sorted { a, b in a.inside != b.inside ? a.inside : a.name < b.name }
            .map { MentionChoice.member($0.member, inside: $0.inside) }
        let all = (everyone && "everyone".hasPrefix(wanted) ? [MentionChoice.everyone] : []) + people
        return Array(all.prefix(rows))
    }

    /// What the @everyone row notes.
    public static func everyoneNote(channel: String) -> String { "Notify everyone in #\(channel)" }

    /// What a row for someone not in the channel notes.
    public static func notIn(channel: String) -> String { "Not in #\(channel)" }
}

// MARK: Mentions in a body

/// A run of a message's body: plain words, or a mention as written.
public struct BodySegment: Equatable, Sendable {
    public let text: String
    public let mention: MentionSummary?
}

public enum MentionSpans {
    /// A body cut at its mentions' UTF-8 byte ranges (half-open), in order;
    /// a range that is out of the body, overlaps one before it or does not
    /// fall between characters is left as plain words.
    public static func segments(body: String, mentions: [MentionSummary]) -> [BodySegment] {
        let bytes = Array(body.utf8)
        var out: [BodySegment] = []
        var at = 0
        func words(_ from: Int, _ to: Int) -> String? { String(bytes: bytes[from..<to], encoding: .utf8) }
        for m in mentions.sorted(by: { $0.start < $1.start }) {
            let start = Int(m.start), end = Int(m.end)
            guard start >= at, end > start, end <= bytes.count, let said = words(start, end), let plain = words(at, start) else { continue }
            if !plain.isEmpty { out.append(BodySegment(text: plain, mention: nil)) }
            out.append(BodySegment(text: said, mention: m))
            at = end
        }
        if at < bytes.count, let rest = words(at, bytes.count) { out.append(BodySegment(text: rest, mention: nil)) }
        return out.isEmpty ? [BodySegment(text: body, mention: nil)] : out
    }
}

// MARK: Read state

/// The read state the screens count from (the core's `read_states`, read
/// again after each snapshot, applied event and mark): by channel.
public struct ReadCounts: Equatable, Sendable {
    public private(set) var states: [String: ReadStateSummary] = [:]

    public init(_ states: [ReadStateSummary] = []) {
        for s in states { self.states[s.channelId] = s }
    }

    /// How many messages others sent after this account's position, and how
    /// many of those are for it; nothing for a conversation not counted.
    public func unread(_ channelID: String) -> (unread: UInt32, mentions: UInt32) {
        states[channelID].map { ($0.unread, $0.mentions) } ?? (0, 0)
    }

    /// Whether anything in these conversations is unread, and how much of it
    /// is for this account: a server's channels and its DMs, for its tile on
    /// the rail and its Mentions row.
    public func any(_ channels: [ChannelSummary]) -> (unread: Bool, mentions: UInt32) {
        channels.reduce((false, 0)) { sum, c in
            let (u, m) = unread(c.channelId)
            return (sum.0 || u > 0, sum.1 + m)
        }
    }

    /// The app icon's badge: everything unread that is for this account.
    public var badge: UInt32 { states.values.reduce(0) { $0 + $1.mentions } }

    /// Home's Mentions row: the mentions of every server, that is, the badge
    /// without the personal conversations', where every message counts.
    public func serverMentions(personal: [ChannelSummary]) -> UInt32 {
        badge - personal.reduce(0) { $0 + unread($1.channelId).mentions }
    }
}

/// When the open conversation is marked read: its newest message on screen
/// and the app in front, at most once a second each, and only when that
/// moves the position or clears a count.
public struct ReadMarks: Equatable, Sendable {
    public static let every: TimeInterval = 1
    private var marked: [String: (seq: UInt64, at: Date)] = [:]

    public init() {}

    public static func == (a: ReadMarks, b: ReadMarks) -> Bool { a.marked.mapValues(\.seq) == b.marked.mapValues(\.seq) }

    /// Whether to mark `channelID` read through `seq` now; it counts as marked from here.
    public mutating func claim(_ channelID: String, seq: UInt64, state: ReadStateSummary?, now: Date) -> Bool {
        guard seq > 0 else { return false }
        let moves = state.map { seq > $0.readSeq || $0.unread > 0 } ?? true
        let recent = marked[channelID].map { $0.seq >= seq && now.timeIntervalSince($0.at) < Self.every } ?? false
        guard moves, !recent else { return false }
        marked[channelID] = (seq, now)
        return true
    }
}

/// One message of a conversation, as the "New messages" line reads it.
public struct ReadEntry: Equatable, Sendable {
    public let id: String
    public let seq: UInt64
    /// Not counted: a notice, or this account's own.
    public let counts: Bool

    public init(id: String, seq: UInt64, counts: Bool) { self.id = id; self.seq = seq; self.counts = counts }
}

public enum NewMessagesLine {
    public static let words = "New messages"

    /// Where the line goes: above the first message after `readTo` that
    /// someone else sent (as the count counts); nil when there is none.
    public static func at(_ entries: [ReadEntry], readTo: UInt64) -> String? {
        entries.first { $0.seq > readTo && $0.counts }?.id
    }

    /// Where a conversation being opened had been read to, for its line:
    /// only while some of it is unread.
    public static func since(_ state: ReadStateSummary?) -> UInt64? {
        guard let state, state.unread > 0 else { return nil }
        return state.readSeq
    }
}

/// Delivered notifications and what is read: a conversation's go when it
/// is read, here or on another device.
public enum ReadNotifications {
    /// The conversations read since `before`: none of them unread now, and
    /// either some were before or the position moved on. A conversation
    /// whose counts did not move keeps its notifications, which may have
    /// arrived before the event that counts them.
    public static func newlyRead(before: ReadCounts, after: ReadCounts) -> Set<String> {
        Set(after.states.values.filter { now in
            guard now.unread == 0 else { return false }
            guard let was = before.states[now.channelId] else { return false }
            return was.unread > 0 || now.readSeq > was.readSeq
        }.map(\.channelId))
    }

    /// Of the notifications delivered (each with the conversation the
    /// extension wrote into it, nil for the generic notice), the ones to
    /// take away after a read push said the badge is `badge`: every one at
    /// 0, since then nothing that notifies is unread; else those of
    /// conversations with nothing unread in `states`, a conversation not
    /// counted counting as read.
    public static func toRemove(delivered: [(id: String, channelID: String?)], states: ReadCounts?, badge: UInt32) -> [String] {
        if badge == 0 { return delivered.map(\.id) }
        guard let states else { return [] }
        return delivered.compactMap { n in
            guard let channel = n.channelID else { return nil }
            return states.unread(channel).unread == 0 ? n.id : nil
        }
    }
}

// MARK: Emoji

/// One emoji the picker offers: it, Unicode's name and the words a search also finds.
public struct EmojiEntry: Equatable, Sendable, Identifiable {
    public let emoji: String
    public let name: String
    public let keywords: [String]
    public var id: String { emoji }

    public init(emoji: String, name: String, keywords: [String]) {
        self.emoji = emoji; self.name = name; self.keywords = keywords
    }
}

/// The picker's emoji (`EmojiData.swift`, Unicode's list up to Unicode 16
/// in the default skin tone), by Unicode's groups in its order, less those
/// newer than the running system draws: the app runs from iOS 17, and an
/// emoji the system's font lacks draws as a box.
public enum EmojiCatalog {
    /// The newest Unicode version whose emoji `os` draws: 16.0 from iOS 18.4
    /// (and macOS 15.4), 15.1 from iOS 17.4 (macOS 14.4), else 15.0.
    public static func newest(drawnBy os: OperatingSystemVersion, macOS: Bool) -> (Int, Int) {
        let at = { (major: Int, minor: Int) in (os.majorVersion, os.minorVersion) >= (major, minor) }
        if macOS ? at(15, 4) : at(18, 4) { return (16, 0) }
        if macOS ? at(14, 4) : at(17, 4) { return (15, 1) }
        return (15, 0)
    }

    /// What this system draws.
    public static let newestDrawn: (Int, Int) = {
        #if os(macOS)
        newest(drawnBy: ProcessInfo.processInfo.operatingSystemVersion, macOS: true)
        #else
        newest(drawnBy: ProcessInfo.processInfo.operatingSystemVersion, macOS: false)
        #endif
    }()

    public static let groupNames = ["Smileys & emotion", "People & body", "Animals & nature", "Food & drink", "Travel & places", "Activities", "Objects", "Symbols", "Flags"]
    /// How many a search shows at most.
    public static let found = 64

    public static let groups: [(name: String, emoji: [EmojiEntry])] = {
        var by = Array(repeating: [EmojiEntry](), count: groupNames.count)
        for line in emojiData.split(separator: "\n") {
            let parts = line.split(separator: "\t", omittingEmptySubsequences: false)
            guard parts.count == 5, let group = Int(parts[1]), by.indices.contains(group) else { continue }
            let version = parts[4].split(separator: ".").compactMap { Int($0) }
            guard version.count == 2, (version[0], version[1]) <= newestDrawn else { continue }
            by[group].append(EmojiEntry(emoji: String(parts[0]), name: String(parts[2]), keywords: parts[3].split(separator: " ").map(String.init)))
        }
        return zip(groupNames, by).map { ($0, $1) }
    }()

    public static var all: [EmojiEntry] { groups.flatMap(\.emoji) }

    /// The emoji whose name or a keyword holds every word of `wanted`.
    public static func search(_ wanted: String) -> [EmojiEntry] {
        let words = wanted.lowercased().split(whereSeparator: \.isWhitespace).map(String.init)
        guard !words.isEmpty else { return [] }
        return Array(all.lazy.filter { e in
            words.allSatisfy { w in e.name.lowercased().contains(w) || e.keywords.contains { $0.contains(w) } }
        }.prefix(found))
    }

    /// An emoji's name, for its label; the emoji itself when the list does not have it.
    public static func name(of emoji: String) -> String {
        all.first { $0.emoji == emoji }?.name ?? emoji
    }
}

/// "Frequently used": what this phone reacted with, newest first, then the
/// quick six, at most `kept`.
public enum FrequentEmoji {
    public static let kept = 16

    public static func list(recent: [String]) -> [String] {
        var all = Array(recent.prefix(kept))
        for q in Replies.quick where !all.contains(q) { all.append(q) }
        return Array(all.prefix(kept))
    }

    /// `recent` after reacting with `emoji`.
    public static func used(_ emoji: String, recent: [String]) -> [String] {
        Array(([emoji] + recent.filter { $0 != emoji }).prefix(kept))
    }
}
