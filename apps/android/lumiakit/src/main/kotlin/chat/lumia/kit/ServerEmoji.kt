// A server's own emoji on the phone (specs/emoji.md, GitHub #17, plan
// step 12), as plain values `src/test` checks on the JVM: what the phone
// draws for one (its picture by address, its name, its shape, its name
// where its picture could not be had and when that is asked for again),
// the server's emoji as the core holds them written into the snapshot,
// what the picker's "Frequently used" holds in a server and out of one,
// the `:word` at the composer's caret and what choosing from its list or
// the picker puts in, checked against the core's emoji at the tap, and
// the words of the Emoji section of Server settings and of
// its refusals, the boards' ("Emoji settings", "Emoji on phones"). Every
// rule the core has is the core's: which names a prefix matches
// (`Client.emojiMatches`), a name from a file's (`emojiNameFromFile`),
// whether a name may be one (`isEmojiName`), the refs a body holds (found
// at send) and the runs a body is drawn with (`parseBodyMarked`).
package chat.lumia.kit

import androidx.compose.runtime.mutableStateMapOf
import uniffi.lumia_ffi.EmojiSummary
import uniffi.lumia_ffi.SnapshotSummary

// What the phone draws

/**
 * One of a server's emoji as the phone draws it: its present [name],
 * where its picture is ([address], one `pictureAddress` allows, so only
 * the instance's own is fetched), its shape in pixels (at most 128 each
 * way), and whether it is an animated GIF.
 */
data class DrawnEmoji(val id: String, val name: String, val address: String, val animated: Boolean, val width: Int, val height: Int) {
    /** `:name:`: what it is named by to a screen reader and in a tooltip, and what it reads as with no picture. */
    val label: String get() = ":$name:"

    /** Its width for a height of one, its shape; square for one whose size is not known. */
    val aspect: Float get() = if (width > 0 && height > 0) width.toFloat() / height else 1f
}

/** [emoji] as drawn, its picture where [pictureOf] allows it; none for one whose picture is nowhere the phone fetches from. */
fun drawnEmoji(emoji: EmojiSummary, pictureOf: (String) -> String?): DrawnEmoji? =
    pictureOf(emoji.imageUrl)?.let { DrawnEmoji(emoji.emojiId, emoji.name, it, emoji.animated, emoji.width.toInt(), emoji.height.toInt()) }

/** An emoji in text is drawn this many times the text's size tall, as wide as its shape (the boards' 1.3 em). */
const val EMOJI_IN_TEXT = 1.3f

/** How long after the first failure in a row an emoji's picture is asked for again; each failure after it doubles that. */
const val EMOJI_RETRY_FIRST_MS = 2_000L

/** The longest an emoji's picture waits to be asked for again. */
const val EMOJI_RETRY_MAX_MS = 60_000L

/** How long after its [failures]th failure in a row a picture is asked for again: 2 s, 4 s, 8 s and so on, at most a minute. */
fun emojiRetryMs(failures: Int): Long =
    (EMOJI_RETRY_FIRST_MS shl (failures - 1).coerceIn(0, 10)).coerceAtMost(EMOJI_RETRY_MAX_MS)

/**
 * The emoji pictures that could not be had, by address: the fetch failed
 * or what came could not be decoded. One of them is its `:name:` where its
 * picture would be (specs/emoji.md 6: an emoji the phone cannot draw reads
 * as its name, never as an empty place), and is asked for again
 * ([emojiRetryMs] after the last failure) for as long as something on
 * screen still wants it; nothing asks for one no screen shows. A failure
 * while one already waits (two places asking at once) is the same
 * failure. A picture had takes it off. Compose state, so what draws one as
 * its name draws it as its picture once it comes.
 */
class EmojiMisses(private val clock: () -> Long = System::currentTimeMillis) {
    private class Miss(val count: Int, val at: Long)

    private val misses = mutableStateMapOf<String, Miss>()

    /** The picture at [address] could not be had. */
    fun missed(address: String) {
        val miss = misses[address]
        if (miss != null && waitMs(address) > 0) return
        misses[address] = Miss((miss?.count ?: 0) + 1, clock())
    }

