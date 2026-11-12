// What a conversation works out before it draws: which messages are one
// block, who is typing and how that reads. Plain values with no Compose
// and no client in them, so `src/test` checks them on the JVM; the same
// rules as the desktop shell's (apps/desktop/src/ui/timeline.rs, `joins`
// and `Place`; src/session/mod.rs and src/ui/conversation.rs for typing).
// And, last, the one rule the session keeps that is no conversation's:
// which of the instance's refusals mean the session is over.
package chat.lumia.kit

import java.time.Instant
import java.time.LocalDate
import java.time.ZoneId
import kotlin.math.abs

// Blocks

/** Who wrote a message and when, in microseconds since the epoch. */
data class Stamp(val author: String, val atMicros: Long)

/** How long after a message another from the same person still goes in its block. */
const val BLOCK_WITHIN_MICROS: Long = 5L * 60 * 1_000_000

private fun dayOf(micros: Long, zone: ZoneId): LocalDate =
    Instant.ofEpochSecond(Math.floorDiv(micros, 1_000_000L)).atZone(zone).toLocalDate()

/**
 * Whether a message goes in the block of the one before it: the same
 * person, on the same day where the reader is (a day's first message
 * starts under its own name), within [BLOCK_WITHIN_MICROS] of it. Measured
 * from the message before and not from the block's first, so a run of
 * quick messages stays one block however long it goes on; and either way
 * round, since two messages' clocks need not be the same one.
 */
fun joins(before: Stamp, stamp: Stamp, zone: ZoneId = ZoneId.systemDefault()): Boolean =
    stamp.author.isNotEmpty() &&
        stamp.author == before.author &&
        abs(stamp.atMicros - before.atMicros) < BLOCK_WITHIN_MICROS &&
        dayOf(stamp.atMicros, zone) == dayOf(before.atMicros, zone)

/** Where a message sits in its block. */
data class BlockPlace(
    /** The block's first message: the one under the avatar, the name and the time. */
    val opens: Boolean,
    /** Its last: the next message, when there is one, opens another. */
    val closes: Boolean,
) {
    companion object {
        /** A message that is a block by itself. */
        val Alone = BlockPlace(opens = true, closes = true)
    }
}

/** Each message's place in its block, for a conversation's messages oldest first. */
fun blockPlaces(stamps: List<Stamp>, zone: ZoneId = ZoneId.systemDefault()): List<BlockPlace> {
    val joined = stamps.indices.map { i -> i > 0 && joins(stamps[i - 1], stamps[i], zone) }
    return stamps.indices.map { i -> BlockPlace(opens = !joined[i], closes = i + 1 >= stamps.size || !joined[i + 1]) }
}

// Typing (protocol 6.7)

/** Someone typing, and when that stops being shown: microseconds since the epoch on this device's clock. */
data class Typist(val accountId: String, val untilMicros: Long)

/**
 * Who is typing in the servers' channels, by channel, whoever began first
 * at the front. A value: every change is a new one, so state that holds it
 * sees the change.
 */
data class Typing(private val byChannel: Map<String, List<Typist>> = emptyMap()) {
    /** The accounts typing in a channel, in the order they began. */
    fun inChannel(channelId: String): List<String> = byChannel[channelId]?.map { it.accountId } ?: emptyList()

    /** Someone is typing until [untilMicros]: a place at the end, or only a later end for someone already there. */
    fun began(channelId: String, accountId: String, untilMicros: Long): Typing {
        val there = byChannel[channelId] ?: emptyList()
        val now = if (there.any { it.accountId == accountId }) {
            there.map { if (it.accountId == accountId) it.copy(untilMicros = untilMicros) else it }
        } else {
            there + Typist(accountId, untilMicros)
        }
        return Typing(byChannel + (channelId to now))
    }

    /** Someone's message arrived in the channel: they have finished typing it. */
    fun stopped(channelId: String, accountId: String): Typing = keeping { channel, typist -> channel != channelId || typist.accountId != accountId }

    /** Without those whose time ran out by [nowMicros]: nothing says that someone stopped, their last word of it only ends. */
    fun expired(nowMicros: Long): Typing = keeping { _, typist -> typist.untilMicros > nowMicros }

    /** When the first of those typing anywhere stops being shown; null when nobody is. */
    val ends: Long? get() = byChannel.values.flatten().minOfOrNull { it.untilMicros }

    private fun keeping(keep: (String, Typist) -> Boolean): Typing {
        val kept = byChannel.mapValues { (channel, typists) -> typists.filter { keep(channel, it) } }.filterValues { it.isNotEmpty() }
        return if (kept == byChannel) this else Typing(kept)
    }
}

/** The line that says who is typing, and how much of its start is names, which are drawn stronger. */
data class TypingWords(val text: String, val named: Int)

/**
 * How those typing are named, whoever began first at the front: one by
 * name, two by both names, more as "Several people", which is no one's name.
 */
fun typingWords(names: List<String>): TypingWords = when (names.size) {
    0 -> TypingWords("", 0)
    1 -> TypingWords("${names[0]} is typing…", names[0].length)
    2 -> "${names[0]} and ${names[1]}".let { both -> TypingWords("$both are typing…", both.length) }
    else -> TypingWords("Several people are typing…", 0)
}

// The session's end (protocol 10 and 12)

/**
 * Whether a failure's code says the instance no longer takes this session:
 * it does not know the token, the refresh token ran out with it, or the
 * session or its device was revoked. Nothing but signing in again follows
 * any of them. The same four as the desktop shell's `Failure::signed_out`
 * and the web's `signedOut`; `Throwable.endsSession` (LumiaClient.kt) asks
 * this of a failure.
 */
fun sessionOver(code: String?): Boolean =
    code == "UNAUTHENTICATED" || code == "TOKEN_EXPIRED" || code == "SESSION_REVOKED" || code == "DEVICE_REVOKED"

/** What the sign-in screen says to someone the instance has sent back to it. */
const val SIGNED_OUT = "You were signed out of this phone. Sign in again to carry on."
