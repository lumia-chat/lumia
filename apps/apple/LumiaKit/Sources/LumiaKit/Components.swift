// The small components both layouts build from, after the design
// system's (apps/web/src/design/components.tsx): avatar, notice, empty
// state, message row, request card, device row, composer, the auth
// card and its fields. Tokens only; no literal colours or sizes.
import LumiaFFI
import SwiftUI

/// A person: their initials on their tone, and their picture over them when
/// they have one (`LumiaSession.picture(of:)`), as the desktop shell's
/// `avatar_of` draws them. The initials show while it loads and in its place
/// when it cannot be had.
public struct LumiaAvatar: View {
    public let initials: String
    public let tone: AvatarTone
    public var size: CGFloat = LumiaTokens.avatarMd
    /// Their picture on the instance; nil for none.
    public var picture: URL?

    public init(initials: String, tone: AvatarTone, size: CGFloat = LumiaTokens.avatarMd, picture: URL? = nil) {
        self.initials = initials; self.tone = tone; self.size = size; self.picture = picture
    }

    public var body: some View {
        LumiaPictureOver(url: picture) {
            Text(initials)
                .font(.system(size: size * 0.4, weight: .semibold, design: .rounded))
                .foregroundStyle(tone.ink.color)
                .frame(width: size, height: size)
                .background(tone.fill.color, in: Circle())
        }
        .frame(width: size, height: size)
        .clipShape(Circle())
    }
}

/// A picture the instance keeps (a server's or a person's) drawn over what
/// stands for it, as on the web and the desktop: what is under shows
/// while the picture loads, and in its place when there is none or it
/// cannot be had. Fills the frame it is given; whoever draws it clips it to
/// its shape.
public struct LumiaPictureOver<Under: View>: View {
    let url: URL?
    @ViewBuilder let under: () -> Under

    public init(url: URL?, @ViewBuilder under: @escaping () -> Under) {
        self.url = url; self.under = under
    }

    public var body: some View {
        // What is under goes once the picture is drawn, as the desktop
        // shell's does: a picture with transparency, or one drawn faded (an
        // absent member's), would show it through.
        if let url {
            AsyncImage(url: url) { phase in
                if let image = phase.image { image.resizable().scaledToFill() } else { under() }
            }
        } else {
            under()
        }
    }
}

public enum NoticeTone { case neutral, accent }

public struct LumiaNotice<Content: View>: View {
    let icon: String
    let tone: NoticeTone
    let action: (label: String, run: () -> Void)?
    @ViewBuilder let content: () -> Content

    public init(icon: String, tone: NoticeTone = .neutral, action: (label: String, run: () -> Void)? = nil, @ViewBuilder content: @escaping () -> Content) {
        self.icon = icon; self.tone = tone; self.action = action; self.content = content
    }

    public var body: some View {
        HStack(alignment: .top, spacing: LumiaTokens.space10) {
            Image(systemName: icon)
                .foregroundStyle(tone == .accent ? LumiaTokens.accentInk.color : LumiaTokens.inkSecondary.color)
                .frame(width: 18)
            content()
                .font(LumiaTokens.textBody.font)
                .foregroundStyle(LumiaTokens.inkBody.color)
                .frame(maxWidth: .infinity, alignment: .leading)
            if let action {
                Button(action.label, action: action.run).buttonStyle(.lumiaSecondary)
            }
        }
        .padding(LumiaTokens.space12)
        .background(tone == .accent ? LumiaTokens.accentWash.color : LumiaTokens.inset.color, in: RoundedRectangle(cornerRadius: LumiaTokens.radius10))
        .overlay(RoundedRectangle(cornerRadius: LumiaTokens.radius10).stroke(tone == .accent ? LumiaTokens.accentLine.color : LumiaTokens.line.color))
    }
}

public struct LumiaEmptyState<Content: View>: View {
    let icon: String
    let title: String
    let action: (label: String, run: () -> Void)?
    @ViewBuilder let content: () -> Content

    public init(icon: String, title: String, action: (label: String, run: () -> Void)? = nil, @ViewBuilder content: @escaping () -> Content = { EmptyView() }) {
        self.icon = icon; self.title = title; self.action = action; self.content = content
    }

    public var body: some View {
        VStack(spacing: LumiaTokens.space12) {
            Image(systemName: icon)
                .font(.system(size: 28))
                .foregroundStyle(LumiaTokens.inkQuiet.color)
            Text(title)
                .font(LumiaTokens.titleChannel.font)
                .foregroundStyle(LumiaTokens.ink.color)
            content()
                .font(LumiaTokens.textBody.font)
                .foregroundStyle(LumiaTokens.inkSecondary.color)
                .multilineTextAlignment(.center)
                .frame(maxWidth: 420)
            if let action {
                Button(action.label, action: action.run).buttonStyle(.lumiaPrimary)
            }
        }
        .frame(maxWidth: .infinity)
        .padding(.vertical, LumiaTokens.space40)
    }
}

/// A message's row, in either layout (`measures.layout`). Grouped, the
/// first of a block is an avatar beside a name, a time and the message;
/// the ones after it are the message alone under those, in the same column
/// (`MessageBlocks` says which is which). A phone has no pointer to show a
/// later row's time under, so it shows none; assistive technology hears
/// whose each row is and when. With avatars hidden there is no column for
/// them: the name starts where the avatar did and opens the profile card
/// as the avatar would. In one line every row is its time, its name and
/// its words, which wrap under their own start, with a reply's reference
/// over the words and the reactions under them at that start. How much
/// room it takes and how large its text is are `measures`, which
/// Preferences chooses. A reply has its reference line over all of it,
/// which always opens a block.
public struct LumiaMessageRow: View {
    public let author: String
    public let tone: AvatarTone
    /// The author's picture, over their initials; nil for none.
    public var picture: URL?
    public let time: String
    public let text: String
    public var deleted = false
    public var place = MessagePlace()
    public var measures = MessageMeasures.standard
    /// Opens who wrote it (their profile card), from the avatar or the name; nil where there is none to open.
    public var openAuthor: (() -> Void)?
    /// What architecture 6.6 adds: whom the body mentions (drawn as pills,
    /// each opening its person through `openPerson`), the reply's reference
    /// line above it, the reactions under it, a wash when it is for this
    /// account (or a jump has just reached it, or it is being replied to),
    /// "(edited)", the field it becomes while edited, and its actions: a
    /// long press opens them, and each is offered to assistive technology.
    public var extras = MessageExtras()
    @State private var hovering = false

