// A server's sidebar beside the rail (architecture 6.8): its Mentions quick
// tile with the server's count (architecture 6.6), its invitations,
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
                    // This server's messages that mention this account or reply to it
                    // (architecture 6.6): the board's quick tiles, of which Threads and
                    // Saved do not exist yet, so Mentions is the row's first alone.
                    let mentions = session.serverUnread(server.serverId).mentions
                    HStack(spacing: LumiaTokens.space6) {
                        Tile(label: "Mentions", icon: "at", count: Int(mentions), spoken: Replies.unreadWords(unread: mentions, mentions: mentions)) { path.append(.mentions(serverID: server.serverId)) }
                            .accessibilityIdentifier("mentions-row")
                        Color.clear.frame(maxWidth: .infinity, maxHeight: 1)
                        Color.clear.frame(maxWidth: .infinity, maxHeight: 1)
                    }
                    .padding(.bottom, LumiaTokens.space12)
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
                    ForEach(channels.filter { !$0.private }, id: \.channelId) { c in channelRow(c) }
                    // The boards' Private, between Channels and Direct messages, only when there
                    // are any; no plus, since Create a channel makes one private. Sections stand
                    // 12 apart, as the boards have them.
                    let privates = channels.filter(\.private)
                    if !privates.isEmpty {
                        HeadingWithAdd(title: "Private")
                            .padding(.top, LumiaTokens.space12)
                        ForEach(privates, id: \.channelId) { c in channelRow(c) }
                    }
                    // The server's own conversations, kept by it (architecture 6.4).
                    HeadingWithAdd(title: "Direct messages", single: server.canStartDms ? ("Message someone", { path.append(.newServerDM(serverID: server.serverId)) }) : nil)
                        .padding(.top, LumiaTokens.space12)
                    ForEach(dms, id: \.channelId) { c in
                        let other = Membership.other(in: c, me: session.me)
                        let name = session.name(of: other)
                        SidebarRow(label: name, unread: session.unread(c.channelId), open: { path.append(.channel(id: c.channelId)) }) {
                            LumiaAvatar(initials: initials(of: name), tone: .of(other), size: LumiaTokens.avatarSm, picture: session.picture(of: other))
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

    private func channelRow(_ c: ChannelSummary) -> some View {
        let unread = session.unread(c.channelId)
        return SidebarRow(label: c.name, unread: unread, open: { path.append(.channel(id: c.channelId)) }) {
            ChannelGlyph(isPrivate: c.private, unread: unread.unread > 0)
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

    // No taller than the heading's words (the boards' headings have no control to make them
    // taller), its target reaching a finger's 44 beyond what it draws.
    private var plus: some View {
        Image(systemName: "plus").font(.system(size: 14, weight: .semibold))
            .foregroundStyle(LumiaTokens.inkMuted.color)
            .frame(width: LumiaTokens.tileSize, height: LumiaTokens.space24)
            .contentShape(Rectangle().inset(by: -LumiaTokens.space10))
    }
}

/// A channel's glyph: `#`, or a lock for a private one; stronger while unread.
struct ChannelGlyph: View {
    let isPrivate: Bool
    var unread = false
    var body: some View {
        Group {
            if isPrivate {
                Image(systemName: "lock").font(.system(size: 12, weight: .semibold)).accessibilityLabel("Private")
            } else {
                Text("#").font(LumiaTokens.textNav.font).accessibilityHidden(true)
            }
        }
        .foregroundStyle(unread ? LumiaTokens.inkMuted.color : LumiaTokens.inkFaint.color)
        .frame(width: LumiaTokens.space12)
    }
}

/// A row of a server's sidebar: a glyph and a name, in the quiet ink.
/// One with unread messages is drawn in the ink at 600, with their count
/// for this account at its end as the full badge (architecture 6.6).
struct SidebarRow<Glyph: View>: View {
    let label: String
    var unread: (unread: UInt32, mentions: UInt32) = (0, 0)
    let open: () -> Void
    @ViewBuilder let glyph: () -> Glyph

    var body: some View {
        Button(action: open) {
            HStack(spacing: LumiaTokens.space10) {
                glyph()
                Text(label).font(LumiaTokens.textNav.font)
                    .fontWeight(unread.unread > 0 ? .semibold : nil)
                    .foregroundStyle(unread.unread > 0 ? LumiaTokens.ink.color : LumiaTokens.inkQuiet.color).lineLimit(1)
                Spacer(minLength: 0)
                if unread.mentions > 0 { LumiaCountBadge(unread.mentions) }
            }
            .padding(.horizontal, LumiaTokens.space10)
            // The phone boards' rows: 44 high.
            .frame(height: LumiaTokens.tileSize)
            .contentShape(RoundedRectangle(cornerRadius: LumiaTokens.radius10))
        }
        .buttonStyle(.plain)
        .modifier(UnreadLabel(unread: unread))
    }
}

/// What assistive technology hears of a row with something unread in it:
/// its name as its contents give it, then "unread" or "2 mentions" as its value.
struct UnreadLabel: ViewModifier {
    let unread: (unread: UInt32, mentions: UInt32)

    func body(content: Content) -> some View {
        content.accessibilityValue(Replies.unreadWords(unread: unread.unread, mentions: unread.mentions))
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
