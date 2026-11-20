// A server's own emoji on the phone (specs/emoji.md, GitHub #17, plan
// step 12), as plain values `src/test` checks on the JVM: what the phone
// draws for one (its picture by address, its name, its shape), what the
// picker's "Frequently used" holds in a server and out of one, the `:word`
// at the composer's caret and what choosing from its list or the picker
// puts in, and the words of the Emoji section of Server settings and of
// its refusals, the boards' ("Emoji settings", "Emoji on phones"). Every
// rule the core has is the core's: which names a prefix matches
// (`Client.emojiMatches`), a name from a file's (`emojiNameFromFile`),
// whether a name may be one (`isEmojiName`), the refs a body holds (found
// at send) and the runs a body is drawn with (`parseBodyMarked`).
package chat.lumia.kit

import uniffi.lumia_ffi.EmojiSummary

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

/** The draft once [words] are put in at the caret, over what is selected, the caret after them: the composer's emoji button. */
fun insertAt(text: String, selectionStart: Int, selectionEnd: Int, words: String): Picked {
    val from = minOf(selectionStart, selectionEnd).coerceIn(0, text.length)
    val to = maxOf(selectionStart, selectionEnd).coerceIn(0, text.length)
    return Picked(text.substring(0, from) + words + text.substring(to), from + words.length)
}

// Server settings: Emoji

/** "12 of 50": how many a server has against its limit, as the section's row and the screen's head say; the count alone where no limit is known. */
fun emojiCount(count: Int, limit: UInt): String = if (limit > 0u) "$count of $limit" else "$count"

/** Whether a server has as many as it may: Upload is then off and says so. A server above a lowered limit is at it too. */
fun emojiFull(count: Int, limit: UInt): Boolean = limit > 0u && count.toLong() >= limit.toLong()

/** What the section says at the limit, in Upload's place. */
fun emojiFullWords(server: String, limit: UInt): String = "$server has as many emoji as it may ($limit). Remove one to upload another."

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
