// A conversation on the phone: a server's channel or DM, and a personal
// conversation. Messages in blocks, or a line each as Preferences says,
// with a divider over each day's first, and what architecture 6.6 adds (`Replies.swift` for the sheets): a
// reply's reference line above it, a message's mentions as pills by name,
// its reactions under it and a wash when it is for this account; a long
// press opens its actions; editing one's own message in place, where its
// words were, and deleting after a confirmation; the reply bar inside the
// composer with its "@ On" switch, the @ list in a server's channel and the
// bar after a send that mentioned someone not in it; the "New messages"
// line, which the list opens at when it is above the fold; and the
// conversation marked read while its newest message is on screen and the
// app is active.
import LumiaFFI
import LumiaKit
import SwiftUI

/// Where the list is asked to scroll: a message, the "New messages" line or its end.
struct ScrollRequest: Equatable {
    let id: String
    let anchor: UnitPoint
    let n: Int
}

/// The list's last row, empty: on screen when the newest message is.
private struct NewestMark: View {
    @Binding var visible: Bool
    var body: some View {
        Color.clear.frame(height: 1).id("end")
            .onAppear { visible = true }
            .onDisappear { visible = false }
            .accessibilityHidden(true)
    }
}

/// Lands a conversation as it opens and marks it read: what both kinds
/// share. Opened, it notes where it had been read to (its "New messages"
/// line), shows a message a Mentions page asked for or else that line, or
/// its end; then, while its newest message is on screen and the app is
/// active, it is marked read through the newest message's `seq`.
private struct Reading: ViewModifier {
    @Environment(\.scenePhase) private var scenePhase
    let session: LumiaSession
    let channelID: String
    /// The newest message's `seq`, 0 for none.
    let newest: UInt64
    /// Not marked: a channel read from Browse, a conversation this device is not in yet.
    let marks: Bool
    let load: () async -> Void
    let jump: (String) -> Void
    @Binding var newestVisible: Bool
    @Binding var scroll: ScrollRequest?
    @State private var landed = false

    func body(content: Content) -> some View {
        content
            .task(id: channelID) {
                landed = false
                await load()
                session.opened(channelID)
                if let reveal = session.pendingReveal, reveal.channelID == channelID {
                    session.pendingReveal = nil
                    jump(reveal.messageID)
                } else if session.newLine(in: channelID) != nil {
                    scroll = ScrollRequest(id: NewMessagesLine.words, anchor: .top, n: (scroll?.n ?? 0) + 1)
                } else {
                    scroll = ScrollRequest(id: "end", anchor: .bottom, n: (scroll?.n ?? 0) + 1)
                }
                // The list has moved to where it lands before anything counts as seen.
                try? await Task.sleep(for: .milliseconds(600))
                landed = true
            }
            .onDisappear { session.left(channelID) }
            .onChange(of: "\(landed)|\(newestVisible)|\(newest)|\(scenePhase == .active)|\(session.unread(channelID).unread)", initial: true) { _, _ in
                guard landed, marks, newestVisible, scenePhase == .active, newest > 0 else { return }
                Task { await session.markRead(channelID: channelID, seq: newest) }
            }
    }
}

// MARK: Editing and deleting

/// A conversation's edits and deletions (architecture 6.6): the message
/// being edited in place and what its field holds, the one whose deletion
/// is being asked about, and what the actions' sheet chose to do once it
/// has gone. What the instance refused of an edit is the session's
/// (`editRefusals`), matched to the edit by its command id.
struct Changes {
    var editing: String?
    var draft = ""
    var deleting: MessageRef?
    var chosen: (MessageRef, MessageAction)?
}

/// What both kinds of conversation do with their `Changes`.
@MainActor
struct Changer {
    let session: LumiaSession
    let channelID: String
    @Binding var changes: Changes
    @Binding var error: String

    /// Edit message: the field in place of the words, holding them as written.
    func start(_ ref: MessageRef) {
        changes.editing = ref.id
        changes.draft = ref.body
        session.clearEditRefusal(ref.id)
    }