    public init(author: String, tone: AvatarTone, picture: URL? = nil, time: String, text: String, deleted: Bool = false, place: MessagePlace = MessagePlace(), measures: MessageMeasures = .standard, openAuthor: (() -> Void)? = nil, extras: MessageExtras = MessageExtras()) {
        self.author = author; self.tone = tone; self.picture = picture; self.time = time; self.text = text; self.deleted = deleted; self.place = place; self.measures = measures; self.openAuthor = openAuthor; self.extras = extras
    }

    /// Whose a row is and when, as assistive technology hears it before the message.
    public static func heading(author: String, time: String) -> String { "\(author), \(time)" }

    /// What was said, as drawn: each mention by name.
    private var said: String {
        if deleted { return "(deleted)" }
        let drawn = MentionSpans.drawn(body: text, mentions: extras.mentions, name: extras.name)
        return extras.edited ? "\(drawn), \(MessageEdits.editedLabel)" : drawn
    }

    private var editing: Bool { extras.edit != nil && !deleted }
    private var oneLine: Bool { measures.layout == .oneLine }
    private var scale: CGFloat { measures.text }

    public var body: some View {
        Group {
            if oneLine { line } else { grouped }
        }
        .padding(.horizontal, LumiaTokens.space12)
        .padding(.top, place.opens && !oneLine ? measures.edge : measures.within)
        .padding(.bottom, place.closes && !oneLine ? measures.edge : measures.within)
        .background {
            background.clipShape(RoundedRectangle(cornerRadius: LumiaTokens.radius12)).accessibilityHidden(true)
        }
        .onHover { hovering = $0 }
        .padding(.bottom, place.closes && !oneLine ? measures.blockGap : 0)
    }

    private var grouped: some View {
        VStack(alignment: .leading, spacing: LumiaTokens.space2) {
            // The reference line's connector starts under the avatar's middle.
            if let quote = extras.quote, !deleted {
                LumiaReplyReference(quote: quote, scale: scale)
                    .padding(.leading, max(0, measures.avatar / 2 - 1))
            }
            VStack(alignment: .leading, spacing: LumiaTokens.space4) {
                if editing { main } else { main.modifier(RowElement(label: Self.heading(author: author, time: time), value: said, actions: actions)) }
                if !extras.reactions.isEmpty, !deleted, !editing {
                    LumiaReactionChips(reactions: extras.reactions, toggle: extras.toggle, add: extras.addReaction, scale: scale)
                        .padding(.leading, measures.gutter)
                }
                if let refused = extras.notSaved, !deleted {
                    LumiaErrorText(refused, scale: scale).padding(.leading, measures.gutter)
                }
            }
        }
    }

    /// One line: the time and the name, then the words in a column of their
    /// own with the reference over them and the reactions under them. The
    /// row's one stop is its words, which say whose they are and when.
    private var line: some View {
        OneLineLayout(gap: LumiaTokens.space8, spacing: LumiaTokens.space2) {
            HStack(alignment: .firstTextBaseline, spacing: LumiaTokens.space8) {
                OneLineTime(time: time, scale: scale)
                Text(author).font(LumiaTokens.textAuthor.font(scale: scale)).foregroundStyle(LumiaTokens.ink.color)
                    .lineLimit(1)
                    .onTapGesture { openAuthor?() }
            }
            .accessibilityHidden(true)
            .contentShape(Rectangle())
            .onLongPressGesture(minimumDuration: 0.4) { if !deleted, !editing { extras.actions?() } }
            if let quote = extras.quote, !deleted {
                LumiaOneLineReference(quote: quote, scale: scale)
            }
            Group {
                if editing { words } else { words.modifier(RowElement(label: Self.heading(author: author, time: time), value: said, actions: actions)) }
            }
            .layoutValue(key: OneLineLayout.Words.self, value: true)
            if !extras.reactions.isEmpty, !deleted, !editing {
                LumiaReactionChips(reactions: extras.reactions, toggle: extras.toggle, add: extras.addReaction, scale: scale)
                    .padding(.top, LumiaTokens.space2)
            }
            if let refused = extras.notSaved, !deleted {
                LumiaErrorText(refused, scale: scale)
            }
        }
    }

    private var background: Color {
        if extras.flashed { return LumiaTokens.accentSoft.color }
        // For this account: the accent's wash, and no rule at its edge.
        if extras.tinted { return LumiaTokens.accentWash.color }
        return hovering || extras.replying || editing ? LumiaTokens.hoverMessage.color : .clear
    }

    private var actions: [(String, () -> Void)] {
        guard !deleted else { return openAuthor.map { [("Open \(author)’s profile", $0)] } ?? [] }
        var all: [(String, () -> Void)] = []
        if let reply = extras.reply { all.append((MessageAction.reply.words, reply)) }
        if let add = extras.addReaction { all.append((MessageAction.addReaction.words, add)) }
        if let edit = extras.editMessage { all.append((MessageAction.edit.words, edit)) }
        if let delete = extras.deleteMessage { all.append((MessageAction.delete.words, delete)) }
        if let actions = extras.actions { all.append(("Message actions", actions)) }
        if let open = extras.quote?.open, extras.quote?.deleted == false { all.append(("Go to the original", open)) }
        if let openAuthor { all.append(("Open \(author)’s profile", openAuthor)) }
        return all
    }

