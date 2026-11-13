// Quote replies, reactions, mentions and read state (architecture 6.6), as
// plain values the screens draw from and `src/test` checks: the quote a
// reply carries, the quick reactions and "Frequently used", the `@word` the
// composer's list completes and who it offers, those a message mentioned
// who are not in its channel and how the bar names them, who reacted and
// how a chip says so, where a body's mentions are in its text, where the
// "New messages" line goes, whether a conversation is to be marked read,
// the counts a sidebar, the rail and the badge show, and which delivered
// notifications are of conversations now read. The desktop shell's rules
// and words (apps/desktop/src/app/replies.rs, src/session/reads.rs,
// src/ui/replies.rs, src/ui/mentions.rs).
package chat.lumia.kit

import uniffi.lumia_ffi.MemberSummary
import uniffi.lumia_ffi.MentionSummary
import uniffi.lumia_ffi.QuoteSummary
import uniffi.lumia_ffi.ReactionSummary
import uniffi.lumia_ffi.ReadStateSummary

/** The quick reactions: first in a message's actions, and first in the picker's "Frequently used". */
val QUICK_REACTIONS = listOf("👍", "❤️", "😂", "🎉", "😮", "😢")

/** How many emoji "Frequently used" keeps. */
const val RECENT_EMOJI = 16

/** How many rows the @ list shows. */
const val MENTION_ROWS = 8

/** The most a quote holds, in characters. */
const val QUOTE_MOST = 2_000

/** How long a message a jump reached stays marked. */
const val FLASH_MS = 1_000L

/** How long the bar says who was invited before it goes. */
const val SAID_FOR_MS = 4_000L

/** The least time between two marks of one conversation. */
const val MARK_EVERY_MS = 1_000L

/** How many pages further back finding a message reads before it gives up. */
const val SEEK_PAGES = 5

// Quotes

/**
 * The quote of a message: the whole of its body, or the part of it chosen,
 * trimmed and kept to the [QUOTE_MOST] characters a quote may hold
 * (counted as the core counts them, by code point).
 */
fun quoteOf(messageId: String, authorAccountId: String, body: String, part: String? = null): QuoteSummary {
    val text = (part ?: body).trim()
    val kept = if (text.codePointCount(0, text.length) > QUOTE_MOST) text.substring(0, text.offsetByCodePoints(0, QUOTE_MOST)) else text
    return QuoteSummary(messageId, authorAccountId, kept, false)
}

/** What the bar over the composer says first: "Replying to Ada"; the name is drawn stronger. */
fun replyingTo(name: String): String = "Replying to $name"

// Reactions

/** "Frequently used": what this phone reacted with, newest first, then the quick six, at most [RECENT_EMOJI]. */
fun frequentEmoji(recent: List<String>): List<String> = (recent + QUICK_REACTIONS.filter { it !in recent }).take(RECENT_EMOJI)

/** What this phone has reacted with once [emoji] is used: it first, the rest after, at most [RECENT_EMOJI]. */
fun usedEmoji(recent: List<String>, emoji: String): List<String> = (listOf(emoji) + recent.filter { it != emoji }).take(RECENT_EMOJI)

/** Whether a press on [emoji] takes this account's reaction away (it has it) rather than adding it. */
fun removes(reactions: List<ReactionSummary>, emoji: String): Boolean = reactions.any { it.reaction == emoji && it.me }

/**
 * Who reacted with an emoji, for its chip: "Ada, Bo and 3 others reacted
 * with 👍", "You" first when this account is one of them. [nameOf] is a
 * person's name, or null for one with none known, who is "Someone".
 */
fun reactedWords(reaction: ReactionSummary, me: String, nameOf: (String) -> String?): String {
    val names = mutableListOf<String>()
    if (reaction.me) names.add("You")
    for (id in reaction.accountIds) if (id != me) names.add(nameOf(id)?.ifEmpty { null } ?: "Someone")
    val unnamed = (reaction.count.toInt() - names.size).coerceAtLeast(0)
    val who = when {
        names.isEmpty() -> if (unnamed == 1) "1 person" else "$unnamed people"
        names.size == 1 && unnamed == 0 -> names[0]
        names.size == 1 -> "${names[0]} and ${others(unnamed)}"
        names.size == 2 && unnamed == 0 -> "${names[0]} and ${names[1]}"
        else -> "${names[0]}, ${names[1]} and ${others(names.size - 2 + unnamed)}"
    }
    return "$who reacted with ${reaction.reaction}"
}

/** The rest after the names: "1 other", "3 others". */
fun others(n: Int): String = if (n == 1) "1 other" else "$n others"

// Mentions: the composer's @ list