    /// The field, for the message being edited.
    func edit(_ ref: MessageRef) -> MessageEdit? {
        guard changes.editing == ref.id else { return nil }
        return MessageEdit(draft: $changes.draft, save: { save(ref) }, cancel: { changes.editing = nil }, room: { session.client.messageRoom($0) })
    }

    /// Return or Save: the field goes at once and the message shows the new
    /// words when the edit comes back; nothing changed sends nothing, and
    /// nothing left asks to delete instead.
    func save(_ ref: MessageRef) {
        // Over the instance's limit the field stays, holding its words.
        guard !MessageCounter.over(session.client.messageRoom(changes.draft)) else { return }
        switch MessageEdits.outcome(original: ref.body, draft: changes.draft) {
        case .unchanged:
            changes.editing = nil
        case .delete:
            changes.deleting = ref
        case let .save(body):
            changes.editing = nil
            Task { await session.edit(channelID: channelID, messageID: ref.id, body: body) }
        }
    }

    /// Delete, from the confirmation.
    func delete(_ ref: MessageRef) {
        if changes.editing == ref.id { changes.editing = nil }
        Task {
            do { try await session.delete(channelID: channelID, messageID: ref.id) } catch let refused { self.error = refused.lumiaMessage }
        }
    }

    /// The sheet has gone: Edit message or Delete message, as it chose.
    func afterSheet() {
        guard let (ref, action) = changes.chosen else { return }
        changes.chosen = nil
        switch action {
        case .edit: start(ref)
        case .delete: changes.deleting = ref
        default: break
        }
    }

}

extension View {
    /// "Delete this message?" over a conversation: the message's first line,
    /// Cancel, and Delete, which the system draws as destructive.
    func deleteConfirmation(_ changer: Changer) -> some View {
        alert(MessageEdits.deleteTitle, isPresented: Binding(get: { changer.changes.deleting != nil }, set: { if !$0 { changer.changes.deleting = nil } }), presenting: changer.changes.deleting) { ref in
            Button("Cancel", role: .cancel) {}
            Button("Delete", role: .destructive) { changer.delete(ref) }
        } message: { ref in
            Text(MessageEdits.firstLine(ref.drawn))
        }
    }
}

// MARK: Personal conversation

struct ConversationScreen: View {
    @Environment(Preferences.self) private var preferences
    let session: LumiaSession
    let channelID: String
    @State private var draft = ""
    @State private var error = ""
    @State private var replying: QuoteSummary?
    @State private var replyingTo = ""
    @State private var sheet: MessageSheet?
    @State private var changes = Changes()
    @State private var flash: String?
    @State private var newestVisible = false
    @State private var scroll: ScrollRequest?
    /// The long messages shown whole after their Show more, while this conversation is open.
    @State private var expanded: Set<String> = []
    @FocusState private var composing: Bool
    /// The message text at the system's text size, which the font size scales.
    @ScaledMetric(relativeTo: .body) private var systemText = LumiaTokens.textMessage.size

    private var changer: Changer { Changer(session: session, channelID: channelID, changes: $changes, error: $error) }

    /// How the messages are drawn: Preferences' message settings, here as in a channel.
    private var measures: MessageMeasures { preferences.measures(system: systemText / LumiaTokens.textMessage.size) }

    private var channel: ChannelSummary? { session.snapshot?.channels.first { $0.channelId == channelID } }
    private var state: DmChannelSummary? { session.dmChannels.first { $0.channelId == channelID } }
    private var log: DmLogSummary? { session.dmLogs[channelID] }
    private var others: [String] { channel?.participantAccountIds.filter { $0 != session.me } ?? [] }
    private var title: String { channel.map { session.conversationName($0) } ?? "Conversation" }
    private var member: Bool { state?.member == true }

