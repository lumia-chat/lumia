// Quote replies, reactions, mentions and read state (architecture 6.6):
// quotes, the reference line's and the reply bar's words, a message's
// actions, editing and deleting, the chips' and the bar's words, the @
// list, a body's mentions and how they are drawn, the counts, marking read,
// the "New messages" line, which notifications go, and the emoji picker's
// list.
@testable import LumiaKit
import LumiaFFI
import XCTest

final class RepliesTests: XCTestCase {
    private func member(_ id: String, name: String, handle: String? = nil, nickname: String = "") -> MemberSummary {
        MemberSummary(accountId: id, handle: handle ?? id, displayName: name, nickname: nickname, pronouns: "", bio: "", avatarUrl: "", channelAddsByInvitation: false, serverDmsOff: false)
    }

    private func mention(_ id: String, _ start: UInt32, _ end: UInt32, everyone: Bool = false) -> MentionSummary {
        MentionSummary(accountId: id, everyone: everyone, start: start, end: end)
    }

    private func state(_ id: String, read: UInt64 = 0, unread: UInt32 = 0, mentions: UInt32 = 0) -> ReadStateSummary {
        ReadStateSummary(channelId: id, readSeq: read, unread: unread, mentions: mentions)
    }

    private func channel(_ id: String, kind: String = "TEXT") -> ChannelSummary {
        ChannelSummary(channelId: id, serverId: kind == "PERSONAL_DM" ? "" : "s", name: id, kind: kind, lastSeq: 0, participantAccountIds: [], host: "", private: false, isDefault: false, memberCount: 0, topic: "")
    }

    func testAQuoteIsTheWholeMessageOrThePartChosenTrimmedAndKeptShort() {
        let whole = Replies.quote(messageID: "m1", authorID: "ada", body: "  Shipped on Friday.\n")
        XCTAssertEqual(whole.messageId, "m1")
        XCTAssertEqual(whole.authorAccountId, "ada")
        XCTAssertEqual(whole.text, "Shipped on Friday.")
        XCTAssertFalse(whole.originalDeleted)
        XCTAssertEqual(Replies.quote(messageID: "m1", authorID: "ada", body: "Shipped on Friday.", part: " Shipped ").text, "Shipped")
        XCTAssertEqual(Replies.quote(messageID: "m", authorID: "a", body: String(repeating: "é", count: 2_500)).text.count, 2_000)
        XCTAssertFalse(whole.silent, "a reply notifies whom it quotes unless the switch is off")
        XCTAssertTrue(Replies.quote(messageID: "m1", authorID: "ada", body: "Shipped.", silent: true).silent)
        XCTAssertEqual(Replies.replyingTo("Ada"), "Replying to Ada")
    }

    func testTheReferenceLineAndTheReplyBarsWords() {
        XCTAssertEqual(Replies.referenceLabel(name: "Maya Chen", quotesMe: false, text: "Morning, Acme"), "Replying to Maya Chen: Morning, Acme. Jump to message")
        XCTAssertEqual(Replies.referenceLabel(name: "Maya Chen", quotesMe: false, text: "Closes next Friday."), "Replying to Maya Chen: Closes next Friday. Jump to message", "no second full stop")
        XCTAssertEqual(Replies.referenceLabel(name: "Sam Rivera", quotesMe: true, text: "Can we get the link?"), "Replying to you: Can we get the link? Jump to message")
        XCTAssertEqual(Replies.referenceLabel(name: nil, quotesMe: false, text: "hi"), "Replying to a message: hi. Jump to message")
        XCTAssertEqual(Replies.referenceName("Maya Chen", quotesMe: false), "Maya Chen")
        XCTAssertEqual(Replies.referenceName("Sam Rivera", quotesMe: true), "@Sam Rivera")
        XCTAssertEqual(Replies.originalDeleted, "Original message was deleted")
        // The "@ On" switch: for a reply to someone else in a server's conversation.
        XCTAssertTrue(Replies.offersMentionSwitch(quotedAuthor: "ada", me: "me", personal: false))
        XCTAssertFalse(Replies.offersMentionSwitch(quotedAuthor: "me", me: "me", personal: false), "not for a reply to oneself")
        XCTAssertFalse(Replies.offersMentionSwitch(quotedAuthor: "ada", me: "me", personal: true), "not in a personal conversation")
        XCTAssertFalse(Replies.offersMentionSwitch(quotedAuthor: "", me: "me", personal: false))
        XCTAssertEqual(Replies.mentionSwitchLabel("Jonah Patel"), "Mention Jonah Patel")
        XCTAssertEqual(Replies.mentionSwitchWords(true), "On")
        XCTAssertEqual(Replies.mentionSwitchWords(false), "Off")
    }

