// Presentation helpers shared by the layouts: initials, the avatar tone an
// id hashes to (the same nine as the web), times.
import Foundation
import LumiaFFI
import SwiftUI

public enum AvatarTone: String, CaseIterable, Sendable {
    case peach, blue, purple, mint, amber, sky, stone, green, indigo

    /// The web's `toneFor`: a stable hash of the id over the nine tones.
    public static func of(_ id: String) -> AvatarTone {
        var h: UInt32 = 0
        for u in id.utf16 { h = h &* 31 &+ UInt32(u) }
        return allCases[Int(h % UInt32(allCases.count))]
    }

    public var fill: LumiaColor {
        switch self {
        case .peach: LumiaTokens.tonePeach
        case .blue: LumiaTokens.toneBlue
        case .purple: LumiaTokens.tonePurple
        case .mint: LumiaTokens.toneMint
        case .amber: LumiaTokens.toneAmber
        case .sky: LumiaTokens.toneSky
        case .stone: LumiaTokens.toneStone
        case .green: LumiaTokens.toneGreen
        case .indigo: LumiaTokens.toneIndigo
        }
    }

    public var ink: LumiaColor {
        switch self {
        case .peach: LumiaTokens.tonePeachInk
        case .blue: LumiaTokens.toneBlueInk
        case .purple: LumiaTokens.tonePurpleInk
        case .mint: LumiaTokens.toneMintInk
        case .amber: LumiaTokens.toneAmberInk
        case .sky: LumiaTokens.toneSkyInk
        case .stone: LumiaTokens.toneStoneInk
        case .green: LumiaTokens.toneGreenInk
        case .indigo: LumiaTokens.toneIndigoInk
        }
    }

    /// A server's initials on `tile` in this tone, as the rail boards draw
    /// them: the design system's `tile-ink-*` where it names one, else the
    /// tone's ink.
    public var monogram: LumiaColor {
        switch self {
        case .indigo: LumiaTokens.tileInkIndigo
        case .green: LumiaTokens.tileInkGreen
        default: ink
        }
    }
}

public func initials(of name: String) -> String {
    let parts = name.split(whereSeparator: \.isWhitespace).prefix(2)
    let s = parts.compactMap { $0.first.map { String($0).uppercased() } }.joined()
    return s.isEmpty ? "?" : s
}

/// The picture an account names, as the desktop shell's `avatar_url_of`
/// has it: this account's own from its snapshot (`own`); anyone else's from
/// what the servers list of them (`listed`), else from their profile looked
/// up (`lookedUp`, asked only when no server lists them). Empty for none, and
/// for no account. It is a path as the instance said it; `pictureURL(_:base:)`
/// decides whether it is fetched at all.
func avatarPath(of accountID: String, me: String, own: String, listed: (String) -> MemberSummary?, lookedUp: (String) -> MemberSummary?) -> String {
    guard !accountID.isEmpty else { return "" }
    if accountID == me { return own }
    return (listed(accountID) ?? lookedUp(accountID))?.avatarUrl ?? ""
}

public func time(ofMicros micros: Int64) -> String {
    let date = Date(timeIntervalSince1970: Double(micros) / 1_000_000)
    return date.formatted(date: .omitted, time: .shortened)
}

/// The times of a day that are drawn widest, as `time(ofMicros:)` writes
/// them (two digits of hour, and in 12 hours both halves of the day): one
/// line's times take the room of the wider, so its names start in one
/// column.
public func widestTimes(calendar: Calendar = .current) -> [String] {
    let today = calendar.startOfDay(for: Date())
    return [10, 22].compactMap { hour in
        calendar.date(bySettingHour: hour, minute: 58, second: 0, of: today)?.formatted(date: .omitted, time: .shortened)
    }
}

public func day(ofMicros micros: Int64) -> String {
    let date = Date(timeIntervalSince1970: Double(micros) / 1_000_000)
    return date.formatted(.dateTime.month(.abbreviated).day())
}
