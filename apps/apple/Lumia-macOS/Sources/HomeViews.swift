// Home: personal DMs grouped by the instance that hosts them, message
// requests, this identity's devices, and starting a conversation; the
// web's home.tsx on the desktop layout.
import LumiaFFI
import LumiaKit
import SwiftUI

struct HomeSidebar: View {
    let session: LumiaSession
    @Binding var view: HomeView
    @Binding var dmID: String?

    private var byHost: [(host: String, channels: [ChannelSummary])] {
        var order: [String] = []
        var groups: [String: [ChannelSummary]] = [:]
        for c in session.snapshot?.channels ?? [] where personalKinds.contains(c.kind) {
            let host = c.host.isEmpty ? "this instance" : c.host
            if groups[host] == nil { order.append(host) }
            groups[host, default: []].append(c)
        }
        return order.map { ($0, groups[$0] ?? []) }
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            HStack(spacing: LumiaTokens.space6) {
                Text("Home").font(LumiaTokens.titleServer.font).foregroundStyle(LumiaTokens.ink.color)
                Spacer(minLength: 0)
                Button { view = .new } label: {
                    Image(systemName: "square.and.pencil").font(.system(size: 14, weight: .medium))
                        .foregroundStyle(LumiaTokens.iconStrong.color).frame(width: 34, height: 34)
                }
                .buttonStyle(.plain).help("New message")
            }
            .padding(.leading, LumiaTokens.space8)
            .frame(height: 38)
            .padding(.bottom, LumiaTokens.space10)
            HStack(spacing: LumiaTokens.space6) {
                tile("Requests", icon: "at", count: session.snapshot?.pendingRequests.count ?? 0, current: view == .requests) { view = .requests }
                tile("Devices", icon: "shield", count: 0, current: view == .devices) { view = .devices }
                tile("New", icon: "square.and.pencil", count: 0, current: view == .new) { view = .new }
            }
            .padding(.bottom, LumiaTokens.space12)
            ScrollView {
                VStack(alignment: .leading, spacing: LumiaTokens.space2) {
                    if byHost.isEmpty {
                        Text("No conversations yet.")
                            .font(LumiaTokens.textMeta.font).foregroundStyle(LumiaTokens.inkQuiet.color)
                            .padding(LumiaTokens.space10)
                    }
                    ForEach(byHost, id: \.host) { group in
                        Text(group.host).font(LumiaTokens.textSection.font).foregroundStyle(LumiaTokens.inkMuted.color)
                            .padding(.horizontal, LumiaTokens.space10).padding(.top, LumiaTokens.space8).padding(.bottom, LumiaTokens.space6)
                        ForEach(group.channels, id: \.channelId) { c in
                            let name = session.conversationName(c)
                            let others = c.participantAccountIds.filter { $0 != session.me }
                            let active = view == .dm && dmID == c.channelId
                            Button { dmID = c.channelId; view = .dm } label: {
                                HStack(spacing: LumiaTokens.space10) {
                                    LumiaAvatar(initials: initials(of: name), tone: .of(others.first ?? c.channelId), size: LumiaTokens.avatarXs)
                                    Text(name).font(LumiaTokens.textNav.font)
                                        .foregroundStyle(active ? LumiaTokens.inkSelected.color : LumiaTokens.inkQuiet.color)
                                        .fontWeight(active ? .bold : .regular)
                                        .lineLimit(1)
                                    Spacer(minLength: 0)
                                }
                                .padding(.horizontal, LumiaTokens.space10)
                                .frame(height: 34)
                                .background(active ? LumiaTokens.surface.color : .clear, in: RoundedRectangle(cornerRadius: LumiaTokens.radius8))
                                .shadow(color: active ? .black.opacity(0.08) : .clear, radius: 2, y: 1)
                                .contentShape(RoundedRectangle(cornerRadius: LumiaTokens.radius8))
                            }
                            .buttonStyle(.plain)
                        }
                    }
                    Button("Add a server") { view = .add }
                        .buttonStyle(.lumiaLink)
                        .padding(.horizontal, LumiaTokens.space10).padding(.top, LumiaTokens.space8)
                }
            }
            Spacer(minLength: 0)
        }
        .padding(.top, LumiaTokens.space14)
        .padding(.leading, LumiaTokens.space12)
        .padding(.trailing, LumiaTokens.space10)
    }

    private func tile(_ label: String, icon: String, count: Int, current: Bool, action: @escaping () -> Void) -> some View {
        Button(action: action) {
            VStack(spacing: LumiaTokens.space4) {
                Image(systemName: icon).font(.system(size: 15, weight: .medium))
                Text(label).font(LumiaTokens.textCompact.font)
            }
            .foregroundStyle(current ? LumiaTokens.accentInk.color : LumiaTokens.inkBody.color)
            .frame(maxWidth: .infinity).frame(height: 58)
            .background(LumiaTokens.tile.color, in: RoundedRectangle(cornerRadius: LumiaTokens.radius14))
            .overlay(RoundedRectangle(cornerRadius: LumiaTokens.radius14).stroke(current ? LumiaTokens.accent.color : LumiaTokens.line.color))
            .overlay(alignment: .topTrailing) {
                if count > 0 {
                    Text("\(count)").font(LumiaTokens.textCount.font).foregroundStyle(LumiaTokens.onAccent.color)
                        .padding(.horizontal, 5).frame(minWidth: 18, minHeight: 18)
                        .background(LumiaTokens.accent.color, in: Capsule())
                        .offset(x: -6, y: 6)
                }
            }
        }
        .buttonStyle(.plain)
    }
}

