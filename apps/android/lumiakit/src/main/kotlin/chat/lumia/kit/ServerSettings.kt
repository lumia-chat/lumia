// What the Server settings screen decides, as plain values `src/test`
// checks on the JVM (architecture 6.7): a server's name as the instance
// keeps it, how long it keeps its messages and its attachments and when a
// change to either deletes some, what a full server does and what its
// files take, the words a refusal reads as, and where a picture the
// instance names is. The web's and the desktop's rules and words.
package chat.lumia.kit

import java.text.BreakIterator

/** The most characters a server's name has, once trimmed. */
const val SERVER_NAME_MAX = 100

/** What the instance says when a name is not 1 to 100 characters, and what the screen says before sending one that is not. */
const val SERVER_NAME_REFUSED = "A server name is 1 to 100 characters."

/** What a member without `MANAGE_SERVER` is told. */
const val SERVER_SETTINGS_DENIED = "You cannot change this server's settings."

/** What a refused choice of default channels reads as (architecture 6.8). */
const val DEFAULT_CHANNELS_REFUSED = "Default channels must be public channels of this server."

/** The most bytes of a picture the instance takes (`Lumia.Media`); a bigger one is not sent. */
const val PICTURE_MAX_BYTES = 8 * 1024 * 1024

/** How many characters a person sees in [text]: its grapheme clusters, as the instance counts a name (Elixir's `String.length`). */
fun characters(text: String): Int {
    val breaks = BreakIterator.getCharacterInstance()
    breaks.setText(text)
    var n = 0
    while (breaks.next() != BreakIterator.DONE) n++
    return n
}

/** Whether [draft], trimmed, is a name a server may have: 1 to 100 characters. */
fun serverNameFits(draft: String): Boolean = characters(draft.trim()) in 1..SERVER_NAME_MAX

/** Whether the name field holds something to save: not empty once trimmed, and not the name the server has. */
fun serverNameChanged(draft: String, current: String): Boolean {
    val name = draft.trim()
    return name.isNotEmpty() && name != current
}

/** How long a server keeps its messages, in days; 0 is for ever. */
object Retention {
    const val FOREVER = 0

    /** The choices offered, longest first: Forever, 1 year, 90 days, 30 days. */
    val standard = listOf(FOREVER, 365, 90, 30)

    /** The choices for a server that keeps its messages [current] days: the standard ones, and that one after them when it is none of them (set elsewhere). */
    fun choices(current: Int): List<Int> = if (current in standard) standard else standard + current

    /** How a choice reads. */
    fun label(days: Int): String = when (days) {
        FOREVER -> "Forever"
        365 -> "1 year"
        1 -> "1 day"
        else -> "$days days"
    }

    /** Whether keeping messages [to] days instead of [from] keeps them for less time, Forever being the longest: then older ones are deleted. */
    fun shortens(from: Int, to: Int): Boolean = to != FOREVER && (from == FOREVER || to < from)

    /** What the screen says above Save when the choice would delete messages; none when it would not. */
    fun warning(from: Int, to: Int): String? =
        if (shortens(from, to)) "Messages older than ${label(to)} will be deleted for everyone. This cannot be undone." else null

    /**
     * What the screen says above Save when keeping attachments [to] days
     * instead of [from] would delete some (specs/media.md 4.5): the daily
     * job lets them go, and their messages keep their words. None when it
     * would not.
     */
    fun attachmentsWarning(from: Int, to: Int): String? =
        if (shortens(from, to)) {
            "Attachments older than ${label(to)} will be deleted for everyone. Their messages stay. This cannot be undone."
        } else {
            null
        }
}

/**
 * What a server does when its files reach the instance's limit
 * (specs/media.md 4.5): refuse new uploads, the default, or let the
 * attachments of its oldest messages go to make room.
 */
object WhenFull {
    /** The choices offered, by whether the oldest attachments go, the default first. */
    val choices = listOf(false to "Refuse new uploads", true to "Delete the oldest attachments")

