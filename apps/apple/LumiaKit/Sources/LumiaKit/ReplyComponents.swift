// The pieces architecture 6.6 adds to a conversation, after the design
// system's (apps/web/src/design/components.tsx and components.css) and the
// "Inline replies" and phone boards (the body itself, its mentions as pills
// by name, is `BodyComponents.swift`): a reply's reference line above it,
// the reaction chips under it, the reply bar inside the composer, the bar over
// it after a send that mentioned people outside the channel, the @ list,
// the "New" line, the count badges and the field a message becomes while
// edited. Tokens only.
import LumiaFFI
import SwiftUI

/// A reply's reference line, above its message (the "Inline replies"
/// board): a connector from the avatar below, the quoted person's small
/// avatar and name (`@` and this account's own for a reply to it, in the
/// accent's ink), and the quoted text on one line, cut short. The whole of
/// it is one button that jumps to the original. A deleted original says so
/// instead and does nothing; a quote whose author is not known (a personal
/// conversation's) leaves the avatar and name out.
public struct LumiaReplyReference: View {
    let quote: QuoteDisplay
    /// The message rows' text scale; the avatar and the connector keep their size.
    var scale: CGFloat = 1

    public init(quote: QuoteDisplay, scale: CGFloat = 1) { self.quote = quote; self.scale = scale }

    /// The quoted person's avatar, and the deleted original's mark.
    static let mark = LumiaTokens.space18

    public var body: some View {
        if quote.deleted {
            HStack(spacing: LumiaTokens.space6) {
                ReplyConnector()
                Image(systemName: "minus").font(.system(size: LumiaTokens.space10, weight: .bold))
                    .foregroundStyle(LumiaTokens.inkMuted.color)
                    .frame(width: Self.mark, height: Self.mark)
                    .background(LumiaTokens.chip.color, in: Circle())
                Text(Replies.originalDeleted).font(.system(size: LumiaTokens.textLinkSm.size * scale)).italic()
                    .foregroundStyle(LumiaTokens.inkMuted.color).lineLimit(1)
            }
            .accessibilityElement(children: .ignore)
            .accessibilityLabel(Replies.originalDeleted)
        } else {
            Button { quote.open?() } label: {
                HStack(spacing: LumiaTokens.space6) {
                    ReplyConnector()
                    if let name = quote.name {
                        LumiaAvatar(initials: initials(of: name), tone: .of(quote.authorID), size: Self.mark, picture: quote.picture)
                        Text(Replies.referenceName(name, quotesMe: quote.quotesMe)).font(LumiaTokens.textLinkSm.font(scale: scale))
                            .foregroundStyle(quote.quotesMe ? LumiaTokens.accentInk.color : LumiaTokens.inkBody.color)
                            .lineLimit(1).fixedSize()
                    }
                    Text(quote.text).font(.system(size: LumiaTokens.textLinkSm.size * scale))
                        .foregroundStyle(LumiaTokens.inkMuted.color).lineLimit(1).truncationMode(.tail)
                }
                .frame(maxWidth: .infinity, alignment: .leading)
                .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .accessibilityLabel(Replies.referenceLabel(name: quote.name, quotesMe: quote.quotesMe, text: quote.text))
        }
    }
}

/// A reply's reference in one line: a short line over the message at its
/// text's start, `↳ Priya: first words…`, cut short at the line's end, the
/// whole of it a button that jumps to the original (no avatar and no
/// connector, as there is no avatar to come from). A deleted original
/// says so instead and does nothing.
public struct LumiaOneLineReference: View {
    let quote: QuoteDisplay
    let scale: CGFloat

    public init(quote: QuoteDisplay, scale: CGFloat = 1) { self.quote = quote; self.scale = scale }

    /// What leads the line.
    static let arrow = "↳ "