    /// The words, or the field in their place while they are edited.
    @ViewBuilder private var words: some View {
        if let edit = extras.edit, !deleted {
            LumiaMessageEditor(draft: edit.draft, save: edit.save, cancel: edit.cancel, scale: scale)
        } else {
            MessageBodyText(text: deleted ? "(deleted)" : text, mentions: deleted ? [] : extras.mentions, deleted: deleted, edited: extras.edited, name: extras.name, openPerson: extras.openPerson, scale: scale)
                .frame(maxWidth: .infinity, alignment: .leading)
                // In one line the words are a subview of their own, so they take the long press themselves.
                .contentShape(Rectangle())
                .onLongPressGesture(minimumDuration: 0.4) { if !deleted { extras.actions?() } }
        }
    }

    private var main: some View {
        HStack(alignment: .top, spacing: measures.avatar > 0 ? measures.beside : 0) {
            if measures.avatar > 0 {
                if place.opens {
                    LumiaAvatar(initials: initials(of: author), tone: tone, size: measures.avatar, picture: picture)
                        .onTapGesture { openAuthor?() }
                } else {
                    // The avatar's column, empty: the message lines up under the block's first.
                    Color.clear.frame(width: measures.avatar, height: 0)
                }
            }
            VStack(alignment: .leading, spacing: measures.underName) {
                if place.opens {
                    HStack(alignment: .firstTextBaseline, spacing: LumiaTokens.space8) {
                        // The boards' inks, as the desktop shell's row has them.
                        Text(author).font(LumiaTokens.textAuthor.font(scale: scale)).foregroundStyle(LumiaTokens.ink.color)
                            .onTapGesture { openAuthor?() }
                        Text(time).font(LumiaTokens.textMeta.font(scale: scale)).foregroundStyle(LumiaTokens.inkMuted.color)
                    }
                }
                words
            }
        }
        // The whole row takes the long press, not only its words.
        .contentShape(Rectangle())
        .onLongPressGesture(minimumDuration: 0.4) { if !deleted, !editing { extras.actions?() } }
    }
}

/// A message's time in one line, in the meta type, as wide as the widest
/// time of a day, so the names after it start in one column. 12 or 24
/// hours as the system says (`time(ofMicros:)`).
struct OneLineTime: View {
    let time: String
    let scale: CGFloat

    var body: some View {
        ZStack(alignment: .leading) {
            ForEach(widestTimes(), id: \.self) { Text($0).hidden() }
            Text(time).foregroundStyle(LumiaTokens.inkMuted.color)
        }
        .font(LumiaTokens.textMeta.font(scale: scale))
        .monospacedDigit()
        .lineLimit(1)
        .fixedSize()
    }
}

/// Lays out a message in one line: its first subview, the time and the
/// name, at the start, standing on the first baseline of the words (the
/// subview marked `Words`); the rest in a column beside it, top to bottom,
/// `spacing` apart, so the words wrap under their own start and the
/// reference over them and the reactions under them start there too. A
/// long name is cut short before it takes more than `leadShare` of the
/// width.
struct OneLineLayout: Layout {
    let gap: CGFloat
    let spacing: CGFloat
    static let leadShare: CGFloat = 0.45

    struct Words: LayoutValueKey { static let defaultValue = false }

    private struct Frames {
        var lead = CGRect.zero
        var column: [CGRect] = []
        var size = CGSize.zero
    }

    private func frames(_ subviews: Subviews, width: CGFloat?) -> Frames {
        guard let first = subviews.first else { return Frames() }
        let ideal = first.sizeThatFits(.unspecified)
        let leadProposal = ProposedViewSize(width: width.map { min(ideal.width, $0 * Self.leadShare) } ?? ideal.width, height: nil)
        let lead = first.sizeThatFits(leadProposal)
        let leadBaseline = first.dimensions(in: leadProposal)[.firstTextBaseline]
        let x = lead.width + gap
        let columnProposal = ProposedViewSize(width: width.map { max(0, $0 - x) }, height: nil)
        var out = Frames()
        var y: CGFloat = 0
        var baseline: CGFloat?
        for piece in subviews.dropFirst() {
            let size = piece.sizeThatFits(columnProposal)
            if !out.column.isEmpty { y += spacing }
            if piece[Words.self] { baseline = y + piece.dimensions(in: columnProposal)[.firstTextBaseline] }
            out.column.append(CGRect(x: x, y: y, width: size.width, height: size.height))
            y += size.height
        }
        // The lead on the words' first line; a reply's reference is over both.
        var leadY = (baseline ?? leadBaseline) - leadBaseline
        if leadY < 0 {
            out.column = out.column.map { $0.offsetBy(dx: 0, dy: -leadY) }
            y -= leadY
            leadY = 0
        }
        out.lead = CGRect(x: 0, y: leadY, width: lead.width, height: lead.height)
        out.size = CGSize(width: width ?? (out.column.map(\.maxX).max() ?? x), height: max(y, out.lead.maxY))
        return out
    }

    func sizeThatFits(proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) -> CGSize {
        frames(subviews, width: proposal.width).size
    }

    func placeSubviews(in bounds: CGRect, proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) {
        guard let first = subviews.first else { return }
        let f = frames(subviews, width: bounds.width)
        first.place(at: CGPoint(x: bounds.minX + f.lead.minX, y: bounds.minY + f.lead.minY), proposal: ProposedViewSize(f.lead.size))
        for (piece, frame) in zip(subviews.dropFirst(), f.column) {
            piece.place(at: CGPoint(x: bounds.minX + frame.minX, y: bounds.minY + frame.minY), proposal: ProposedViewSize(width: bounds.width - frame.minX, height: nil))
        }
    }
}

/// A message row as one stop for assistive technology, the same whether or
/// not it shows its name and time: "Grace Hopper, 12:01 PM", then what was
/// said; its reference line and its reactions are stops of their own.
private struct RowElement: ViewModifier {
    let label: String
    let value: String
    let actions: [(String, () -> Void)]

    func body(content: Content) -> some View {
        content
            .accessibilityElement(children: .ignore)
            .accessibilityLabel(label)
            .accessibilityValue(value)
            .accessibilityActions {
                ForEach(Array(actions.enumerated()), id: \.offset) { _, a in Button(a.0, action: a.1) }
            }
    }
}