    var body: some View {
        PanelScreen(back: "Home", title: title, subtitle: [others.map { session.pronouns(of: $0) }.filter { !$0.isEmpty }.joined(separator: ", "), (channel?.host).flatMap { $0.isEmpty ? nil : "on \($0)" } ?? ""].filter { !$0.isEmpty }.joined(separator: " · ").nilIfEmpty, lead: {
            LumiaAvatar(initials: initials(of: title), tone: .of(others.first ?? channelID), size: LumiaTokens.avatarMd, picture: others.count == 1 ? session.picture(of: others[0]) : nil)
        }) {
            ScrollViewReader { proxy in
                ScrollView {
                    LazyVStack(alignment: .leading, spacing: 0) {
                        VStack(alignment: .leading, spacing: LumiaTokens.space8) {
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
                        }
                        .padding(.horizontal, LumiaTokens.space16)
                        .padding(.bottom, LumiaTokens.space8)
                        // The same blocks and day dividers as a server's channel.
                        let sent = log?.messages ?? []
                        let places = MessageBlocks.places(sent.map { MessageStamp(authorID: authorID(of: $0), atMicros: $0.sentAtMicros, quotes: $0.reply != nil) })
                        let line = session.newLine(in: channelID)
                        ForEach(Array(zip(sent, places)), id: \.0.messageId) { m, place in
                            if place.startsDay { LumiaDayDivider(label: dayLabel(ofMicros: m.sentAtMicros), measures: measures) }
                            if line == m.messageId { LumiaNewMessagesLine(scale: measures.text).id(NewMessagesLine.words) }
                            LumiaMessageRow(author: who(m), tone: .of(authorID(of: m)), picture: session.picture(of: accountID(of: m)), time: time(ofMicros: m.sentAtMicros), text: m.body, deleted: m.deleted, place: place, measures: measures, extras: extras(m))
                                .id(m.messageId)
                        }
                        if !error.isEmpty { LumiaErrorText(error).padding(.horizontal, LumiaTokens.space16) }
                        NewestMark(visible: $newestVisible)
                    }
                    .padding(.top, LumiaTokens.space16)
                    .padding(.bottom, LumiaTokens.space8)
                }
                .defaultScrollAnchor(.bottom)
                .scrollDismissesKeyboard(.interactively)
                .onChange(of: log?.messages.last?.messageId) { _, _ in
                    // A new message keeps the list at its end when it was there, or when it is one's own.
                    if newestVisible || log?.messages.last?.mine == true { proxy.scrollTo("end", anchor: .bottom) }
                }
                .onChange(of: scroll) { _, request in
                    guard let request else { return }
                    withAnimation { proxy.scrollTo(request.id, anchor: request.anchor) }
                }
            }
            // No one is said to be typing here, and this device says nothing of
            // it either: the instance is not to learn who is writing in a
            // personal conversation (protocol 6.7; an open item of the E2EE spec).
            // The reply bar has no "@ On" here: every message of a personal conversation notifies.
            LumiaComposer(text: $draft, placeholder: member ? "Message \(title)" : "Setting up the encrypted group…", disabled: !member, hint: nil, focused: $composing, reply: replying.map { _ in ComposerReply(name: replyingTo, mention: nil) { replying = nil } }, editLast: editLast, textScale: measures.text, room: { session.client.messageRoom($0) }) {
                Task { await send() }
            } above: {
                EmptyView()
            }
        }
        .modifier(Reading(session: session, channelID: channelID, newest: log?.messages.last?.seq ?? 0, marks: member, load: {
            await session.refreshDmChannels()
            await session.refreshDmLog(channelID: channelID)
        }, jump: jump, newestVisible: $newestVisible, scroll: $scroll))
        // A message's sheet takes the keyboard from the composer, so its words can be selected.
        .onChange(of: sheet) { _, open in if open != nil { composing = false } }
        .sheet(item: $sheet, onDismiss: changer.afterSheet) { sheet in
            MessageSheetView(sheet: sheet, react: react, reply: { m, part in startReply(id: m.id, name: m.authorName, body: m.drawn, part: part) }, chose: { changes.chosen = ($0, $1) })
        }
        .deleteConfirmation(changer)
        .onChange(of: channelID) { _, _ in expanded = [] }
    }

    /// Whose a message is, for its block and its avatar's tone:
    /// the account. A message that names none is the other person's when
    /// there is only one, so their rows wear the tone the header and Home
    /// give them; in a group it is left to its identity.
    private func authorID(of m: DmMessageSummary) -> String {
        if m.mine { return session.me }
        if !m.senderAccountId.isEmpty { return m.senderAccountId }
        return others.count == 1 ? others[0] : m.senderIdentityId
    }

