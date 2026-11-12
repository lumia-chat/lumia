// Preferences: how Lumia looks on this phone. Two choices, each taking
// effect as it is made and kept in `UserDefaults` for the next launch:
// light or dark (or as the phone is), and how much room messages take.
// They are this phone's, not the account's: nothing here is sent to the
// instance, and signing out leaves them as they are. The same choices,
// names and defaults as the desktop shell's (`apps/desktop/src/settings.rs`),
// without its scroll bars: a phone's scroll indicators are the system's.
import Foundation
import Observation
import SwiftUI

/// Light, dark, or whichever the phone is in.
public enum Appearance: String, CaseIterable, Sendable {
    case light, dark, system

    /// What a phone that has never chosen has.
    public static let standard = Appearance.system

    /// The choice as it was stored; what is not understood is the default.
    public init(stored: String?) { self = stored.flatMap(Appearance.init(rawValue:)) ?? .standard }

    /// Whether the app is dark, given whether the phone is.
    public func dark(systemDark: Bool) -> Bool {
        switch self {
        case .light: false
        case .dark: true
        case .system: systemDark
        }
    }

    /// What the app asks of the system: a scheme, or none, which leaves the
    /// app following the phone as it changes.
    public var colorScheme: ColorScheme? {
        switch self {
        case .light: .light
        case .dark: .dark
        case .system: nil
        }
    }

    public var label: String {
        switch self {
        case .light: "Light"
        case .dark: "Dark"
        case .system: "System"
        }
    }
}

/// How much room a conversation's messages take.
public enum Density: String, CaseIterable, Sendable {
    /// The boards' measures.
    case comfortable
    /// A smaller avatar and less room around each message and between
    /// blocks: more of a conversation on the same screen.
    case compact

    /// What a phone that has never chosen has.
    public static let standard = Density.comfortable

    /// The choice as it was stored; what is not understood is the default.
    public init(stored: String?) { self = stored.flatMap(Density.init(rawValue:)) ?? .standard }

    public var label: String {
        switch self {
        case .comfortable: "Comfortable"
        case .compact: "Compact"
        }
    }
}

/// A message row's measures, which the density chooses: comfortable is the
/// boards'; compact is the same row with the small avatar and half the
/// room, so more of a conversation fits. The text is the same size in
/// both. The desktop shell's `Measures` (`apps/desktop/src/ui/timeline.rs`),
/// value for value.
public struct MessageMeasures: Equatable, Sendable {
    public let avatar: CGFloat
    /// The room over a block's first message and under its last, inside the row.
    public let edge: CGFloat
    /// The room between one block and the next. The boards first drew 2
    /// here, which read as crowded in the app (Ryan, 2026-11-12).
    public let blockGap: CGFloat
    /// Between the avatar and what is beside it.
    public let beside: CGFloat
    /// Between the name and the message under it, and between two messages
    /// of a block: the boards draw a block's messages as the paragraphs of one.
    public let underName: CGFloat
    /// Over and under a day's divider.
    public let divider: CGFloat

    /// Between the lines of a message, the same at either density.
    public static let leading = LumiaTokens.space4

    /// Above and below a row inside its block. The boards put a paragraph's
    /// room (`underName`) between two line boxes; a line box there has its
    /// leading in it and a `Text` here does not (the leading is
    /// `lineSpacing`, between lines only), so two messages stand that
    /// leading and the paragraph's room apart, half from each.
    public var within: CGFloat { (Self.leading + underName) / 2 }

    public static func of(_ density: Density) -> MessageMeasures {
        switch density {
        case .comfortable:
            MessageMeasures(avatar: LumiaTokens.avatarLg, edge: LumiaTokens.space8, blockGap: LumiaTokens.space8, beside: LumiaTokens.space14, underName: LumiaTokens.space4, divider: LumiaTokens.space8)
        case .compact:
            MessageMeasures(avatar: LumiaTokens.avatarSm, edge: LumiaTokens.space4, blockGap: LumiaTokens.space4, beside: LumiaTokens.space10, underName: LumiaTokens.space2, divider: LumiaTokens.space4)
        }
    }
}

/// The choices as the app holds them: read once at launch, written as
/// each is made.
@MainActor
@Observable
public final class Preferences {
    public var appearance: Appearance {
        didSet { defaults.set(appearance.rawValue, forKey: Self.appearanceKey) }
    }

    public var density: Density {
        didSet { defaults.set(density.rawValue, forKey: Self.densityKey) }
    }

    @ObservationIgnored private let defaults: UserDefaults

    public nonisolated static let appearanceKey = "appearance"
    public nonisolated static let densityKey = "density"

    /// What a debug build's launch may name, for the smoke runs
    /// (`scripts/smoke-ios.sh`): `LUMIA_DEV_APPEARANCE` (light, dark or
    /// system) and `LUMIA_DEV_DENSITY` (comfortable or compact). A release
    /// build reads none.
    public nonisolated static var launchEnvironment: [String: String] {
        #if DEBUG
        ProcessInfo.processInfo.environment
        #else
        [:]
        #endif
    }

    /// The stored choices, each the default where nothing is stored or what
    /// is stored is not understood. A choice `environment` names is this
    /// launch's and is not stored: the next launch without it reads what
    /// was there before.
    public init(defaults: UserDefaults = .standard, environment: [String: String] = Preferences.launchEnvironment) {
        self.defaults = defaults
        appearance = environment["LUMIA_DEV_APPEARANCE"].flatMap(Appearance.init(rawValue:))
            ?? Appearance(stored: defaults.string(forKey: Self.appearanceKey))
        density = environment["LUMIA_DEV_DENSITY"].flatMap(Density.init(rawValue:))
            ?? Density(stored: defaults.string(forKey: Self.densityKey))
    }
}
