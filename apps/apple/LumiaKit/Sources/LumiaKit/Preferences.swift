// Preferences: how Lumia looks on this phone. Light or dark (or as the
// phone is), and how a conversation's messages are drawn: their layout,
// the room between them, their avatars, their text's size, and (once
// attachments and unfurling exist) whether images and embeds are shown or
// left as links. Each takes effect as it is made and is kept in
// `UserDefaults` for the next launch, each on its own key and each read as
// its default when missing or not understood. Comfortable, Compact and
// Minimal are presets that write the message settings and are not stored
// themselves. They are this phone's, not the account's: nothing here is
// sent to the instance, and signing out leaves them as they are. The same
// choices, value names and defaults on every client (the plan's "Display
// settings"), without the desktop's scroll bars: a phone's scroll
// indicators are the system's.
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

/// How a conversation's messages are laid out.
public enum MessageLayout: String, CaseIterable, Sendable {
    /// The blocks as the boards draw them: the avatar, the name and the time
    /// over a person's first message, the rest under it.
    case grouped
    /// Every message on a line of its own that starts with its time and its
    /// name, `12:04  Priya  message text`, wrapping under its text's start.
    case oneLine = "one_line"

    public static let standard = MessageLayout.grouped

    public init(stored: String?) { self = stored.flatMap(MessageLayout.init(rawValue:)) ?? .standard }

    public var label: String {
        switch self {
        case .grouped: "Grouped"
        case .oneLine: "One line"
        }
    }
}

/// How much room there is around and between messages.
public enum MessageSpacing: String, CaseIterable, Sendable {
    /// The boards' room: what Comfortable was before there were settings.
    case roomy
    /// Half of it: what Compact was.
    case cozy
    /// Cozy with less over and between blocks; in one line, no room between lines.
    case tight

    public static let standard = MessageSpacing.roomy

    public init(stored: String?) { self = stored.flatMap(MessageSpacing.init(rawValue:)) ?? .standard }

    public var label: String {
        switch self {
        case .roomy: "Roomy"
        case .cozy: "Cozy"
        case .tight: "Tight"
        }
    }
}

/// The avatar beside a block of messages, whatever the spacing.
public enum AvatarSize: String, CaseIterable, Sendable {
    case large, small, hidden

    public static let standard = AvatarSize.large

    public init(stored: String?) { self = stored.flatMap(AvatarSize.init(rawValue:)) ?? .standard }

    /// Its side in points; none for hidden, which leaves no gutter.
    public var points: CGFloat {
        switch self {
        case .large: LumiaTokens.avatarLg
        case .small: LumiaTokens.avatarSm
        case .hidden: 0
        }
    }

    public var label: String {
        switch self {
        case .large: "Large"
        case .small: "Small"
        case .hidden: "Hidden"
        }
    }
}

/// Whether an image or an embed is drawn in a message or left as its link.
/// Nothing draws either yet: these are in the model and the presets only,
/// with no control until attachments and unfurling exist.
public enum MediaDisplay: String, CaseIterable, Sendable {
    case shown, link

    public static let standard = MediaDisplay.shown

    public init(stored: String?) { self = stored.flatMap(MediaDisplay.init(rawValue:)) ?? .standard }
}

/// The message text's size in points. Every text in a message row and the
/// composer's are scaled by the same ratio over 15, on top of the system's
/// text size; avatars and room are not.
public struct FontSize: Hashable, Sendable {
    public let points: Int

    /// The sizes there are, smallest first.
    public static let choices = [13, 14, 15, 16, 18, 20].map(FontSize.init(unchecked:))

    /// The design's message text, which is what a phone that has never chosen has.
    public static let standard = FontSize(unchecked: 15)

    private init(unchecked points: Int) { self.points = points }

    /// One of the sizes there are; nil for any other.
    public init?(_ points: Int) {
        guard Self.choices.contains(where: { $0.points == points }) else { return nil }
        self.points = points
    }

    /// The size as it was stored, a number; anything else is the default.
    public init(stored: Any?) {
        self = (stored as? NSNumber).flatMap { number in
            // A number that is not a whole one (15.5) is not a size.
            Double(number.intValue) == number.doubleValue ? FontSize(number.intValue) : nil
        } ?? .standard
    }

    /// What each text in a message row is multiplied by.
    public var scale: CGFloat { CGFloat(points) / LumiaTokens.textMessage.size }

    public var label: String { "\(points)" }
}

/// What a preset writes: every message setting but the font size.
public struct MessageDisplay: Equatable, Sendable {
    public var layout: MessageLayout
    public var spacing: MessageSpacing
    public var avatars: AvatarSize
    public var images: MediaDisplay
    public var embeds: MediaDisplay