    /// The account a message is from, for its picture: none when it is
    /// known only by its identity (a group's), which keeps its initials.
    private func accountID(of m: DmMessageSummary) -> String {
        if m.mine { return session.me }
        if !m.senderAccountId.isEmpty { return m.senderAccountId }
        return others.count == 1 ? others[0] : ""
    }

    private func who(_ m: DmMessageSummary) -> String {
        m.mine ? session.selfName : (!m.senderAccountId.isEmpty || others.count != 1 ? session.name(of: m.senderAccountId) : session.name(of: others[0]))
    }

    /// A message as its actions need it: only one's own is edited or deleted here.
    private func ref(_ m: DmMessageSummary) -> MessageRef {
        MessageRef(id: m.messageId, authorID: authorID(of: m), authorName: who(m), body: m.body, drawn: m.body, actions: MessageAction.offered(mine: m.mine, personal: true, canManageMessages: false))
    }

    private func extras(_ m: DmMessageSummary) -> MessageExtras {
        var x = MessageExtras()
        // A quote whose author is not known draws no name.
        x.quote = m.reply.map { q in QuoteDisplay(authorID: q.authorAccountId, name: q.authorAccountId.isEmpty ? nil : session.name(of: q.authorAccountId), picture: session.picture(of: q.authorAccountId), text: q.text, deleted: q.originalDeleted, quotesMe: q.authorAccountId == session.me, open: { jump(q.messageId) }) }
        x.reactions = reactionDisplays(m.reactions, session: session)
        x.flashed = flash == m.messageId
        x.replying = replying?.messageId == m.messageId
        x.edited = m.edited
        x.notSaved = session.editRefusals[m.messageId]
        x.expanded = expanded.contains(m.messageId)
        x.expand = { expanded.formSymmetricDifference([m.messageId]) }
        guard member else { return x }
        let ref = ref(m)
        x.edit = changer.edit(ref)
        x.toggle = { react(m.messageId, $0) }
        x.addReaction = { sheet = .picker(messageID: m.messageId) }
        x.reply = { startReply(id: m.messageId, name: who(m), body: ref.drawn, part: nil) }
        if ref.actions.contains(.edit) { x.editMessage = { changer.start(ref) } }
        if ref.actions.contains(.delete) { x.deleteMessage = { changes.deleting = ref } }
        x.actions = { sheet = .actions(ref) }
        return x
    }

    /// Up in an empty composer: this account's newest message, edited.
    private func editLast() {
        let all = log?.messages ?? []
        guard member, let id = MessageEdits.newestOwn(all.map { ($0.messageId, $0.mine && !$0.deleted) }), let m = all.first(where: { $0.messageId == id }) else { return }
        changer.start(ref(m))
    }

    private func startReply(id: String, name: String, body: String, part: String?) {
        replying = Replies.quote(messageID: id, authorID: "", body: body, part: part)
        replyingTo = name
        composing = true
    }

    private func react(_ messageID: String, _ emoji: String) {
        Task {
            do { try await session.react(channelID: channelID, messageID: messageID, emoji: emoji) } catch { self.error = error.lumiaMessage }
        }
    }

    /// A quote's original: scrolled to and marked for a moment, when it is here.
    private func jump(_ messageID: String) {
        Task {
            guard await session.reveal(channelID: channelID, messageID: messageID) else { return }
            scroll = ScrollRequest(id: messageID, anchor: .center, n: (scroll?.n ?? 0) + 1)
            flash = messageID
            try? await Task.sleep(for: .seconds(Replies.flashSeconds))
            if flash == messageID { flash = nil }
        }
    }

    private func send() async {
        let body = draft.trimmingCharacters(in: .whitespacesAndNewlines)
        // Over the instance's limit the draft stays, as the counter says.
        guard !body.isEmpty, !MessageCounter.over(session.client.messageRoom(body)) else { return }
        let quote = replying
        draft = ""
        replying = nil
        error = ""
        do { try await session.dmSend(channelID: channelID, body: body, quote: quote) } catch { self.error = error.lumiaMessage }
    }