    func testAMessagesActionsAreItsAuthorsAndAModeratorsDelete() {
        let words = { (a: [MessageAction]) in a.map(\.words) }
        XCTAssertEqual(words(MessageAction.offered(mine: true, personal: false, canManageMessages: false)), ["Add reaction", "Reply", "Edit message", "Copy text", "Delete message"])
        XCTAssertEqual(words(MessageAction.offered(mine: false, personal: false, canManageMessages: false)), ["Add reaction", "Reply", "Copy text"])
        XCTAssertEqual(words(MessageAction.offered(mine: false, personal: false, canManageMessages: true)), ["Add reaction", "Reply", "Copy text", "Delete message"], "a moderator deletes, and edits nobody's")
        XCTAssertEqual(words(MessageAction.offered(mine: false, personal: true, canManageMessages: true)), ["Add reaction", "Reply", "Copy text"], "nobody else's in a personal conversation")
        XCTAssertEqual(words(MessageAction.offered(mine: true, personal: true, canManageMessages: false)), ["Add reaction", "Reply", "Edit message", "Copy text", "Delete message"])
    }

    func testSavingAnEditSendsWhatChangedAndAsksToDeleteWhatIsEmptied() {
        XCTAssertEqual(MessageEdits.outcome(original: "Shipped.", draft: "  Shipped on Friday.\n"), .save("Shipped on Friday."))
        XCTAssertEqual(MessageEdits.outcome(original: "Shipped.", draft: "Shipped. "), .unchanged)
        XCTAssertEqual(MessageEdits.outcome(original: "Shipped.", draft: " \n "), .delete)
        XCTAssertEqual(MessageEdits.edited, "(edited)")
        XCTAssertEqual(MessageEdits.editedLabel, "edited")
        XCTAssertEqual(MessageEdits.deleteTitle, "Delete this message?")
        XCTAssertEqual(MessageEdits.notSaved("You can only edit your own messages."), "Not saved: You can only edit your own messages.")
        XCTAssertEqual(MessageEdits.firstLine("\n  Shipped on Friday.  \nAnd more."), "Shipped on Friday.")
        XCTAssertEqual(MessageEdits.firstLine(""), "")
        // Return on a phone's keyboard is a line break typed: one character more.
        XCTAssertTrue(MessageEdits.returnTyped(old: "Shipped", new: "Shipped\n"))
        XCTAssertTrue(MessageEdits.returnTyped(old: "a\nb", new: "a\n\nb"))
        XCTAssertFalse(MessageEdits.returnTyped(old: "Shipped", new: "Shipped!"))
        XCTAssertFalse(MessageEdits.returnTyped(old: "Shipped", new: "Shipped\nand more"), "a paste is not Return")
        // Up in an empty composer: the newest that can be edited.
        XCTAssertEqual(MessageEdits.newestOwn([("a", true), ("b", false), ("c", true), ("d", false)]), "c")
        XCTAssertNil(MessageEdits.newestOwn([("a", false)]))
    }

    func testAQuotedMessageOpensABlockOfItsOwnThatTheNextMayJoin() {
        let at: Int64 = 1_700_000_000_000_000
        let places = MessageBlocks.places([
            MessageStamp(authorID: "ada", atMicros: at),
            MessageStamp(authorID: "ada", atMicros: at + 1_000_000, quotes: true),
            MessageStamp(authorID: "ada", atMicros: at + 2_000_000),
        ])
        XCTAssertEqual(places.map(\.opens), [true, true, false])
        XCTAssertEqual(places.map(\.closes), [true, false, true])
    }

