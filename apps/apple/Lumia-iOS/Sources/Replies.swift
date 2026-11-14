// Quote replies, reactions, mentions, editing and deleting on the phone
// (architecture 6.6), the desktop shell's and the web's states and words in
// iOS's idiom: a long press on a message opens its actions as a sheet (the
// message's words, selectable, whose selection menu offers "Quote reply",
// then Add reaction, Reply, Edit message for its author, Copy text, and
// Delete message for its author or a moderator); Add reaction is the emoji
// picker as a sheet; and the Mentions page of a server or of Home. Message
// text in the conversation's list is not selectable: a long press there is
// the actions, as in the system's own Messages, and selecting part of a
// message is done in the sheet.
import LumiaFFI
import LumiaKit
import SwiftUI
import UIKit

/// What a long press, a chip's "Add reaction" or VoiceOver opened over a conversation.
enum MessageSheet: Identifiable, Equatable {
    case actions(MessageRef)
    case picker(messageID: String)

    var id: String {
        switch self {
        case let .actions(m): "actions-\(m.id)"
        case let .picker(id): "picker-\(id)"
        }
    }
}

/// A message as its actions need it.
struct MessageRef: Equatable {
    let id: String
    let authorID: String
    let authorName: String
    /// Its words as written: what an edit starts from.
    let body: String
    /// Its words as drawn, each mention by name: what is copied and quoted.
    let drawn: String
    /// What it offers, in the sheet's order.
    let actions: [MessageAction]
}

/// The sheet a conversation opens over a message: its actions, or the
/// picker. Edit message and Delete message are done once the sheet has gone
/// (`chose`), so the field can take the keyboard and the alert can show.
struct MessageSheetView: View {
    let sheet: MessageSheet
    let react: (String, String) -> Void
    let reply: (MessageRef, String?) -> Void
    let chose: (MessageRef, MessageAction) -> Void
    @State private var picking: String?
    @Environment(\.dismiss) private var dismiss

    var body: some View {
        Group {
            if let picking {
                EmojiPicker { emoji in react(picking, emoji); dismiss() }
            } else if case let .picker(id) = sheet {
                EmojiPicker { emoji in react(id, emoji); dismiss() }
            } else if case let .actions(message) = sheet {
                MessageActions(message: message) { action, part in
                    switch action {
                    case .addReaction: picking = message.id
                    case .reply: reply(message, part); dismiss()
                    case .copy: UIPasteboard.general.string = message.drawn; dismiss()
                    case .edit, .delete: chose(message, action); dismiss()
                    }
                }
            }
        }
        .presentationBackground(LumiaTokens.surface.color)
        .presentationDragIndicator(.visible)
    }
}

/// A message's actions: its words to select a part of to quote, then Add
/// reaction, Reply, Edit message, Copy text and Delete message, as offered.
struct MessageActions: View {
    let message: MessageRef
    /// An action chosen; Reply carries the words selected, nil for the whole.
    let run: (MessageAction, String?) -> Void

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: LumiaTokens.space16) {
                VStack(alignment: .leading, spacing: LumiaTokens.space4) {
                    Text(message.authorName).font(LumiaTokens.textAuthor.font).foregroundStyle(LumiaTokens.ink.color)
                    // Selecting words here offers "Quote reply" in the selection's menu.
                    QuotableText(text: message.drawn) { part in run(.reply, part) }
                }
                .padding(LumiaTokens.space12)
                .frame(maxWidth: .infinity, alignment: .leading)
                .background(LumiaTokens.inset.color, in: RoundedRectangle(cornerRadius: LumiaTokens.radius12))
                VStack(spacing: 0) {
                    ForEach(Array(message.actions.enumerated()), id: \.element) { n, action in
                        if n > 0 { Divider().overlay(LumiaTokens.line.color) }
                        row(action)
                    }
                }
                .background(LumiaTokens.tile.color, in: RoundedRectangle(cornerRadius: LumiaTokens.radius12))
                .overlay(RoundedRectangle(cornerRadius: LumiaTokens.radius12).stroke(LumiaTokens.line.color))
            }
            .padding(LumiaTokens.space16)
            .padding(.top, LumiaTokens.space8)
        }
        .presentationDetents([.medium, .large])
    }

    static func icon(_ action: MessageAction) -> String {
        switch action {
        case .addReaction: "face.smiling"
        case .reply: "arrow.uturn.left"
        case .edit: "pencil"
        case .copy: "doc.on.doc"
        case .delete: "trash"
        }
    }

    /// One action; Delete message in the danger ink, as the system draws a destructive item.
    private func row(_ action: MessageAction) -> some View {
        let danger = action == .delete
        return Button { run(action, nil) } label: {
            HStack(spacing: LumiaTokens.space12) {
                Image(systemName: Self.icon(action)).frame(width: LumiaTokens.space24)
                    .foregroundStyle(danger ? LumiaTokens.dnd.color : LumiaTokens.iconStrong.color)
                Text(action.words).font(LumiaTokens.textNav.font)
                    .foregroundStyle(danger ? LumiaTokens.dnd.color : LumiaTokens.ink.color)
                Spacer(minLength: 0)
            }
            .padding(.horizontal, LumiaTokens.space14)
            .frame(height: LumiaTokens.controlHeight)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .accessibilityIdentifier("action-\(action.rawValue)")
    }
}