    public init(layout: MessageLayout, spacing: MessageSpacing, avatars: AvatarSize, images: MediaDisplay, embeds: MediaDisplay) {
        self.layout = layout; self.spacing = spacing; self.avatars = avatars; self.images = images; self.embeds = embeds
    }

    /// Each setting's own default, which is Comfortable.
    public static let standard = MessageDisplay(layout: .standard, spacing: .standard, avatars: .standard, images: .standard, embeds: .standard)
}

/// Comfortable, Compact and Minimal: names for a set of values, which
/// choosing one writes. A preset is never stored; the page shows the one
/// whose values all match what is set, or none and "Custom".
public enum MessagePreset: String, CaseIterable, Sendable {
    case comfortable, compact, minimal

    public var display: MessageDisplay {
        switch self {
        case .comfortable: MessageDisplay(layout: .grouped, spacing: .roomy, avatars: .large, images: .shown, embeds: .shown)
        case .compact: MessageDisplay(layout: .grouped, spacing: .cozy, avatars: .small, images: .shown, embeds: .shown)
        case .minimal: MessageDisplay(layout: .oneLine, spacing: .tight, avatars: .hidden, images: .link, embeds: .link)
        }
    }

    /// The preset `display` is, if any. One line draws no avatar, so there
    /// the avatars are not compared.
    public static func matching(_ display: MessageDisplay) -> MessagePreset? {
        allCases.first { preset in
            var set = display
            if set.layout == .oneLine, preset.display.layout == .oneLine { set.avatars = preset.display.avatars }
            return set == preset.display
        }
    }

    /// What the page says under the presets when none matches.
    public static let custom = "Custom: these settings match no preset."

    public var label: String {
        switch self {
        case .comfortable: "Comfortable"
        case .compact: "Compact"
        case .minimal: "Minimal"
        }
    }
}

/// How a conversation's rows are drawn: their layout, their avatar and
/// room, which the spacing and the avatars choose, and their text's scale.
/// Roomy is the boards' room, cozy half of it, tight cozy with less over
/// and between blocks.
public struct MessageMeasures: Equatable, Sendable {
    public var layout = MessageLayout.grouped
    /// The avatar's side; 0 when avatars are hidden or in one line.
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
    /// In one line, the room between one message's line and the next's.
    public var line: CGFloat = 0
    /// What every text in a row is multiplied by: the font size over 15,
    /// times the system's text size.
    public var text: CGFloat = 1

    public init(layout: MessageLayout = .grouped, avatar: CGFloat, edge: CGFloat, blockGap: CGFloat, beside: CGFloat, underName: CGFloat, divider: CGFloat, line: CGFloat = 0, text: CGFloat = 1) {
        self.layout = layout; self.avatar = avatar; self.edge = edge; self.blockGap = blockGap; self.beside = beside; self.underName = underName; self.divider = divider; self.line = line; self.text = text
    }

    /// Between the lines of a message at the design's size, the same at
    /// every spacing.
    public static let leading = LumiaTokens.space4

    /// Between the lines of a message as drawn: the leading grows with the text.
    public var lineSpacing: CGFloat { Self.leading * text }

    /// Where the words start: past the avatar and the room beside it, or
    /// at the row's edge when there is no avatar to leave room for.
    public var gutter: CGFloat { avatar > 0 ? avatar + beside : 0 }

    /// Above and below a row inside its block. The boards put a paragraph's
    /// room (`underName`) between two line boxes; a line box there has its
    /// leading in it and a `Text` here does not (the leading is
    /// `lineSpacing`, between lines only), so two messages stand that
    /// leading and the paragraph's room apart, half from each. In one line
    /// it is the leading and the room between lines.
    public var within: CGFloat { (lineSpacing + (layout == .oneLine ? line : underName)) / 2 }

    /// The room a spacing gives with avatars of a size, at the design's text size.
    public static func of(spacing: MessageSpacing, avatars: AvatarSize, layout: MessageLayout = .grouped) -> MessageMeasures {
        let avatar = layout == .oneLine ? 0 : avatars.points
        return switch spacing {
        case .roomy:
            MessageMeasures(layout: layout, avatar: avatar, edge: LumiaTokens.space8, blockGap: LumiaTokens.space8, beside: LumiaTokens.space14, underName: LumiaTokens.space4, divider: LumiaTokens.space8, line: LumiaTokens.space4)
        case .cozy:
            MessageMeasures(layout: layout, avatar: avatar, edge: LumiaTokens.space4, blockGap: LumiaTokens.space4, beside: LumiaTokens.space10, underName: LumiaTokens.space2, divider: LumiaTokens.space4, line: LumiaTokens.space2)
        case .tight:
            MessageMeasures(layout: layout, avatar: avatar, edge: LumiaTokens.space2, blockGap: LumiaTokens.space2, beside: LumiaTokens.space10, underName: LumiaTokens.space2, divider: LumiaTokens.space4, line: 0)
        }
    }