    func testWhoReactedReadsAsTheDesktopsDoes() {
        let names = ["ada": "Ada", "bo": "Bo", "cy": "Cy"]
        let words = { (r: ReactionSummary) in Replies.reactedWords(r, me: "me", name: { names[$0] ?? "" }) }
        XCTAssertEqual(words(ReactionSummary(reaction: "👍", count: 2, me: true, accountIds: ["me", "ada"])), "You and Ada reacted with 👍")
        XCTAssertEqual(words(ReactionSummary(reaction: "👍", count: 1, me: false, accountIds: ["ada"])), "Ada reacted with 👍")
        XCTAssertEqual(words(ReactionSummary(reaction: "🎉", count: 5, me: false, accountIds: ["ada", "bo"])), "Ada, Bo and 3 others reacted with 🎉")
        XCTAssertEqual(words(ReactionSummary(reaction: "🎉", count: 3, me: false, accountIds: ["ada", "bo", "cy"])), "Ada, Bo and 1 other reacted with 🎉")
        XCTAssertEqual(words(ReactionSummary(reaction: "🎉", count: 2, me: false, accountIds: ["ada"])), "Ada and 1 other reacted with 🎉")
        XCTAssertEqual(words(ReactionSummary(reaction: "😮", count: 1, me: false, accountIds: ["zed"])), "Someone reacted with 😮")
        XCTAssertEqual(words(ReactionSummary(reaction: "😮", count: 4, me: false, accountIds: [])), "4 people reacted with 😮")
    }

    func testThoseMentionedOutsideTheChannelAndTheBarsWords() {
        let mentions = [mention("ada", 0, 4), mention("bo", 5, 8), mention("me", 9, 12), mention("", 13, 22, everyone: true), mention("bo", 23, 26), mention("cy", 27, 30)]
        XCTAssertEqual(Replies.outside(mentions, inside: ["ada"], me: "me"), ["bo", "cy"])
        XCTAssertEqual(Replies.outside(mentions, inside: nil, me: "me"), [], "nobody when who is in the channel is not known")
        XCTAssertEqual(Replies.outsideWords(names: ["Ada"], channel: "design"), "Ada isn’t in #design, so they won’t be notified.")
        XCTAssertEqual(Replies.outsideWords(names: ["Ada", "Bo"], channel: "design"), "Ada and Bo aren’t in #design, so they won’t be notified.")
        XCTAssertEqual(Replies.outsideWords(names: ["Ada", "Bo", "Cy"], channel: "design"), "Ada, Bo and 1 other aren’t in #design, so they won’t be notified.")
        XCTAssertEqual(Replies.outsideWords(names: ["Ada", "Bo", "Cy", "Di"], channel: "design"), "Ada, Bo and 2 others aren’t in #design, so they won’t be notified.")
        XCTAssertEqual(Replies.invitedWords(names: ["Ada"], channel: "design"), "Invited Ada to #design.")
        XCTAssertEqual(Replies.invitedWords(names: ["Ada", "Bo"], channel: "design"), "Invited 2 people to #design.")
    }

    func testUnreadLabelsAndPlaces() {
        XCTAssertEqual(Replies.unreadLabel("design channel", unread: 0, mentions: 0), "design channel")
        XCTAssertEqual(Replies.unreadLabel("design channel", unread: 3, mentions: 0), "design channel, unread")
        XCTAssertEqual(Replies.unreadLabel("Mentions", unread: 1, mentions: 1), "Mentions, 1 mention")
        XCTAssertEqual(Replies.unreadLabel("Mentions", unread: 2, mentions: 2), "Mentions, 2 mentions")
        XCTAssertEqual(Replies.unreadWords(unread: 0, mentions: 0), "")
        XCTAssertEqual(Replies.unreadWords(unread: 4, mentions: 0), "unread")
        XCTAssertEqual(Replies.unreadWords(unread: 4, mentions: 3), "3 mentions")
        XCTAssertEqual(Replies.mentionPlace(channel: "design", server: nil), "#design")
        XCTAssertEqual(Replies.mentionPlace(channel: "design", server: "Acme"), "Acme · #design")
    }