    /** What the screen says above Save when the choice would start deleting attachments; none when it would not. */
    fun warning(from: Boolean, to: Boolean): String? =
        if (to && !from) {
            "When an upload would not fit, the oldest messages' attachments are deleted for everyone to make room."
        } else {
            null
        }
}

/**
 * What saving the Files card sends (`setServerFiles`): each setting that
 * differs from what the server has, the rest none, which leaves it as it
 * is. [changed] is whether there is anything to send.
 */
data class FilesChange(val attachmentRetentionDays: Int?, val deleteOldestWhenFull: Boolean?) {
    val changed: Boolean get() = attachmentRetentionDays != null || deleteOldestWhenFull != null

    companion object {
        fun of(keptDays: Int, keptDeleteOldest: Boolean, days: Int, deleteOldest: Boolean) =
            FilesChange(days.takeIf { it != keptDays }, deleteOldest.takeIf { it != keptDeleteOldest })
    }
}

/**
 * What a server's files take, as its settings say it (specs/media.md
 * 4.5): "1.2 GB of 5 GB used", or "1.2 GB used" where the instance sets
 * no limit ([maxBytes] 0).
 */
fun storageWords(usedBytes: Long, maxBytes: Long): String =
    if (maxBytes > 0) "${sizeWords(usedBytes)} of ${sizeWords(maxBytes)} used" else "${sizeWords(usedBytes)} used"

/**
 * The servers whose "Show deleted messages" a snapshot changed, from each
 * server's setting before and after (by server id); a server in only one
 * of them changed nothing held. Their conversations are read again from
 * the store, which leaves deleted messages out where the setting is off
 * (architecture 6.7), so one on screen gains or loses its "(deleted)" lines
 * at once.
 */
fun showDeletedChanged(before: Map<String, Boolean>, after: Map<String, Boolean>): Set<String> =
    after.filter { (id, show) -> before[id]?.let { it != show } ?: false }.keys

/**
 * The words for a refused change of a server's name or retention, by the
 * error's code and `details.field`; none for a refusal that has no words of
 * its own, which reads as the instance's message.
 */
fun serverSettingsRefused(code: String?, field: String?): String? = when {
    code == "PERMISSION_DENIED" -> SERVER_SETTINGS_DENIED
    code == "INVALID_ARGUMENT" && field == "name" -> SERVER_NAME_REFUSED
    code == "INVALID_ARGUMENT" && field == "default_channels" -> DEFAULT_CHANNELS_REFUSED
    else -> null
}

/**
 * The words for a refused picture, by the error's code and
 * `details.reason`, as the web's account photo has them; none for a
 * refusal that has no words of its own.
 */
fun pictureRefused(code: String?, reason: String?): String? = when (code) {
    "PERMISSION_DENIED" -> SERVER_SETTINGS_DENIED
    "INVALID_ARGUMENT" -> when (reason) {
        "too_large" -> PICTURE_TOO_LARGE
        "unsupported" -> "That file is not a picture Lumia can read. Choose a PNG, JPEG, WebP or GIF."
        else -> "That picture could not be read."
    }
    "FAILED_PRECONDITION" -> "This instance does not keep pictures."
    "RATE_LIMITED" -> "The instance is busy. Try again in a moment."
    else -> null
}

/** A picture over [PICTURE_MAX_BYTES], said before it is sent and when the instance refuses one. */
const val PICTURE_TOO_LARGE = "That picture is too large. Choose one under 8 MB."

/**
 * Where a picture the instance names is, to fetch and draw: the instance's
 * own picture path with a name of 64 lowercase hexadecimal digits, on the
 * instance at [baseUrl], and nothing else (the core's `is_own_avatar`), so
 * nothing an instance says can make the phone fetch from anywhere else.
 * None for an empty path, which is no picture.
 */
fun pictureAddress(baseUrl: String, path: String): String? {
    val name = path.removePrefix("/api/v1/avatars/")
    if (name == path || name.length != 64 || name.any { it !in '0'..'9' && it !in 'a'..'f' }) return null
    return baseUrl.trimEnd('/') + path
}