    /// The rows as `display` and `fontSize` have them, with `system`, the
    /// system's text size as a ratio (1 at the phone's default), on top.
    public static func of(_ display: MessageDisplay, fontSize: FontSize = .standard, system: CGFloat = 1) -> MessageMeasures {
        var m = of(spacing: display.spacing, avatars: display.avatars, layout: display.layout)
        m.text = fontSize.scale * system
        return m
    }

    /// Comfortable at the design's size, which is what was drawn before there were settings.
    public static let standard = MessageMeasures.of(.standard)
}

/// The choices as the app holds them: read once at launch, written as
/// each is made.
@MainActor
@Observable
public final class Preferences {
    public var appearance: Appearance {
        didSet { defaults.set(appearance.rawValue, forKey: Self.appearanceKey) }
    }

    public var layout: MessageLayout {
        didSet { defaults.set(layout.rawValue, forKey: Self.layoutKey) }
    }

    public var spacing: MessageSpacing {
        didSet { defaults.set(spacing.rawValue, forKey: Self.spacingKey) }
    }

    public var avatars: AvatarSize {
        didSet { defaults.set(avatars.rawValue, forKey: Self.avatarsKey) }
    }

    public var fontSize: FontSize {
        didSet { defaults.set(fontSize.points, forKey: Self.fontSizeKey) }
    }

    public var images: MediaDisplay {
        didSet { defaults.set(images.rawValue, forKey: Self.imagesKey) }
    }

    public var embeds: MediaDisplay {
        didSet { defaults.set(embeds.rawValue, forKey: Self.embedsKey) }
    }

    /// The message settings a preset writes, together; set, each is stored.
    public var display: MessageDisplay {
        get { MessageDisplay(layout: layout, spacing: spacing, avatars: avatars, images: images, embeds: embeds) }
        set {
            layout = newValue.layout; spacing = newValue.spacing; avatars = newValue.avatars
            images = newValue.images; embeds = newValue.embeds
        }
    }

    /// The preset whose values are all set, or nil for Custom. Choosing one
    /// writes its values; the font size stays as it is.
    public var preset: MessagePreset? {
        get { MessagePreset.matching(display) }
        set { if let newValue { display = newValue.display } }
    }

    /// How a conversation's rows are drawn, with the system's text size as
    /// a ratio on top of the font size.
    public func measures(system: CGFloat = 1) -> MessageMeasures {
        .of(display, fontSize: fontSize, system: system)
    }

    @ObservationIgnored private let defaults: UserDefaults

    public nonisolated static let appearanceKey = "appearance"
    public nonisolated static let layoutKey = "layout"
    public nonisolated static let spacingKey = "spacing"
    public nonisolated static let avatarsKey = "avatars"
    public nonisolated static let fontSizeKey = "font_size"
    public nonisolated static let imagesKey = "images"
    public nonisolated static let embedsKey = "embeds"

    /// What a debug build's launch may name, for the smoke runs
    /// (`scripts/smoke-ios.sh`): `LUMIA_DEV_APPEARANCE` (light, dark or
    /// system), `LUMIA_DEV_PRESET` (comfortable, compact or minimal) and
    /// `LUMIA_DEV_FONT_SIZE` (13 to 20). A release build reads none.
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
    /// was there before. Nothing is read from the `density` the phone kept
    /// before there were these settings.
    public init(defaults: UserDefaults = .standard, environment: [String: String] = Preferences.launchEnvironment) {
        self.defaults = defaults
        appearance = environment["LUMIA_DEV_APPEARANCE"].flatMap(Appearance.init(rawValue:))
            ?? Appearance(stored: defaults.string(forKey: Self.appearanceKey))
        let named = environment["LUMIA_DEV_PRESET"].flatMap(MessagePreset.init(rawValue:))?.display
        layout = named?.layout ?? MessageLayout(stored: defaults.string(forKey: Self.layoutKey))
        spacing = named?.spacing ?? MessageSpacing(stored: defaults.string(forKey: Self.spacingKey))
        avatars = named?.avatars ?? AvatarSize(stored: defaults.string(forKey: Self.avatarsKey))
        images = named?.images ?? MediaDisplay(stored: defaults.string(forKey: Self.imagesKey))
        embeds = named?.embeds ?? MediaDisplay(stored: defaults.string(forKey: Self.embedsKey))
        fontSize = environment["LUMIA_DEV_FONT_SIZE"].flatMap(Int.init).flatMap(FontSize.init)
            ?? FontSize(stored: defaults.object(forKey: Self.fontSizeKey))
    }
}