    func testTheAtWordIsTheOneAtTheDraftsEnd() {
        XCTAssertEqual(MentionQuery.at("hi @"), MentionQuery(before: "hi ", query: ""))
        XCTAssertEqual(MentionQuery.at("@gr"), MentionQuery(before: "", query: "gr"))
        XCTAssertEqual(MentionQuery.at("hi @grace.h-o_p"), MentionQuery(before: "hi ", query: "grace.h-o_p"))
        XCTAssertNil(MentionQuery.at("mail me@example"), "an @ inside a word")
        XCTAssertNil(MentionQuery.at("hi @grace "), "a word that ended")
        XCTAssertNil(MentionQuery.at("hi"))
        XCTAssertEqual(MentionQuery.at("hi @gr")?.picking("grace"), "hi @grace ")
    }

    func testTheAtListPutsEveryoneThenThoseInTheChannelThenTheRest() {
        let members = [member("me", name: "Me"), member("bo", name: "Bo Diddley"), member("grace", name: "Grace Hopper"), member("gus", name: "Augustus", handle: "gus"), member("ada", name: "Ada Lovelace", nickname: "Countess")]
        let name = { (m: MemberSummary) in m.displayName }
        let all = MentionList.choices(query: "", members: members, inside: ["grace", "ada"], me: "me", everyone: true, name: name)
        XCTAssertEqual(all, [.everyone, .member(members[4], inside: true), .member(members[2], inside: true), .member(members[3], inside: false), .member(members[1], inside: false)])
        XCTAssertEqual(MentionList.choices(query: "G", members: members, inside: nil, me: "me", everyone: true, name: name), [.member(members[3], inside: true), .member(members[2], inside: true)], "by handle or name, every one in the channel while that is not known")
        XCTAssertEqual(MentionList.choices(query: "count", members: members, inside: nil, me: "me", everyone: false, name: name), [.member(members[4], inside: true)], "by nickname")
        XCTAssertEqual(MentionList.choices(query: "ev", members: members, inside: nil, me: "me", everyone: false, name: name), [], "no everyone for one who may not")
        XCTAssertEqual(MentionList.choices(query: "ev", members: members, inside: nil, me: "me", everyone: true, name: name), [.everyone])
        let many = (0..<12).map { member("p\($0)", name: "P \($0)") }
        XCTAssertEqual(MentionList.choices(query: "", members: many, inside: nil, me: "me", everyone: true, name: name).count, 8)
        XCTAssertEqual(MentionList.everyoneNote(channel: "design"), "Notify everyone in #design")
        XCTAssertEqual(MentionList.notIn(channel: "design"), "Not in #design")
    }

    func testABodyIsCutAtItsMentionsByteRanges() {
        // "é" is two bytes: the ranges are UTF-8's.
        let body = "é @ada and @everyone!"
        let segments = MentionSpans.segments(body: body, mentions: [mention("", 12, 21, everyone: true), mention("ada", 3, 7)])
        XCTAssertEqual(segments.map(\.text), ["é ", "@ada", " and ", "@everyone", "!"])
        XCTAssertEqual(segments.map { $0.mention?.accountId }, [nil, "ada", nil, "", nil])
        XCTAssertEqual(MentionSpans.segments(body: "hi", mentions: []).map(\.text), ["hi"])
        // Out of the body, overlapping, or inside a character: plain words.
        XCTAssertEqual(MentionSpans.segments(body: "hi @ada", mentions: [mention("x", 3, 40)]).map(\.text), ["hi @ada"])
        XCTAssertEqual(MentionSpans.segments(body: "é", mentions: [mention("x", 1, 2)]).map(\.text), ["é"])
        XCTAssertEqual(MentionSpans.segments(body: "@ada", mentions: [mention("a", 0, 4), mention("b", 1, 3)]).map(\.text), ["@ada"])
    }