    public var body: some View {
        if quote.deleted {
            Text(verbatim: Self.arrow + Replies.originalDeleted).italic()
                .font(.system(size: LumiaTokens.textLinkSm.size * scale))
                .foregroundStyle(LumiaTokens.inkMuted.color).lineLimit(1)
                .accessibilityLabel(Replies.originalDeleted)
        } else {
            Button { quote.open?() } label: {
                words
                    .font(.system(size: LumiaTokens.textLinkSm.size * scale))
                    .foregroundStyle(LumiaTokens.inkMuted.color)
                    .lineLimit(1).truncationMode(.tail)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .accessibilityLabel(Replies.referenceLabel(name: quote.name, quotesMe: quote.quotesMe, text: quote.text))
        }
    }

    /// The arrow, whom it quotes in the name's weight (in the accent's ink
    /// for this account), and the quoted words.
    private var words: Text {
        guard let name = quote.name else { return Text(verbatim: Self.arrow + quote.text) }
        return Text(verbatim: Self.arrow)
            + Text(verbatim: Replies.referenceName(name, quotesMe: quote.quotesMe) + ":")
                .fontWeight(LumiaTokens.textLinkSm.fontWeight)
                .foregroundStyle(quote.quotesMe ? LumiaTokens.accentInk.color : LumiaTokens.inkBody.color)
            + Text(verbatim: " " + quote.text)
    }
}

/// The reference line's connector: down from the reply's avatar, round the
/// corner and along to the quoted person's (the board's 26 by 12 stroke,
/// set 8 down so its run meets the small avatar's middle).
struct ReplyConnector: View {
    var body: some View {
        ReplyConnectorShape()
            .stroke(LumiaTokens.borderHover.color, style: StrokeStyle(lineWidth: LumiaTokens.space2, lineCap: .round))
            .frame(width: LumiaTokens.space24 + LumiaTokens.space2, height: LumiaTokens.space12)
            .padding(.top, LumiaTokens.space8)
            .accessibilityHidden(true)
    }
}

/// `M1 12V7a5 5 0 0 1 5-5h19`, in its 26 by 12 box.
struct ReplyConnectorShape: Shape {
    func path(in rect: CGRect) -> Path {
        var p = Path()
        p.move(to: CGPoint(x: rect.minX + 1, y: rect.maxY))
        p.addLine(to: CGPoint(x: rect.minX + 1, y: rect.minY + 7))
        p.addArc(tangent1End: CGPoint(x: rect.minX + 1, y: rect.minY + 2), tangent2End: CGPoint(x: rect.minX + 6, y: rect.minY + 2), radius: 5)
        p.addLine(to: CGPoint(x: rect.maxX - 1, y: rect.minY + 2))
        return p
    }
}

/// A message's reactions as the phone boards' chips, the one this account
/// has in the accent; a tap toggles it, a long press says who. Then the way
/// to add another.
public struct LumiaReactionChips: View {
    let reactions: [ReactionDisplay]
    let toggle: (String) -> Void
    let add: (() -> Void)?
    /// The message rows' text scale: a chip's words grow, and the chip with them past its 32.
    let scale: CGFloat

    public init(reactions: [ReactionDisplay], toggle: @escaping (String) -> Void, add: (() -> Void)?, scale: CGFloat = 1) {
        self.reactions = reactions; self.toggle = toggle; self.add = add; self.scale = scale
    }

    /// A chip's height, the phone boards' 32.
    static let height = LumiaTokens.rowHeight

