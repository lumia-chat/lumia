// A message body drawn from the core's blocks (protocol 14, issue #18),
// after the desktop shell's (apps/desktop/src/ui/body): paragraphs with
// their line breaks, emphasis, strike, links, inline code as a chip and
// mentions as the design's pills; code blocks that scroll sideways and
// copy; quotes beside a rule; lists after their markers; "(edited)" after
// the last words. Then what a long one collapses with, and the composer's
// counter. The rules are `Bodies.swift`. Tokens only.
import LumiaFFI
import SwiftUI
#if canImport(UIKit)
import UIKit
#else
import AppKit
#endif

/// The type of code in a message, which the design system does not name
/// yet, derived as the desktop shell's `tokens.rs` does: a code block a
/// step under the message size, inline code at nine tenths of the words
/// around it.
enum BodyType {
    static let code = LumiaTokens.textMessage.size - LumiaTokens.space2
    static let inlineCode: CGFloat = 0.9
    /// A code block's strip over its lines (its language and Copy), and the language's size.
    static let codeStrip: CGFloat = 36
    static let codeLabel: CGFloat = 12
}

/// A message's words as the core reads them (`parseBody`), its mentions
/// drawn as the design's `lu-mention` pills: `@` and the person's name
/// (`name` gives it; a mention of someone not known stays as written), the
/// accent's ink on its soft fill, with the pill's rounded ends where the
/// system can draw them (iOS 18), a plain fill before. A pill naming a
/// person opens them through `openPerson` where given; a link opens in the
/// system's browser. An edited message says so after its last paragraph's
/// words, or on a line of its own after a block that is not one. `scale`
/// is the message rows' text scale, the leading growing with it.
public struct MessageBodyText: View {
    let blocks: [BodyBlock]
    let mentions: [MentionSummary]
    let deleted: Bool
    let edited: Bool
    let name: (String) -> String
    let openPerson: ((String) -> Void)?
    let scale: CGFloat

    /// `text` read by the core; a deleted message's placeholder is drawn as it is.
    public init(text: String, mentions: [MentionSummary], deleted: Bool = false, edited: Bool = false, name: @escaping (String) -> String = { _ in "" }, openPerson: ((String) -> Void)? = nil, scale: CGFloat = 1) {
        let blocks = deleted ? [BodyBlock.para(inline: BodyInline(text: text, runs: [], links: []))] : BodyBlocks.parsed(text, mentions: mentions)
        self.init(blocks: blocks, mentions: deleted ? [] : mentions, deleted: deleted, edited: edited, name: name, openPerson: openPerson, scale: scale)
    }

    /// Blocks already read, as a row that also needs them for its actions has them.
    public init(blocks: [BodyBlock], mentions: [MentionSummary], deleted: Bool = false, edited: Bool = false, name: @escaping (String) -> String = { _ in "" }, openPerson: ((String) -> Void)? = nil, scale: CGFloat = 1) {
        self.blocks = blocks; self.mentions = mentions; self.deleted = deleted; self.edited = edited; self.name = name; self.openPerson = openPerson; self.scale = scale
    }

    static let scheme = "lumia-mention"

    public var body: some View {
        let edited = edited && !deleted
        let inline = BodyBlocks.editedInline(blocks)
        VStack(alignment: .leading, spacing: 0) {
            BodyBlocksView(blocks: blocks, inItem: false, edited: edited && inline, look: look)
            if edited, !inline {
                Text(verbatim: MessageEdits.edited).font(LumiaTokens.textMeta.font(scale: scale)).foregroundStyle(LumiaTokens.inkMuted.color)
                    .padding(.top, LumiaTokens.space4)
            }
        }
        .font(LumiaTokens.textMessage.font(scale: scale))
        .lineSpacing(MessageMeasures.leading * scale)
        .foregroundStyle(deleted ? LumiaTokens.inkQuiet.color : LumiaTokens.inkBody.color)
        .italic(deleted)
        .tint(LumiaTokens.link.color)
        .environment(\.openURL, OpenURLAction { url in
            if url.scheme == Self.scheme {
                openPerson?(url.absoluteString.replacingOccurrences(of: "\(Self.scheme):", with: ""))
                return .handled
            }
            // A message is text someone else wrote: only a web address opens.
            return BodyText.isWeb(url.absoluteString) ? .systemAction : .discarded
        })
    }

    private var look: BodyLook {
        BodyLook(mentions: mentions, name: name, opens: openPerson != nil, scale: scale)
    }
}

/// What every paragraph of a body is drawn with.
struct BodyLook {
    let mentions: [MentionSummary]
    let name: (String) -> String
    /// A pill naming a person opens them.
    let opens: Bool
    let scale: CGFloat
}

