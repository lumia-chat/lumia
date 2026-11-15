// Setting up private messages on this phone (architecture 5.3 and 5.4,
// identity spec 4.1, 4.3 and 4.7, issue #20), as values the tests cover:
// which screen a phone gets from the identity its account is bound to and
// the one it holds (`PrivateMessages`), the Home reminder's seven days
// (`PhraseReminder`), the phrase's words as the screen, the printed page
// and the saved file have them (`PhraseWords`), the three-word check's
// fields (`PhraseCheck`), Restore's one field (`RestoreWords`), its steps
// and the devices it keeps (`RestoreStep`, `RestoreKeep`) and what
// Start over says of the people who will see the key change
// (`StartOverWords`). The core makes, keeps, asks about and checks the
// phrase; these are the phone's words and choices around it.
import Foundation
import LumiaFFI

/// Where this phone stands with private messages.
public enum PrivateMessages: Equatable, Sendable {
    /// Not asked of the instance yet, or the asking did not get through.
    case unknown
    /// This phone holds the identity the account is bound to.
    case ready
    /// Neither the account nor this phone has an identity, or only the
    /// phone does: set up here, silently, binding the account to it.
    case setUp
    /// Setting up needs the person to confirm it's them first: no reauth
    /// token is in hand (a session reopened at launch carries none).
    case confirm
    /// The account has an identity this phone does not hold: it needs Link,
    /// Restore or, as a last resort, Start over ("Private messages need this phone").
    case bringIn
    /// Setting up failed otherwise; the words, and it is tried again.
    case failed(String)

    /// The step for the identity the account is bound to and the one this
    /// phone holds (both hex, nil for none), as the core's `identity_setup`
    /// decides it: the same is ready, the account's alone or a different
    /// one needs bringing in, and an account with none is set up here.
    public static func step(account: String?, device: String?) -> PrivateMessages {
        switch (account, device) {
        case let (account?, device?) where account == device: .ready
        case (_?, _): .bringIn
        case (nil, _): .setUp
        }
    }

    /// What a setup that failed with `error` leaves.
    public static func after(_ error: Error) -> PrivateMessages {
        switch error as? LumiaError {
        case .ReauthNeeded?: .confirm
        case .AccountHasIdentity?: .bringIn
        default: .failed(error.lumiaMessage)
        }
    }

    /// Whether a private message can be started or accepted here.
    public var ready: Bool { self == .ready }

    /// Whether this phone is known to need something before it can: a
    /// state still being found out is not.
    public var needsSomething: Bool {
        switch self {
        case .confirm, .bringIn, .failed: true
        case .unknown, .ready, .setUp: false
        }
    }
}

/// The Home reminder to save the recovery phrase: shown while the phrase
/// waits to be confirmed, unless "In a week" put it off, which holds for
/// seven days on this phone (identity spec 4.1 step 7). Devices' card is
/// not put off.
public struct PhraseReminder: Equatable, Sendable {
    public static let putOffFor: TimeInterval = 7 * 24 * 60 * 60
    static let key = "phrase_reminder_until"

    /// Until when the reminder is put off; nil when it is not.
    public private(set) var until: Date?

    public init(defaults: UserDefaults = .standard) {
        until = defaults.object(forKey: Self.key) as? Date
    }

    /// Whether Home shows it now.
    public func shows(unconfirmed: Bool, now: Date = Date()) -> Bool {
        guard unconfirmed else { return false }
        guard let until else { return true }
        return now >= until
    }

    /// "In a week": put off for seven days from `now`, and kept.
    public mutating func putOff(now: Date = Date(), defaults: UserDefaults = .standard) {
        until = now.addingTimeInterval(Self.putOffFor)
        defaults.set(until, forKey: Self.key)
    }

    /// Nothing put off any more: the phrase was confirmed, so a later one
    /// (after starting over) is reminded of at once.
    public mutating func clear(defaults: UserDefaults = .standard) {
        until = nil
        defaults.removeObject(forKey: Self.key)
    }
}

/// The recovery phrase's 24 words as the phrase screen shows them, as the
/// printed page has them and as a saved file holds them.
public enum PhraseWords {
    public static let count = 24

