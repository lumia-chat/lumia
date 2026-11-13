// A server's sidebar beside the rail (architecture 6.8): its invitations,
// the channels this account is in under a Channels heading whose +
// offers Create a channel and Browse channels, its server DMs under a
// Direct messages heading whose + picks a member to message, and the
// links to inviting people, this account's own settings there and the
// server's settings.
import LumiaFFI
import LumiaKit
import SwiftUI

struct ServerScreen: View {
    let session: LumiaSession
    let server: ServerSummary
    @Binding var path: [Screen]
    @State private var busy: String?
    @State private var error = ""

    private var all: [ChannelSummary] { session.snapshot?.channels ?? [] }
    /// The channels this account is in: the snapshot holds no others.
    private var channels: [ChannelSummary] { Membership.channels(all, serverID: server.serverId) }
    private var dms: [ChannelSummary] { Membership.serverDMs(all, serverID: server.serverId) }
    private var invitations: [ChannelInvitationSummary] { Membership.invitations(session.snapshot?.channelInvitations ?? [], serverID: server.serverId) }

    var body: some View {
        VStack(spacing: 0) {
            SideHeader(title: server.name, meta: session.host, action: ("square.and.pencil", "New message", { path.append(.newMessage) }))
            ScrollView {
                VStack(alignment: .leading, spacing: LumiaTokens.space2) {
                    ForEach(invitations, id: \.channel.channelId) { invitation in
                        InvitationRow(
                            line: Membership.invitationLine(channel: invitation.channel.name, inviter: session.name(of: invitation.invitedByAccountId)),
                            busy: busy == invitation.channel.channelId,
                            join: { Task { await join(invitation.channel) } },
                            dismiss: { Task { await dismiss(invitation.channel) } }
                        )
                    }
                    if !error.isEmpty { LumiaErrorText(error).padding(.horizontal, LumiaTokens.space10) }
                    HeadingWithAdd(title: "Channels") {
                        // Creating one needs `MANAGE_CHANNELS`; Browse is everyone's.
                        if server.canManageChannels {
                            Button { path.append(.newChannel(serverID: server.serverId)) } label: { Label("Create a channel", systemImage: "number") }
                        }
                        Button { path.append(.browse(serverID: server.serverId)) } label: { Label("Browse channels", systemImage: "magnifyingglass") }
                    }
                    if channels.isEmpty {
                        Text("No channels yet.")
                            .font(LumiaTokens.textMeta.font).foregroundStyle(LumiaTokens.inkQuiet.color)
                            .padding(.horizontal, LumiaTokens.space10).padding(.vertical, LumiaTokens.space4)
                        Button("Browse channels") { path.append(.browse(serverID: server.serverId)) }
                            .buttonStyle(.lumiaLink)
                            .padding(.horizontal, LumiaTokens.space10)
                    }
                    ForEach(channels, id: \.channelId) { c in
                        SidebarRow(label: c.name, open: { path.append(.channel(id: c.channelId)) }) {
                            ChannelGlyph(isPrivate: c.private)
                        }
                    }
                    // The server's own conversations, kept by it (architecture 6.4).
                    HeadingWithAdd(title: "Direct messages", single: server.canStartDms ? ("Message someone", { path.append(.newServerDM(serverID: server.serverId)) }) : nil)
                        .padding(.top, LumiaTokens.space8)
                    ForEach(dms, id: \.channelId) { c in
                        let other = Membership.other(in: c, me: session.me)
                        let name = session.name(of: other)
                        SidebarRow(label: name, open: { path.append(.channel(id: c.channelId)) }) {
                            LumiaAvatar(initials: initials(of: name), tone: .of(other), size: LumiaTokens.avatarXs)
                                .accessibilityHidden(true)
                        }
                    }
                    Button("Invite people") { path.append(.invite(serverID: server.serverId)) }
                        .buttonStyle(.lumiaLink)
                        .padding(.horizontal, LumiaTokens.space10).padding(.top, LumiaTokens.space12)
                    // Every member's own switches there (architecture 6.8).
                    Button("Your settings in \(server.name)") { path.append(.yourSettings(serverID: server.serverId)) }
                        .buttonStyle(.lumiaLink)
                        .padding(.horizontal, LumiaTokens.space10).padding(.top, LumiaTokens.space4)
                    // Offered to whoever may change them (architecture 6.7): the
                    // account's own permission there, from the snapshot.
                    if server.canManage {
                        Button("Server settings") { path.append(.serverSettings(serverID: server.serverId)) }
                            .buttonStyle(.lumiaLink)
                            .padding(.horizontal, LumiaTokens.space10).padding(.top, LumiaTokens.space4)
                    }
                }
                .padding(.horizontal, LumiaTokens.space12)
                .padding(.bottom, LumiaTokens.space24)
            }
            Text(session.connection.label)
                .font(LumiaTokens.textMeta.font).foregroundStyle(LumiaTokens.inkQuiet.color)
                .padding(.horizontal, LumiaTokens.space16).padding(.vertical, LumiaTokens.space6)
        }
    }