// MARK: Add a server

/// A server of your own, or joining one from an invite link or code:
/// what the link leads to first, on which instance, then the join
/// (architecture 6.1).
struct AddServerView: View {
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
        PageHeader(title: "Add a server", topic: "Your own, or one you were invited to")
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
            .frame(maxWidth: 520, alignment: .leading)
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

struct HomeMain: View {
    let session: LumiaSession
    let view: HomeView
    @Binding var dmID: String?
    let go: (HomeView, String?) -> Void
    let openServer: (String) -> Void

    var body: some View {
        if view == .add {
            AddServerView(session: session, opened: openServer)
        } else if !session.hasIdentity {
            NoIdentityView()
        } else {
            switch view {
            case .requests: RequestsView(session: session, go: go)
            case .devices: DevicesView(session: session)
            case .profile: ProfileView(session: session)
            case .new: NewMessageView(session: session, go: go)
            case .add: EmptyView()
            case .dm:
                if let dmID, let channel = session.snapshot?.channels.first(where: { $0.channelId == dmID }) {
                    ConversationView(session: session, channel: channel)
                } else {
                    PageHeader(title: "Home")
                    LumiaEmptyState(icon: "lock", title: "Your conversations", action: ("New message", { go(.new, nil) })) {
                        Text("Personal messages are end-to-end encrypted. Pick one on the left, or start one with someone you share a server with.")
                    }
                    Spacer()
                }
            }
        }
    }
}

struct PageHeader: View {
    let title: String
    var topic: String?
    var body: some View {
        LumiaPaneHeader(title: title, topic: topic)
    }
}

struct NoIdentityView: View {
    @Environment(AppStore.self) private var store
    var body: some View {
        PageHeader(title: "Home")
        LumiaEmptyState(icon: "shield", title: "Link this computer to your identity", action: ("Link this computer", { store.route = .link })) {
            Text("Personal messages are end-to-end encrypted between your devices and theirs. This computer has no device key yet; a device you already use approves it.")
        }
        Spacer()
    }
}

struct ConversationView: View {
    let session: LumiaSession
    let channel: ChannelSummary
    @State private var draft = ""
    @State private var error = ""

    private var state: DmChannelSummary? { session.dmChannels.first { $0.channelId == channel.channelId } }
    private var log: DmLogSummary? { session.dmLogs[channel.channelId] }
    private var others: [String] { channel.participantAccountIds.filter { $0 != session.me } }
    private var title: String { session.conversationName(channel) }

    var body: some View {
        VStack(spacing: 0) {
            // The other side's pronouns, from their verified profile, then the host.
            PageHeader(title: title, topic: [others.map { session.pronouns(of: $0) }.filter { !$0.isEmpty }.joined(separator: ", "), channel.host.isEmpty ? "" : "on \(channel.host)"].filter { !$0.isEmpty }.joined(separator: " · ").nilIfEmpty)
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
                            LumiaDayDivider(label: "Messages")
                        }
                        ForEach(log?.messages ?? [], id: \.messageId) { m in
                            let who = m.mine ? session.selfName : (!m.senderAccountId.isEmpty || others.count != 1 ? session.name(of: m.senderAccountId) : session.name(of: others[0]))
                            let toneID = m.mine ? session.me : (m.senderAccountId.isEmpty ? m.senderIdentityId : m.senderAccountId)
                            LumiaMessageRow(author: who, tone: .of(toneID), time: time(ofMicros: m.sentAtMicros), text: m.body, deleted: m.deleted)
                                .id(m.messageId)
                        }
                        if !error.isEmpty { LumiaErrorText(error) }
                    }
                    .padding(LumiaTokens.space14)
                }
                .defaultScrollAnchor(.bottom)
                .onChange(of: log?.messages.count ?? 0) { _, _ in
                    if let last = log?.messages.last { proxy.scrollTo(last.messageId, anchor: .bottom) }
                }
            }
            LumiaComposer(text: $draft, placeholder: state?.member == true ? "Message \(title)" : "Setting up the encrypted group…", disabled: state?.member != true) {
                Task { await send() }
            }
        }
        .task(id: channel.channelId) {
            await session.refreshDmChannels()
            await session.refreshDmLog(channelID: channel.channelId)
        }
    }

    private func send() async {
        let body = draft.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !body.isEmpty else { return }
        draft = ""
        error = ""
        do { try await session.dmSend(channelID: channel.channelId, body: body) } catch { self.error = error.lumiaMessage }
    }

    private func rejoin() async {
        error = ""
        do { try await session.dmRejoin(channelID: channel.channelId) } catch { self.error = error.lumiaMessage }
    }
}

