// The small components both layouts build from, after the design
// system's (apps/web/src/design/components.tsx): avatar, notice, empty
// state, message row, request card, device row, composer, the auth
// card and its fields. Tokens only; no literal colours or sizes.
import LumiaFFI
import SwiftUI

public struct LumiaAvatar: View {
    public let initials: String
    public let tone: AvatarTone
    public var size: CGFloat = LumiaTokens.avatarMd

    public init(initials: String, tone: AvatarTone, size: CGFloat = LumiaTokens.avatarMd) {
        self.initials = initials; self.tone = tone; self.size = size
    }

    public var body: some View {
        Text(initials)
            .font(.system(size: size * 0.4, weight: .semibold, design: .rounded))
            .foregroundStyle(tone.ink.color)
            .frame(width: size, height: size)
            .background(tone.fill.color, in: Circle())
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

/// A message's row. The first of a block is an avatar beside a name, a
/// time and the message; the ones after it are the message alone under
/// those, in the same column (`MessageBlocks` says which is which). A
/// phone has no pointer to show a later row's time under, so it shows
/// none; assistive technology hears whose each row is and when.
public struct LumiaMessageRow: View {
    public let author: String
    public let tone: AvatarTone
    public let time: String
    public let text: String
    public var deleted = false
    public var place = MessagePlace()
    @State private var hovering = false

    /// The room between one block and the next. The boards first drew 2
    /// here, which read as crowded in the app (Ryan, 2026-11-12).
    public static let blockGap = LumiaTokens.space8

    /// Above and below a row inside its block. The boards draw a block's
    /// messages as the paragraphs of one, 4 points between two line boxes;
    /// a line box there has its leading in it and a `Text` here does not
    /// (the leading is `lineSpacing`, between lines only), so two messages
    /// stand that leading and the paragraph's 4 points apart, half from each.
    static let within = (LumiaTokens.space4 + LumiaTokens.space4) / 2

    public init(author: String, tone: AvatarTone, time: String, text: String, deleted: Bool = false, place: MessagePlace = MessagePlace()) {
        self.author = author; self.tone = tone; self.time = time; self.text = text; self.deleted = deleted; self.place = place
    }

    /// Whose a row is and when, as assistive technology hears it before the message.
    public static func heading(author: String, time: String) -> String { "\(author), \(time)" }

    private var said: String { deleted ? "(deleted)" : text }

    public var body: some View {
        HStack(alignment: .top, spacing: LumiaTokens.space14) {
            if place.opens {
                LumiaAvatar(initials: initials(of: author), tone: tone, size: LumiaTokens.avatarLg)
            } else {
                // The avatar's column, empty: the message lines up under the block's first.
                Color.clear.frame(width: LumiaTokens.avatarLg, height: 0)
            }
            VStack(alignment: .leading, spacing: LumiaTokens.space4) {
                if place.opens {
                    HStack(alignment: .firstTextBaseline, spacing: LumiaTokens.space8) {
                        Text(author).font(LumiaTokens.textAuthor.font).foregroundStyle(LumiaTokens.inkEmphasis.color)
                        Text(time).font(LumiaTokens.textMeta.font).foregroundStyle(LumiaTokens.inkSecondary.color)
                    }
                }
                Text(said)
                    .font(LumiaTokens.textMessage.font)
                    .lineSpacing(LumiaTokens.space4)
                    .foregroundStyle(deleted ? LumiaTokens.inkQuiet.color : LumiaTokens.inkBody.color)
                    .italic(deleted)
                    .textSelection(.enabled)
                    .frame(maxWidth: .infinity, alignment: .leading)
            }
        }
        .padding(.horizontal, LumiaTokens.space12)
        .padding(.top, place.opens ? LumiaTokens.space8 : Self.within)
        .padding(.bottom, place.closes ? LumiaTokens.space8 : Self.within)
        .background(hovering ? LumiaTokens.hoverMessage.color : .clear, in: RoundedRectangle(cornerRadius: LumiaTokens.radius12))
        .onHover { hovering = $0 }
        // One stop per message, the same whether or not the row shows its
        // name and time: "Grace Hopper, 12:01 PM", then what was said.
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(Self.heading(author: author, time: time))
        .accessibilityValue(said)
        .padding(.bottom, place.closes ? Self.blockGap : 0)
    }
}

public struct LumiaDayDivider: View {
    public let label: String
    public init(label: String) { self.label = label }
    public var body: some View {
        HStack(spacing: LumiaTokens.space12) {
            Rectangle().fill(LumiaTokens.line.color).frame(height: 1)
            Text(label).font(LumiaTokens.textSection.font).foregroundStyle(LumiaTokens.inkQuiet.color)
            Rectangle().fill(LumiaTokens.line.color).frame(height: 1)
        }
        .padding(.horizontal, LumiaTokens.space10)
        .padding(.vertical, LumiaTokens.space8)
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

public struct LumiaComposer: View {
    @Binding var text: String
    let placeholder: String
    let disabled: Bool
    let hint: String?
    let typing: [String]?
    let send: () -> Void

    /// `typing` names those typing in the conversation, for the line under
    /// the field: nil where there is never such a line (a personal
    /// conversation), and empty where nobody is typing just now, which
    /// keeps the line's room so the field does not move when someone starts.
    public init(text: Binding<String>, placeholder: String, disabled: Bool = false, hint: String? = "Return to send", typing: [String]? = nil, send: @escaping () -> Void) {
        self._text = text; self.placeholder = placeholder; self.disabled = disabled; self.hint = hint; self.typing = typing; self.send = send
    }

    private var canSend: Bool { !disabled && !text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty }

    public var body: some View {
        VStack(alignment: .leading, spacing: LumiaTokens.space8) {
            HStack(alignment: .center, spacing: LumiaTokens.space2) {
                Image(systemName: "plus")
                    .font(.system(size: 15, weight: .medium))
                    .foregroundStyle(LumiaTokens.inkQuiet.color)
                    .frame(width: 36, height: 36)
                TextField(placeholder, text: $text, axis: .vertical)
                    .textFieldStyle(.plain)
                    .lineLimit(1...8)
                    .font(LumiaTokens.textMessage.font)
                    .foregroundStyle(LumiaTokens.inkField.color)
                    .disabled(disabled)
                    .onSubmit(send)
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
                .padding(.leading, LumiaTokens.space4)
            }
            .padding(.vertical, LumiaTokens.space6)
            .padding(.leading, LumiaTokens.space8)
            .padding(.trailing, LumiaTokens.space6)
            .background(LumiaTokens.field.color, in: RoundedRectangle(cornerRadius: LumiaTokens.radius16))
            .overlay(RoundedRectangle(cornerRadius: LumiaTokens.radius16).stroke(LumiaTokens.borderComposer.color))
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
    let tag: String?
    /// `ONLINE`, `AWAY` or `DO_NOT_DISTURB`, shown as a dot; nil for not here.
    let status: String?

    public init(name: String, subtitle: String? = nil, tone: AvatarTone, tag: String? = nil, status: String? = nil) {
        self.name = name; self.subtitle = subtitle; self.tone = tone; self.tag = tag; self.status = status
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
            LumiaAvatar(initials: initials(of: name), tone: tone, size: LumiaTokens.avatarMd)
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

/// The aside's tabs: one segment on a track, the current one raised.
public struct LumiaSegmentedTabs: View {
    public struct Tab: Identifiable, Hashable {
        public let id: String
        public let label: String
        public let count: Int?
        public init(id: String, label: String, count: Int? = nil) { self.id = id; self.label = label; self.count = count }
    }
    let tabs: [Tab]
    @Binding var selected: String

    public init(tabs: [Tab], selected: Binding<String>) { self.tabs = tabs; self._selected = selected }

    public var body: some View {
        HStack(spacing: LumiaTokens.space4) {
            ForEach(tabs) { tab in
                let on = tab.id == selected
                Button { selected = tab.id } label: {
                    HStack(spacing: LumiaTokens.space6) {
                        Text(tab.label).font(LumiaTokens.textButtonSecondary.font)
                        if let count = tab.count { Text("\(count)").font(LumiaTokens.textTag.font).foregroundStyle(LumiaTokens.inkSecondary.color) }
                    }
                    .foregroundStyle(on ? LumiaTokens.ink.color : LumiaTokens.inkQuiet.color)
                    .frame(maxWidth: .infinity).frame(height: 34)
                    .background(on ? LumiaTokens.tabActive.color : .clear, in: RoundedRectangle(cornerRadius: LumiaTokens.radius9))
                }
                .buttonStyle(.plain)
            }
        }
        .padding(LumiaTokens.space4)
        .background(LumiaTokens.track.color, in: RoundedRectangle(cornerRadius: LumiaTokens.radius12))
    }
}

public struct LumiaRequestCard: View {
    let name: String
    let handle: String?
    let tone: AvatarTone
    let meta: String
    let busy: Bool
    let accept: () -> Void
    let decline: () -> Void
    let block: () -> Void

    public init(name: String, handle: String?, tone: AvatarTone, meta: String, busy: Bool, accept: @escaping () -> Void, decline: @escaping () -> Void, block: @escaping () -> Void) {
        self.name = name; self.handle = handle; self.tone = tone; self.meta = meta; self.busy = busy
        self.accept = accept; self.decline = decline; self.block = block
    }

    public var body: some View {
        HStack(spacing: LumiaTokens.space12) {
            LumiaAvatar(initials: initials(of: name), tone: tone, size: LumiaTokens.avatarLg)
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

    public init(_ label: String, text: Binding<String>, placeholder: String = "", secure: Bool = false) {
        self.label = label; self._text = text; self.placeholder = placeholder; self.secure = secure
    }

    public var body: some View {
        VStack(alignment: .leading, spacing: LumiaTokens.space4) {
            Text(label).font(LumiaTokens.textLabel.font).foregroundStyle(LumiaTokens.inkSecondary.color)
            Group {
                if secure { SecureField(placeholder, text: $text) } else { TextField(placeholder, text: $text) }
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

public struct LumiaErrorText: View {
    let text: String
    public init(_ text: String) { self.text = text }
    public var body: some View {
        Text(text).font(LumiaTokens.textMeta.font).foregroundStyle(LumiaTokens.dnd.color)
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