    private func rejoin() async {
        error = ""
        do { try await session.dmRejoin(channelID: channelID) } catch { self.error = error.lumiaMessage }
    }
}

// MARK: Channel

/// A server's channel, or one of its DMs (architecture 6.4 and 6.8): the
/// messages in blocks and the notices between them as one quiet line
/// each. A channel this account is not in (opened from Browse) reads the
/// same, with a bar to join in place of the composer; a server DM is
/// titled by the other person and says once, at its top, whose it is.
struct ChannelScreen: View {
    @Environment(Preferences.self) private var preferences
    @Environment(\.openPerson) private var openPerson
    let session: LumiaSession
    let channelID: String
    @Binding var path: [Screen]
    @State private var draft = ""
    @State private var error = ""
    @State private var joining = false
    @State private var replying: QuoteSummary?
    /// The reply bar's "@ On": on for each new reply.
    @State private var mentionOn = true
    @State private var sheet: MessageSheet?
    @State private var changes = Changes()
    @State private var outside: OutsideNote?
    @State private var flash: String?
    @State private var newestVisible = false
    @State private var scroll: ScrollRequest?
    /// The long messages shown whole after their Show more, while this conversation is open.
    @State private var expanded: Set<String> = []
    @FocusState private var composing: Bool
    /// The message text at the system's text size, which the font size scales.
    @ScaledMetric(relativeTo: .body) private var systemText = LumiaTokens.textMessage.size

    private var changer: Changer { Changer(session: session, channelID: channelID, changes: $changes, error: $error) }

    /// How the messages are drawn: Preferences' message settings. The list,
    /// and of the rest only the composer's words, which take the font size:
    /// the header and the typing line stay as they are.
    private var measures: MessageMeasures { preferences.measures(system: systemText / LumiaTokens.textMessage.size) }

    private var channel: ChannelSummary? { session.channel(channelID) }
    private var server: ServerSummary? { session.snapshot?.servers.first { $0.serverId == channel?.serverId } }
    private var messages: [MessageSummary] { session.messages[channelID] ?? [] }
    private var name: String { channel?.name ?? "channel" }
    private var isDM: Bool { channel?.kind == serverDMKind }
    /// Who a server DM is with.
    private var other: String { channel.map { Membership.other(in: $0, me: session.me) } ?? "" }
    private var title: String { isDM ? session.name(of: other) : "# \(name)" }
    /// In it, rather than reading it from Browse.
    private var member: Bool { session.isIn(channelID) }

    /// The @ list's rows for the `@word` the draft ends in; none in a DM.
    private var choices: [MentionChoice] {
        guard member, !isDM, let query = MentionQuery.at(draft), let server else { return [] }
        return MentionList.choices(query: query.query, members: session.serverMembers[server.serverId] ?? [], inside: session.channelMemberIDs[channelID], me: session.me, everyone: server.canMentionEveryone) { session.name(of: $0.accountId) }
    }

