// The pieces architecture 6.6 adds to a conversation, after the design
// system's (apps/web/src/design/components.tsx and components.css): a
// message body with its mentions as pills, the quote above a message, the
// reaction chips under it, the bars over the composer (the quote being
// replied with, and what a send that mentioned people outside the channel
// can do about it), the @ list, the "New messages" line and the count
// badge. Tokens only.
import LumiaFFI
import SwiftUI

/// A message's words, its mentions drawn as the design's `lu-mention`
/// pills: the accent's ink on its soft fill, as written. A pill naming a
/// person opens them through `openPerson` where given.
public struct MessageBodyText: View {
    let text: String
    let mentions: [MentionSummary]
    let deleted: Bool
    let openPerson: ((String) -> Void)?

    public init(text: String, mentions: [MentionSummary], deleted: Bool = false, openPerson: ((String) -> Void)? = nil) {
        self.text = text; self.mentions = mentions; self.deleted = deleted; self.openPerson = openPerson
    }

    static let scheme = "lumia-mention"

    private var attributed: AttributedString {
        var all = AttributedString()
        for segment in MentionSpans.segments(body: text, mentions: mentions) {
            var part = AttributedString(segment.text)
            if let m = segment.mention {
                part.swiftUI.foregroundColor = LumiaTokens.accentInk.color
                part.swiftUI.backgroundColor = LumiaTokens.accentSoft.color
                part.swiftUI.font = LumiaTokens.textMessage.font.weight(.semibold)
                if !m.everyone, openPerson != nil { part.link = URL(string: "\(Self.scheme):\(m.accountId)") }
            }
            all += part
        }
        return all
    }

    public var body: some View {
        Text(mentions.isEmpty ? AttributedString(text) : attributed)
            .font(LumiaTokens.textMessage.font)
            .lineSpacing(MessageMeasures.leading)
            .foregroundStyle(deleted ? LumiaTokens.inkQuiet.color : LumiaTokens.inkBody.color)
            .italic(deleted)
            .tint(LumiaTokens.accentInk.color)
            .environment(\.openURL, OpenURLAction { url in
                guard url.scheme == Self.scheme else { return .discarded }
                openPerson?(url.absoluteString.replacingOccurrences(of: "\(Self.scheme):", with: ""))
                return .handled
            })
    }
}

/// A quote above a message's words: the accent's rule on its left, the
/// quoted author's name and the text, two lines at most. A tap jumps to
/// the original; a deleted original says so instead and does nothing.
public struct LumiaQuoteBlock: View {
    let quote: QuoteDisplay

    public init(quote: QuoteDisplay) { self.quote = quote }

    public var body: some View {
        HStack(spacing: 0) {
            (quote.deleted ? LumiaTokens.lineStrong.color : LumiaTokens.accent.color)
                .frame(width: 3)
            VStack(alignment: .leading, spacing: 1) {
                if quote.deleted {
                    Text(Replies.originalDeleted).font(LumiaTokens.textCompact.font).foregroundStyle(LumiaTokens.inkMuted.color)
                } else {
                    if let name = quote.name { Text(name).font(LumiaTokens.textSection.font).foregroundStyle(LumiaTokens.inkEmphasis.color) }
                    Text(quote.text).font(LumiaTokens.textCompact.font).foregroundStyle(LumiaTokens.inkSecondary.color).lineLimit(2)
                }
            }
            .padding(.vertical, LumiaTokens.space4)
            .padding(.leading, LumiaTokens.space10)
            .padding(.trailing, LumiaTokens.space12)
            .frame(maxWidth: .infinity, alignment: .leading)
        }
        .background(LumiaTokens.inset.color)
        .clipShape(UnevenRoundedRectangle(bottomTrailingRadius: LumiaTokens.radius8, topTrailingRadius: LumiaTokens.radius8))
        .contentShape(Rectangle())
        .onTapGesture { if !quote.deleted { quote.open?() } }
        .padding(.vertical, LumiaTokens.space2)
    }
}

/// A message's reactions as the design's chips, the one this account has in
/// the accent; a tap toggles it, a long press says who. Then the way to add
/// another.
public struct LumiaReactionChips: View {
    let reactions: [ReactionDisplay]
    let toggle: (String) -> Void
    let add: (() -> Void)?

    public init(reactions: [ReactionDisplay], toggle: @escaping (String) -> Void, add: (() -> Void)?) {
        self.reactions = reactions; self.toggle = toggle; self.add = add
    }