/// A message being edited in place: what the field holds, and its two ways out.
public struct MessageEdit {
    public let draft: Binding<String>
    public let save: () -> Void
    public let cancel: () -> Void

    public init(draft: Binding<String>, save: @escaping () -> Void, cancel: @escaping () -> Void) {
        self.draft = draft; self.save = save; self.cancel = cancel
    }
}

/// What a message row draws beside its words (architecture 6.6).
public struct MessageExtras {
    public var mentions: [MentionSummary] = []
    /// A mentioned person's name as drawn; empty for one not known.
    public var name: (String) -> String = { _ in "" }
    public var quote: QuoteDisplay?
    public var reactions: [ReactionDisplay] = []
    /// It mentions this account or everyone, or quotes this account.
    public var tinted = false
    /// A jump has just reached it.
    public var flashed = false
    /// It is the one being replied to.
    public var replying = false
    /// Its author has edited it.
    public var edited = false
    /// It is being edited: the field in place of its words.
    public var edit: MessageEdit?
    /// What the instance refused of its edit: "Not saved: …".
    public var notSaved: String?
    public var toggle: (String) -> Void = { _ in }
    public var addReaction: (() -> Void)?
    public var reply: (() -> Void)?
    public var editMessage: (() -> Void)?
    public var deleteMessage: (() -> Void)?
    public var actions: (() -> Void)?
    public var openPerson: ((String) -> Void)?

    public init() {}
}

/// A reply's quote as its reference line draws it: who wrote the original
/// (`name` nil for one not known, `authorID` for their avatar's tone),
/// whether that is this account, the text, whether the original is gone,
/// and the jump to it.
public struct QuoteDisplay {
    public let authorID: String
    public let name: String?
    /// The quoted person's picture; nil for none.
    public let picture: URL?
    public let text: String
    public let deleted: Bool
    public let quotesMe: Bool
    public let open: (() -> Void)?

    public init(authorID: String, name: String?, picture: URL? = nil, text: String, deleted: Bool, quotesMe: Bool, open: (() -> Void)?) {
        self.authorID = authorID; self.name = name; self.picture = picture; self.text = text; self.deleted = deleted; self.quotesMe = quotesMe; self.open = open
    }
}

/// One emoji's chip: how many, whether this account is among them, and who.
public struct ReactionDisplay: Identifiable {
    public let emoji: String
    public let count: UInt32
    public let me: Bool
    public let words: String
    public var id: String { emoji }

    public init(emoji: String, count: UInt32, me: Bool, words: String) {
        self.emoji = emoji; self.count = count; self.me = me; self.words = words
    }
}

/// A notice the instance posted in a channel ("Ada added Grace"): one
/// quiet line in the messages' column, with its time after it; no avatar
/// and nothing to do with it. A block of its own (`messageStamp`). In one
/// line its time comes first, in the times' column, then the line.
public struct LumiaNoticeRow: View {
    public let text: String
    public let time: String
    public var measures = MessageMeasures.standard

    public init(text: String, time: String, measures: MessageMeasures = .standard) {
        self.text = text; self.time = time; self.measures = measures
    }

    public var body: some View {
        if measures.layout == .oneLine {
            HStack(alignment: .firstTextBaseline, spacing: LumiaTokens.space8) {
                OneLineTime(time: time, scale: measures.text)
                Text(text).font(LumiaTokens.textMeta.font(scale: measures.text)).foregroundStyle(LumiaTokens.inkSecondary.color)
                Spacer(minLength: 0)
            }
            .padding(.horizontal, LumiaTokens.space12)
            .padding(.vertical, measures.within)
            .accessibilityElement(children: .combine)
        } else {
            HStack(alignment: .firstTextBaseline, spacing: LumiaTokens.space8) {
                Text(text).font(LumiaTokens.textMeta.font(scale: measures.text)).foregroundStyle(LumiaTokens.inkSecondary.color)
                Text(time).font(LumiaTokens.textMeta.font(scale: measures.text)).foregroundStyle(LumiaTokens.inkQuiet.color)
                Spacer(minLength: 0)
            }
            // The message text's column: past the avatar's.
            .padding(.leading, LumiaTokens.space12 + measures.gutter)
            .padding(.trailing, LumiaTokens.space12)
            .padding(.vertical, measures.within)
            .padding(.bottom, measures.blockGap)
            .accessibilityElement(children: .combine)
        }
    }
}

public struct LumiaDayDivider: View {
    public let label: String
    public var measures = MessageMeasures.standard
    public init(label: String, measures: MessageMeasures = .standard) { self.label = label; self.measures = measures }
    public var body: some View {
        HStack(spacing: LumiaTokens.space12) {
            Rectangle().fill(LumiaTokens.line.color).frame(height: 1)
            Text(label).font(LumiaTokens.textSection.font(scale: measures.text)).foregroundStyle(LumiaTokens.inkQuiet.color)
            Rectangle().fill(LumiaTokens.line.color).frame(height: 1)
        }
        .padding(.horizontal, LumiaTokens.space10)
        .padding(.vertical, measures.divider)
    }
}

/// Who is typing (protocol 6.7): three dots and the words, the names in
/// stronger ink. The dots brighten in turn; under Reduce Motion they are
/// still and whole.
public struct LumiaTypingHint: View {
    let words: TypingWords
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    /// A dot (4 across) and the room between two (3), as the boards' `m-typing`.
    static let dot = LumiaTokens.space4
    static let dotGap = (LumiaTokens.space2 + LumiaTokens.space4) / 2

    public init(words: TypingWords) { self.words = words }

    public var body: some View {
        HStack(spacing: LumiaTokens.space8) {
            Group {
                if reduceMotion {
                    dots(at: nil)
                } else {
                    TimelineView(.animation(minimumInterval: 1 / TypingDots.framesPerSecond)) { frame in
                        dots(at: frame.date.timeIntervalSinceReferenceDate)
                    }
                }
            }
            .accessibilityHidden(true)
            (Text(words.names).fontWeight(LumiaTokens.textSection.fontWeight).foregroundStyle(LumiaTokens.inkEmphasis.color) + Text(words.rest))
                .font(LumiaTokens.textMeta.font)
                .foregroundStyle(LumiaTokens.inkMuted.color)
                .lineLimit(1)
        }
        .accessibilityElement(children: .combine)
    }