    /** The picture at [address] was had. */
    fun had(address: String) {
        misses.remove(address)
    }

    /** Whether the picture at [address] could not be had when last asked for: its emoji is drawn as its name. */
    fun missing(address: String): Boolean = address in misses

    /** How long until the picture at [address] may be asked for again; 0 for now, and for one that never failed. */
    fun waitMs(address: String): Long {
        val miss = misses[address] ?: return 0
        return (miss.at + emojiRetryMs(miss.count) - clock()).coerceAtLeast(0)
    }

    /** Every failure forgotten but those at the addresses [keep] says to keep. */
    fun forget(keep: (String) -> Boolean) {
        for (address in misses.keys.toList()) if (!keep(address)) misses.remove(address)
    }
}

// What the phone holds

/**
 * The snapshot with [serverId]'s emoji as [emoji], the core's
 * (`serverEmoji`), which has them from the account log's `EmojiUpdated`
 * as `EmojiChanged` is said: so a removal or a rename is drawn and offered
 * at once whether or not the snapshot over HTTP came (specs/emoji.md 8). A
 * server the snapshot does not hold is left out; every other server's are
 * their own.
 */
/**
 * The Mentions lists held (by server id, `""` Home's, every server's) to
 * read again when [serverId]'s emoji change: its own and Home's. Their
 * pages come from the instance, not the core's store, so the core cannot
 * name their reactions anew as it does the store's; the instance names a
 * reaction by the emoji as it now is, or as it was last called once
 * removed (specs/emoji.md 7).
 */
fun mentionListsFor(serverId: String, held: Collection<String>): List<String> = held.filter { it.isEmpty() || it == serverId }

/**
 * A list read a page at a time, read again from its start as far as it
 * had been read ([count] items) or to its end: each [page] the items after
 * the last one given (`before`, none for the first) and whether there are
 * more. What it answers is the items and whether there are more after them.
 */
suspend fun <T> readPagesAgain(count: Int, after: (T) -> String, page: suspend (before: String?) -> Pair<List<T>, Boolean>): Pair<List<T>, Boolean> {
    val read = mutableListOf<T>()
    var more = true
    while (more && read.size < count) {
        val (items, hasMore) = page(read.lastOrNull()?.let(after))
        read += items
        more = hasMore && items.isNotEmpty()
    }
    return read to more
}

fun SnapshotSummary.withEmoji(serverId: String, emoji: List<EmojiSummary>): SnapshotSummary =
    if (servers.none { it.serverId == serverId && it.emoji != emoji }) this
    else copy(servers = servers.map { if (it.serverId == serverId) it.copy(emoji = emoji) else it })

// The picker and the composer

/** Something chosen in the picker: one of Unicode's, or one of the server's. */
sealed class EmojiPick {
    data class Unicode(val emoji: String) : EmojiPick()
    data class Custom(val emoji: EmojiSummary) : EmojiPick()

    /**
     * What choosing it from the composer's emoji button puts in at the
     * caret: a Unicode emoji as itself, a server's as its `:name:` and a
     * space, which the core finds and keeps beside the text when it is sent.
     */
    val inserted: String
        get() = when (this) {
            is Unicode -> emoji
            is Custom -> ":${emoji.name}: "
        }

    /** How "Frequently used" keeps it among the phone's own (`AppStore.used`). */
    val kept: String
        get() = when (this) {
            is Unicode -> emoji
            is Custom -> CUSTOM_KEPT + emoji.emojiId
        }
}

/** What a server's emoji is kept as among those this phone used: this, then its id. No Unicode emoji starts so. */
const val CUSTOM_KEPT = "custom:"

/**
 * "Frequently used" in the picker (specs/emoji.md 10): this phone's own,
 * newest first, then the quick six it has not used, at most
 * [RECENT_EMOJI]. A server's emoji is among them only in that server,
 * where [server] holds it, by id, with its present name; one of another
 * server's, or one removed, is not. With no [server] (a personal
 * conversation) there are only Unicode's.
 */