/// A message's words, selectable, with "Quote reply" first in the menu a
/// selection opens (`UITextView`'s edit menu, iOS 16 and later).
struct QuotableText: UIViewRepresentable {
    let text: String
    let quote: (String) -> Void

    func makeUIView(context: Context) -> UITextView {
        let view = UITextView()
        view.isEditable = false
        view.isSelectable = true
        view.isScrollEnabled = false
        view.backgroundColor = .clear
        view.textContainerInset = .zero
        view.textContainer.lineFragmentPadding = 0
        view.delegate = context.coordinator
        view.setContentCompressionResistancePriority(.defaultLow, for: .horizontal)
        view.accessibilityHint = "Select words to quote just those"
        return view
    }

    func updateUIView(_ view: UITextView, context: Context) {
        context.coordinator.quote = quote
        // Set again only when the words change: setting them clears a
        // selection, and the sheet is drawn again while one is being made.
        guard context.coordinator.shown != text else { return }
        context.coordinator.shown = text
        let style = NSMutableParagraphStyle()
        style.lineSpacing = MessageMeasures.leading
        view.attributedText = NSAttributedString(string: text, attributes: [
            .font: UIFont.systemFont(ofSize: LumiaTokens.textMessage.size),
            .foregroundColor: UIColor(LumiaTokens.inkBody.color),
            .paragraphStyle: style,
        ])
    }

    func sizeThatFits(_ proposal: ProposedViewSize, uiView: UITextView, context: Context) -> CGSize? {
        let width = proposal.width ?? UIScreen.main.bounds.width
        let fits = uiView.sizeThatFits(CGSize(width: width, height: .greatestFiniteMagnitude))
        return CGSize(width: width, height: fits.height)
    }

    func makeCoordinator() -> Coordinator { Coordinator(quote: quote) }

    final class Coordinator: NSObject, UITextViewDelegate {
        var quote: (String) -> Void
        /// The words the view holds.
        var shown: String?
        init(quote: @escaping (String) -> Void) { self.quote = quote }

        func textView(_ textView: UITextView, editMenuForTextIn range: NSRange, suggestedActions: [UIMenuElement]) -> UIMenu? {
            guard range.length > 0, let words = (textView.text as NSString?)?.substring(with: range),
                  !words.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty else { return nil }
            let quoteReply = UIAction(title: "Quote reply", image: UIImage(systemName: "arrowshape.turn.up.left")) { [weak self] _ in
                self?.quote(words)
            }
            return UIMenu(children: [quoteReply] + suggestedActions)
        }
    }
}

/// The emoji picker (architecture 6.6) as a sheet: a search field,
/// "Frequently used" from this phone's own reactions, and every emoji by
/// Unicode's groups (`EmojiCatalog`, up to Unicode 16, default skin tone).
struct EmojiPicker: View {
    let pick: (String) -> Void
    @State private var search = ""

    private let columns = [GridItem(.adaptive(minimum: 44), spacing: LumiaTokens.space2)]