struct ConversationAside: View {
    let session: LumiaSession
    let channelID: String

    var body: some View {
        if let channel = session.snapshot?.channels.first(where: { $0.channelId == channelID }) {
            VStack(alignment: .leading, spacing: 0) {
                LumiaSegmentedTabs(tabs: [.init(id: "details", label: "Details")], selected: .constant("details"))
                    .padding(LumiaTokens.space12)
                Rectangle().fill(LumiaTokens.line.color).frame(height: 1)
                Text("In this conversation").font(LumiaTokens.textSection.font).foregroundStyle(LumiaTokens.inkMuted.color)
                    .padding(.horizontal, LumiaTokens.space18).padding(.top, LumiaTokens.space12).padding(.bottom, LumiaTokens.space6)
                ForEach(channel.participantAccountIds, id: \.self) { id in
                    let handle = (session.people[id] ?? session.profile(of: id))?.handle ?? ""
                    LumiaMemberRow(name: session.name(of: id), subtitle: handle.isEmpty ? nil : "@\(handle)", tone: .of(id), tag: id == session.me ? "You" : nil)
                        .padding(.horizontal, LumiaTokens.space8)
                }
                LumiaNotice(icon: "lock") { Text("History backup is off for this identity. Messages live only on your devices.") }
                    .padding(LumiaTokens.space14)
                Spacer()
            }
        }
    }
}

struct RequestsView: View {
    let session: LumiaSession
    let go: (HomeView, String?) -> Void
    @State private var busy: String?
    @State private var error = ""