    /// The dots `seconds` into their cycle, or still and whole for nil.
    private func dots(at seconds: Double?) -> some View {
        HStack(spacing: Self.dotGap) {
            ForEach(0..<TypingDots.count, id: \.self) { n in
                Circle().fill(LumiaTokens.inkMuted.color).frame(width: Self.dot, height: Self.dot)
                    .opacity(seconds.map { TypingDots.opacity(dot: n, at: $0) } ?? 1)
            }
        }
    }
}

public struct LumiaComposer<Above: View>: View {
    @Binding var text: String
    let placeholder: String
    let disabled: Bool
    let hint: String?
    let typing: [String]?
    let focused: FocusState<Bool>.Binding?
    let reply: ComposerReply?
    let editLast: (() -> Void)?
    /// The message rows' text scale (Preferences' font size and the
    /// system's text size), which the field's words take too.
    var textScale: CGFloat = 1
    let send: () -> Void
    @ViewBuilder let above: () -> Above

    /// `typing` names those typing in the conversation, for the line under
    /// the field: nil where there is never such a line (a personal
    /// conversation), and empty where nobody is typing just now, which
    /// keeps the line's room so the field does not move when someone starts.
    /// `above` is what stands over the field (the quote being replied with,
    /// the @ list, the bar after a send); `focused`, where given, is whether
    /// the field has the keyboard. `reply` is the reply bar, drawn inside
    /// the composer's frame over its field, which it rings in the active
    /// border; Escape cancels it. `editLast` is Up in an empty composer on
    /// a keyboard that has one: this account's newest message, edited.
    public init(text: Binding<String>, placeholder: String, disabled: Bool = false, hint: String? = "Return to send", typing: [String]? = nil, focused: FocusState<Bool>.Binding? = nil, reply: ComposerReply? = nil, editLast: (() -> Void)? = nil, textScale: CGFloat = 1, send: @escaping () -> Void, @ViewBuilder above: @escaping () -> Above) {
        self._text = text; self.placeholder = placeholder; self.disabled = disabled; self.hint = hint; self.typing = typing; self.focused = focused; self.reply = reply; self.editLast = editLast; self.textScale = textScale; self.send = send; self.above = above
    }

    private var canSend: Bool { !disabled && !text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty }

    @ViewBuilder private var field: some View {
        let base = TextField(placeholder, text: $text, axis: .vertical)
        if let focused { base.focused(focused) } else { base }
    }

    public var body: some View {
        VStack(alignment: .leading, spacing: LumiaTokens.space8) {
            above()
            VStack(spacing: 0) {
                if let reply { LumiaReplyStrip(reply: reply) }
                HStack(alignment: .center, spacing: LumiaTokens.space2) {
                    Image(systemName: "plus")
                        .font(.system(size: 15, weight: .medium))
                        .foregroundStyle(LumiaTokens.inkQuiet.color)
                        .frame(width: 36, height: 36)
                    field
                        .accessibilityIdentifier("composer")
                        .textFieldStyle(.plain)
                        .lineLimit(1...8)
                        .font(LumiaTokens.textMessage.font(scale: textScale))
                        .foregroundStyle(LumiaTokens.inkField.color)
                        .disabled(disabled)
                        .onSubmit(send)
                        .onKeyPress(.upArrow) {
                            guard text.isEmpty, let editLast else { return .ignored }
                            editLast()
                            return .handled
                        }
                        .onKeyPress(.escape) {
                            guard let reply else { return .ignored }
                            reply.cancel()
                            return .handled
                        }
                        .padding(.horizontal, LumiaTokens.space8)
                        .frame(minHeight: 42)
                    Button(action: send) {
                        Image(systemName: "arrow.up").font(.system(size: 14, weight: .bold))
                            .foregroundStyle(LumiaTokens.onAccent.color)
                            .frame(width: 40, height: 40)
                            .background(LumiaTokens.accent.color, in: Circle())
                            .opacity(canSend ? 1 : 0.5)
                    }
                    .buttonStyle(.plain)
                    .disabled(!canSend)
                    .keyboardShortcut(.return, modifiers: [])
                    .accessibilityLabel("Send")
                    .padding(.leading, LumiaTokens.space4)
                }
                .padding(.vertical, LumiaTokens.space6)
                .padding(.leading, LumiaTokens.space8)
                .padding(.trailing, LumiaTokens.space6)
            }
            .background(LumiaTokens.field.color)
            .clipShape(RoundedRectangle(cornerRadius: LumiaTokens.radius16))
            .overlay(RoundedRectangle(cornerRadius: LumiaTokens.radius16).stroke(reply == nil ? LumiaTokens.borderComposer.color : LumiaTokens.borderActive.color))
            .shadow(color: .black.opacity(0.08), radius: 14, y: 8)
            if typing != nil || hint != nil {
                // The line under the field: who is typing at its start, as
                // the boards have it, and what Return does at its end. A
                // line of its type tall whether or not anyone is typing.
                ZStack(alignment: .leading) {
                    Text(verbatim: " ").font(LumiaTokens.textMeta.font).hidden().accessibilityHidden(true)
                    HStack(spacing: LumiaTokens.space12) {
                        if let words = typingWords(typing ?? []) {
                            LumiaTypingHint(words: words)
                        }
                        Spacer(minLength: 0)
                        if let hint {
                            Text(hint).font(LumiaTokens.textMeta.font).foregroundStyle(LumiaTokens.inkMuted.color)
                        }
                    }
                }
                .padding(.horizontal, LumiaTokens.space8)
            }
        }
        .padding(.horizontal, LumiaTokens.space24)
        .padding(.top, LumiaTokens.space4)
        .padding(.bottom, LumiaTokens.space14)
    }
}