    var body: some View {
        PanelScreen(back: server?.name ?? "the server", title: title, subtitle: server?.name, actions: isDM ? [] : [("person.2", "Members", { path.append(.channelMembers(channelID: channelID)) })], lead: {
            if isDM { LumiaAvatar(initials: initials(of: title), tone: .of(other), size: LumiaTokens.avatarMd, picture: session.picture(of: other)) }
        }) {
            ScrollViewReader { proxy in
                ScrollView {
                    LazyVStack(alignment: .leading, spacing: 0) {
                        if isDM {
                            LumiaNotice(icon: "shield") { Text(Membership.serverDMNote(server: server?.name ?? "this server")) }
                                .padding(.horizontal, LumiaTokens.space16)
                                .padding(.vertical, LumiaTokens.space8)
                        } else if messages.isEmpty {
                            LumiaEmptyState(icon: "bubble.left", title: "Nothing here yet") { Text("Say something.") }
                        }
                        // What one person sends close together is one block, and
                        // each day's first message carries the day's divider; a
                        // notice is a block of its own, and so is a quote.
                        let places = MessageBlocks.places(messages.map(messageStamp))
                        let line = session.newLine(in: channelID)
                        ForEach(Array(zip(messages, places)), id: \.0.messageId) { m, place in
                            if place.startsDay { LumiaDayDivider(label: dayLabel(ofMicros: m.createdAtMicros), measures: measures) }
                            if line == m.messageId { LumiaNewMessagesLine(scale: measures.text).id(NewMessagesLine.words) }
                            if !m.notice.isEmpty {
                                if let text = noticeText(kind: m.notice, actor: session.name(of: m.authorAccountId), subject: session.name(of: m.noticeAccountId)) {
                                    LumiaNoticeRow(text: text, time: time(ofMicros: m.createdAtMicros), measures: measures)
                                        .id(m.messageId)
                                }
                            } else {
                                LumiaMessageRow(author: session.name(of: m.authorAccountId), tone: .of(m.authorAccountId), picture: session.picture(of: m.authorAccountId), time: time(ofMicros: m.createdAtMicros), text: m.body, deleted: m.deleted, place: place, measures: measures, openAuthor: openAuthor(m.authorAccountId), extras: extras(m))
                                    .id(m.messageId)
                            }
                        }
                        if !error.isEmpty { LumiaErrorText(error).padding(.horizontal, LumiaTokens.space16) }
                        NewestMark(visible: $newestVisible)
                    }
                    .padding(.vertical, LumiaTokens.space8)
                }
                .defaultScrollAnchor(.bottom)
                .scrollDismissesKeyboard(.interactively)
                .onChange(of: messages.last?.messageId) { _, _ in
                    // A new message keeps the list at its end when it was there, or when it is one's own.
                    if newestVisible || messages.last?.authorAccountId == session.me { proxy.scrollTo("end", anchor: .bottom) }
                }
                .onChange(of: scroll) { _, request in
                    guard let request else { return }
                    withAnimation { proxy.scrollTo(request.id, anchor: request.anchor) }
                }
            }
            if member {
                // Who is typing goes under the field, in a server's channel only (protocol 6.7).
                LumiaComposer(text: $draft, placeholder: isDM ? "Message \(title)" : "Message #\(name)", hint: nil, typing: session.typingNames(in: channelID), focused: $composing, reply: replyBar, editLast: editLast, textScale: measures.text, room: { session.client.messageRoom($0) }) {
                    Task { await send() }
                } above: {
                    composerBars
                }
            } else {
                JoinBar(line: Membership.notIn(name), busy: joining) { Task { await join() } }
            }
        }
        .modifier(Reading(session: session, channelID: channelID, newest: messages.last?.seq ?? 0, marks: member, load: {
            await session.refreshMessages(channelID: channelID)
            if member, !isDM { await session.loadChannelMembers(channelID: channelID) }
        }, jump: jump, newestVisible: $newestVisible, scroll: $scroll))
        // Every edit that leaves something in the field is typing; emptied
        // is not (a send). The core spaces what is sent.
        .onChange(of: draft) { _, draft in
            if !draft.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty { session.typed(channelID: channelID) }
        }
        .onChange(of: session.commandError) { _, _ in
            if let refused = session.takeCommandError() { error = refused }
        }
        // A message's sheet takes the keyboard from the composer, so its words can be selected.
        .onChange(of: sheet) { _, open in if open != nil { composing = false } }
        .sheet(item: $sheet, onDismiss: changer.afterSheet) { sheet in
            MessageSheetView(sheet: sheet, react: react, reply: { m, part in startReply(id: m.id, authorID: m.authorID, body: m.drawn, part: part) }, chose: { changes.chosen = ($0, $1) })
        }
        .deleteConfirmation(changer)
        .onChange(of: channelID) { _, _ in expanded = [] }
    }

    /// The reply bar inside the composer, with its "@ On" for a reply to
    /// someone else; none for a reply to oneself.
    private var replyBar: ComposerReply? {
        guard let replying else { return nil }
        let switchable = Replies.offersMentionSwitch(quotedAuthor: replying.authorAccountId, me: session.me, personal: false)
        return ComposerReply(name: session.name(of: replying.authorAccountId), mention: switchable ? $mentionOn : nil) { self.replying = nil }
    }

