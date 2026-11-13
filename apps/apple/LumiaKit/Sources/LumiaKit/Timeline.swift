// What a conversation's list is before it is drawn: which messages go in
// one block, where a day begins and what it is called, who is typing and
// how that reads. The rules are the desktop shell's
// (apps/desktop/src/ui/timeline.rs, src/session/mod.rs and
// src/ui/conversation.rs); they are values here so `swift test` covers
// them without a screen.
import Foundation
import SwiftUI

// MARK: Blocks

/// Who wrote a message and when: all the block rule reads of it.
public struct MessageStamp: Equatable, Sendable {
    public let authorID: String
    public let atMicros: Int64
    /// It quotes another message: it opens a block of its own, under its
    /// name (architecture 6.6), which the next may join.
    public let quotes: Bool

    public init(authorID: String, atMicros: Int64, quotes: Bool = false) {
        self.authorID = authorID; self.atMicros = atMicros; self.quotes = quotes
    }

    var date: Date { Date(timeIntervalSince1970: Double(atMicros) / 1_000_000) }
}

/// Where a row sits in its block, and in its day.
public struct MessagePlace: Equatable, Sendable {
    /// The block's first message: the one under the avatar, the name and the time.
    public let opens: Bool
    /// Its last: the next row, when there is one, opens another.
    public let closes: Bool
    /// The first message of its local day: the day's divider goes above it.
    public let startsDay: Bool

    public init(opens: Bool = true, closes: Bool = true, startsDay: Bool = false) {
        self.opens = opens; self.closes = closes; self.startsDay = startsDay
    }
}

/// Messages one person sends close together are one block: the avatar,
/// the name and the time once, over the first, and the rest under it.
public enum MessageBlocks {
    /// How long after a message another from the same person still goes in its block.
    public static let withinMicros: Int64 = 5 * 60 * 1_000_000

    /// Whether a message goes in the block of the one before it: the same
    /// person, on the same local day (a day's first message carries the
    /// divider), within `withinMicros` of it. Measured from the message
    /// before, not the block's first, so a run of quick messages stays one
    /// block however long it goes on; and either way round, since two
    /// devices' clocks stamp a conversation's messages.
    public static func joins(_ before: MessageStamp, _ this: MessageStamp, calendar: Calendar = .current) -> Bool {
        !this.authorID.isEmpty
            && this.authorID == before.authorID
            && abs(this.atMicros - before.atMicros) < withinMicros
            && calendar.isDate(this.date, inSameDayAs: before.date)
    }

    /// Each message's place, in the order given (oldest first).
    public static func places(_ stamps: [MessageStamp], calendar: Calendar = .current) -> [MessagePlace] {
        let joined = stamps.indices.map { i in i > 0 && !stamps[i].quotes && joins(stamps[i - 1], stamps[i], calendar: calendar) }
        return stamps.indices.map { i in
            MessagePlace(
                opens: !joined[i],
                closes: i + 1 == stamps.count || !joined[i + 1],
                startsDay: i == 0 || !calendar.isDate(stamps[i].date, inSameDayAs: stamps[i - 1].date)
            )
        }
    }
}

/// How a day reads above its first message, as the web's `dayLabel`:
/// "Today", "Yesterday", or the date, with its year when that is not this one.
public func dayLabel(ofMicros micros: Int64, now: Date = Date(), calendar: Calendar = .current, locale: Locale = .current) -> String {
    let date = Date(timeIntervalSince1970: Double(micros) / 1_000_000)
    if calendar.isDate(date, inSameDayAs: now) { return "Today" }
    // By calendar day, not 24 hours: a day around a clock change is not 24 hours long.
    if let yesterday = calendar.date(byAdding: .day, value: -1, to: now), calendar.isDate(date, inSameDayAs: yesterday) { return "Yesterday" }
    let style = Date.FormatStyle(locale: locale, calendar: calendar, timeZone: calendar.timeZone).month(.wide).day()
    return date.formatted(calendar.isDate(date, equalTo: now, toGranularity: .year) ? style : style.year())
}

