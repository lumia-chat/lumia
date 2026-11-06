// The screens pushed over the rail and sidebar, each a floating panel:
// a conversation, a channel, requests, a new message, devices. The same
// states, wording and error handling as the web's and the macOS app's.
import LumiaFFI
import LumiaKit
import SwiftUI

// MARK: Conversation

struct ConversationScreen: View {
    let session: LumiaSession
    let channelID: String
    @State private var draft = ""
    @State private var error = ""

    private var channel: ChannelSummary? { session.snapshot?.channels.first { $0.channelId == channelID } }
    private var state: DmChannelSummary? { session.dmChannels.first { $0.channelId == channelID } }
    private var log: DmLogSummary? { session.dmLogs[channelID] }
    private var others: [String] { channel?.participantAccountIds.filter { $0 != session.me } ?? [] }
    private var title: String { channel.map { session.conversationName($0) } ?? "Conversation" }

    var body: some View {
        PanelScreen(back: "Home", title: title, subtitle: (channel?.host).flatMap { $0.isEmpty ? nil : "on \($0)" }, lead: {
            LumiaAvatar(initials: initials(of: title), tone: .of(others.first ?? channelID), size: LumiaTokens.avatarMd)
        }) {
            ScrollViewReader { proxy in
                ScrollView {
                    LazyVStack(alignment: .leading, spacing: LumiaTokens.space8) {
                        LumiaNotice(icon: "shield") {
                            Text("Messages here are end-to-end encrypted. Only your devices and those of \(others.map { session.name(of: $0) }.joined(separator: ", ")) can read them.")
                        }
                        if let state, !state.member {
                            LumiaNotice(icon: "lock", tone: .accent, action: ("Rejoin", { Task { await rejoin() } })) {
                                Text("This device is not in the conversation's group yet. The device that created it sends a welcome; if that never comes, rejoin from the instance's group info.")
                            }
                        }
                        if let log, log.beforeJoin > 0 {
                            LumiaNotice(icon: "lock") {
                                Text("\(log.beforeJoin) earlier \(log.beforeJoin == 1 ? "message was" : "messages were") sent before this device joined and cannot be read here.")
                            }
                        }
                        if let log, log.undecryptable > 0 {
                            LumiaNotice(icon: "lock") { Text("\(log.undecryptable) could not be decrypted.") }
                        }
                        if let log, !log.messages.isEmpty {
                            LumiaDayDivider(label: "Messages").padding(.horizontal, -LumiaTokens.space16)
                        }
                        ForEach(log?.messages ?? [], id: \.messageId) { m in
                            let who = m.mine ? session.selfName : (!m.senderAccountId.isEmpty || others.count != 1 ? session.name(of: m.senderAccountId) : session.name(of: others[0]))
                            let toneID = m.mine ? session.me : (m.senderAccountId.isEmpty ? m.senderIdentityId : m.senderAccountId)
                            LumiaMessageRow(author: who, tone: .of(toneID), time: time(ofMicros: m.sentAtMicros), text: m.body, deleted: m.deleted)
                                .padding(.horizontal, -LumiaTokens.space16)
                                .id(m.messageId)
                        }
                        if !error.isEmpty { LumiaErrorText(error) }
                    }
                    .padding(LumiaTokens.space16)
                }
                .scrollDismissesKeyboard(.interactively)
                .onChange(of: log?.messages.count ?? 0) { _, _ in
                    if let last = log?.messages.last { proxy.scrollTo(last.messageId, anchor: .bottom) }
                }
            }
            LumiaComposer(text: $draft, placeholder: state?.member == true ? "Message \(title)" : "Setting up the encrypted group…", disabled: state?.member != true, hint: nil) {
                Task { await send() }
            }
        }
        .task(id: channelID) {
            await session.refreshDmChannels()
            await session.refreshDmLog(channelID: channelID)
        }
    }

    private func send() async {
        let body = draft.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !body.isEmpty else { return }
        draft = ""
        error = ""
        do { try await session.dmSend(channelID: channelID, body: body) } catch { self.error = error.lumiaMessage }
    }

    private func rejoin() async {
        error = ""
        do { try await session.dmRejoin(channelID: channelID) } catch { self.error = error.lumiaMessage }
    }
}