/// Blocks in a column, the desktop shell's room between them; the last
/// paragraph carries "(edited)" when `edited`.
struct BodyBlocksView: View {
    let blocks: [BodyBlock]
    let inItem: Bool
    let edited: Bool
    let look: BodyLook

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            ForEach(Array(blocks.enumerated()), id: \.offset) { i, block in
                self.block(block, edited: edited && i == blocks.count - 1)
                    .padding(.top, BodyBlocks.gap(before: i, in: blocks, inItem: inItem))
            }
        }
    }

    @ViewBuilder private func block(_ block: BodyBlock, edited: Bool) -> some View {
        switch block {
        case let .para(inline):
            BodyParagraph(inline: inline, edited: edited, look: look)
        case let .code(lang, text):
            LumiaCodeBlock(text: text, lang: lang, scale: look.scale)
        case let .quote(inner):
            // The rule as tall as what it quotes.
            HStack(alignment: .top, spacing: 0) {
                RoundedRectangle(cornerRadius: LumiaTokens.space2).fill(LumiaTokens.lineStrong.color)
                    .frame(width: LumiaTokens.space2).frame(maxHeight: .infinity)
                AnyView(BodyBlocksView(blocks: inner, inItem: false, edited: false, look: look))
                    .padding(.leading, LumiaTokens.space12)
                    .frame(maxWidth: .infinity, alignment: .leading)
            }
            .fixedSize(horizontal: false, vertical: true)
        case let .list(start, items):
            VStack(alignment: .leading, spacing: LumiaTokens.space4) {
                ForEach(Array(items.enumerated()), id: \.offset) { k, item in
                    HStack(alignment: .firstTextBaseline, spacing: 0) {
                        Text(verbatim: BodyBlocks.marker(start: start, at: k))
                            .foregroundStyle(LumiaTokens.inkSecondary.color)
                            .padding(.trailing, LumiaTokens.space8 * look.scale)
                            .frame(width: LumiaTokens.space28 * look.scale, alignment: .trailing)
                            .accessibilityHidden(true)
                        AnyView(BodyBlocksView(blocks: item.blocks, inItem: true, edited: false, look: look))
                            .frame(maxWidth: .infinity, alignment: .leading)
                    }
                }
            }
        }
    }
}

/// A paragraph as one `Text`, so it wraps as one: its words with their
/// styles, each mention a pill and each inline code a chip, "(edited)"
/// after it when `edited`. A word too long for the line breaks rather
/// than overflow the row.
struct BodyParagraph: View {
    let inline: BodyInline
    let edited: Bool
    let look: BodyLook

    /// The pill's and the chip's room either side of their words: a narrow
    /// space that does not break, as near as running words come to the
    /// design's padding.
    static let pad = "\u{202F}"

    var body: some View {
        if #available(iOS 18, macOS 15, *) {
            words(rounded: true).textRenderer(BodyChipRenderer(mention: LumiaTokens.accentSoft.color, code: LumiaTokens.inset.color))
        } else {
            words(rounded: false)
        }
    }

    /// The words as one `Text`; `rounded` marks the pills and chips for the
    /// renderer that draws their ends.
    private func words(rounded: Bool) -> Text {
        var all = Text(verbatim: "")
        for piece in BodyText.pieces(inline) {
            switch piece.kind {
            case .words:
                all = all + Text(styled(piece.text))
            case .code:
                if rounded, #available(iOS 18, macOS 15, *) {
                    all = all + Text(chip(piece.text, filled: false)).customAttribute(BodyChip(code: true))
                } else {
                    all = all + Text(chip(piece.text, filled: true))
                }
            case let .mention(index):
                guard look.mentions.indices.contains(index) else {
                    all = all + Text(styled(piece.text))
                    continue
                }
                let mention = look.mentions[index]
                let segment = BodySegment(text: String(piece.text.characters), mention: mention)
                if rounded, #available(iOS 18, macOS 15, *) {
                    all = all + Text(pill(segment, mention, filled: false)).customAttribute(BodyChip(code: false))
                } else {
                    all = all + Text(pill(segment, mention, filled: true))
                }
            }
        }
        if edited {
            all = all + Text(verbatim: " " + MessageEdits.edited).font(LumiaTokens.textMeta.font(scale: look.scale)).foregroundStyle(LumiaTokens.inkMuted.color)
        }
        return all
    }

    /// Plain words: a line through what is struck, links in the link style.
    private func styled(_ text: AttributedString) -> AttributedString {
        var text = text
        for (intent, range) in text.runs[\.inlinePresentationIntent] where intent?.contains(.strikethrough) == true {
            text[range].swiftUI.strikethroughStyle = .single
        }
        for (link, range) in text.runs[\.link] where link != nil {
            text[range].swiftUI.foregroundColor = LumiaTokens.link.color
            text[range].swiftUI.underlineStyle = Text.LineStyle(pattern: .solid, color: LumiaTokens.link.color.opacity(0.4))
        }
        return text
    }

    /// Inline code: the mono face a step under the words, on the inset ground.
    private func chip(_ text: AttributedString, filled: Bool) -> AttributedString {
        var part = styled(AttributedString(Self.pad) + text + AttributedString(Self.pad))
        part.swiftUI.font = .system(size: LumiaTokens.textMessage.size * BodyType.inlineCode * look.scale, design: .monospaced)
        if filled { part.swiftUI.backgroundColor = LumiaTokens.inset.color }
        return part
    }

    private func pill(_ segment: BodySegment, _ mention: MentionSummary, filled: Bool) -> AttributedString {
        // One pill, never cut across two lines: its spaces do not break.
        let said = segment.drawn(name: look.name).replacingOccurrences(of: " ", with: "\u{00A0}")
        var part = AttributedString(Self.pad + said + Self.pad)
        part.swiftUI.foregroundColor = LumiaTokens.accentInk.color
        part.swiftUI.font = LumiaTokens.textMessage.font(scale: look.scale).weight(.semibold)
        if filled { part.swiftUI.backgroundColor = LumiaTokens.accentSoft.color }
        if !mention.everyone, look.opens { part.link = URL(string: "\(MessageBodyText.scheme):\(mention.accountId)") }
        return part
    }
}