// MARK: Typing

/// Who is typing in the servers' channels (protocol 6.7), by channel,
/// whoever began first at the front. Nothing arrives when someone stops:
/// each entry carries when it runs out, on this device's clock.
public struct TypingRoster: Equatable, Sendable {
    private struct Entry: Equatable, Sendable {
        let accountID: String
        var untilMicros: Int64
    }

    private var channels: [String: [Entry]] = [:]

    public init() {}

    /// Someone said they are typing until `untilMicros`. Never this
    /// account (`me`); a later word from the same account only moves its end.
    public mutating func began(channelID: String, accountID: String, untilMicros: Int64, me: String) {
        guard accountID != me else { return }
        var typing = channels[channelID] ?? []
        if let i = typing.firstIndex(where: { $0.accountID == accountID }) {
            typing[i].untilMicros = untilMicros
        } else {
            typing.append(Entry(accountID: accountID, untilMicros: untilMicros))
        }
        channels[channelID] = typing
    }

    /// Whoever's message arrived has finished typing it.
    public mutating func stopped(channelID: String, accountID: String) {
        guard var typing = channels[channelID] else { return }
        typing.removeAll { $0.accountID == accountID }
        channels[channelID] = typing.isEmpty ? nil : typing
    }

    /// Drops whoever's typing has run out by `nowMicros`. True when someone was dropped.
    @discardableResult
    public mutating func expire(nowMicros: Int64) -> Bool {
        let before = channels
        channels = channels.compactMapValues { typing in
            let left = typing.filter { $0.untilMicros > nowMicros }
            return left.isEmpty ? nil : left
        }
        return channels != before
    }

    /// The accounts typing in a channel, in the order they began.
    public func accountIDs(in channelID: String) -> [String] {
        (channels[channelID] ?? []).map(\.accountID)
    }

    /// When the first of those typing runs out; nil when nobody is.
    public var nextEnd: Int64? {
        channels.values.joined().map(\.untilMicros).min()
    }

    public mutating func clear() { channels = [:] }
}

/// What the line under the composer says of those typing: their names,
/// which it draws in stronger ink, and the rest.
public struct TypingWords: Equatable, Sendable {
    public let names: String
    public let rest: String
    public var text: String { names + rest }
}

/// "Grace Hopper is typing…", two people by name, "Several people"
/// beyond that; nil when nobody is.
public func typingWords(_ names: [String]) -> TypingWords? {
    switch names.count {
    case 0: nil
    case 1: TypingWords(names: names[0], rest: " is typing…")
    case 2: TypingWords(names: "\(names[0]) and \(names[1])", rest: " are typing…")
    default: TypingWords(names: "", rest: "Several people are typing…")
    }
}

/// The typing line's three dots, as the boards' `mDot` keyframes: each
/// brightens and dims in turn, the second and third a little behind the
/// one before.
public enum TypingDots {
    public static let count = 3
    /// One cycle, in seconds.
    public static let period = 1.2
    /// How far each dot runs behind the one before it, in seconds.
    public static let lag = 0.15
    /// A dot at rest; it reaches 1 at `peak` of the cycle and is back at `settled`.
    public static let dim = 0.3
    static let peak = 0.4
    static let settled = 0.8
    /// How often the dots are redrawn while someone is typing.
    public static let framesPerSecond = 30.0

    /// The opacity of dot `dot` (0, 1, 2) `seconds` into the animation.
    public static func opacity(dot: Int, at seconds: Double) -> Double {
        let t = (seconds - Double(dot) * lag).truncatingRemainder(dividingBy: period)
        let phase = (t < 0 ? t + period : t) / period
        let ease = UnitCurve.easeInOut
        if phase < peak { return dim + (1 - dim) * ease.value(at: phase / peak) }
        if phase < settled { return 1 - (1 - dim) * ease.value(at: (phase - peak) / (settled - peak)) }
        return dim
    }
}