// MARK: Channel

struct ChannelScreen: View {
    let session: LumiaSession
    let channelID: String
    @State private var draft = ""
    @State private var error = ""

    private var channel: ChannelSummary? { session.snapshot?.channels.first { $0.channelId == channelID } }
    private var server: ServerSummary? { session.snapshot?.servers.first { $0.serverId == channel?.serverId } }
    private var messages: [MessageSummary] { session.messages[channelID] ?? [] }
    private var name: String { channel?.name ?? "channel" }

    var body: some View {
        PanelScreen(back: server?.name ?? "the server", title: "# \(name)", subtitle: server?.name) {
            ScrollViewReader { proxy in
                ScrollView {
                    LazyVStack(alignment: .leading, spacing: 0) {
                        if messages.isEmpty {
                            LumiaEmptyState(icon: "bubble.left", title: "Nothing here yet") { Text("Say something.") }
                        } else {
                            LumiaDayDivider(label: "Today")
                        }
                        ForEach(messages, id: \.messageId) { m in
                            LumiaMessageRow(author: session.name(of: m.authorAccountId), tone: .of(m.authorAccountId), time: time(ofMicros: m.createdAtMicros), text: m.body, deleted: m.deleted)
                                .id(m.messageId)
                        }
                        if !error.isEmpty { LumiaErrorText(error).padding(.horizontal, LumiaTokens.space16) }
                    }
                    .padding(.vertical, LumiaTokens.space8)
                }
                .scrollDismissesKeyboard(.interactively)
                .onChange(of: messages.count) { _, _ in
                    if let last = messages.last { proxy.scrollTo(last.messageId, anchor: .bottom) }
                }
            }
            LumiaComposer(text: $draft, placeholder: "Message #\(name)", hint: nil) { Task { await send() } }
        }
        .task(id: channelID) { await session.refreshMessages(channelID: channelID) }
    }

    private func send() async {
        let body = draft.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !body.isEmpty else { return }
        draft = ""
        error = ""
        do { try await session.sendMessage(channelID: channelID, body: body) } catch { self.error = error.lumiaMessage }
    }
}

// MARK: Requests

struct RequestsScreen: View {
    let session: LumiaSession
    @Binding var path: [Screen]
    @State private var busy: String?
    @State private var error = ""

    private var requests: [DmRequestSummary] { session.snapshot?.pendingRequests ?? [] }

    var body: some View {
        PanelScreen(back: "Home", title: "Requests", subtitle: requests.isEmpty ? "No one waiting" : "\(requests.count) waiting for you") {
            ScrollView {
                VStack(alignment: .leading, spacing: LumiaTokens.space12) {
                    LumiaNotice(icon: "shield") {
                        Text("A request shows who is asking and what you share, nothing else. Accepting opens an encrypted conversation; the sender cannot see that you looked.")
                    }
                    if !error.isEmpty { LumiaErrorText(error) }
                    if requests.isEmpty {
                        LumiaEmptyState(icon: "at", title: "Nothing waiting") { Text("Declined requests disappear; blocked people cannot ask again.") }
                    } else {
                        SectionTitle("Waiting for you").padding(.horizontal, -LumiaTokens.space10)
                        ForEach(requests, id: \.requestId) { r in
                            RequestRow(
                                name: session.name(of: r.fromAccountId),
                                handle: (session.people[r.fromAccountId] ?? session.profile(of: r.fromAccountId))?.handle,
                                tone: .of(r.fromAccountId),
                                meta: meta(r),
                                busy: busy == r.requestId,
                                accept: { Task { await respond(r.requestId, "accept") } },
                                decline: { Task { await respond(r.requestId, "decline") } },
                                block: { Task { await respond(r.requestId, "block") } }
                            )
                        }
                    }
                }
                .padding(LumiaTokens.space16)
            }
        }
    }

    private func meta(_ r: DmRequestSummary) -> String {
        var parts: [String] = []
        if !r.sharedServerIds.isEmpty { parts.append("Shares \(r.sharedServerIds.count == 1 ? "a server" : "\(r.sharedServerIds.count) servers") with you") }
        if r.mutualFriendCount > 0 { parts.append("\(r.mutualFriendCount) mutual \(r.mutualFriendCount == 1 ? "friend" : "friends")") }
        return parts.isEmpty ? "Nothing in common yet" : parts.joined(separator: " · ")
    }