fun frequentIn(recent: List<String>, server: List<EmojiSummary>?): List<EmojiPick> {
    val byId = server?.associateBy { it.emojiId } ?: emptyMap()
    val used = recent.mapNotNull { kept ->
        if (kept.startsWith(CUSTOM_KEPT)) byId[kept.removePrefix(CUSTOM_KEPT)]?.let { EmojiPick.Custom(it) } else EmojiPick.Unicode(kept)
    }
    val unicode = used.filterIsInstance<EmojiPick.Unicode>().map { it.emoji }.toSet()
    return (used + QUICK_REACTIONS.filter { it !in unicode }.map { EmojiPick.Unicode(it) }).take(RECENT_EMOJI)
}

/** A search's words as the core's matches take them: what is between colons a person may type around a name. */
fun emojiSearchPrefix(search: String): String = search.trim().trim(':')

/** A `:word` in the draft, from its `:` ([start]) to the caret ([end]), and what follows the `:` ([prefix]). */
data class EmojiQuery(val start: Int, val end: Int, val prefix: String)

private fun nameChar(c: Char): Boolean = c in 'a'..'z' || c in 'A'..'Z' || c in '0'..'9' || c == '_'

/**
 * The `:word` ending at the caret of [text], for the composer's list
 * (specs/emoji.md 10): a `:` at the start or after whitespace, so the
 * colon of a time ("12:30") or of a word ("note:") opens none, and at
 * least one of what a name may hold after it, capitals too, which the
 * core lowercases. Whether it lists anything, and what, is the core's
 * (`emojiMatches`: nothing under two). None while a selection is made or
 * the caret is anywhere else.
 */
fun emojiAt(text: String, selectionStart: Int, selectionEnd: Int): EmojiQuery? {
    if (selectionStart != selectionEnd || selectionEnd < 0 || selectionEnd > text.length) return null
    val before = text.substring(0, selectionEnd)
    val start = before.lastIndexOf(':')
    if (start < 0) return null
    val prefix = before.substring(start + 1)
    val opens = start == 0 || before[start - 1].isWhitespace()
    return if (opens && prefix.isNotEmpty() && prefix.all(::nameChar)) EmojiQuery(start, selectionEnd, prefix) else null
}

/** The draft once a row of the `:` list is chosen: the `:word` becomes the emoji's `:name:` and a space, the caret after them. */
fun pickEmoji(text: String, query: EmojiQuery, name: String): Picked {
    val words = ":$name: "
    return Picked(text.substring(0, query.start) + words + text.substring(query.end), query.start + words.length)
}

/**
 * One of the server's emoji chosen from a list drawn a moment ago (the `:`
 * list, the picker), as [now] has it: the core's emoji as they are at the
 * tap (for the `:` list, what the prefix typed now matches). The same
 * emoji by id under its present name, or none when it is not there any
 * more. So what goes in is the emoji chosen, never a name that has since
 * become another emoji's (a rename and an upload under the old name, told
 * as one change) nor one removed since.
 */
fun chosenNow(chosen: EmojiSummary, now: List<EmojiSummary>): EmojiSummary? = now.firstOrNull { it.emojiId == chosen.emojiId }

/** The draft once [words] are put in at the caret, over what is selected, the caret after them: the composer's emoji button. */
fun insertAt(text: String, selectionStart: Int, selectionEnd: Int, words: String): Picked {
    val from = minOf(selectionStart, selectionEnd).coerceIn(0, text.length)
    val to = maxOf(selectionStart, selectionEnd).coerceIn(0, text.length)
    return Picked(text.substring(0, from) + words + text.substring(to), from + words.length)
}

// Server settings: Emoji

/**
 * "12 of 50": how many a server has against its limit, as the section's
 * row and the screen's head say. The instance always says the limit that
 * applies (`Server.emoji_limit`), and 0 is one: "0 of 0".
 */
fun emojiCount(count: Int, limit: UInt): String = "$count of $limit"

/** Whether a server has as many as it may: Upload is then off and says so. A server above a lowered limit is at it too, and one whose limit is 0 always. */
fun emojiFull(count: Int, limit: UInt): Boolean = count.toLong() >= limit.toLong()

/** What the section says at the limit, in Upload's place; at a limit of 0 removing one makes no room, so nothing says to remove one (the web's words). */
fun emojiFullWords(server: String, limit: UInt): String =
    "$server has as many emoji as it may ($limit)." + if (limit == 0u) "" else " Remove one to upload another."

