// Quote replies, reactions, mentions and read state as values: the quote a
// reply carries, "Frequently used", the @ list and what a pick writes,
// who a send leaves unnotified and how the bar says it, who reacted, where
// a body's mentions are, the "New messages" line, marking read, the
// counts, the notifications a read takes away, and the emoji offered.
package chat.lumia.kit

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import uniffi.lumia_ffi.MemberSummary
import uniffi.lumia_ffi.MentionSummary
import uniffi.lumia_ffi.ReactionSummary
import uniffi.lumia_ffi.ReadStateSummary

private fun member(id: String, display: String = "", handle: String = id, nickname: String = "") =
    MemberSummary(id, handle, display, nickname, "", "", "", false, false)

private fun read(id: String, seq: ULong = 0uL, unread: UInt = 0u, mentions: UInt = 0u) = ReadStateSummary(id, seq, unread, mentions)

class RepliesTest {
    @Test
    fun aQuoteIsTheWholeMessageOrThePartChosenTrimmedAndKeptShort() {
        val whole = quoteOf("m1", "ada", "  Shipped on **Friday**.  ")
        assertEquals("m1", whole.messageId)
        assertEquals("ada", whole.authorAccountId)
        assertEquals("Shipped on **Friday**.", whole.text)
        assertFalse(whole.originalDeleted)
        assertEquals("Friday", quoteOf("m1", "ada", "Shipped on Friday.", " Friday ").text)
        // Counted by character, as the core counts it: an emoji is one.
        val long = "🎉".repeat(QUOTE_MOST + 5)
        val kept = quoteOf("m1", "ada", long).text
        assertEquals(QUOTE_MOST, kept.codePointCount(0, kept.length))
        assertEquals("Replying to Ada", replyingTo("Ada"))
    }

    @Test
    fun frequentlyUsedIsThisPhonesOwnThenTheQuickSix() {
        assertEquals(QUICK_REACTIONS, frequentEmoji(emptyList()))
        assertEquals(listOf("🔥", "👍", "❤️", "😂", "🎉", "😮", "😢"), frequentEmoji(listOf("🔥", "👍")))
        assertEquals(listOf("🎉", "🔥"), usedEmoji(listOf("🔥", "🎉"), "🎉"))
        val many = (0 until 20).map { "e$it" }
        assertEquals(RECENT_EMOJI, usedEmoji(many, "x").size)
        assertEquals(RECENT_EMOJI, frequentEmoji(many).size)
    }

    @Test
    fun aPressTakesAReactionAwayOnlyWhenThisAccountHasIt() {
        val reactions = listOf(ReactionSummary("👍", 2u, true, listOf("me", "ada")), ReactionSummary("🎉", 1u, false, listOf("ada")))
        assertTrue(removes(reactions, "👍"))
        assertFalse(removes(reactions, "🎉"))
        assertFalse(removes(reactions, "😢"))
    }

    @Test
    fun whoReactedReadsAsTheDesktopSaysIt() {
        val names = mapOf("ada" to "Ada", "bo" to "Bo", "cy" to "Cy")
        fun words(count: UInt, me: Boolean, ids: List<String>) = reactedWords(ReactionSummary("👍", count, me, ids), "me", names::get)
        assertEquals("You reacted with 👍", words(1u, true, listOf("me")))
        assertEquals("You and Ada reacted with 👍", words(2u, true, listOf("me", "ada")))
        assertEquals("Ada and Bo reacted with 👍", words(2u, false, listOf("ada", "bo")))
        assertEquals("Ada, Bo and 1 other reacted with 👍", words(3u, false, listOf("ada", "bo", "cy")))
        assertEquals("Ada, Bo and 3 others reacted with 👍", words(5u, false, listOf("ada", "bo")))
        assertEquals("Ada and 1 other reacted with 👍", words(2u, false, listOf("ada")))
        assertEquals("Someone reacted with 👍", words(1u, false, listOf("stranger")))
        assertEquals("3 people reacted with 👍", words(3u, false, emptyList()))
    }

    @Test
    fun theAtListOpensOnAnAtAfterWhitespaceAndFollowsTheCaret() {
        assertEquals(MentionQuery(0, 1, ""), mentionAt("@", 1, 1))
        assertEquals(MentionQuery(3, 7, "gra"), mentionAt("hi @gra", 7, 7))
        assertEquals(MentionQuery(3, 9, "a.b-c"), mentionAt("hi @a.b-c", 9, 9))
        assertNull(mentionAt("mail@gra", 8, 8))
        assertNull(mentionAt("hi @gra ", 8, 8))
        assertNull(mentionAt("hi @gra", 4, 7))
        assertNull(mentionAt("no at here", 10, 10))
        assertNull(mentionAt("hi @grä", 7, 7))
    }