    private func respond(_ id: String, _ response: String) async {
        busy = id
        error = ""
        defer { busy = nil }
        do {
            if let channelID = try await session.dmRespond(requestID: id, response: response), response == "accept" {
                path = [.conversation(id: channelID)]
            }
        } catch {
            self.error = error.lumiaMessage
        }
    }
}

/// A request on a phone: who and what you share, the answers on their own line.
struct RequestRow: View {
    let name: String
    let handle: String?
    let tone: AvatarTone
    let meta: String
    let busy: Bool
    let accept: () -> Void
    let decline: () -> Void
    let block: () -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: LumiaTokens.space10) {
            HStack(spacing: LumiaTokens.space12) {
                LumiaAvatar(initials: initials(of: name), tone: tone, size: LumiaTokens.avatarLg)
                VStack(alignment: .leading, spacing: LumiaTokens.space2) {
                    HStack(alignment: .firstTextBaseline, spacing: LumiaTokens.space6) {
                        Text(name).font(LumiaTokens.textAuthor.font).foregroundStyle(LumiaTokens.inkEmphasis.color)
                        if let handle, !handle.isEmpty { Text("@\(handle)").font(LumiaTokens.textMeta.font).foregroundStyle(LumiaTokens.inkMuted.color) }
                    }
                    Text(meta).font(LumiaTokens.textMeta.font).foregroundStyle(LumiaTokens.inkSecondary.color)
                }
            }
            HStack(spacing: LumiaTokens.space8) {
                Button("Accept", action: accept).buttonStyle(.lumiaPrimary)
                Button("Decline", action: decline).buttonStyle(.lumiaSecondary)
                Spacer()
                Button("Block", action: block).buttonStyle(.lumiaLink)
            }
        }
        .disabled(busy)
        .padding(LumiaTokens.space14)
        .background(LumiaTokens.tile.color, in: RoundedRectangle(cornerRadius: LumiaTokens.radius14))
        .overlay(RoundedRectangle(cornerRadius: LumiaTokens.radius14).stroke(LumiaTokens.line.color))
    }
}

// MARK: New message

// MARK: Add a server

/// A server of your own, or joining one from an invite link or code:
/// what the link leads to first, on which instance, then the join
/// (architecture 6.1).
struct AddServerScreen: View {
    let session: LumiaSession
    let opened: (String) -> Void
    @State private var name = ""
    @State private var creating = false
    @State private var createError = ""
    @State private var text = ""
    @State private var preview: InvitePreviewSummary?
    @State private var error = ""
    @State private var busy = false

    var body: some View {
        PanelScreen(back: "Home", title: "Add a server", subtitle: "Your own, or one you were invited to") {
            ScrollView {
                VStack(alignment: .leading, spacing: LumiaTokens.space16) {
                    LumiaCard(title: "Create a server", subtitle: "You own it; invite people once it exists.", width: nil) {
                        LumiaField("Name", text: $name, placeholder: "Our place")
                        if !createError.isEmpty { LumiaErrorText(createError) }
                        Button("Create") { Task { await create() } }
                            .buttonStyle(.lumiaPrimary).disabled(creating || name.trimmingCharacters(in: .whitespaces).isEmpty)
                    }
                    LumiaCard(title: "Join with an invite", subtitle: "Paste the link someone sent you; what it leads to shows first.", width: nil) {
                        LumiaField("Invite link or code", text: $text, placeholder: "https://chat.example.com/invite/…")
                            .textInputAutocapitalization(.never).autocorrectionDisabled()
                        if !error.isEmpty { LumiaErrorText(error) }
                        Button("Look it up") { Task { await look() } }
                            .buttonStyle(.lumiaPrimary).disabled(busy || text.trimmingCharacters(in: .whitespaces).isEmpty)
                        if let p = preview {
                            Text("\(p.serverName) · \(p.memberCount) \(p.memberCount == 1 ? "member" : "members") on \(p.host), run by \(p.operatorName.isEmpty ? "an unnamed operator" : p.operatorName)\(p.asGuest ? " · you would join as a guest" : ""). Your account on this instance joins it; nothing else is shared.")
                                .font(LumiaTokens.textMeta.font).foregroundStyle(LumiaTokens.inkSecondary.color)
                            Button("Join \(p.serverName)") { Task { await join(p) } }
                                .buttonStyle(.lumiaPrimary).disabled(busy)
                        }
                    }
                }
                .padding(LumiaTokens.space16)
            }
        }
    }