/** A row's line under its name: who uploaded it, and "Animated" for one that plays. */
fun uploadedBy(name: String, animated: Boolean): String = "Uploaded by $name" + if (animated) " · Animated" else ""

/** The question before one is removed, and what it says under it. */
fun removeEmojiQuestion(name: String): String = "Remove :$name:?"
const val REMOVE_EMOJI_LINE = "Messages that used it will show its name instead."

/** The line under the name field while nothing is wrong with it. */
fun emojiNameHint(name: String): String = "2 to 32 letters, digits and _. It is typed as :${name.ifEmpty { "name" }}: in a message."

/** What the section says when the server has none yet. */
fun noEmojiWords(server: String): String = "$server has no emoji yet."

/** The most bytes of a picture the instance takes for an emoji (`Lumia.Media`, as for an avatar); a bigger one is not sent. */
const val EMOJI_PICTURE_MAX_BYTES = PICTURE_MAX_BYTES

// The refusals, in the boards' words, as the web's and iOS's say them

const val EMOJI_NAME_RULE = "A name is 2 to 32 of a to z, 0 to 9 and _, with no spaces."
const val EMOJI_TOO_LARGE = "This picture is too large for an emoji. An animated one can have at most 200 frames and come to 512 KB once made."
const val EMOJI_UNSUPPORTED = "Lumia can’t use this file. Choose a PNG, JPEG, WebP or GIF."
const val EMOJI_UNREADABLE = "This picture couldn’t be read."

/** Without Manage emoji (it was taken away while the screen was open). */
fun emojiDenied(server: String): String = "You can’t manage $server’s emoji."

/** A rename or a removal of one removed meanwhile. */
const val EMOJI_GONE = "That emoji has been removed."

/** An instance that keeps no pictures keeps no emoji (`FAILED_PRECONDITION`, `state` `blobs`). */
const val EMOJI_NO_PICTURES = "This instance does not keep pictures."

/** Too many requests (`RATE_LIMITED`). */
const val EMOJI_BUSY = "The instance is busy. Try again in a moment."

/** A name another of the server's emoji has. */
fun emojiTaken(server: String, name: String): String = "$server already has :$name:. Choose another name."

/** Where a refusal is said: under the name, under the picture, or over the form. */
enum class EmojiAt { NAME, PICTURE, FORM }

data class EmojiRefusal(val at: EmojiAt, val words: String)

/**
 * The words for a refused upload, rename or removal, by the error's
 * [code] and [details] (specs/emoji.md 4), and where they go; [name] is
 * the name sent, lowercased as the instance keeps it, and [server] the
 * server's name. Anything without words of its own is the instance's
 * [message], over the form.
 */
fun emojiRefusal(code: String?, details: Map<String, String>, server: String, name: String, limit: UInt, message: String): EmojiRefusal = when (code) {
    "ALREADY_EXISTS" -> EmojiRefusal(EmojiAt.NAME, emojiTaken(server, name))
    "INVALID_ARGUMENT" -> when (details["field"]) {
        "picture" -> EmojiRefusal(
            EmojiAt.PICTURE,
            when (details["reason"]) {
                "too_large" -> EMOJI_TOO_LARGE
                "unsupported" -> EMOJI_UNSUPPORTED
                else -> EMOJI_UNREADABLE
            },
        )
        else -> EmojiRefusal(EmojiAt.NAME, EMOJI_NAME_RULE)
    }
    "RESOURCE_EXHAUSTED" -> EmojiRefusal(EmojiAt.FORM, emojiFullWords(server, details["limit"]?.toUIntOrNull() ?: limit))
    "PERMISSION_DENIED" -> EmojiRefusal(EmojiAt.FORM, emojiDenied(server))
    "NOT_FOUND" -> EmojiRefusal(EmojiAt.FORM, EMOJI_GONE)
    "FAILED_PRECONDITION" -> EmojiRefusal(EmojiAt.FORM, EMOJI_NO_PICTURES)
    "RATE_LIMITED" -> EmojiRefusal(EmojiAt.FORM, EMOJI_BUSY)
    else -> EmojiRefusal(EmojiAt.FORM, message)
}