/// Marks a mention's run, or inline code's, for `BodyChipRenderer`.
@available(iOS 18, macOS 15, *)
struct BodyChip: TextAttribute {
    let code: Bool
}

/// Draws each mention's pill and each inline code's chip under its words:
/// the soft fill with the design's 6-point corners for a pill, the inset
/// ground with smaller ones for code, a point over and under the line.
@available(iOS 18, macOS 15, *)
struct BodyChipRenderer: TextRenderer {
    let mention: Color
    let code: Color

    func draw(layout: Text.Layout, in ctx: inout GraphicsContext) {
        for line in layout {
            // A pill may be several runs on a line: one fill for them all.
            var chip: (rect: CGRect, code: Bool)?
            func close() {
                if let (rect, isCode) = chip {
                    let shape = Path(roundedRect: rect.insetBy(dx: 0, dy: -LumiaTokens.space2 / 2), cornerRadius: isCode ? LumiaTokens.radius5 : LumiaTokens.radius6)
                    ctx.fill(shape, with: .color(isCode ? code : mention))
                }
                chip = nil
            }
            for run in line {
                if let mark = run[BodyChip.self] {
                    if let open = chip, open.code == mark.code {
                        chip = (open.rect.union(run.typographicBounds.rect), mark.code)
                    } else {
                        close()
                        chip = (run.typographicBounds.rect, mark.code)
                    }
                } else {
                    close()
                }
            }
            close()
            for run in line { ctx.draw(run) }
        }
    }
}

/// A fenced code block: its lines in the mono face on the inset ground,
/// never wrapped (a long line scrolls the block sideways, inside itself)
/// and never scrolled up and down (it is as tall as its lines). On a phone
/// Copy is always shown, so it sits in a strip of its own over the code,
/// with the language the fence names, and never covers a line.
public struct LumiaCodeBlock: View {
    let text: String
    let lang: String?
    let scale: CGFloat

    public init(text: String, lang: String? = nil, scale: CGFloat = 1) { self.text = text; self.lang = lang; self.scale = scale }

    public var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            HStack(spacing: LumiaTokens.space8) {
                Text(verbatim: lang ?? "")
                    .font(.system(size: BodyType.codeLabel * scale, design: .monospaced))
                    .foregroundStyle(LumiaTokens.inkMuted.color)
                    .lineLimit(1)
                    .accessibilityHidden(lang == nil)
                Spacer(minLength: 0)
                LumiaCopyCode(text: text, scale: scale)
            }
            .padding(.leading, LumiaTokens.space12)
            .padding(.trailing, LumiaTokens.space4)
            .frame(minHeight: BodyType.codeStrip * scale)
            .overlay(alignment: .bottom) { Rectangle().fill(LumiaTokens.line.color).frame(height: 1) }
            ScrollView(.horizontal) {
                Text(verbatim: text)
                    .font(.system(size: BodyType.code * scale, design: .monospaced))
                    .foregroundStyle(LumiaTokens.inkBody.color)
                    .italic(false)
                    .fixedSize()
                    .padding(.horizontal, LumiaTokens.space12)
                    .padding(.vertical, LumiaTokens.space10)
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(LumiaTokens.inset.color, in: RoundedRectangle(cornerRadius: LumiaTokens.radius12))
        .clipShape(RoundedRectangle(cornerRadius: LumiaTokens.radius12))
        .overlay(RoundedRectangle(cornerRadius: LumiaTokens.radius12).stroke(LumiaTokens.line.color))
    }
}