    private func create() async {
        creating = true
        createError = ""
        defer { creating = false }
        do {
            opened(try await session.createServer(name: name))
        } catch {
            createError = error.lumiaMessage
        }
    }

    private func look() async {
        busy = true
        error = ""
        preview = nil
        defer { busy = false }
        do {
            preview = try await session.invitePreview(text)
        } catch {
            self.error = error.lumiaCode == "NOT_FOUND" ? "That invite is unknown, expired or used up." : error.lumiaMessage
        }
    }

    private func join(_ p: InvitePreviewSummary) async {
        busy = true
        error = ""
        defer { busy = false }
        do {
            opened(try await session.acceptInvite(p.code))
        } catch {
            self.error = error.lumiaMessage
        }
    }
}

// MARK: New channel

/// A text channel in the server; it opens once made.
struct NewChannelScreen: View {
    let session: LumiaSession
    let server: ServerSummary?
    let opened: (String) -> Void
    @State private var name = ""
    @State private var error = ""
    @State private var busy = false

    var body: some View {
        PanelScreen(back: server?.name ?? "Server", title: "New channel", subtitle: "A text channel everyone in \(server?.name ?? "the server") can see") {
            ScrollView {
                VStack(alignment: .leading, spacing: LumiaTokens.space12) {
                    LumiaField("Name", text: $name, placeholder: "general")
                        .textInputAutocapitalization(.never).autocorrectionDisabled()
                    if !error.isEmpty { LumiaErrorText(error) }
                    Button("Create") { Task { await create() } }
                        .buttonStyle(.lumiaPrimary).disabled(busy || name.trimmingCharacters(in: .whitespaces).isEmpty)
                }
                .padding(LumiaTokens.space16)
            }
        }
    }

    private func create() async {
        guard let server else { return }
        busy = true
        error = ""
        defer { busy = false }
        do {
            opened(try await session.createChannel(serverID: server.serverId, name: name))
        } catch {
            self.error = error.lumiaCode == "PERMISSION_DENIED" ? "You cannot add channels to this server." : error.lumiaMessage
        }
    }
}

// MARK: Invite people

/// Invite links for the server: a new one with an expiry and a use limit,
/// ready to copy or share, and the ones that exist, each revocable.
struct InviteScreen: View {
    let session: LumiaSession
    let server: ServerSummary?
    @State private var expiresInHours: UInt32 = 0
    @State private var maxUses: UInt32 = 0
    @State private var invite: InviteSummary?
    @State private var invites: [InviteSummary] = []
    @State private var error = ""
    @State private var busy = false
    @State private var copied = false