    func testAnEditsRefusalIsMatchedByItsCommand() {
        var sent = EditsSent()
        sent.sent(command: "c1", message: "m1")
        sent.sent(command: "c2", message: "m2")
        XCTAssertNil(sent.refused(command: "c9"), "another command's refusal is not an edit's")
        XCTAssertEqual(sent.refused(command: "c2"), "m2")
        XCTAssertNil(sent.refused(command: "c2"), "each is answered once")
        XCTAssertEqual(sent.refused(command: "c1"), "m1")
    }

    func testAMentionIsDrawnAsThePersonsNameAndCopiedSo() {
        let names = ["ada": "Ada Lovelace"]
        let name = { (id: String) in names[id] ?? "" }
        let body = "é @ada and @everyone, @zed!"
        let mentions = [mention("ada", 3, 7), mention("", 12, 21, everyone: true), mention("zed", 23, 27)]
        XCTAssertEqual(MentionSpans.drawn(body: body, mentions: mentions, name: name), "é @Ada Lovelace and @everyone, @zed!", "someone not known stays as written")
        XCTAssertEqual(MentionSpans.drawn(body: "no one", mentions: [], name: name), "no one")
    }

    func testCountsByConversationServerAndBadge() {
        let reads = ReadCounts([state("general", unread: 3), state("design", unread: 2, mentions: 2), state("dm", unread: 4, mentions: 4)])
        XCTAssertEqual(reads.unread("design").mentions, 2)
        XCTAssertEqual(reads.unread("nowhere").unread, 0)
        XCTAssertEqual(reads.any([channel("general")]).unread, true)
        XCTAssertEqual(reads.any([channel("general")]).mentions, 0)
        XCTAssertEqual(reads.any([channel("general"), channel("design")]).mentions, 2)
        XCTAssertEqual(reads.any([channel("quiet")]).unread, false)
        XCTAssertEqual(reads.badge, 6)
        XCTAssertEqual(reads.serverMentions(personal: [channel("dm", kind: "PERSONAL_DM")]), 2)
    }

    func testMarkingReadMovesSomethingAtMostOnceASecond() {
        var marks = ReadMarks()
        let now = Date()
        XCTAssertFalse(marks.claim("c", seq: 0, state: nil, now: now), "nothing read yet")
        XCTAssertFalse(marks.claim("c", seq: 5, state: state("c", read: 5), now: now), "already read through it")
        XCTAssertTrue(marks.claim("c", seq: 6, state: state("c", read: 5, unread: 1), now: now))
        XCTAssertFalse(marks.claim("c", seq: 6, state: state("c", read: 5, unread: 1), now: now.addingTimeInterval(0.5)), "on its way")
        XCTAssertTrue(marks.claim("c", seq: 6, state: state("c", read: 5, unread: 1), now: now.addingTimeInterval(1.5)), "a second on, still unread")
        XCTAssertTrue(marks.claim("c", seq: 7, state: state("c", read: 6), now: now.addingTimeInterval(1.6)), "a newer message")
        XCTAssertTrue(marks.claim("d", seq: 2, state: state("d", read: 2, unread: 1), now: now), "a count to clear")
    }

    func testTheNewMessagesLineGoesAboveTheFirstUnreadSomeoneElseSent() {
        let entries = [ReadEntry(id: "a", seq: 1, counts: true), ReadEntry(id: "b", seq: 2, counts: false), ReadEntry(id: "c", seq: 3, counts: true)]
        XCTAssertEqual(NewMessagesLine.at(entries, readTo: 1), "c", "not this account's own")
        XCTAssertEqual(NewMessagesLine.at(entries, readTo: 0), "a")
        XCTAssertNil(NewMessagesLine.at(entries, readTo: 3))
        XCTAssertEqual(NewMessagesLine.since(state("c", read: 4, unread: 2)), 4)
        XCTAssertNil(NewMessagesLine.since(state("c", read: 4)), "nothing unread, no line")
        XCTAssertNil(NewMessagesLine.since(nil))
        XCTAssertEqual(NewMessagesLine.drawn, "New")
        XCTAssertEqual(NewMessagesLine.words, "New messages", "what assistive technology hears")
    }