    /// What the phrase screen, the page and the file say first.
    public static let warning = "Whoever has these 24 words has your identity and your private messages. Lumia can't recover them for you."
    /// The sheet "Save as a file" opens first, its title, line and note.
    public static let saveTitle = "Save your phrase as a file?"
    public static let saveLine = "This file is your identity: anyone who opens it can read your private messages."
    public static let saveNote = "If you save it to iCloud Drive or another folder that syncs, a copy goes to that cloud too. Paper, or a drive you keep offline, is safer."
    /// The name a saved file is offered under.
    public static let fileName = "Lumia recovery phrase"

    /// The words of a phrase, in order, in lower case.
    public static func words(_ phrase: String) -> [String] {
        phrase.lowercased().split(whereSeparator: \.isWhitespace).map(String.init)
    }

    /// The words numbered from 1 in `columns` columns, read down each
    /// column first (1 to 12 on the left, 13 to 24 on the right in two).
    public static func columns(_ words: [String], _ columns: Int) -> [[(number: Int, word: String)]] {
        guard columns > 0, !words.isEmpty else { return [] }
        let numbered = words.enumerated().map { (number: $0.offset + 1, word: $0.element) }
        let rows = (numbered.count + columns - 1) / columns
        return stride(from: 0, to: numbered.count, by: rows).map { Array(numbered[$0..<min($0 + rows, numbered.count)]) }
    }

    /// The saved file: the warning, then a numbered word to a line.
    public static func file(_ words: [String]) -> String {
        var lines = ["Lumia recovery phrase", "", warning, ""]
        lines += words.enumerated().map { "\($0.offset + 1). \($0.element)" }
        return lines.joined(separator: "\n") + "\n"
    }

    /// The printed page: a title, the warning and the numbered words in
    /// two columns, as a plain table: the system's print formatter reads
    /// simple markup only.
    public static func page(_ words: [String]) -> String {
        let cols = columns(words, 2)
        let rows = (0..<(cols.first?.count ?? 0)).map { row in
            cols.map { column in
                guard row < column.count else { return "<td></td><td></td>" }
                let cell = column[row]
                return "<td style=\"padding:4pt 8pt 4pt 0;text-align:right;color:#555\">\(cell.number)</td><td style=\"padding:4pt 32pt 4pt 0;font-family:Menlo,monospace;font-size:15pt\">\(escape(cell.word))</td>"
            }.joined()
        }
        return """
        <html><body style="font-family:Helvetica,sans-serif;color:#000">\
        <h1 style="font-size:20pt">Lumia recovery phrase</h1>\
        <p style="font-size:12pt">\(escape(warning))</p>\
        <table>\(rows.map { "<tr>\($0)</tr>" }.joined())</table>\
        </body></html>
        """
    }

    private static func escape(_ s: String) -> String {
        s.replacingOccurrences(of: "&", with: "&amp;").replacingOccurrences(of: "<", with: "&lt;").replacingOccurrences(of: ">", with: "&gt;")
    }
}

/// "Check your phrase": the words asked for at the core's three positions,
/// each field completed from the wordlist as it is typed.
public struct PhraseCheck: Equatable, Sendable {
    /// The positions asked for, 1 to 24, in the order the core gave.
    public let positions: [UInt32]
    /// What is in each field, by the same order.
    public var answers: [String]

    public init(positions: [UInt32]) {
        self.positions = positions
        answers = positions.map { _ in "" }
    }

    /// A field's words as the core compares them.
    public static func normal(_ text: String) -> String {
        text.trimmingCharacters(in: .whitespacesAndNewlines).lowercased()
    }

    /// The wordlist's words a field offers under it: none while it is
    /// empty, nor once it holds the one word its letters lead to.
    public static func suggestions(_ text: String, completions: (String) -> [String] = { phraseCompletions(prefix: $0) }) -> [String] {
        let typed = normal(text)
        guard !typed.isEmpty else { return [] }
        let found = completions(typed)
        return found == [typed] ? [] : found
    }

    /// Whether a field holds a word of the wordlist.
    public static func isWord(_ text: String, completions: (String) -> [String] = { phraseCompletions(prefix: $0) }) -> Bool {
        let typed = normal(text)
        return !typed.isEmpty && completions(typed).contains(typed)
    }