    var body: some View {
        PanelScreen(back: server?.name ?? "Server", title: "Invite people", subtitle: "A link to join \(server?.name ?? "the server")") {
            ScrollView {
                VStack(alignment: .leading, spacing: LumiaTokens.space12) {
                    Text("Expires").font(LumiaTokens.textLabel.font).foregroundStyle(LumiaTokens.inkSecondary.color)
                    Picker("Expires", selection: $expiresInHours) {
                        Text("Never").tag(UInt32(0))
                        Text("In a day").tag(UInt32(24))
                        Text("In a week").tag(UInt32(168))
                    }
                    .pickerStyle(.segmented)
                    Text("Uses").font(LumiaTokens.textLabel.font).foregroundStyle(LumiaTokens.inkSecondary.color)
                    Picker("Uses", selection: $maxUses) {
                        Text("Unlimited").tag(UInt32(0))
                        Text("Once").tag(UInt32(1))
                        Text("Ten").tag(UInt32(10))
                    }
                    .pickerStyle(.segmented)
                    if !error.isEmpty { LumiaErrorText(error) }
                    Button("Make a link") { Task { await make() } }
                        .buttonStyle(.lumiaPrimary).disabled(busy)
                    if let invite {
                        Text(invite.url)
                            .font(LumiaTokens.textMeta.font).foregroundStyle(LumiaTokens.ink.color)
                            .textSelection(.enabled)
                            .padding(LumiaTokens.space12)
                            .frame(maxWidth: .infinity, alignment: .leading)
                            .background(LumiaTokens.surface.color, in: RoundedRectangle(cornerRadius: LumiaTokens.radius10))
                        HStack(spacing: LumiaTokens.space8) {
                            Button(copied ? "Copied" : "Copy link") {
                                UIPasteboard.general.string = invite.url
                                copied = true
                            }
                            .buttonStyle(.lumiaPrimary)
                            if let url = URL(string: invite.url) {
                                ShareLink(item: url) { Text("Share") }.buttonStyle(.lumiaSecondary)
                            }
                        }
                    }
                    if !invites.isEmpty {
                        SectionTitle("Invites")
                        ForEach(invites, id: \.code) { i in
                            HStack(spacing: LumiaTokens.space10) {
                                VStack(alignment: .leading, spacing: 0) {
                                    Text(i.code).font(LumiaTokens.textMember.font).foregroundStyle(LumiaTokens.ink.color)
                                    Text(describe(i)).font(LumiaTokens.textMeta.font).foregroundStyle(LumiaTokens.inkQuiet.color)
                                }
                                Spacer()
                                Button("Revoke") { Task { await revoke(i) } }
                                    .buttonStyle(.lumiaLink).disabled(busy)
                            }
                            .padding(.vertical, LumiaTokens.space4)
                        }
                    }
                }
                .padding(LumiaTokens.space16)
            }
        }
        .task { await load() }
    }

    private func describe(_ i: InviteSummary) -> String {
        let uses = i.maxUses == 0 ? "\(i.uses) uses" : "\(i.uses)/\(i.maxUses) uses"
        guard let micros = i.expiresAtMicros else { return "\(uses) · no expiry" }
        let at = Date(timeIntervalSince1970: TimeInterval(micros) / 1_000_000)
        return "\(uses) · expires \(at.formatted(date: .abbreviated, time: .shortened))"
    }

    private func load() async {
        guard let server else { return }
        do {
            invites = try await session.invites(serverID: server.serverId)
        } catch {
            self.error = error.lumiaCode == "PERMISSION_DENIED" ? "You cannot invite people to this server." : error.lumiaMessage
        }
    }

    private func make() async {
        guard let server else { return }
        busy = true
        error = ""
        copied = false
        defer { busy = false }
        do {
            let made = try await session.createInvite(serverID: server.serverId, maxUses: maxUses, expiresInHours: expiresInHours == 0 ? nil : expiresInHours)
            invite = made
            invites.insert(made, at: 0)
        } catch {
            self.error = error.lumiaCode == "PERMISSION_DENIED" ? "You cannot invite people to this server." : error.lumiaCode == "RATE_LIMITED" ? "Too many links in the last hour; try later." : error.lumiaMessage
        }
    }

    private func revoke(_ i: InviteSummary) async {
        busy = true
        error = ""
        defer { busy = false }
        do {
            try await session.revokeInvite(i.code)
            invites.removeAll { $0.code == i.code }
            if invite?.code == i.code { invite = nil }
        } catch {
            self.error = error.lumiaMessage
        }
    }
}

struct NewMessageScreen: View {
    let session: LumiaSession
    @Binding var path: [Screen]
    @State private var query = ""
    @State private var note = ""
    @State private var busy: String?

    private var list: [MemberSummary] {
        let q = query.trimmingCharacters(in: .whitespaces).lowercased()
        return session.people.values
            .filter { $0.accountId != session.me }
            .filter { q.isEmpty || $0.handle.lowercased().contains(q) || $0.displayName.lowercased().contains(q) || $0.nickname.lowercased().contains(q) }
            .sorted { ($0.displayName.isEmpty ? $0.handle : $0.displayName).localizedCompare($1.displayName.isEmpty ? $1.handle : $1.displayName) == .orderedAscending }
    }