    /// Yes to an invitation is joining the channel, which then opens.
    private func join(_ channel: ChannelSummary) async {
        busy = channel.channelId
        error = ""
        defer { busy = nil }
        do {
            try await session.joinChannel(channelID: channel.channelId)
            path.append(.channel(id: channel.channelId))
        } catch {
            self.error = error.lumiaMessage
        }
    }

    private func dismiss(_ channel: ChannelSummary) async {
        busy = channel.channelId
        error = ""
        defer { busy = nil }
        do { try await session.dismissChannelInvitation(channelID: channel.channelId) } catch { self.error = error.lumiaMessage }
    }
}

/// A sidebar heading with its +: a menu of what it offers, or one thing done at once.
struct HeadingWithAdd<Items: View>: View {
    let title: String
    var single: (label: String, run: () -> Void)?
    @ViewBuilder var items: () -> Items

    init(title: String, single: (label: String, run: () -> Void)? = nil, @ViewBuilder items: @escaping () -> Items = { EmptyView() }) {
        self.title = title; self.single = single; self.items = items
    }

    var body: some View {
        HStack(spacing: 0) {
            SectionTitle(title)
            Spacer(minLength: 0)
            if let single {
                Button(action: single.run) { plus }
                    .buttonStyle(.plain)
                    .accessibilityLabel(single.label)
            } else if Items.self != EmptyView.self {
                Menu { items() } label: { plus }
                    .menuOrder(.fixed)
                    .buttonStyle(.plain)
                    .accessibilityLabel("Add to \(title)")
            }
        }
    }

    private var plus: some View {
        Image(systemName: "plus").font(.system(size: 14, weight: .semibold))
            .foregroundStyle(LumiaTokens.inkMuted.color)
            .frame(width: LumiaTokens.controlHeight, height: LumiaTokens.controlHeight)
            .contentShape(Rectangle())
    }
}

/// A channel's glyph: `#`, or a lock for a private one.
struct ChannelGlyph: View {
    let isPrivate: Bool
    var body: some View {
        Group {
            if isPrivate {
                Image(systemName: "lock").font(.system(size: 12, weight: .semibold)).accessibilityLabel("Private")
            } else {
                Text("#").font(LumiaTokens.textNav.font).accessibilityHidden(true)
            }
        }
        .foregroundStyle(LumiaTokens.inkMuted.color)
        .frame(width: LumiaTokens.space12)
    }
}

/// A row of a server's sidebar: a glyph and a name.
struct SidebarRow<Glyph: View>: View {
    let label: String
    let open: () -> Void
    @ViewBuilder let glyph: () -> Glyph

    var body: some View {
        Button(action: open) {
            HStack(spacing: LumiaTokens.space8) {
                glyph()
                Text(label).font(LumiaTokens.textNav.font).foregroundStyle(LumiaTokens.ink.color).lineLimit(1)
                Spacer(minLength: 0)
            }
            .padding(.horizontal, LumiaTokens.space10)
            .frame(height: LumiaTokens.controlHeight)
            .contentShape(RoundedRectangle(cornerRadius: LumiaTokens.radius10))
        }
        .buttonStyle(.plain)
    }
}

/// An invitation into a channel, above the channels: who asked, and the two answers.
struct InvitationRow: View {
    let line: String
    let busy: Bool
    let join: () -> Void
    let dismiss: () -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: LumiaTokens.space8) {
            Text(line).font(LumiaTokens.textMember.font).foregroundStyle(LumiaTokens.inkBody.color)
            HStack(spacing: LumiaTokens.space8) {
                Button("Join", action: join).buttonStyle(.lumiaPrimary)
                Button("Dismiss", action: dismiss).buttonStyle(.lumiaSecondary)
            }
        }
        .disabled(busy)
        .padding(LumiaTokens.space12)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(LumiaTokens.accentWash.color, in: RoundedRectangle(cornerRadius: LumiaTokens.radius10))
        .overlay(RoundedRectangle(cornerRadius: LumiaTokens.radius10).stroke(LumiaTokens.accentLine.color))
        .padding(.bottom, LumiaTokens.space8)
    }
}