    var body: some View {
        VStack(spacing: 0) {
            HStack(spacing: LumiaTokens.space8) {
                Image(systemName: "magnifyingglass").foregroundStyle(LumiaTokens.inkQuiet.color)
                TextField("Search emoji", text: $search)
                    .textFieldStyle(.plain)
                    .font(LumiaTokens.textBody.font)
                    .foregroundStyle(LumiaTokens.inkField.color)
                    .textInputAutocapitalization(.never).autocorrectionDisabled()
            }
            .padding(.horizontal, LumiaTokens.space12)
            .frame(height: LumiaTokens.controlHeight)
            .background(LumiaTokens.field.color, in: RoundedRectangle(cornerRadius: LumiaTokens.radius10))
            .overlay(RoundedRectangle(cornerRadius: LumiaTokens.radius10).stroke(LumiaTokens.borderComposer.color))
            .padding(LumiaTokens.space16)
            ScrollView {
                LazyVStack(alignment: .leading, spacing: 0) {
                    let wanted = search.trimmingCharacters(in: .whitespaces)
                    if wanted.isEmpty {
                        heading("Frequently used")
                        grid(FrequentEmoji.list(recent: FrequentEmoji.recent).map { EmojiEntry(emoji: $0, name: EmojiCatalog.name(of: $0), keywords: []) }, key: "f")
                        ForEach(EmojiCatalog.groups, id: \.name) { group in
                            heading(group.name)
                            grid(group.emoji, key: group.name)
                        }
                    } else {
                        let found = EmojiCatalog.search(wanted)
                        if found.isEmpty {
                            Text("No emoji match that.").font(LumiaTokens.textMeta.font).foregroundStyle(LumiaTokens.inkQuiet.color).padding(LumiaTokens.space10)
                        } else {
                            grid(found, key: "s")
                        }
                    }
                }
                .padding(.horizontal, LumiaTokens.space12)
                .padding(.bottom, LumiaTokens.space16)
            }
            .scrollDismissesKeyboard(.interactively)
        }
        .presentationDetents([.medium, .large])
        .accessibilityElement(children: .contain)
        .accessibilityLabel("Add reaction")
    }

    private func heading(_ words: String) -> some View {
        Text(words).font(LumiaTokens.textSection.font).foregroundStyle(LumiaTokens.inkMuted.color)
            .padding(.horizontal, LumiaTokens.space4).padding(.top, LumiaTokens.space8).padding(.bottom, LumiaTokens.space4)
            .accessibilityAddTraits(.isHeader)
    }

    private func grid(_ emoji: [EmojiEntry], key: String) -> some View {
        LazyVGrid(columns: columns, spacing: LumiaTokens.space2) {
            ForEach(emoji) { e in
                Button { pick(e.emoji) } label: {
                    Text(e.emoji).font(.system(size: 28)).frame(width: 44, height: 44).contentShape(Rectangle())
                }
                .buttonStyle(.plain)
                .accessibilityLabel(e.name)
            }
        }
        .id(key)
    }
}

/// A person's name as a message draws it (a mention, who reacted): empty
/// for someone not known, where the session would give their id's start.
@MainActor
func knownName(_ id: String, session: LumiaSession) -> String {
    let n = session.name(of: id)
    return n == String(id.prefix(8)) ? "" : n
}

/// How a conversation draws a message's reactions: who, as the chip says it.
@MainActor
func reactionDisplays(_ reactions: [ReactionSummary], session: LumiaSession) -> [ReactionDisplay] {
    reactions.map { r in
        ReactionDisplay(emoji: r.reaction, count: r.count, me: r.me, words: Replies.reactedWords(r, me: session.me) { knownName($0, session: session) })
    }
}

/// The bar after a send naming those mentioned who are not in the channel:
/// who, and "Add to #name"; after an add that invited some, who was invited,
/// for a few seconds.
struct OutsideNote: Equatable {
    var channelID: String
    var accountIDs: [String]
    var adding = false
    var said: String?
    var error = ""
}

// MARK: Mentions

/// Mentions (architecture 6.6): the messages that notify this account (they
/// mention it or everyone, or quote it), newest first, of one server or,
/// from Home, of every server. Each says where it was said over the message
/// as a conversation draws it; a tap opens its channel at it. Older ones are
/// read as the list nears its end.
struct MentionsScreen: View {
    let session: LumiaSession
    /// nil: every server's, from Home.
    let serverID: String?
    let open: (MessageSummary) -> Void

    private var page: MentionsList? { session.mentionsPages[serverID ?? ""] }
    private var server: ServerSummary? { session.snapshot?.servers.first { $0.serverId == serverID } }

    var body: some View {
        PanelScreen(back: server?.name ?? "Home", title: "Mentions", subtitle: serverID == nil ? "Messages in every server that mention you or reply to you" : "Messages here that mention you or reply to you") {
            ScrollView {
                LazyVStack(alignment: .leading, spacing: LumiaTokens.space8) {
                    if let error = page?.error { LumiaErrorText(error) }
                    let messages = page?.messages ?? []
                    if messages.isEmpty {
                        if page == nil || page?.loading == true {
                            ProgressView().frame(maxWidth: .infinity).padding(.vertical, LumiaTokens.space40)
                        } else {
                            LumiaEmptyState(icon: "at", title: Replies.noMentions) { Text(Replies.noMentionsLine) }
                        }
                    }
                    ForEach(Array(messages.enumerated()), id: \.element.messageId) { n, m in
                        item(m)
                            .onAppear {
                                if n + 5 >= messages.count { Task { await session.loadMentions(serverID: serverID, more: true) } }
                            }
                    }
                }
                .padding(LumiaTokens.space16)
            }
        }
        .task { await session.loadMentions(serverID: serverID, more: false) }
    }