    var body: some View {
        PanelScreen(back: "Home", title: "New message", subtitle: "Someone you share a server with") {
            ScrollView {
                VStack(alignment: .leading, spacing: LumiaTokens.space12) {
                    LumiaField("Find a person", text: $query, placeholder: "Name or handle")
                        .textInputAutocapitalization(.never).autocorrectionDisabled()
                    if !note.isEmpty { LumiaNotice(icon: "at") { Text(note) } }
                    if list.isEmpty {
                        LumiaEmptyState(icon: "at", title: "No one to show") { Text("Join a server to find people to message.") }
                    } else {
                        ForEach(list, id: \.accountId) { p in
                            let name = p.displayName.isEmpty ? p.handle : p.displayName
                            HStack(spacing: LumiaTokens.space10) {
                                LumiaAvatar(initials: initials(of: name), tone: .of(p.accountId), size: LumiaTokens.avatarSm)
                                VStack(alignment: .leading, spacing: 0) {
                                    Text(name).font(LumiaTokens.textMember.font).foregroundStyle(LumiaTokens.ink.color)
                                    Text("@\(p.handle)").font(LumiaTokens.textMeta.font).foregroundStyle(LumiaTokens.inkQuiet.color)
                                }
                                Spacer()
                                Button("Message") { Task { await message(p) } }
                                    .buttonStyle(.lumiaSecondary).disabled(busy == p.accountId)
                            }
                            .padding(.vertical, LumiaTokens.space4)
                        }
                    }
                }
                .padding(LumiaTokens.space16)
            }
            .scrollDismissesKeyboard(.interactively)
        }
    }

    private func message(_ p: MemberSummary) async {
        busy = p.accountId
        note = ""
        defer { busy = nil }
        do {
            if let channelID = try await session.dmOpenOrRequest(accountID: p.accountId) {
                path = [.conversation(id: channelID)]
            } else {
                note = "Request sent to \(p.displayName.isEmpty ? p.handle : p.displayName). The conversation opens when they accept."
            }
        } catch {
            note = error.lumiaCode == "ALREADY_EXISTS" ? "You already asked; waiting for their answer." : error.lumiaMessage
        }
    }
}

// MARK: Devices

// The person's own profile (architecture 6.4): the signed profile the
// identity carries, display name, pronouns and bio, signed by this
// phone's device key and sent to the instance as the account's. The same
// screen as the web's and the macOS app's.
struct ProfileScreen: View {
    @Environment(AppStore.self) private var store
    let session: LumiaSession
    @State private var profile: ProfileSummary?
    @State private var displayName = ""
    @State private var pronouns = ""
    @State private var bio = ""
    @State private var busy = false
    @State private var error = ""
    @State private var saved = false
    // The form waits for the profile held: fields shown before it lands
    // would be overwritten by it, typing and all.
    @State private var loaded = false

    var body: some View {
        PanelScreen(back: "Home", title: "Profile", subtitle: "How people see you", actions: []) {
            ScrollView {
                VStack(alignment: .leading, spacing: LumiaTokens.space12) {
                    if session.hasIdentity, loaded {
                        LumiaCard(title: handleTitle, subtitle: subtitle, width: nil) {
                            VStack(alignment: .leading, spacing: LumiaTokens.space12) {
                                LumiaField("Display name", text: $displayName, placeholder: "How people see you")
                                    .textContentType(.nickname)
                                LumiaField("Pronouns", text: $pronouns, placeholder: "they/them")
                                    .textInputAutocapitalization(.never).autocorrectionDisabled()
                                LumiaField("Bio", text: $bio, placeholder: "A line about you")
                                if !error.isEmpty { LumiaErrorText(error) }
                                if saved {
                                    Text("Saved. This instance shows it now.").font(LumiaTokens.textMeta.font).foregroundStyle(LumiaTokens.inkSecondary.color)
                                }
                                Button { Task { await save() } } label: { Text("Save").frame(maxWidth: .infinity) }
                                    .buttonStyle(.lumiaPrimary).disabled(busy)
                            }
                        }
                    } else if !session.hasIdentity {
                        LumiaNotice(icon: "shield", tone: .accent, action: ("Link this phone", { store.route = .link })) {
                            Text("Your profile travels with your identity, signed by its devices. This phone holds no device key yet; a device you already use approves it.")
                        }
                    }
                }
                .padding(LumiaTokens.space16)
            }
        }
        .scrollDismissesKeyboard(.interactively)
        .task { await load() }
    }