    /// Confirm does something once every field holds a word of the list.
    public func canConfirm(completions: (String) -> [String] = { phraseCompletions(prefix: $0) }) -> Bool {
        !answers.isEmpty && answers.allSatisfy { Self.isWord($0, completions: completions) }
    }

    /// What is sent: the fields' words.
    public var words: [String] { answers.map(Self.normal) }
}

/// Restore's one field: the 24 words in order, counted as they are typed.
public enum RestoreWords {
    public static func count(_ text: String) -> Int { PhraseWords.words(text).count }

    /// "12 of 24".
    public static func countLabel(_ text: String) -> String { "\(count(text)) of \(PhraseWords.count)" }

    /// Restore turns on at 24 words.
    public static func canRestore(_ text: String) -> Bool { count(text) == PhraseWords.count }

    /// What is sent: the words in lower case, one space between.
    public static func phrase(_ text: String) -> String { PhraseWords.words(text).joined(separator: " ") }
}

/// Restoring's steps (identity spec 4.3 step 3): first whether the person
/// still has a device with their private messages, when linking from it is
/// what they want; then the phrase; then, when the identity has devices
/// restoring would stop, which of them to keep. The phrase is held in the
/// screen's memory between the last two and never kept anywhere.
public enum RestoreStep: Equatable, Sendable {
    case ask, phrase, keep

    /// What follows the phrase, by the devices the instance says restoring
    /// would stop: none, and it restores straight away (nil); else they are
    /// listed to keep.
    public static func afterPhrase(devices: [DeviceSummary]) -> RestoreStep? {
        devices.isEmpty ? nil : .keep
    }
}

/// "Keep any devices?": how each device the restore would stop reads, and
/// what the button says by how many are ticked.
public enum RestoreKeep {
    /// A device's kind as the other clients name it, for one with no label.
    public static func kindName(_ kind: String) -> String {
        switch kind {
        case "web": "Browser"
        case "native": "Computer"
        case "mobile": "Phone"
        default: kind.isEmpty ? "Device" : kind
        }
    }

    public static func name(_ device: DeviceSummary) -> String {
        device.label.isEmpty ? kindName(device.kind) : device.label
    }

    /// The line under the name: "Added Nov 12", or a browser's "On
    /// chat.acme.dev · added Nov 1"; nothing for a device with no date.
    public static func added(_ device: DeviceSummary, calendar: Calendar = .current, locale: Locale = .current) -> String {
        var day = ""
        if device.addedAtMicros > 0 {
            let date = Date(timeIntervalSince1970: Double(device.addedAtMicros) / 1_000_000)
            day = date.formatted(Date.FormatStyle(locale: locale, calendar: calendar, timeZone: calendar.timeZone).month(.abbreviated).day())
        }
        let origin = host(device.webOrigin)
        switch (origin.isEmpty, day.isEmpty) {
        case (true, true): return ""
        case (true, false): return "Added \(day)"
        case (false, true): return "On \(origin)"
        case (false, false): return "On \(origin) · added \(day)"
        }
    }

    /// An origin as a person reads it: "https://chat.acme.dev" is "chat.acme.dev".
    static func host(_ origin: String) -> String {
        for scheme in ["https://", "http://"] where origin.hasPrefix(scheme) {
            return String(origin.dropFirst(scheme.count))
        }
        return origin
    }

    /// The button, by how many devices are ticked.
    public static func button(keeping: Int) -> String {
        switch keeping {
        case 0: "Restore and stop them all"
        case 1: "Restore and keep 1 device"
        default: "Restore and keep \(keeping) devices"
        }
    }
}

/// What Start over says before anything is sent (identity spec 4.7): who
/// will see a key change, which is everyone this account shares a personal
/// conversation with, and nobody when there is none.
public enum StartOverWords {
    /// The other people in this account's personal conversations.
    public static func people(in channels: [ChannelSummary], me: String) -> Int {
        var others = Set<String>()
        for c in channels where personalKinds.contains(c.kind) {
            others.formUnion(c.participantAccountIds.filter { $0 != me })
        }
        return others.count
    }

    public static func keyChange(people: Int) -> String {
        switch people {
        case 0: "Nobody has talked with you privately yet, so nobody will see a key change."
        case 1: "The 1 person you talk to privately will see that your key changed."
        default: "The \(people) people you talk to privately will see that your key changed."
        }
    }
}