    private func place(_ m: MessageSummary) -> String {
        guard let channel = session.channel(m.channelId) else { return "" }
        let server = serverID == nil ? session.snapshot?.servers.first { $0.serverId == channel.serverId }?.name : nil
        return Replies.mentionPlace(channel: channel.name, server: server)
    }

    private func item(_ m: MessageSummary) -> some View {
        let author = session.name(of: m.authorAccountId)
        let at = Date(timeIntervalSince1970: Double(m.createdAtMicros) / 1_000_000).formatted(.dateTime.month(.abbreviated).day().hour().minute())
        let where_ = place(m)
        return Button { open(m) } label: {
            VStack(alignment: .leading, spacing: LumiaTokens.space8) {
                Text(where_).font(LumiaTokens.textSection.font).foregroundStyle(LumiaTokens.inkMuted.color)
                // The reply's reference line, as the conversation draws it; the item itself is the jump.
                if let q = m.reply {
                    LumiaReplyReference(quote: QuoteDisplay(authorID: q.authorAccountId, name: q.authorAccountId.isEmpty ? nil : session.name(of: q.authorAccountId), picture: session.picture(of: q.authorAccountId), text: q.text, deleted: q.originalDeleted, quotesMe: q.authorAccountId == session.me, open: nil))
                        .padding(.leading, LumiaTokens.avatarMd / 2 - 1)
                        .allowsHitTesting(false)
                }
                HStack(alignment: .top, spacing: LumiaTokens.space12) {
                    LumiaAvatar(initials: initials(of: author), tone: .of(m.authorAccountId), size: LumiaTokens.avatarMd, picture: session.picture(of: m.authorAccountId))
                    VStack(alignment: .leading, spacing: LumiaTokens.space4) {
                        HStack(alignment: .firstTextBaseline, spacing: LumiaTokens.space8) {
                            Text(author).font(LumiaTokens.textAuthor.font).foregroundStyle(LumiaTokens.ink.color)
                            Text(at).font(LumiaTokens.textMeta.font).foregroundStyle(LumiaTokens.inkMuted.color)
                        }
                        MessageBodyText(text: m.body, mentions: m.mentions, edited: m.edited, name: { knownName($0, session: session) })
                            .frame(maxWidth: .infinity, alignment: .leading)
                            .multilineTextAlignment(.leading)
                    }
                }
            }
            .padding(LumiaTokens.space12)
            .frame(maxWidth: .infinity, alignment: .leading)
            .background(LumiaTokens.inset.color, in: RoundedRectangle(cornerRadius: LumiaTokens.radius12))
            .contentShape(RoundedRectangle(cornerRadius: LumiaTokens.radius12))
        }
        .buttonStyle(.plain)
        .accessibilityElement(children: .ignore)
        .accessibilityLabel("\(author) in \(where_), \(at): \(MentionSpans.drawn(body: m.body, mentions: m.mentions) { knownName($0, session: session) })")
        .accessibilityAddTraits(.isButton)
    }
}

/// Home's Mentions row (no board has a place for it): the `at` glyph, the word, the count.
struct MentionsRow: View {
    let count: UInt32
    let open: () -> Void

    var body: some View {
        Button(action: open) {
            HStack(spacing: LumiaTokens.space8) {
                Image(systemName: "at").font(.system(size: 13, weight: .semibold))
                    .foregroundStyle(LumiaTokens.inkMuted.color)
                    .frame(width: LumiaTokens.space12)
                Text("Mentions").font(LumiaTokens.textNav.font)
                    .fontWeight(count > 0 ? .semibold : nil)
                    .foregroundStyle(count > 0 ? LumiaTokens.inkEmphasis.color : LumiaTokens.ink.color)
                Spacer(minLength: 0)
                if count > 0 { LumiaCountBadge(count) }
            }
            .padding(.horizontal, LumiaTokens.space10)
            .frame(height: LumiaTokens.controlHeight)
            .contentShape(RoundedRectangle(cornerRadius: LumiaTokens.radius10))
        }
        .buttonStyle(.plain)
        .accessibilityLabel("Mentions")
        .accessibilityValue(Replies.unreadWords(unread: count, mentions: count))
        .accessibilityIdentifier("mentions-row")
    }
}