/// A code block's Copy: its text, without the fences, to the clipboard;
/// then "Copied" for a moment, which a screen reader is told.
struct LumiaCopyCode: View {
    let text: String
    let scale: CGFloat
    @State private var copied = false

    var body: some View {
        Button {
            Self.copy(text)
            copied = true
            Task {
                try? await Task.sleep(for: .seconds(CodeCopy.copiedSeconds))
                copied = false
            }
        } label: {
            Text(copied ? CodeCopy.copied : CodeCopy.copy)
                .font(LumiaTokens.textLinkSm.font(scale: scale))
                .foregroundStyle(LumiaTokens.inkSecondary.color)
                .padding(.horizontal, LumiaTokens.space10)
                .frame(minWidth: LumiaTokens.tileSize, minHeight: BodyType.codeStrip * scale)
                .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .accessibilityLabel(CodeCopy.label)
    }

    /// Copies `text` and says so: what the button and a row's own "Copy code" do.
    @MainActor
    static func copy(_ text: String) {
        #if canImport(UIKit)
        UIPasteboard.general.string = text
        #else
        NSPasteboard.general.clearContents()
        NSPasteboard.general.setString(text, forType: .string)
        #endif
        AccessibilityNotification.Announcement(CodeCopy.copied).post()
    }
}

// MARK: Collapse

extension MessageMeasures {
    /// One line of a message's words as drawn: the font's own line at the
    /// text scale, and the leading under it.
    public var lineHeight: CGFloat {
        let size = LumiaTokens.textMessage.size * text
        #if canImport(UIKit)
        let font = UIFont.systemFont(ofSize: size).lineHeight
        #else
        let system = NSFont.systemFont(ofSize: size)
        let font = system.ascender - system.descender + system.leading
        #endif
        return font + lineSpacing
    }
}

/// A body drawn short while it is taller than `BodyCollapse.over` lines
/// and not `expanded`: its first `BodyCollapse.shown` lines, the last of
/// them fading into whatever is under the row (its wash, its hover). `tall`
/// says whether it is that long, measured as drawn, for Show more.
struct BodyCollapsing: ViewModifier {
    let measures: MessageMeasures
    let expanded: Bool
    @Binding var tall: Bool

    func body(content: Content) -> some View {
        let line = measures.lineHeight
        let short = BodyCollapse.height(lines: BodyCollapse.shown, line: line, spacing: measures.lineSpacing)
        let collapsed = tall && !expanded
        content
            .fixedSize(horizontal: false, vertical: true)
            .onGeometryChange(for: Bool.self) { proxy in
                BodyCollapse.collapses(height: proxy.size.height, line: line, spacing: measures.lineSpacing)
            } action: { tall = $0 }
            .frame(height: collapsed ? short : nil, alignment: .top)
            .clipped()
            .mask {
                if collapsed {
                    // From half a line over the last, to nothing at its foot.
                    LinearGradient(stops: [.init(color: .black, location: 0), .init(color: .black, location: max(0, 1 - 1.5 * line / short)), .init(color: .clear, location: 1)], startPoint: .top, endPoint: .bottom)
                } else {
                    Color.black
                }
            }
    }
}

/// Show more under a collapsed body, Show less under one shown whole: a
/// small text button in the accent's ink that says which it is.
struct LumiaShowMore: View {
    let expanded: Bool
    let scale: CGFloat
    let toggle: () -> Void

    var body: some View {
        Button(action: toggle) {
            Text(BodyCollapse.words(expanded: expanded))
                .font(LumiaTokens.textLinkSm.font(scale: scale))
                .foregroundStyle(LumiaTokens.accentInk.color)
                .padding(.vertical, LumiaTokens.space2)
                .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .accessibilityValue(BodyCollapse.value(expanded: expanded))
    }
}

// MARK: The counter

/// The composer's counter, small at its trailing edge: what is left, muted,
/// or how much too long, in the danger ink.
public struct LumiaMessageCounter: View {
    let room: MessageRoom
    let scale: CGFloat

    public init(room: MessageRoom, scale: CGFloat = 1) { self.room = room; self.scale = scale }

    public var body: some View {
        Text(MessageCounter.words(left: room.left))
            .font(LumiaTokens.textMeta.font(scale: scale))
            .monospacedDigit()
            .foregroundStyle(MessageCounter.over(room) ? LumiaTokens.danger.color : LumiaTokens.inkMuted.color)
            .lineLimit(1)
            .fixedSize()
    }
}

extension View {
    /// Tells a screen reader once as a draft goes over the limit, and not
    /// again on every key while it stays over.
    func announcesGoingOver(_ room: MessageRoom?) -> some View {
        onChange(of: room) { old, new in
            guard MessageCounter.wentOver(from: old, to: new), let new else { return }
            AccessibilityNotification.Announcement(MessageCounter.words(left: new.left)).post()
        }
    }
}
