// A server's own emoji drawn (specs/emoji.md, the "Emoji …" boards): one
// as its picture fitted in a box, playing when animated and still under
// Reduce Motion, named `:name:` to assistive technology; and the `:` list
// over the composer. In text an emoji is drawn by `EmojiInline` in
// `BodyComponents.swift`. Tokens only.
import LumiaFFI
import SwiftUI

/// One of a server's emoji as its picture, fitted in a square of `side`
/// at its shape (the boards' `CustomEmoji` with a box): a chip's 18, the
/// `:` list's 24, the picker's 28, a settings row's 32. Nothing is drawn
/// while its picture is on its way.
public struct LumiaCustomEmoji: View {
    let drawing: EmojiDrawing
    let side: CGFloat
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    public init(_ drawing: EmojiDrawing, side: CGFloat) { self.drawing = drawing; self.side = side }

    public var body: some View {
        let size = drawing.fitted(in: side)
        Group {
            if let picture = drawing.picture {
                if picture.animated, !EmojiPicture.still(reduceMotion) {
                    TimelineView(.animation(minimumInterval: picture.shortest)) { frame in
                        still(picture.frame(at: frame.date.timeIntervalSinceReferenceDate))
                    }
                } else {
                    still(picture.frame(at: nil))
                }
            } else {
                Color.clear
            }
        }
        .frame(width: size.width, height: size.height)
        .frame(width: side, height: side)
        .accessibilityElement()
        .accessibilityLabel(drawing.label)
        .accessibilityAddTraits(.isImage)
    }

    private func still(_ frame: CGImage) -> some View {
        Image(decorative: frame, scale: 1).resizable().interpolation(.high).aspectRatio(contentMode: .fit)
    }
}

/// The `:` list over the composer (the "Emoji composer" board's
/// `EmojiList`): the server's emoji whose names match what follows the
/// `:`, in the core's order, each with its picture and `:name:`; one chosen
/// completes it with a space after.
public struct LumiaEmojiList: View {
    public struct Row: Identifiable {
        public let id: String
        public let name: String
        public let drawing: EmojiDrawing?

        public init(id: String, name: String, drawing: EmojiDrawing?) { self.id = id; self.name = name; self.drawing = drawing }
    }

    let rows: [Row]
    let pick: (Int) -> Void

    public init(rows: [Row], pick: @escaping (Int) -> Void) { self.rows = rows; self.pick = pick }

    public var body: some View {
        VStack(alignment: .leading, spacing: LumiaTokens.space2) {
            ForEach(Array(rows.enumerated()), id: \.element.id) { n, row in
                Button { pick(n) } label: {
                    HStack(spacing: LumiaTokens.space10) {
                        Group {
                            if let drawing = row.drawing { LumiaCustomEmoji(drawing, side: EmojiMetrics.list) } else { Color.clear }
                        }
                        .frame(width: LumiaTokens.avatarXs, height: LumiaTokens.avatarXs)
                        Text(verbatim: ":\(row.name):").font(LumiaTokens.textMember.font).foregroundStyle(LumiaTokens.ink.color).lineLimit(1)
                        Spacer(minLength: 0)
                    }
                    .padding(.horizontal, LumiaTokens.space10)
                    .frame(minHeight: LumiaTokens.controlHeight)
                    .contentShape(RoundedRectangle(cornerRadius: LumiaTokens.radius8))
                }
                .buttonStyle(.plain)
                .accessibilityElement(children: .ignore)
                .accessibilityLabel(":\(row.name):")
                .accessibilityAddTraits(.isButton)
                .accessibilityIdentifier("emoji-option-\(row.name)")
            }
        }
        .padding(LumiaTokens.space6)
        .background(LumiaTokens.surface.color, in: RoundedRectangle(cornerRadius: LumiaTokens.radius14))
        .overlay(RoundedRectangle(cornerRadius: LumiaTokens.radius14).stroke(LumiaTokens.lineStrong.color))
        .shadow(color: .black.opacity(0.1), radius: 12, y: 6)
        .accessibilityElement(children: .contain)
        .accessibilityLabel("Emoji")
    }
}