/** An `@word` in the draft, from its `@` ([start]) to the caret ([end]), and what follows the `@`. */
data class MentionQuery(val start: Int, val end: Int, val query: String)

private fun handleChar(c: Char): Boolean = c in 'a'..'z' || c in 'A'..'Z' || c in '0'..'9' || c == '_' || c == '.' || c == '-'

/**
 * The `@word` ending at the caret of [text]: an `@` at the start or after
 * whitespace, and what can be in a handle after it. None while a selection
 * is made ([selectionStart] is not [selectionEnd]) or the caret is
 * anywhere else.
 */
fun mentionAt(text: String, selectionStart: Int, selectionEnd: Int): MentionQuery? {
    if (selectionStart != selectionEnd || selectionEnd < 0 || selectionEnd > text.length) return null
    val before = text.substring(0, selectionEnd)
    val start = before.lastIndexOf('@')
    if (start < 0) return null
    val query = before.substring(start + 1)
    val opens = start == 0 || before[start - 1].isWhitespace()
    return if (opens && query.all(::handleChar)) MentionQuery(start, selectionEnd, query) else null
}

/** One row of the @ list. */
sealed class MentionChoice {
    /** `@everyone`, for one who may. */
    data object Everyone : MentionChoice()

    /** A member of the server; one not in the channel ([inside] false) is marked so. */
    data class Member(val member: MemberSummary, val inside: Boolean) : MentionChoice()
}

/**
 * The @ list's rows for [query]: everyone first for one who may mention
 * everyone ([everyone]) when it matches, then the server's [members] but
 * this account whose handle, display name or nickname starts with it, in
 * any case, those in the channel first ([inside]; everyone counts as in it
 * while who is in it is not known), then by name; at most [MENTION_ROWS].
 */
fun mentionChoices(query: String, members: List<MemberSummary>, me: String, inside: Set<String>?, everyone: Boolean): List<MentionChoice> {
    val wanted = query.lowercase()
    fun starts(name: String) = name.lowercase().startsWith(wanted)
    val people = members
        .filter { it.accountId != me && (starts(it.handle) || starts(it.displayName) || starts(it.nickname)) }
        .map { MentionChoice.Member(it, inside?.contains(it.accountId) ?: true) }
        .sortedWith(compareBy<MentionChoice.Member>({ !it.inside }, { nameOf(it.member).lowercase() }))
    val all = (if (everyone && "everyone".startsWith(wanted)) listOf(MentionChoice.Everyone) else emptyList()) + people
    return all.take(MENTION_ROWS)
}

/** The draft once a row is picked, and where the caret goes: the `@word` becomes the handle and a space. */
data class Picked(val text: String, val caret: Int)

fun pickMention(text: String, query: MentionQuery, choice: MentionChoice): Picked {
    val handle = when (choice) {
        MentionChoice.Everyone -> "everyone"
        is MentionChoice.Member -> choice.member.handle
    }
    val words = "@$handle "
    return Picked(text.substring(0, query.start) + words + text.substring(query.end), query.start + words.length)
}

/** The note on a row of the @ list: what @everyone does, or that a member is not in the channel; none otherwise. */
fun mentionNote(choice: MentionChoice, channel: String): String? = when (choice) {
    MentionChoice.Everyone -> "Notify everyone in #$channel"
    is MentionChoice.Member -> if (choice.inside) null else "Not in #$channel"
}

// Mentions: after a send

/**
 * Of the people [mentions] names, those not in the channel, once each and
 * never this account, as far as who is in it is known ([inside]; none
 * while it is not).
 */
fun outsideOf(mentions: List<MentionSummary>, inside: Set<String>?, me: String): List<String> {
    if (inside == null) return emptyList()
    return mentions.filter { !it.everyone && it.accountId.isNotEmpty() && it.accountId != me && it.accountId !in inside }
        .map { it.accountId }.distinct()
}

/** What the bar after a send says: "Ada isn’t in #design, so they won’t be notified." */
fun outsideWords(names: List<String>, channel: String): String = when (names.size) {
    0 -> ""
    1 -> "${names[0]} isn’t in #$channel, so they won’t be notified."
    2 -> "${names[0]} and ${names[1]} aren’t in #$channel, so they won’t be notified."
    else -> "${names[0]}, ${names[1]} and ${others(names.size - 2)} aren’t in #$channel, so they won’t be notified."
}

/** What the bar says for a few seconds once an add made invitations of some: "Invited Ada to #design." */
fun invitedWords(names: List<String>, channel: String): String =
    if (names.size == 1) "Invited ${names[0]} to #$channel." else "Invited ${names.size} people to #$channel."

