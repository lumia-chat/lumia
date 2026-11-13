// The types the generated tokens are expressed in, and how they become
// SwiftUI values. Colours resolve per appearance; type styles map onto
// the system fonts until the brand faces ship as TrueType (the design's
// files are WOFF2, which Apple's font loader does not read).
import SwiftUI

/// A colour as the tokens carry it.
public struct LumiaRGBA: Sendable, Hashable {
    public let r: Double, g: Double, b: Double, a: Double
    public init(r: Double, g: Double, b: Double, a: Double) { self.r = r; self.g = g; self.b = b; self.a = a }
    public var color: Color { Color(.sRGB, red: r, green: g, blue: b, opacity: a) }
}

/// A colour with a value per appearance.
public struct LumiaColor: Sendable, Hashable {
    public let light: LumiaRGBA, dark: LumiaRGBA
    public init(light: LumiaRGBA, dark: LumiaRGBA) { self.light = light; self.dark = dark }

    /// The SwiftUI colour that follows the system appearance.
    public var color: Color {
        #if os(macOS)
        Color(nsColor: NSColor(name: nil) { appearance in
            let dark = appearance.bestMatch(from: [.aqua, .darkAqua]) == .darkAqua
            let c = dark ? self.dark : self.light
            return NSColor(srgbRed: c.r, green: c.g, blue: c.b, alpha: c.a)
        })
        #else
        Color(uiColor: UIColor { traits in
            let c = traits.userInterfaceStyle == .dark ? self.dark : self.light
            return UIColor(red: c.r, green: c.g, blue: c.b, alpha: c.a)
        })
        #endif
    }
}

/// A type style: size in points, CSS weight, display face or text face.
public struct LumiaType: Sendable, Hashable {
    public let size: CGFloat, weight: Int, display: Bool
    public init(size: CGFloat, weight: Int, display: Bool) { self.size = size; self.weight = weight; self.display = display }

    /// The CSS weight as the system's nearest.
    public var fontWeight: Font.Weight {
        switch weight {
        case ..<450: .regular
        case ..<550: .medium
        case ..<650: .semibold
        case ..<750: .bold
        default: .heavy
        }
    }

    public var font: Font { font(scale: 1) }

    /// The style at `scale` times its size: a message row's text at the
    /// font size Preferences has.
    public func font(scale: CGFloat) -> Font {
        display ? .system(size: size * scale, weight: fontWeight, design: .rounded) : .system(size: size * scale, weight: fontWeight)
    }
}

public extension Color {
    /// Shorthand: `Color.lumia(\.ink)`.
    static func lumia(_ key: KeyPath<LumiaTokens.Type, LumiaColor>) -> Color { LumiaTokens.self[keyPath: key].color }
}