    func testAConversationsNotificationsGoOnceItIsRead() {
        let before = ReadCounts([state("a", read: 3, unread: 2), state("b", read: 5), state("c", read: 1, unread: 1), state("d", read: 2)])
        let after = ReadCounts([state("a", read: 5), state("b", read: 5), state("c", read: 2, unread: 1), state("d", read: 4), state("e", read: 9)])
        XCTAssertEqual(ReadNotifications.newlyRead(before: before, after: after), ["a", "d"], "read, or read on; not one whose counts stood still, nor one never counted before")

        let delivered: [(id: String, channelID: String?)] = [("1", "a"), ("2", "c"), ("3", nil), ("4", "gone")]
        let states = ReadCounts([state("a", read: 5), state("c", read: 1, unread: 1)])
        XCTAssertEqual(ReadNotifications.toRemove(delivered: delivered, states: states, badge: 1), ["1", "4"])
        XCTAssertEqual(ReadNotifications.toRemove(delivered: delivered, states: nil, badge: 0), ["1", "2", "3", "4"], "at 0 nothing that notifies is unread")
        XCTAssertEqual(ReadNotifications.toRemove(delivered: delivered, states: nil, badge: 2), [])
    }

    func testThePickersEmojiAreUnicodesUpTo16ByGroup() {
        XCTAssertEqual(EmojiCatalog.groups.map(\.name), ["Smileys & emotion", "People & body", "Animals & nature", "Food & drink", "Travel & places", "Activities", "Objects", "Symbols", "Flags"])
        XCTAssertTrue(EmojiCatalog.groups.allSatisfy { !$0.emoji.isEmpty })
        XCTAssertGreaterThan(EmojiCatalog.all.count, 1_800)
        XCTAssertEqual(EmojiCatalog.groups[0].emoji.first?.emoji, "😀")
        for quick in Replies.quick { XCTAssertTrue(EmojiCatalog.all.contains { $0.emoji == quick }, "\(quick) is in the list") }
        XCTAssertFalse(EmojiCatalog.all.contains { $0.emoji == "👋🏻" }, "the default skin tone only")
        XCTAssertTrue(EmojiCatalog.all.contains { $0.emoji == "🫩" }, "Unicode 16's face with bags under eyes")
        XCTAssertEqual(Set(EmojiCatalog.all.map(\.emoji)).count, EmojiCatalog.all.count)
        XCTAssertEqual(EmojiCatalog.search("party popper").first?.emoji, "🎉")
        XCTAssertEqual(EmojiCatalog.search("tada").first?.emoji, "🎉", "by shortcode")
        XCTAssertTrue(EmojiCatalog.search("   ").isEmpty)
        XCTAssertLessThanOrEqual(EmojiCatalog.search("a").count, 64)
        XCTAssertEqual(EmojiCatalog.name(of: "👍"), "thumbs up")
    }

    func testFrequentlyUsedStartsWithWhatThisPhoneChose() {
        XCTAssertEqual(FrequentEmoji.list(recent: []), Replies.quick)
        // The emoji a system draws: Unicode 16's from iOS 18.4, 15.1's from 17.4.
        let ios = { (major: Int, minor: Int) in OperatingSystemVersion(majorVersion: major, minorVersion: minor, patchVersion: 0) }
        XCTAssertTrue(EmojiCatalog.newest(drawnBy: ios(18, 4), macOS: false) == (16, 0))
        XCTAssertTrue(EmojiCatalog.newest(drawnBy: ios(18, 3), macOS: false) == (15, 1))
        XCTAssertTrue(EmojiCatalog.newest(drawnBy: ios(17, 0), macOS: false) == (15, 0))
        XCTAssertTrue(EmojiCatalog.newest(drawnBy: ios(15, 4), macOS: true) == (16, 0))
        XCTAssertEqual(FrequentEmoji.list(recent: ["🦀", "👍"]), ["🦀", "👍", "❤️", "😂", "🎉", "😮", "😢"])
        let recent = FrequentEmoji.used("🦀", recent: ["👍", "🦀"])
        XCTAssertEqual(recent, ["🦀", "👍"])
        XCTAssertEqual(FrequentEmoji.list(recent: (0..<20).map { "\($0)" }).count, 16)
    }
}