public extension LumiaComposer where Above == EmptyView {
    init(text: Binding<String>, placeholder: String, disabled: Bool = false, hint: String? = "Return to send", typing: [String]? = nil, send: @escaping () -> Void) {
        self.init(text: text, placeholder: placeholder, disabled: disabled, hint: hint, typing: typing, focused: nil, reply: nil, editLast: nil, send: send) { EmptyView() }
    }
}

/// The floating panel a pane sits in: the surface, an 18 px radius, the
/// panel edge and a soft shadow (the design's `lu-panel`).
public struct LumiaPanel: ViewModifier {
    public func body(content: Content) -> some View {
        content
            .background(LumiaTokens.surface.color, in: RoundedRectangle(cornerRadius: LumiaTokens.radius18))
            .overlay(RoundedRectangle(cornerRadius: LumiaTokens.radius18).stroke(LumiaTokens.panelEdge.color))
            .shadow(color: .black.opacity(0.12), radius: 16, y: 8)
    }
}

public extension View {
    func lumiaPanel() -> some View { modifier(LumiaPanel()) }
}

/// A pane's header: a glyph, the title, a rule and a topic, then actions.
public struct LumiaPaneHeader<Glyph: View, Actions: View>: View {
    let title: String
    let topic: String?
    @ViewBuilder let glyph: () -> Glyph
    @ViewBuilder let actions: () -> Actions

    public init(title: String, topic: String? = nil, @ViewBuilder glyph: @escaping () -> Glyph = { EmptyView() }, @ViewBuilder actions: @escaping () -> Actions = { EmptyView() }) {
        self.title = title; self.topic = topic; self.glyph = glyph; self.actions = actions
    }

    public var body: some View {
        VStack(spacing: 0) {
            HStack(spacing: LumiaTokens.space8) {
                glyph()
                Text(title).font(LumiaTokens.titleChannel.font).foregroundStyle(LumiaTokens.ink.color).lineLimit(1)
                if let topic, !topic.isEmpty {
                    Rectangle().fill(LumiaTokens.lineStrong.color).frame(width: 1, height: 20).padding(.horizontal, LumiaTokens.space6)
                    Text(topic).font(LumiaTokens.textBody.font).foregroundStyle(LumiaTokens.inkSecondary.color).lineLimit(1)
                }
                Spacer(minLength: LumiaTokens.space8)
                actions()
            }
            .padding(.leading, LumiaTokens.space24)
            .padding(.trailing, LumiaTokens.space14)
            .frame(height: LumiaTokens.headerHeight)
            Rectangle().fill(LumiaTokens.line.color).frame(height: 1)
        }
    }
}

/// A member in the aside: avatar, name, a line under it.
public struct LumiaMemberRow: View {
    let name: String
    let subtitle: String?
    let tone: AvatarTone
    let picture: URL?
    let tag: String?
    /// `ONLINE`, `AWAY` or `DO_NOT_DISTURB`, shown as a dot; nil for not here.
    let status: String?

    public init(name: String, subtitle: String? = nil, tone: AvatarTone, picture: URL? = nil, tag: String? = nil, status: String? = nil) {
        self.name = name; self.subtitle = subtitle; self.tone = tone; self.picture = picture; self.tag = tag; self.status = status
    }

    private var dot: Color? {
        switch status {
        case "ONLINE": LumiaTokens.online.color
        case "AWAY": LumiaTokens.away.color
        case "DO_NOT_DISTURB": LumiaTokens.unread.color
        default: nil
        }
    }

    public var body: some View {
        HStack(spacing: LumiaTokens.space12) {
            LumiaAvatar(initials: initials(of: name), tone: tone, size: LumiaTokens.avatarMd, picture: picture)
                .overlay(alignment: .bottomTrailing) {
                    if let dot {
                        Circle().fill(dot).frame(width: 10, height: 10)
                            .overlay(Circle().stroke(LumiaTokens.surface.color, lineWidth: 2))
                            .offset(x: 1, y: 1)
                    }
                }
                .opacity(status == nil ? 0.6 : 1)
            VStack(alignment: .leading, spacing: 0) {
                HStack(spacing: LumiaTokens.space6) {
                    Text(name).font(LumiaTokens.textMember.font).foregroundStyle(LumiaTokens.ink.color).lineLimit(1)
                    if let tag { LumiaTag(tag) }
                }
                if let subtitle, !subtitle.isEmpty {
                    Text(subtitle).font(LumiaTokens.textMeta.font).foregroundStyle(LumiaTokens.inkSecondary.color).lineLimit(1)
                }
            }
            Spacer(minLength: 0)
        }
        .padding(.horizontal, LumiaTokens.space10)
        .padding(.vertical, LumiaTokens.space6)
    }
}

/// The design's segmented tabs: one segment each on a track, the current
/// one raised. Also a row of choices where one is chosen (a status, an
/// appearance): `label` names what is being chosen, and each segment is an
/// element assistive technology reads with whether it is the one selected.
public struct LumiaSegmentedTabs<ID: Hashable>: View {
    public struct Tab: Identifiable, Hashable {
        public let id: ID
        public let label: String
        public let count: Int?
        public init(id: ID, label: String, count: Int? = nil) { self.id = id; self.label = label; self.count = count }
    }
    let label: String?
    let tabs: [Tab]
    @Binding var selected: ID

    public init(_ label: String? = nil, tabs: [Tab], selected: Binding<ID>) { self.label = label; self.tabs = tabs; self._selected = selected }

    /// How far a label may shrink to fit its segment: to the tag's size from the button's.
    static var narrowest: CGFloat { LumiaTokens.textTag.size / LumiaTokens.textButtonSecondary.size }