    public var body: some View {
        LumiaFlow(spacing: LumiaTokens.space6) {
            ForEach(reactions) { r in
                Button { toggle(r.emoji) } label: {
                    HStack(spacing: LumiaTokens.space6) {
                        Text(r.emoji).font(LumiaTokens.textCompact.font)
                        Text("\(r.count)").font(LumiaTokens.textLabel.font)
                            .foregroundStyle(r.me ? LumiaTokens.accentInk.color : LumiaTokens.inkBody.color)
                    }
                    .padding(.horizontal, LumiaTokens.space10)
                    .frame(height: LumiaTokens.space28)
                    .background(r.me ? LumiaTokens.accentSoft.color : LumiaTokens.reaction.color, in: Capsule())
                    .overlay(Capsule().stroke(r.me ? LumiaTokens.reactionOnBorder.color : LumiaTokens.reaction.color))
                    .contentShape(Capsule())
                }
                .buttonStyle(.plain)
                .contextMenu { Text(r.words) }
                .accessibilityLabel("\(r.words), \(r.count)")
                .accessibilityAddTraits(r.me ? .isSelected : [])
            }
            if let add {
                Button(action: add) {
                    Image(systemName: "face.smiling").font(.system(size: 15))
                        .foregroundStyle(LumiaTokens.inkMuted.color)
                        .frame(width: LumiaTokens.space28, height: LumiaTokens.space28)
                        .contentShape(Capsule())
                }
                .buttonStyle(.plain)
                .accessibilityLabel("Add reaction")
            }
        }
        .accessibilityElement(children: .contain)
        .accessibilityLabel("Reactions")
    }
}

/// Lays its children out in rows, wrapping where the width runs out.
public struct LumiaFlow: Layout {
    let spacing: CGFloat

    public init(spacing: CGFloat) { self.spacing = spacing }

    public func sizeThatFits(proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) -> CGSize {
        let rows = arrange(subviews, width: proposal.width ?? .infinity)
        return CGSize(width: rows.map(\.width).max() ?? 0, height: rows.reduce(0) { $0 + $1.height } + spacing * CGFloat(max(0, rows.count - 1)))
    }

    public func placeSubviews(in bounds: CGRect, proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) {
        var y = bounds.minY
        for row in arrange(subviews, width: bounds.width) {
            var x = bounds.minX
            for i in row.items {
                let size = subviews[i].sizeThatFits(.unspecified)
                subviews[i].place(at: CGPoint(x: x, y: y + (row.height - size.height) / 2), proposal: .unspecified)
                x += size.width + spacing
            }
            y += row.height + spacing
        }
    }

    private func arrange(_ subviews: Subviews, width: CGFloat) -> [(items: [Int], width: CGFloat, height: CGFloat)] {
        var rows: [(items: [Int], width: CGFloat, height: CGFloat)] = []
        for i in subviews.indices {
            let size = subviews[i].sizeThatFits(.unspecified)
            if let last = rows.last, last.width + spacing + size.width <= width {
                rows[rows.count - 1] = (last.items + [i], last.width + spacing + size.width, max(last.height, size.height))
            } else {
                rows.append(([i], size.width, size.height))
            }
        }
        return rows
    }
}

/// A bar over the composer: the inset, the accent's rule on its left, what
/// it says and what can be done at its end.
public struct LumiaComposerBar<Words: View, End: View>: View {
    @ViewBuilder let words: () -> Words
    @ViewBuilder let end: () -> End

    public init(@ViewBuilder words: @escaping () -> Words, @ViewBuilder end: @escaping () -> End) {
        self.words = words; self.end = end
    }

    public var body: some View {
        HStack(spacing: LumiaTokens.space10) {
            words().frame(maxWidth: .infinity, alignment: .leading)
            end()
        }
        .padding(.horizontal, LumiaTokens.space12)
        .padding(.vertical, LumiaTokens.space8)
        .background {
            ZStack(alignment: .leading) {
                LumiaTokens.inset.color
                LumiaTokens.accent.color.frame(width: LumiaTokens.space2)
            }
            .clipShape(RoundedRectangle(cornerRadius: LumiaTokens.radius12))
        }
    }
}

/// Over the composer while replying: "Replying to Ada", the quote on one
/// line, and the way out of it.
public struct LumiaReplyBar: View {
    let name: String
    let text: String
    let cancel: () -> Void

    public init(name: String, text: String, cancel: @escaping () -> Void) {
        self.name = name; self.text = text; self.cancel = cancel
    }