    public var body: some View {
        LumiaFlow(spacing: LumiaTokens.space6) {
            ForEach(reactions) { r in
                Button { toggle(r.emoji) } label: {
                    HStack(spacing: LumiaTokens.space6) {
                        Text(r.emoji).font(LumiaTokens.textCompact.font(scale: scale))
                        Text("\(r.count)").font(LumiaTokens.textLinkSm.font(scale: scale))
                            .foregroundStyle(r.me ? LumiaTokens.accentInk.color : LumiaTokens.inkSecondary.color)
                    }
                    .padding(.horizontal, LumiaTokens.space12)
                    .frame(minHeight: Self.height)
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
                    Image(systemName: "face.smiling").font(LumiaTokens.textCompact.font(scale: scale))
                        .foregroundStyle(LumiaTokens.inkMuted.color)
                        .frame(width: Self.height, height: Self.height)
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

/// What the composer holds while replying: to whom, the "@ On" switch
/// (nil where none is offered: a reply to oneself, a personal
/// conversation), and the way out of it.
public struct ComposerReply {
    public let name: String
    public let mention: Binding<Bool>?
    public let cancel: () -> Void

    public init(name: String, mention: Binding<Bool>?, cancel: @escaping () -> Void) {
        self.name = name; self.mention = mention; self.cancel = cancel
    }
}

/// The reply bar (the "Inline replies" board's composer): a strip inside
/// the composer's frame, over its field: "Replying to **Ada**", the "@ On"
/// switch and Cancel reply. No line of the quoted text.
struct LumiaReplyStrip: View {
    let reply: ComposerReply

    /// The switch's and Cancel reply's height.
    static let control = LumiaTokens.space28

    var body: some View {
        HStack(spacing: LumiaTokens.space10) {
            Image(systemName: "arrow.uturn.left").font(.system(size: LumiaTokens.textLabel.size, weight: .medium))
                .foregroundStyle(LumiaTokens.inkSecondary.color)
                .accessibilityHidden(true)
            (Text(verbatim: "Replying to ") + Text(verbatim: reply.name).fontWeight(.bold).foregroundStyle(LumiaTokens.ink.color))
                .font(.system(size: LumiaTokens.textLabel.size))
                .foregroundStyle(LumiaTokens.inkSecondary.color)
                .lineLimit(1)
                .frame(maxWidth: .infinity, alignment: .leading)
            if let mention = reply.mention {
                let on = mention.wrappedValue
                Button { mention.wrappedValue.toggle() } label: {
                    HStack(spacing: LumiaTokens.space4) {
                        Image(systemName: "at").font(.system(size: LumiaTokens.textLinkSm.size, weight: .bold))
                        Text(Replies.mentionSwitchWords(on))
                    }
                    .font(.system(size: LumiaTokens.textMeta.size, weight: .bold))
                    .foregroundStyle(on ? LumiaTokens.accentInk.color : LumiaTokens.inkQuiet.color)
                    .padding(.horizontal, LumiaTokens.space10)
                    .frame(height: Self.control)
                    .background(on ? LumiaTokens.accentSoft.color : LumiaTokens.chip.color, in: Capsule())
                    .contentShape(Capsule())
                }
                .buttonStyle(.plain)
                .accessibilityLabel(Replies.mentionSwitchLabel(reply.name))
                .accessibilityValue(Replies.mentionSwitchWords(on))
                .accessibilityAddTraits(on ? .isSelected : [])
            }
            Button(action: reply.cancel) {
                Image(systemName: "xmark").font(.system(size: LumiaTokens.textCompact.size, weight: .semibold))
                    .foregroundStyle(LumiaTokens.inkQuiet.color)
                    .frame(width: Self.control, height: Self.control)
                    .contentShape(RoundedRectangle(cornerRadius: LumiaTokens.radius8))
            }
            .buttonStyle(.plain)
            .accessibilityLabel("Cancel reply")
        }
        .padding(.leading, LumiaTokens.space14)
        .padding(.trailing, LumiaTokens.space8)
        .padding(.vertical, LumiaTokens.space14 / 2)
        .background(LumiaTokens.inset.color)
        .overlay(alignment: .bottom) { LumiaTokens.borderComposer.color.frame(height: 1) }
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
        public let picture: URL?

        public init(id: String, name: String, handle: String, note: String, tone: AvatarTone?, picture: URL? = nil) {
            self.id = id; self.name = name; self.handle = handle; self.note = note; self.tone = tone; self.picture = picture
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
                            LumiaAvatar(initials: initials(of: row.name), tone: tone, size: LumiaTokens.avatarXs, picture: row.picture)
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
/// account had read to when the conversation opened: the accent's rule,
/// then "New" (the boards' unread divider).
public struct LumiaNewMessagesLine: View {
    /// The message rows' text scale.
    let scale: CGFloat
    public init(scale: CGFloat = 1) { self.scale = scale }
    public var body: some View {
        HStack(spacing: LumiaTokens.space12) {
            LumiaTokens.accentLine.color.frame(height: 1)
            Text(NewMessagesLine.drawn).font(.system(size: LumiaTokens.textSection.size * scale, weight: .bold)).foregroundStyle(LumiaTokens.accentInk.color)
        }
        .padding(.horizontal, LumiaTokens.space10)
        .padding(.vertical, LumiaTokens.space6)
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(NewMessagesLine.words)
    }
}

/// The design system's count badges: how many, on the accent. `full` is a
/// sidebar row's (20 high); `ringed` the rail's, ringed in the rail's own
/// colour so it stands off the tile it sits on; `small` a quick tile's (18).
public struct LumiaCountBadge: View {
    public enum Style: Sendable { case full, ringed, small }
    let count: UInt32
    let style: Style
    public init(_ count: UInt32, style: Style = .full) { self.count = count; self.style = style }

    public var body: some View {
        let side = style == .small ? LumiaTokens.space18 : LumiaTokens.space18 + LumiaTokens.space2
        let ring = style == .ringed ? LumiaTokens.space2 : 0
        Text("\(count)")
            .font(.system(size: style == .small ? LumiaTokens.textCount.size - 0.5 : LumiaTokens.textCount.size, weight: .bold))
            .foregroundStyle(LumiaTokens.onAccent.color)
            .padding(.horizontal, (style == .full ? LumiaTokens.space12 : LumiaTokens.space10) / 2 + ring)
            .frame(minWidth: side, minHeight: side)
            .background(LumiaTokens.accent.color, in: Capsule())
            .overlay { if ring > 0 { Capsule().strokeBorder(LumiaTokens.rail.color, lineWidth: ring) } }
            .fixedSize()
            .accessibilityHidden(true)
    }
}

/// A message's words as a field, in place of them while its author edits it
/// (architecture 6.6): the design's text field, growing with what it holds
/// up to 12 lines, with the keyboard and the caret at its end. Return saves
/// and Escape cancels on a keyboard that has them (Shift-Return a new line);
/// a phone's own keyboard saves with its Return, and the line under the
/// field says Cancel and Save as buttons, since a phone has no Escape to
/// name. Near the instance's limit (`room`) the counter stands at that
/// line's end, and over it Save and Return do nothing and the words stay.
public struct LumiaMessageEditor: View {
    @Binding var draft: String
    let save: () -> Void
    let cancel: () -> Void
    /// The room a draft has under the limit; nil counts nothing.
    let room: ((String) -> MessageRoom)?
    /// The message rows' text scale, which the words keep while edited.
    let scale: CGFloat
    @FocusState private var focused: Bool
    @State private var newline = false

    public init(draft: Binding<String>, save: @escaping () -> Void, cancel: @escaping () -> Void, room: ((String) -> MessageRoom)? = nil, scale: CGFloat = 1) {
        self._draft = draft; self.save = save; self.cancel = cancel; self.room = room; self.scale = scale
    }

    public var body: some View {
        // What is counted is what is saved: the text without its blanks at either end.
        let left = room?(draft.trimmingCharacters(in: .whitespacesAndNewlines))
        let over = MessageCounter.over(left)
        VStack(alignment: .leading, spacing: LumiaTokens.space6) {
            TextField("", text: $draft, axis: .vertical)
                .focused($focused)
                .textFieldStyle(.plain)
                .font(LumiaTokens.textMessage.font(scale: scale))
                .foregroundStyle(LumiaTokens.inkField.color)
                .lineLimit(1...12)
                .padding(.horizontal, LumiaTokens.space12)
                .padding(.vertical, LumiaTokens.space10)
                .background(LumiaTokens.field.color, in: RoundedRectangle(cornerRadius: LumiaTokens.radius12))
                .overlay(RoundedRectangle(cornerRadius: LumiaTokens.radius12).stroke(LumiaTokens.borderActive.color))
                .onKeyPress(.escape) { cancel(); return .handled }
                .onKeyPress(keys: [.return]) { press in
                    newline = press.modifiers.contains(.shift)
                    return .ignored
                }
                .onChange(of: draft) { old, new in
                    guard MessageEdits.returnTyped(old: old, new: new) else { return }
                    if newline { newline = false; return }
                    draft = old
                    if !MessageCounter.over(room?(old.trimmingCharacters(in: .whitespacesAndNewlines))) { save() }
                }
                .announcesGoingOver(left)
                .accessibilityLabel("Edit message")
                .accessibilityIdentifier("message-editor")
            HStack(spacing: LumiaTokens.space4) {
                Button("Cancel", action: cancel)
                Text(verbatim: "·").accessibilityHidden(true)
                Button("Save", action: save).fontWeight(.semibold).foregroundStyle(LumiaTokens.accentInk.color)
                    .disabled(over)
                    .opacity(over ? 0.5 : 1)
                if let left, left.shown {
                    Spacer(minLength: LumiaTokens.space8)
                    LumiaMessageCounter(room: left, scale: scale)
                }
            }
            .buttonStyle(.plain)
            .font(LumiaTokens.textMeta.font(scale: scale))
            .foregroundStyle(LumiaTokens.inkMuted.color)
        }
        .onAppear { focused = true }
    }
}
