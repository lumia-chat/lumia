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

public struct LumiaMessageRow: View {
    public let author: String
    public let tone: AvatarTone
    public let time: String
    public let text: String
    public var deleted = false

    public init(author: String, tone: AvatarTone, time: String, text: String, deleted: Bool = false) {
        self.author = author; self.tone = tone; self.time = time; self.text = text; self.deleted = deleted
    }

    public var body: some View {
        HStack(alignment: .top, spacing: LumiaTokens.space12) {
            LumiaAvatar(initials: initials(of: author), tone: tone, size: LumiaTokens.avatarMd)
            VStack(alignment: .leading, spacing: LumiaTokens.space2) {
                HStack(alignment: .firstTextBaseline, spacing: LumiaTokens.space8) {
                    Text(author).font(LumiaTokens.textAuthor.font).foregroundStyle(LumiaTokens.inkEmphasis.color)
                    Text(time).font(LumiaTokens.textMeta.font).foregroundStyle(LumiaTokens.inkQuiet.color)
                }
                Text(deleted ? "(deleted)" : text)
                    .font(LumiaTokens.textMessage.font)
                    .foregroundStyle(deleted ? LumiaTokens.inkQuiet.color : LumiaTokens.inkBody.color)
                    .italic(deleted)
                    .textSelection(.enabled)
                    .frame(maxWidth: .infinity, alignment: .leading)
            }
        }
        .padding(.horizontal, LumiaTokens.space16)
        .padding(.vertical, LumiaTokens.space6)
    }
}

public struct LumiaDayDivider: View {
    public let label: String
    public init(label: String) { self.label = label }
    public var body: some View {
        HStack(spacing: LumiaTokens.space10) {
            Rectangle().fill(LumiaTokens.line.color).frame(height: 1)
            Text(label).font(LumiaTokens.textSection.font).foregroundStyle(LumiaTokens.inkQuiet.color)
            Rectangle().fill(LumiaTokens.line.color).frame(height: 1)
        }
        .padding(.horizontal, LumiaTokens.space16)
        .padding(.vertical, LumiaTokens.space8)
    }
}

public struct LumiaComposer: View {
    @Binding var text: String
    let placeholder: String
    let disabled: Bool
    let send: () -> Void

    public init(text: Binding<String>, placeholder: String, disabled: Bool = false, send: @escaping () -> Void) {
        self._text = text; self.placeholder = placeholder; self.disabled = disabled; self.send = send
    }

    public var body: some View {
        HStack(alignment: .bottom, spacing: LumiaTokens.space8) {
            TextField(placeholder, text: $text, axis: .vertical)
                .textFieldStyle(.plain)
                .lineLimit(1...8)
                .font(LumiaTokens.textMessage.font)
                .disabled(disabled)
                .onSubmit(send)
            Button(action: send) {
                Image(systemName: "arrow.up").font(.system(size: 13, weight: .bold))
            }
            .buttonStyle(.lumiaPrimary)
            .disabled(disabled || text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
            .keyboardShortcut(.return, modifiers: [])
        }
        .padding(LumiaTokens.space10)
        .background(LumiaTokens.field.color, in: RoundedRectangle(cornerRadius: LumiaTokens.radius12))
        .overlay(RoundedRectangle(cornerRadius: LumiaTokens.radius12).stroke(LumiaTokens.borderComposer.color))
        .padding([.horizontal, .bottom], LumiaTokens.space16)
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