    /// Over the composer: the bar after a send and the @ list.
    @ViewBuilder private var composerBars: some View {
        if let outside {
            LumiaComposerBar {
                VStack(alignment: .leading, spacing: LumiaTokens.space8) {
                    Text(outside.said ?? Replies.outsideWords(names: outside.accountIDs.map { session.name(of: $0) }, channel: name))
                        .font(LumiaTokens.textMeta.font).foregroundStyle(LumiaTokens.inkBody.color)
                        .fixedSize(horizontal: false, vertical: true)
                        .frame(maxWidth: .infinity, alignment: .leading)
                    if !outside.error.isEmpty { LumiaErrorText(outside.error) }
                    if outside.said == nil {
                        HStack(spacing: LumiaTokens.space10) {
                            Button("Add to #\(name)") { Task { await addOutside() } }
                                .buttonStyle(.lumiaSecondary).disabled(outside.adding)
                            Button("Dismiss") { self.outside = nil }.buttonStyle(.lumiaLink)
                        }
                    }
                }
            } end: { EmptyView() }
        }
        let rows = choices
        if !rows.isEmpty {
            LumiaMentionList(rows: rows.map(row)) { n in
                guard let query = MentionQuery.at(draft), rows.indices.contains(n) else { return }
                switch rows[n] {
                case .everyone: draft = query.picking("everyone")
                case let .member(m, _): draft = query.picking(m.handle)
                }
            }
        }
    }

    private func row(_ choice: MentionChoice) -> LumiaMentionList.Row {
        switch choice {
        case .everyone:
            return .init(id: "everyone", name: "@everyone", handle: "", note: MentionList.everyoneNote(channel: name), tone: nil)
        case let .member(m, inside):
            return .init(id: m.accountId, name: session.name(of: m.accountId), handle: "@\(m.handle)", note: inside ? "" : MentionList.notIn(channel: name), tone: .of(m.accountId), picture: session.picture(of: m.accountId))
        }
    }

    /// A message as its actions need it: its author edits it, and deletes
    /// it as one who may manage the server's messages may.
    private func ref(_ m: MessageSummary) -> MessageRef {
        let drawn = MentionSpans.drawn(body: m.body, mentions: m.mentions) { knownName($0, session: session) }
        let actions = MessageAction.offered(mine: m.authorAccountId == session.me, personal: false, canManageMessages: session.canManageMessages(channelID: channelID))
        return MessageRef(id: m.messageId, authorID: m.authorAccountId, authorName: session.name(of: m.authorAccountId), body: m.body, drawn: drawn, actions: actions)
    }

    private func extras(_ m: MessageSummary) -> MessageExtras {
        var x = MessageExtras()
        x.mentions = m.mentions
        x.name = { knownName($0, session: session) }
        x.quote = m.reply.map { q in QuoteDisplay(authorID: q.authorAccountId, name: q.authorAccountId.isEmpty ? nil : session.name(of: q.authorAccountId), picture: session.picture(of: q.authorAccountId), text: q.text, deleted: q.originalDeleted, quotesMe: q.authorAccountId == session.me, open: { jump(q.messageId) }) }
        x.reactions = reactionDisplays(m.reactions, session: session)
        x.tinted = m.mentionsMe
        x.flashed = flash == m.messageId
        x.replying = replying?.messageId == m.messageId
        x.edited = m.edited
        x.notSaved = session.editRefusals[m.messageId]
        x.expanded = expanded.contains(m.messageId)
        x.expand = { expanded.formSymmetricDifference([m.messageId]) }
        if let serverID = channel?.serverId, !serverID.isEmpty {
            x.openPerson = { openPerson(PersonInServer(accountID: $0, serverID: serverID)) }
        }
        guard member else { return x }
        let ref = ref(m)
        x.edit = changer.edit(ref)
        x.toggle = { react(m.messageId, $0) }
        x.addReaction = { sheet = .picker(messageID: m.messageId) }
        x.reply = { startReply(id: m.messageId, authorID: m.authorAccountId, body: ref.drawn, part: nil) }
        if ref.actions.contains(.edit) { x.editMessage = { changer.start(ref) } }
        if ref.actions.contains(.delete) { x.deleteMessage = { changes.deleting = ref } }
        x.actions = { sheet = .actions(ref) }
        return x
    }