    public var body: some View {
        LumiaComposerBar {
            VStack(alignment: .leading, spacing: LumiaTokens.space2) {
                (Text("Replying to ") + Text(name).fontWeight(.semibold).foregroundColor(LumiaTokens.inkEmphasis.color))
                    .font(LumiaTokens.textMeta.font).foregroundStyle(LumiaTokens.inkMuted.color).lineLimit(1)
                Text(text).font(LumiaTokens.textCompact.font).foregroundStyle(LumiaTokens.inkSecondary.color).lineLimit(1)
            }
            .accessibilityElement(children: .combine)
        } end: {
            Button(action: cancel) {
                Image(systemName: "xmark").font(.system(size: 13, weight: .semibold))
                    .foregroundStyle(LumiaTokens.inkQuiet.color)
                    .frame(width: LumiaTokens.controlHeight, height: LumiaTokens.controlHeight)
                    .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .accessibilityLabel("Cancel reply")
        }
    }
}

/// The @ list over the composer: everyone first for one who may, then the
/// server's members that match, each marked when not in the channel.
public struct LumiaMentionList: View {
    public struct Row: Identifiable {
        public let id: String
        public let name: String
        /// `@handle`; empty for everyone.
        public let handle: String
        public let note: String
        public let tone: AvatarTone?

        public init(id: String, name: String, handle: String, note: String, tone: AvatarTone?) {
            self.id = id; self.name = name; self.handle = handle; self.note = note; self.tone = tone
        }
    }

    let rows: [Row]
    let pick: (Int) -> Void

    public init(rows: [Row], pick: @escaping (Int) -> Void) { self.rows = rows; self.pick = pick }

    public var body: some View {
        VStack(alignment: .leading, spacing: LumiaTokens.space2) {
            ForEach(Array(rows.enumerated()), id: \.element.id) { n, row in
                Button { pick(n) } label: {
                    HStack(spacing: LumiaTokens.space10) {
                        if let tone = row.tone {
                            LumiaAvatar(initials: initials(of: row.name), tone: tone, size: LumiaTokens.avatarXs)
                        } else {
                            Image(systemName: "at").font(.system(size: 12, weight: .semibold))
                                .foregroundStyle(LumiaTokens.accentInk.color)
                                .frame(width: LumiaTokens.avatarXs, height: LumiaTokens.avatarXs)
                                .background(LumiaTokens.accentSoft.color, in: Circle())
                        }
                        Text(row.name).font(LumiaTokens.textMember.font).foregroundStyle(LumiaTokens.ink.color).lineLimit(1)
                        if !row.handle.isEmpty {
                            Text(row.handle).font(LumiaTokens.textMeta.font).foregroundStyle(LumiaTokens.inkMuted.color).lineLimit(1)
                        }
                        Spacer(minLength: 0)
                        if !row.note.isEmpty {
                            Text(row.note).font(LumiaTokens.textMeta.font).foregroundStyle(LumiaTokens.inkMuted.color).lineLimit(1)
                        }
                    }
                    .padding(.horizontal, LumiaTokens.space10)
                    .frame(minHeight: LumiaTokens.controlHeight)
                    .contentShape(RoundedRectangle(cornerRadius: LumiaTokens.radius8))
                }
                .buttonStyle(.plain)
                .accessibilityElement(children: .ignore)
                .accessibilityLabel([row.name + (row.handle.isEmpty ? "" : " \(row.handle)"), row.note].filter { !$0.isEmpty }.joined(separator: ", "))
                .accessibilityAddTraits(.isButton)
            }
        }
        .padding(LumiaTokens.space6)
        .background(LumiaTokens.surface.color, in: RoundedRectangle(cornerRadius: LumiaTokens.radius14))
        .overlay(RoundedRectangle(cornerRadius: LumiaTokens.radius14).stroke(LumiaTokens.lineStrong.color))
        .shadow(color: .black.opacity(0.1), radius: 12, y: 6)
        .accessibilityElement(children: .contain)
        .accessibilityLabel("People to mention")
    }
}

/// The "New messages" line, above the first message after where this
/// account had read to when the conversation opened.
public struct LumiaNewMessagesLine: View {
    public init() {}
    public var body: some View {
        HStack(spacing: LumiaTokens.space12) {
            LumiaTokens.accent.color.frame(height: 1)
            Text(NewMessagesLine.words).font(LumiaTokens.textSection.font).foregroundStyle(LumiaTokens.accent.color)
        }
        .padding(.horizontal, LumiaTokens.space10)
        .padding(.vertical, LumiaTokens.space8)
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(NewMessagesLine.words)
    }
}

/// The design system's count badge: how many mentions, on the accent.
public struct LumiaCountBadge: View {
    let count: UInt32
    public init(_ count: UInt32) { self.count = count }
    public var body: some View {
        Text("\(count)").font(LumiaTokens.textCount.font).foregroundStyle(LumiaTokens.onAccent.color)
            .padding(.horizontal, LumiaTokens.space6)
            .frame(minWidth: LumiaTokens.space18, minHeight: LumiaTokens.space18)
            .background(LumiaTokens.accent.color, in: Capsule())
            .accessibilityHidden(true)
    }
}