// Mentions: drawing

/**
 * Where [mentions] are in [body] as the text is indexed here (UTF-16), from
 * the UTF-8 byte ranges the core gives, half-open: `start until end`. A
 * range that is not on a character's edge or runs past the body is left
 * out, never drawn wrong.
 */
fun mentionRanges(body: String, mentions: List<MentionSummary>): List<IntRange> {
    if (mentions.isEmpty()) return emptyList()
    // The UTF-16 index at each UTF-8 offset that starts a character, and at the end.
    val at = HashMap<Int, Int>()
    var bytes = 0
    var i = 0
    while (i < body.length) {
        at[bytes] = i
        val cp = body.codePointAt(i)
        bytes += when {
            cp < 0x80 -> 1
            cp < 0x800 -> 2
            cp < 0x10000 -> 3
            else -> 4
        }
        i += Character.charCount(cp)
    }
    at[bytes] = body.length
    return mentions.mapNotNull { m ->
        val start = at[m.start.toInt()]
        val end = at[m.end.toInt()]
        if (start != null && end != null && end > start) start until end else null
    }
}

// Read state

/** What is unread in a conversation: messages others sent after this account's place, and how many of those are for it. */
data class Unread(val unread: Int, val mentions: Int) {
    companion object {
        val None = Unread(0, 0)
    }
}

/** A conversation's read state by channel id; nothing for one not counted. */
fun unreadOf(states: Map<String, ReadStateSummary>, channelId: String): Unread =
    states[channelId]?.let { Unread(it.unread.toInt(), it.mentions.toInt()) } ?: Unread.None

/** Whether anything in these conversations is unread, and how much of it is for this account (a server's, on the rail). */
fun unreadIn(states: Map<String, ReadStateSummary>, channelIds: List<String>): Unread =
    channelIds.map { unreadOf(states, it) }.fold(Unread.None) { a, b -> Unread(a.unread + b.unread, a.mentions + b.mentions) }

/** The badge: everything unread that is for this account. */
fun badgeOf(states: Collection<ReadStateSummary>): Int = states.sumOf { it.mentions.toInt() }

/** A row's name for a screen reader with what is unread in it: "design channel, 2 mentions", or ", unread". */
fun withUnread(label: String, unread: Unread): String = when {
    unread.mentions == 1 -> "$label, 1 mention"
    unread.mentions > 1 -> "$label, ${unread.mentions} mentions"
    unread.unread > 0 -> "$label, unread"
    else -> label
}

/**
 * Where the "New messages" line goes: above the first of [items] after
 * where this account had read to ([readTo]) that counts as unread
 * ([counts]: someone else's message, not a notice); -1 for none.
 */
fun <T> firstUnread(items: List<T>, readTo: ULong, seq: (T) -> ULong, counts: (T) -> Boolean): Int =
    items.indexOfFirst { seq(it) > readTo && counts(it) }

/** When a conversation was last marked read, and through which event. */
data class Marked(val through: ULong, val atMillis: Long)

/**
 * Whether to mark a conversation read through [seq], its newest message
 * being on screen in front of the person: only when that moves this
 * account's place or clears a count, and at most once a second
 * ([MARK_EVERY_MS]) for the same place.
 */
fun shouldMark(state: ReadStateSummary?, seq: ULong, last: Marked?, nowMillis: Long): Boolean {
    if (seq == 0uL) return false
    val moves = state == null || seq > state.readSeq || state.unread > 0u
    val recent = last != null && last.through >= seq && nowMillis - last.atMillis < MARK_EVERY_MS
    return moves && !recent
}

/** Where a mention was said, over it on a Mentions page: `#design`, or at Home, across every server, `Acme · #design`. */
fun mentionPlace(channel: String?, server: String?, home: Boolean): String {
    if (channel == null) return ""
    val name = "#$channel"
    return if (home && server != null) "$server · $name" else name
}

// Notifications

/**
 * Of the conversations a delivered notification is about ([delivered],
 * by channel id), those now read: counted, with nothing unread. One not
 * counted is left, there being nothing to say it was read.
 */
fun readNow(states: Collection<ReadStateSummary>, delivered: Collection<String>): Set<String> {
    val byId = states.associateBy { it.channelId }
    return delivered.filter { byId[it]?.unread == 0u }.toSet()
}

/** A push's badge, as a data message carries it (a string) or UnifiedPush's notice (a number); none when it has none. */
fun pushBadge(value: Any?): Int? = when (value) {
    is Number -> value.toInt().takeIf { it >= 0 }
    is String -> value.toIntOrNull()?.takeIf { it >= 0 }
    else -> null
}