    @Test
    fun theAtListOffersEveryoneThenMembersInTheChannelFirst() {
        val members = listOf(member("me", "Me"), member("bo", "Bo Diddley", "bo"), member("grace", "Grace Hopper", "grace"), member("gus", "", "gus", "Gus"))
        val inside = setOf("me", "grace")
        val all = mentionChoices("", members, "me", inside, everyone = true)
        assertEquals(MentionChoice.Everyone, all[0])
        assertEquals(listOf("grace", "bo", "gus"), all.drop(1).map { (it as MentionChoice.Member).member.accountId })
        assertEquals(listOf(true, false, false), all.drop(1).map { (it as MentionChoice.Member).inside })
        // By handle, display name or nickname, in any case; everyone only for one who may, and when it matches.
        assertEquals(listOf("grace", "gus"), mentionChoices("G", members, "me", inside, everyone = true).map { (it as MentionChoice.Member).member.accountId })
        // A name's start, not its middle.
        assertTrue(mentionChoices("did", members, "me", inside, everyone = false).isEmpty())
        assertEquals(listOf("bo"), mentionChoices("BO", members, "me", inside, everyone = false).map { (it as MentionChoice.Member).member.accountId })
        assertEquals(listOf(MentionChoice.Everyone), mentionChoices("every", members, "me", inside, everyone = true))
        assertTrue(mentionChoices("every", members, "me", inside, everyone = false).isEmpty())
        // Who is in the channel not known yet: nobody is marked.
        assertTrue(mentionChoices("", members, "me", null, everyone = false).all { (it as MentionChoice.Member).inside })
        val crowd = (0 until 12).map { member("p$it", "Person $it", "p$it") }
        assertEquals(MENTION_ROWS, mentionChoices("p", crowd, "me", null, everyone = false).size)
        assertEquals("Not in #general", mentionNote(MentionChoice.Member(members[1], inside = false), "general"))
        assertNull(mentionNote(MentionChoice.Member(members[2], inside = true), "general"))
        assertEquals("Notify everyone in #general", mentionNote(MentionChoice.Everyone, "general"))
    }

    @Test
    fun aPickWritesTheHandleAndASpace() {
        val text = "hi @gra and more"
        val query = mentionAt(text, 7, 7)!!
        assertEquals(Picked("hi @grace  and more", 10), pickMention(text, query, MentionChoice.Member(member("grace", "Grace Hopper", "grace"), true)))
        assertEquals(Picked("@everyone ", 10), pickMention("@ev", mentionAt("@ev", 3, 3)!!, MentionChoice.Everyone))
    }

    @Test
    fun thoseMentionedOutsideTheChannelAreNamedByTheBar() {
        val mentions = listOf(
            MentionSummary("bo", false, 0u, 3u),
            MentionSummary("grace", false, 4u, 10u),
            MentionSummary("bo", false, 11u, 14u),
            MentionSummary("", true, 15u, 24u),
            MentionSummary("me", false, 25u, 28u),
        )
        assertEquals(listOf("bo"), outsideOf(mentions, setOf("grace"), "me"))
        assertTrue(outsideOf(mentions, null, "me").isEmpty())
        assertEquals("Ada isn’t in #design, so they won’t be notified.", outsideWords(listOf("Ada"), "design"))
        assertEquals("Ada and Bo aren’t in #design, so they won’t be notified.", outsideWords(listOf("Ada", "Bo"), "design"))
        assertEquals("Ada, Bo and 1 other aren’t in #design, so they won’t be notified.", outsideWords(listOf("Ada", "Bo", "Cy"), "design"))
        assertEquals("Ada, Bo and 2 others aren’t in #design, so they won’t be notified.", outsideWords(listOf("Ada", "Bo", "Cy", "Di"), "design"))
        assertEquals("Invited Ada to #design.", invitedWords(listOf("Ada"), "design"))
        assertEquals("Invited 2 people to #design.", invitedWords(listOf("Ada", "Bo"), "design"))
    }

    @Test
    fun aMentionsBytesBecomeWhereItIsInTheText() {
        // "é" is two bytes, "🎉" four (and two UTF-16 units).
        val body = "é @ada 🎉 @bo"
        val ada = MentionSummary("ada", false, 3u, 7u)
        val bo = MentionSummary("bo", false, 13u, 16u)
        val ranges = mentionRanges(body, listOf(ada, bo))
        assertEquals(listOf("@ada", "@bo"), ranges.map { body.substring(it.first, it.last + 1) })
        // Not on a character's edge, or past the end: left out.
        assertTrue(mentionRanges(body, listOf(MentionSummary("x", false, 1u, 4u), MentionSummary("y", false, 13u, 40u))).isEmpty())
    }