    private var handleTitle: String {
        (session.snapshot?.handle).flatMap { $0.isEmpty ? nil : "@\($0)" } ?? "You"
    }

    private var subtitle: String {
        profile.map { "Version \($0.version), signed by this phone's device key; every instance you sign in to takes it." } ?? "Nothing set yet: people see your handle."
    }

    private func load() async {
        profile = try? await session.profile()
        displayName = profile?.displayName ?? ""
        pronouns = profile?.pronouns ?? ""
        bio = profile?.bio ?? ""
        loaded = true
    }

    private func save() async {
        busy = true
        error = ""
        saved = false
        defer { busy = false }
        do {
            profile = try await session.setProfile(displayName: displayName, pronouns: pronouns, bio: bio)
            saved = true
        } catch {
            switch error.lumiaCode {
            case "FAILED_PRECONDITION": self.error = "This account is not linked to your identity, so the instance keeps the profile it has."
            case "CONFLICT": self.error = "A newer profile is already held. Reload and try again."
            default: self.error = error.lumiaMessage
            }
        }
    }
}

struct DevicesScreen: View {
    @Environment(AppStore.self) private var store
    let session: LumiaSession
    @State private var revoking: DeviceSummary?
    @State private var password = ""
    @State private var busy = false
    @State private var error = ""
    // Passkeys (architecture 6.3): the account's sign-in keys, listed and
    // removable here, made on the instance's web page in a browser sheet.
    @State private var passkeys: [PasskeySummary] = []
    @State private var passkeyBusy = false
    @State private var passkeyError = ""

    private var devices: [DeviceSummary] { session.roster?.devices ?? [] }
    private var active: [DeviceSummary] { devices.filter { !$0.revoked } }

    private func label(_ d: DeviceSummary) -> String {
        if d.thisDevice { return "This \(UIDevice.current.model)" }
        if !d.label.isEmpty { return d.label }
        switch d.kind {
        case "web": return "Browser"
        case "native": return "Computer"
        case "mobile": return "Phone"
        default: return d.kind.isEmpty ? "Device" : d.kind
        }
    }

    private func meta(_ d: DeviceSummary) -> String {
        var parts: [String] = []
        if !d.webOrigin.isEmpty { parts.append(d.webOrigin) }
        if d.addedAtMicros > 0 { parts.append("added \(day(ofMicros: d.addedAtMicros))") }
        parts.append(String(d.deviceId.prefix(8)))
        return parts.joined(separator: " · ")
    }

    var body: some View {
        PanelScreen(back: "Home", title: "Devices", subtitle: session.roster == nil ? "No identity on this phone" : "\(active.count) linked to you", actions: [("plus", "Link a device", { store.route = .link })]) {
            ScrollView {
                VStack(alignment: .leading, spacing: LumiaTokens.space12) {
                    LumiaNotice(icon: "shield") {
                        Text("Your identity lives on these devices, not on any instance. Revoking one takes effect on every instance that holds your roster.")
                    }
                    LumiaCard(title: "Passkeys", subtitle: "Sign in to this instance with a key this phone keeps, instead of a password.", width: nil) {
                        VStack(alignment: .leading, spacing: LumiaTokens.space8) {
                            ForEach(passkeys, id: \.passkeyId) { p in
                                HStack {
                                    Text(passkeyLine(p)).font(LumiaTokens.textMeta.font).foregroundStyle(LumiaTokens.inkSecondary.color)
                                    Spacer()
                                    Button("Remove") { Task { await removePasskey(p) } }.buttonStyle(.lumiaLink)
                                }
                            }
                            if !passkeyError.isEmpty { LumiaErrorText(passkeyError) }
                            Button { Task { await addPasskey() } } label: { Label("Add a passkey", systemImage: "key").frame(maxWidth: .infinity) }
                                .buttonStyle(.lumiaSecondary).disabled(passkeyBusy)
                            Text("Made on the instance's web page, in a browser sheet signed in as you.")
                                .font(LumiaTokens.textMeta.font).foregroundStyle(LumiaTokens.inkQuiet.color)
                        }
                    }
                    if session.roster != nil {
                        ForEach(active, id: \.deviceId) { d in
                            LumiaDeviceRow(device: d, label: label(d), meta: meta(d)) { revoking = d; password = ""; error = "" }
                                .padding(.horizontal, LumiaTokens.space14)
                                .background(LumiaTokens.tile.color, in: RoundedRectangle(cornerRadius: LumiaTokens.radius14))
                                .overlay(RoundedRectangle(cornerRadius: LumiaTokens.radius14).stroke(d.thisDevice ? LumiaTokens.accent.color : LumiaTokens.line.color))
                        }
                        let revoked = devices.filter(\.revoked)
                        if !revoked.isEmpty {
                            SectionTitle("Revoked").padding(.horizontal, -LumiaTokens.space10)
                            ForEach(revoked, id: \.deviceId) { d in
                                LumiaDeviceRow(device: d, label: String(d.deviceId.prefix(8)), meta: "", revoke: nil)
                                    .padding(.horizontal, LumiaTokens.space14)
                            }
                        }
                    } else {
                        LumiaEmptyState(icon: "shield", title: "No identity on this phone", action: ("Link this phone", { store.route = .link })) {
                            Text("A device you already use approves this one.")
                        }
                    }
                }
                .padding(LumiaTokens.space16)
            }
        }
        .task {
            await session.refreshRoster()
            await loadPasskeys()
        }
        .sheet(item: Binding(get: { revoking }, set: { if $0 == nil { revoking = nil } })) { d in
            revokeSheet(d)
                .presentationDetents([.medium])
                .presentationDragIndicator(.visible)
                .presentationBackground(LumiaTokens.surface.color)
        }
    }