    public var body: some View {
        HStack(spacing: LumiaTokens.space4) {
            ForEach(tabs) { tab in
                let on = tab.id == selected
                Button { selected = tab.id } label: {
                    HStack(spacing: LumiaTokens.space6) {
                        // A label wider than its segment (a third of a phone's card
                        // is narrower than "Do not disturb") is drawn smaller, not cut short.
                        Text(tab.label).font(LumiaTokens.textButtonSecondary.font).lineLimit(1).minimumScaleFactor(Self.narrowest)
                        if let count = tab.count { Text("\(count)").font(LumiaTokens.textTag.font).foregroundStyle(LumiaTokens.inkSecondary.color) }
                    }
                    .foregroundStyle(on ? LumiaTokens.ink.color : LumiaTokens.inkQuiet.color)
                    .frame(maxWidth: .infinity).frame(height: 34)
                    .background(on ? LumiaTokens.tabActive.color : .clear, in: RoundedRectangle(cornerRadius: LumiaTokens.radius9))
                    // The whole segment takes the tap, not only its word.
                    .contentShape(RoundedRectangle(cornerRadius: LumiaTokens.radius9))
                }
                .buttonStyle(.plain)
                .accessibilityAddTraits(on ? .isSelected : [])
            }
        }
        .padding(LumiaTokens.space4)
        .background(LumiaTokens.track.color, in: RoundedRectangle(cornerRadius: LumiaTokens.radius12))
        .accessibilityElement(children: .contain)
        .accessibilityLabel(label ?? "")
    }
}

public struct LumiaRequestCard: View {
    let name: String
    let handle: String?
    let tone: AvatarTone
    let picture: URL?
    let meta: String
    let busy: Bool
    let accept: () -> Void
    let decline: () -> Void
    let block: () -> Void

    public init(name: String, handle: String?, tone: AvatarTone, picture: URL? = nil, meta: String, busy: Bool, accept: @escaping () -> Void, decline: @escaping () -> Void, block: @escaping () -> Void) {
        self.name = name; self.handle = handle; self.tone = tone; self.picture = picture; self.meta = meta; self.busy = busy
        self.accept = accept; self.decline = decline; self.block = block
    }

    public var body: some View {
        HStack(spacing: LumiaTokens.space12) {
            LumiaAvatar(initials: initials(of: name), tone: tone, size: LumiaTokens.avatarLg, picture: picture)
            VStack(alignment: .leading, spacing: LumiaTokens.space2) {
                Text(name).font(LumiaTokens.textAuthor.font).foregroundStyle(LumiaTokens.inkEmphasis.color)
                if let handle, !handle.isEmpty {
                    Text("@\(handle)").font(LumiaTokens.textMeta.font).foregroundStyle(LumiaTokens.inkSecondary.color)
                }
                Text(meta).font(LumiaTokens.textMeta.font).foregroundStyle(LumiaTokens.inkQuiet.color)
            }
            Spacer()
            Button("Accept", action: accept).buttonStyle(.lumiaPrimary)
            Button("Decline", action: decline).buttonStyle(.lumiaSecondary)
            Button("Block", action: block).buttonStyle(.lumiaSecondary)
        }
        .disabled(busy)
        .padding(LumiaTokens.space12)
        .background(LumiaTokens.tile.color, in: RoundedRectangle(cornerRadius: LumiaTokens.radius12))
        .overlay(RoundedRectangle(cornerRadius: LumiaTokens.radius12).stroke(LumiaTokens.line.color))
    }
}

public struct LumiaDeviceRow: View {
    let device: DeviceSummary
    let label: String
    let meta: String
    let revoke: (() -> Void)?

    public init(device: DeviceSummary, label: String, meta: String, revoke: (() -> Void)?) {
        self.device = device; self.label = label; self.meta = meta; self.revoke = revoke
    }

    var icon: String {
        switch device.kind {
        case "web": "globe"
        case "mobile": "iphone"
        case "native": "desktopcomputer"
        default: "key"
        }
    }

    public var body: some View {
        HStack(spacing: LumiaTokens.space12) {
            Image(systemName: icon).foregroundStyle(LumiaTokens.iconStrong.color).frame(width: 24)
            VStack(alignment: .leading, spacing: LumiaTokens.space2) {
                HStack(spacing: LumiaTokens.space6) {
                    Text(label).font(LumiaTokens.textAuthor.font).foregroundStyle(device.revoked ? LumiaTokens.inkQuiet.color : LumiaTokens.inkEmphasis.color)
                    if device.thisDevice { LumiaTag("This device") }
                    if device.revoked { LumiaTag("Revoked") }
                }
                if !meta.isEmpty { Text(meta).font(LumiaTokens.textMeta.font).foregroundStyle(LumiaTokens.inkQuiet.color) }
            }
            Spacer()
            if let revoke, !device.thisDevice, !device.revoked {
                Button("Revoke", action: revoke).buttonStyle(.lumiaSecondary)
            }
        }
        .padding(.vertical, LumiaTokens.space8)
    }
}

public struct LumiaTag: View {
    let text: String
    public init(_ text: String) { self.text = text }
    public var body: some View {
        Text(text)
            .font(LumiaTokens.textTag.font)
            .foregroundStyle(LumiaTokens.inkSecondary.color)
            .padding(.horizontal, LumiaTokens.space6)
            .padding(.vertical, LumiaTokens.space2)
            .background(LumiaTokens.chip.color, in: Capsule())
    }
}

/// The auth card: title, subtitle, a form. The desktop's fixed width, or
/// the width it is given (a phone's).
public struct LumiaCard<Content: View>: View {
    let title: String
    let subtitle: String?
    let width: CGFloat?
    @ViewBuilder let content: () -> Content

    public init(title: String, subtitle: String? = nil, width: CGFloat? = LumiaTokens.authWidth, @ViewBuilder content: @escaping () -> Content) {
        self.title = title; self.subtitle = subtitle; self.width = width; self.content = content
    }