    @Test
    fun theNewMessagesLineIsAboveTheFirstUnreadFromSomeoneElse() {
        data class M(val seq: ULong, val author: String, val notice: Boolean = false)
        val ms = listOf(M(1u, "ada"), M(2u, "me"), M(3u, "ada", notice = true), M(4u, "me"), M(5u, "ada"))
        assertEquals(4, firstUnread(ms, 1uL, { it.seq }) { it.author != "me" && !it.notice })
        assertEquals(-1, firstUnread(ms, 5uL, { it.seq }) { it.author != "me" && !it.notice })
    }

    @Test
    fun aConversationIsMarkedWhenThatMovesSomethingAndAtMostOnceASecond() {
        assertTrue(shouldMark(null, 5uL, null, 0))
        assertFalse(shouldMark(null, 0uL, null, 0))
        assertFalse(shouldMark(read("c", seq = 5uL), 5uL, null, 0))
        assertTrue(shouldMark(read("c", seq = 5uL), 6uL, null, 0))
        assertTrue(shouldMark(read("c", seq = 5uL, unread = 1u), 5uL, null, 0))
        assertFalse(shouldMark(read("c", seq = 4uL), 5uL, Marked(5uL, 1_000), 1_500))
        assertTrue(shouldMark(read("c", seq = 4uL), 5uL, Marked(5uL, 1_000), 2_000))
        assertTrue(shouldMark(read("c", seq = 4uL), 6uL, Marked(5uL, 1_000), 1_500))
    }

    @Test
    fun countsAddUpForTheRailTheSidebarAndTheBadge() {
        val states = listOf(read("a", unread = 3u, mentions = 1u), read("b", unread = 2u), read("dm", unread = 4u, mentions = 4u)).associateBy { it.channelId }
        assertEquals(Unread(3, 1), unreadOf(states, "a"))
        assertEquals(Unread.None, unreadOf(states, "unknown"))
        assertEquals(Unread(5, 1), unreadIn(states, listOf("a", "b", "unknown")))
        assertEquals(5, badgeOf(states.values))
        assertEquals("design channel, 2 mentions", withUnread("design channel", Unread(3, 2)))
        assertEquals("design channel, 1 mention", withUnread("design channel", Unread(3, 1)))
        assertEquals("design channel, unread", withUnread("design channel", Unread(3, 0)))
        assertEquals("design channel", withUnread("design channel", Unread.None))
        assertEquals("#design", mentionPlace("design", "Acme", home = false))
        assertEquals("Acme · #design", mentionPlace("design", "Acme", home = true))
        assertEquals("", mentionPlace(null, "Acme", home = true))
    }

    @Test
    fun aReadTakesAwayTheNotificationsOfConversationsWithNothingUnread() {
        val states = listOf(read("a"), read("b", unread = 1u))
        assertEquals(setOf("a"), readNow(states, listOf("a", "b", "unknown")))
        assertEquals(3, pushBadge("3"))
        assertEquals(3, pushBadge(3))
        assertNull(pushBadge(null))
        assertNull(pushBadge("x"))
        assertNull(pushBadge(-1))
    }

    @Test
    fun aQuoteOpensABlockOfItsOwn() {
        val places = blockPlaces(listOf(Stamp("ada", 0), Stamp("ada", 1_000_000, quotes = true), Stamp("ada", 2_000_000)))
        assertEquals(listOf(true, true, false), places.map { it.opens })
    }

    @Test
    fun theEmojiOfferedStopAtUnicode16AndAreFoundByName() {
        val all = EmojiSet.all
        assertTrue(all.size > 1_800)
        assertEquals(EmojiSet.groupNames.values.toList(), EmojiSet.byGroup.map { it.first })
        for (quick in QUICK_REACTIONS) assertTrue(quick, all.any { it.emoji == quick })
        // Unicode 16's "face with bags under eyes" is offered; 17's "distorted face" is not.
        assertTrue(all.any { it.name == "face with bags under eyes" })
        assertFalse(all.any { it.name == "distorted face" })
        // The default skin tone only.
        assertFalse(all.any { it.name.contains("skin tone") })
        assertEquals(listOf("partying face", "party popper"), EmojiSet.search("party").map { it.name })
        assertEquals("😂", EmojiSet.search("joy").first().emoji)
        assertEquals("👍", EmojiSet.search("thumbs up").first().emoji)
        assertTrue(EmojiSet.search("  ").isEmpty())
        assertEquals("party popper", EmojiSet.nameOf("🎉"))
    }
}