    private func revokeSheet(_ d: DeviceSummary) -> some View {
        VStack(alignment: .leading, spacing: LumiaTokens.space16) {
            VStack(alignment: .leading, spacing: LumiaTokens.space4) {
                Text("Revoke \(label(d))?").font(LumiaTokens.titleAuth.font).foregroundStyle(LumiaTokens.ink.color)
                Text("It stops reading new messages the moment every instance has the change. Confirm with your password.")
                    .font(LumiaTokens.textSubtitle.font).foregroundStyle(LumiaTokens.inkSecondary.color)
            }
            LumiaField("Password", text: $password, placeholder: "Your password", secure: true)
                .textContentType(.password)
                .onSubmit { Task { await revoke(d) } }
            if !error.isEmpty { LumiaErrorText(error) }
            HStack(spacing: LumiaTokens.space8) {
                Button("Revoke \(label(d))") { Task { await revoke(d) } }
                    .buttonStyle(.lumiaPrimary).disabled(busy || password.isEmpty)
                Button("Keep it") { revoking = nil }.buttonStyle(.lumiaSecondary)
            }
            Spacer()
        }
        .padding(LumiaTokens.space24)
        .padding(.top, LumiaTokens.space8)
    }

    private func passkeyLine(_ p: PasskeySummary) -> String {
        let added = p.createdAtMicros.map { day(ofMicros: $0) } ?? "—"
        let used = p.lastUsedAtMicros.map { day(ofMicros: $0) } ?? "never"
        return "\(p.label.isEmpty ? "Passkey" : p.label) · added \(added) · last used \(used)"
    }

    private func loadPasskeys() async {
        passkeys = (try? await session.passkeys()) ?? []
    }

    private func removePasskey(_ p: PasskeySummary) async {
        passkeyError = ""
        do {
            try await session.removePasskey(id: p.passkeyId)
            await loadPasskeys()
        } catch {
            passkeyError = error.lumiaMessage
        }
    }

    private func addPasskey() async {
        passkeyBusy = true
        passkeyError = ""
        defer { passkeyBusy = false }
        do {
            try await session.openWebSignedIn(next: "devices")
            await loadPasskeys()
        } catch {
            passkeyError = error.lumiaMessage
        }
    }

    private func revoke(_ d: DeviceSummary) async {
        busy = true
        error = ""
        defer { busy = false }
        do {
            try await session.identityRevoke(deviceID: d.deviceId, password: password)
            revoking = nil
            password = ""
        } catch {
            self.error = error.lumiaCode == "UNAUTHENTICATED" ? "That password was not accepted." : error.lumiaMessage
        }
    }
}

extension DeviceSummary: @retroactive Identifiable {
    public var id: String { deviceId }
}