    public var body: some View {
        VStack(alignment: .leading, spacing: LumiaTokens.space16) {
            VStack(alignment: .leading, spacing: LumiaTokens.space4) {
                Text(title).font(LumiaTokens.titleAuth.font).foregroundStyle(LumiaTokens.ink.color)
                if let subtitle { Text(subtitle).font(LumiaTokens.textSubtitle.font).foregroundStyle(LumiaTokens.inkSecondary.color) }
            }
            content()
        }
        .padding(LumiaTokens.space24)
        .frame(width: width)
        .frame(maxWidth: width == nil ? .infinity : nil)
        .background(LumiaTokens.surface.color, in: RoundedRectangle(cornerRadius: LumiaTokens.radius16))
        .overlay(RoundedRectangle(cornerRadius: LumiaTokens.radius16).stroke(LumiaTokens.lineAuth.color))
    }
}

public struct LumiaField: View {
    let label: String
    @Binding var text: String
    var placeholder = ""
    var secure = false
    /// What assistive technology calls the field; its placeholder when nil, as before.
    var spoken: String?

    public init(_ label: String, text: Binding<String>, placeholder: String = "", secure: Bool = false, spoken: String? = nil) {
        self.label = label; self._text = text; self.placeholder = placeholder; self.secure = secure; self.spoken = spoken
    }

    public var body: some View {
        VStack(alignment: .leading, spacing: LumiaTokens.space4) {
            Text(label).font(LumiaTokens.textLabel.font).foregroundStyle(LumiaTokens.inkSecondary.color)
            Group {
                if secure { SecureField(placeholder, text: $text) } else if let spoken { TextField(placeholder, text: $text).accessibilityLabel(spoken) } else { TextField(placeholder, text: $text) }
            }
            .textFieldStyle(.plain)
            .font(LumiaTokens.textBody.font)
            .padding(.horizontal, LumiaTokens.space10)
            .frame(height: LumiaTokens.controlHeight)
            .background(LumiaTokens.field.color, in: RoundedRectangle(cornerRadius: LumiaTokens.radius8))
            .overlay(RoundedRectangle(cornerRadius: LumiaTokens.radius8).stroke(LumiaTokens.borderForm.color))
        }
    }
}

/// A switch with its label and a line under it saying what it does.
public struct LumiaSwitch: View {
    let label: String
    let caption: String?
    @Binding var isOn: Bool

    public init(_ label: String, caption: String? = nil, isOn: Binding<Bool>) {
        self.label = label; self.caption = caption; self._isOn = isOn
    }

    public var body: some View {
        Toggle(isOn: $isOn) {
            VStack(alignment: .leading, spacing: LumiaTokens.space2) {
                Text(label).font(LumiaTokens.textMember.font).foregroundStyle(LumiaTokens.ink.color)
                if let caption {
                    Text(caption).font(LumiaTokens.textMeta.font).foregroundStyle(LumiaTokens.inkSecondary.color)
                        .fixedSize(horizontal: false, vertical: true)
                }
            }
        }
        .tint(LumiaTokens.accent.color)
    }
}

/// A box that is ticked or not, with its label: one of several chosen.
public struct LumiaCheckRow<Label: View>: View {
    let checked: Bool
    let toggle: () -> Void
    @ViewBuilder let label: () -> Label

    public init(checked: Bool, toggle: @escaping () -> Void, @ViewBuilder label: @escaping () -> Label) {
        self.checked = checked; self.toggle = toggle; self.label = label
    }

    public var body: some View {
        Button(action: toggle) {
            HStack(spacing: LumiaTokens.space12) {
                Image(systemName: checked ? "checkmark.square.fill" : "square")
                    .font(.system(size: 20))
                    .foregroundStyle(checked ? LumiaTokens.accent.color : LumiaTokens.inkQuiet.color)
                label()
                Spacer(minLength: 0)
            }
            .padding(.vertical, LumiaTokens.space6)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .accessibilityAddTraits(checked ? .isSelected : [])
    }
}

public struct LumiaErrorText: View {
    let text: String
    /// A message row's text scale, for "Not saved: …" under a message.
    var scale: CGFloat = 1
    public init(_ text: String, scale: CGFloat = 1) { self.text = text; self.scale = scale }
    public var body: some View {
        Text(text).font(LumiaTokens.textMeta.font(scale: scale)).foregroundStyle(LumiaTokens.dnd.color)
            .frame(maxWidth: .infinity, alignment: .leading)
    }
}

// MARK: Button styles

public struct LumiaPrimaryButton: ButtonStyle {
    @Environment(\.isEnabled) var enabled
    public func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .font(LumiaTokens.textButton.font)
            .foregroundStyle(LumiaTokens.onAccent.color)
            .padding(.horizontal, LumiaTokens.space14)
            .frame(minHeight: LumiaTokens.controlHeight)
            .background(configuration.isPressed ? LumiaTokens.accentHover.color : LumiaTokens.accent.color, in: RoundedRectangle(cornerRadius: LumiaTokens.radius8))
            .opacity(enabled ? 1 : 0.5)
    }
}

public struct LumiaSecondaryButton: ButtonStyle {
    @Environment(\.isEnabled) var enabled
    public func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .font(LumiaTokens.textButtonSecondary.font)
            .foregroundStyle(LumiaTokens.ink.color)
            .padding(.horizontal, LumiaTokens.space12)
            .frame(minHeight: LumiaTokens.controlHeight)
            .background(configuration.isPressed ? LumiaTokens.hoverSecondary.color : LumiaTokens.raised.color, in: RoundedRectangle(cornerRadius: LumiaTokens.radius8))
            .overlay(RoundedRectangle(cornerRadius: LumiaTokens.radius8).stroke(LumiaTokens.border.color))
            .opacity(enabled ? 1 : 0.5)
    }
}

public struct LumiaLinkButton: ButtonStyle {
    public func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .font(LumiaTokens.textLinkSm.font)
            .foregroundStyle(configuration.isPressed ? LumiaTokens.linkHover.color : LumiaTokens.link.color)
    }
}

public extension ButtonStyle where Self == LumiaPrimaryButton { static var lumiaPrimary: LumiaPrimaryButton { .init() } }
public extension ButtonStyle where Self == LumiaSecondaryButton { static var lumiaSecondary: LumiaSecondaryButton { .init() } }
public extension ButtonStyle where Self == LumiaLinkButton { static var lumiaLink: LumiaLinkButton { .init() } }