    /// Up in an empty composer: this account's newest message, edited.
    private func editLast() {
        let editable = messages.map { ($0.messageId, $0.authorAccountId == session.me && $0.notice.isEmpty && !$0.deleted) }
        guard member, let id = MessageEdits.newestOwn(editable), let m = messages.first(where: { $0.messageId == id }) else { return }
        changer.start(ref(m))
    }

    /// Opens who wrote a message, in this server.
    private func openAuthor(_ accountID: String) -> (() -> Void)? {
        guard let serverID = channel?.serverId, !serverID.isEmpty, !accountID.isEmpty else { return nil }
        return { openPerson(PersonInServer(accountID: accountID, serverID: serverID)) }
    }

    /// "Reply", or "Quote reply" over words selected: the reply bar in the
    /// composer, its "@ On" on, and the keyboard.
    private func startReply(id: String, authorID: String, body: String, part: String?) {
        replying = Replies.quote(messageID: id, authorID: authorID, body: body, part: part)
        mentionOn = true
        composing = true
    }

    private func react(_ messageID: String, _ emoji: String) {
        Task {
            do { try await session.react(channelID: channelID, messageID: messageID, emoji: emoji) } catch { self.error = error.lumiaMessage }
        }
    }

    /// A quote's original, or a mention opened from a Mentions page:
    /// scrolled to and marked for a moment once it is here.
    private func jump(_ messageID: String) {
        Task {
            guard await session.reveal(channelID: channelID, messageID: messageID) else { return }
            // A beat for the rows read from further back to be laid out.
            try? await Task.sleep(for: .milliseconds(100))
            scroll = ScrollRequest(id: messageID, anchor: .center, n: (scroll?.n ?? 0) + 1)
            flash = messageID
            try? await Task.sleep(for: .seconds(Replies.flashSeconds))
            if flash == messageID { flash = nil }
        }
    }

    private func send() async {
        let body = draft.trimmingCharacters(in: .whitespacesAndNewlines)
        // Over the instance's limit the draft stays, as the counter says.
        guard !body.isEmpty, !MessageCounter.over(session.client.messageRoom(body)) else { return }
        // "@ On" turned off: the reply notifies nobody.
        var quote = replying
        if let q = quote { quote?.silent = Replies.offersMentionSwitch(quotedAuthor: q.authorAccountId, me: session.me, personal: false) && !mentionOn }
        draft = ""
        replying = nil
        error = ""
        do {
            let missed = try await session.send(channelID: channelID, body: body, quote: quote)
            outside = missed.isEmpty ? nil : OutsideNote(channelID: channelID, accountIDs: missed)
        } catch {
            self.error = error.lumiaMessage
        }
    }

    /// The bar's "Add to #name": those added get the channel's notice, and
    /// the bar goes; those who take adds as invitations are invited, which
    /// it says for a few seconds.
    private func addOutside() async {
        guard let note = outside else { return }
        outside?.adding = true
        outside?.error = ""
        do {
            let added = try await session.addOutside(channelID: note.channelID, accountIDs: note.accountIDs)
            if added.invitedAccountIds.isEmpty {
                outside = nil
                return
            }
            let said = Replies.invitedWords(names: added.invitedAccountIds.map { session.name(of: $0) }, channel: name)
            outside?.said = said
            outside?.adding = false
            try? await Task.sleep(for: .seconds(Replies.saidSeconds))
            if outside?.said == said { outside = nil }
        } catch {
            outside?.adding = false
            outside?.error = MembershipRefusal.add(code: error.lumiaCode, message: error.lumiaMessage)
        }
    }

    /// In from the bar: the composer comes back and the channel is in the sidebar.
    private func join() async {
        joining = true
        error = ""
        defer { joining = false }
        do { try await session.joinChannel(channelID: channelID) } catch { self.error = error.lumiaMessage }
    }
}