    var body: some View {
        PageHeader(title: "Requests", topic: "People who want to message you")
        ScrollView {
            VStack(alignment: .leading, spacing: LumiaTokens.space12) {
                LumiaNotice(icon: "shield") {
                    Text("A request shows who is asking and what you share, nothing else. Accepting opens an encrypted conversation; the sender cannot see that you looked.")
                }
                if !error.isEmpty { LumiaErrorText(error) }
                let requests = session.snapshot?.pendingRequests ?? []
                if requests.isEmpty {
                    LumiaEmptyState(icon: "at", title: "Nothing waiting") { Text("Declined requests disappear; blocked people cannot ask again.") }
                } else {
                    ForEach(requests, id: \.requestId) { r in
                        LumiaRequestCard(
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
                go(.dm, channelID)
            }
        } catch {
            self.error = error.lumiaMessage
        }
    }
}

struct NewMessageView: View {
    let session: LumiaSession
    let go: (HomeView, String?) -> Void
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
        PageHeader(title: "New message", topic: "Someone you share a server with")
        ScrollView {
            VStack(alignment: .leading, spacing: LumiaTokens.space12) {
                LumiaField("Find a person", text: $query, placeholder: "Name or handle")
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
    }

    private func message(_ p: MemberSummary) async {
        busy = p.accountId
        note = ""
        defer { busy = nil }
        do {
            if let channelID = try await session.dmOpenOrRequest(accountID: p.accountId) {
                go(.dm, channelID)
            } else {
                note = "Request sent to \(p.displayName.isEmpty ? p.handle : p.displayName). The conversation opens when they accept."
            }
        } catch {
            note = error.lumiaCode == "ALREADY_EXISTS" ? "You already asked; waiting for their answer." : error.lumiaMessage
        }
    }
}

// The person's own profile (architecture 6.4): the signed profile the
// identity carries, display name, pronouns and bio, signed by this
// computer's device key and sent to the instance as the account's. The
// same screen as the web's.
struct ProfileView: View {
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
        PageHeader(title: "Profile", topic: "How people see you")
        ScrollView {
            VStack(alignment: .leading, spacing: LumiaTokens.space12) {
                if session.hasIdentity, loaded {
                    LumiaCard(title: handleTitle, subtitle: subtitle, width: nil) {
                        VStack(alignment: .leading, spacing: LumiaTokens.space12) {
                            LumiaField("Display name", text: $displayName, placeholder: "How people see you")
                            LumiaField("Pronouns", text: $pronouns, placeholder: "they/them")
                            LumiaField("Bio", text: $bio, placeholder: "A line about you")
                            if !error.isEmpty { LumiaErrorText(error) }
                            if saved {
                                Text("Saved. This instance shows it now.").font(LumiaTokens.textMeta.font).foregroundStyle(LumiaTokens.inkSecondary.color)
                            }
                            Button("Save") { Task { await save() } }.buttonStyle(.lumiaPrimary).disabled(busy)
                        }
                    }
                } else if !session.hasIdentity {
                    LumiaNotice(icon: "shield", action: ("Link this computer", { store.route = .link })) {
                        Text("Your profile travels with your identity, signed by its devices. This computer holds no device key yet; a device you already use approves it.")
                    }
                }
            }
            .padding(LumiaTokens.space16)
        }
        .task { await load() }
    }

    private var handleTitle: String {
        (session.snapshot?.handle).flatMap { $0.isEmpty ? nil : "@\($0)" } ?? "You"
    }

    private var subtitle: String {
        profile.map { "Version \($0.version), signed by this computer's device key; every instance you sign in to takes it." } ?? "Nothing set yet: people see your handle."
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

struct DevicesView: View {
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

    private func label(_ d: DeviceSummary) -> String {
        if d.thisDevice { return "This computer" }
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
        PageHeader(title: "Devices", topic: "Everything that holds your identity")
        ScrollView {
            VStack(alignment: .leading, spacing: LumiaTokens.space12) {
                LumiaNotice(icon: "shield", action: ("Link a device", { store.route = .link })) {
                    Text("Your identity lives on these devices, not on any instance. Revoking one takes effect on every instance that holds your roster.")
                }
                LumiaCard(title: "Passkeys", subtitle: "Sign in to this instance with a key this computer or your phone keeps, instead of a password.", width: nil) {
                    VStack(alignment: .leading, spacing: LumiaTokens.space8) {
                        ForEach(passkeys, id: \.passkeyId) { p in
                            HStack {
                                Text(passkeyLine(p)).font(LumiaTokens.textMeta.font).foregroundStyle(LumiaTokens.inkSecondary.color)
                                Spacer()
                                Button("Remove") { Task { await removePasskey(p) } }.buttonStyle(.lumiaLink)
                            }
                        }
                        if !passkeyError.isEmpty { LumiaErrorText(passkeyError) }
                        Button { Task { await addPasskey() } } label: { Label("Add a passkey", systemImage: "key") }
                            .buttonStyle(.lumiaSecondary).disabled(passkeyBusy)
                        Text("Made on the instance's web page, in a browser sheet signed in as you.")
                            .font(LumiaTokens.textMeta.font).foregroundStyle(LumiaTokens.inkQuiet.color)
                    }
                }
                if let roster = session.roster {
                    ForEach(roster.devices.filter { !$0.revoked }, id: \.deviceId) { d in
                        LumiaDeviceRow(device: d, label: label(d), meta: meta(d)) { revoking = d }
                    }
                    if let d = revoking {
                        LumiaCard(title: "Revoke \(label(d))?", subtitle: "It stops reading new messages the moment every instance has the change. Confirm with your password.") {
                            VStack(alignment: .leading, spacing: LumiaTokens.space12) {
                                LumiaField("Password", text: $password, placeholder: "Your password", secure: true)
                                    .onSubmit { Task { await revoke(d) } }
                                if !error.isEmpty { LumiaErrorText(error) }
                                HStack {
                                    Button("Revoke \(label(d))") { Task { await revoke(d) } }
                                        .buttonStyle(.lumiaPrimary).disabled(busy || password.isEmpty)
                                    Button("Keep it") { revoking = nil; password = ""; error = "" }.buttonStyle(.lumiaSecondary)
                                }
                            }
                        }
                    }
                    let revoked = roster.devices.filter(\.revoked)
                    if !revoked.isEmpty {
                        Text("Revoked").font(LumiaTokens.textSection.font).foregroundStyle(LumiaTokens.inkQuiet.color).padding(.top, LumiaTokens.space8)
                        ForEach(revoked, id: \.deviceId) { d in
                            LumiaDeviceRow(device: d, label: String(d.deviceId.prefix(8)), meta: "", revoke: nil)
                        }
                    }
                } else {
                    LumiaEmptyState(icon: "shield", title: "No identity on this computer")
                }
            }
            .padding(LumiaTokens.space16)
        }
        .task {
            await session.refreshRoster()
            await loadPasskeys()
        }
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
