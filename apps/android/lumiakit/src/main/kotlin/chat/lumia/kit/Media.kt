// Attachments and link cards (specs/media.md 5 and 6), the rules a screen
// draws them by, as plain values `src/test` checks on the JVM: what a
// waiting attachment in the composer is and when a message may go with
// them; which links of a draft are previewed and sent with their cards;
// how one picture fits a message's column and how several share it; how an
// attachment is drawn under Preferences' Images, or once its server let
// it go, and what it is to a screen reader; a size and a length in words.
package chat.lumia.kit

import uniffi.lumia_ffi.AttachmentDraft
import uniffi.lumia_ffi.AttachmentSummary

/** How long typing pauses before the composer asks for its links' cards (spec 6.4). */
const val CARD_PAUSE_MS = 500L

/** The most cards a message carries, and the composer previews (spec 6.1). */
const val MAX_CARDS = 3

/** The most attachments a message carries (spec 2). */
const val MAX_ATTACHMENTS = 10

/** The tallest one picture is drawn in a message, in dp (spec 5). */
const val MEDIA_MAX_HEIGHT = 400f

// The composer's attachments

/**
 * One file in the composer's strip, by the id the screen gave it as it
 * was chosen ([localId]): on its way ([draft] none, no [error]), waiting
 * on the instance under the core's draft, or refused with the words why.
 * [alt] is what the person has typed for it, sent as its alt text.
 */
data class PendingAttachment(
    val localId: String,
    val name: String,
    val sizeBytes: Long,
    /** What the platform said it is (`image/heic`, `video/mp4`), before the core decides. */
    val mediaType: String,
    val draft: AttachmentDraft? = null,
    val error: String? = null,
    val alt: String = "",
) {
    val uploading: Boolean get() = draft == null && error == null
    val ready: Boolean get() = draft != null

    /** The core's kind once processed, else what the platform's type suggests. */
    val kind: String get() = draft?.kind ?: when {
        mediaType == "image/gif" -> "gif"
        mediaType.startsWith("image/") -> "image"
        mediaType.startsWith("video/") -> "video"
        mediaType.startsWith("audio/") -> "audio"
        else -> "file"
    }

    /** Whether it takes alt text: a picture, still or animated. */
    val takesAlt: Boolean get() = kind == "image" || kind == "gif"

    /** What the strip says under its name: its size, that it is uploading, or why it was refused. */
    val status: String get() = when {
        error != null -> error
        uploading -> "Uploading…"
        else -> sizeWords(draft?.sizeBytes?.toLong() ?: sizeBytes)
    }
}

/** The words a refused attachment says on itself. */
fun attachRefused(detail: String): String = "Not attached: $detail"

/** What a file says when its server's files are at their limit (specs/media.md 4.5). */
const val SERVER_OUT_OF_SPACE = "This server is out of space for files."

/**
 * The words an upload refused by the instance says on its file, by the
 * error's [code] and `details.state` ([state]): a full server's
 * (`RESOURCE_EXHAUSTED`, `storage`) in words of its own, anything else as
 * [attachRefused] with the instance's [detail].
 */
fun uploadRefused(code: String?, state: String?, detail: String): String =
    if (code == "RESOURCE_EXHAUSTED" && state == "storage") SERVER_OUT_OF_SPACE else attachRefused(detail)

/**
 * Whether the composer can send: words, or an attachment that is ready,
 * and nothing over the limit. Attachments still uploading do not stop it:
 * the send then waits for them ([waitsFor]).
 */
fun maySend(body: String, pending: List<PendingAttachment>, over: Boolean): Boolean =
    !over && (body.isNotBlank() || pending.any { it.ready || it.uploading })

/** Whether a send asked for now has to wait for attachments still on their way. */
fun waitsFor(pending: List<PendingAttachment>): Boolean = pending.any { it.uploading }

/** The draft ids a send names: those ready, in the strip's order; refused ones stay behind. */
fun draftIds(pending: List<PendingAttachment>): List<String> = pending.mapNotNull { it.draft?.draftId }.take(MAX_ATTACHMENTS)

/** How many more files the strip takes. */
fun roomForAttachments(pending: List<PendingAttachment>): Int = (MAX_ATTACHMENTS - pending.size).coerceAtLeast(0)

// Link cards while composing

/**
 * The links whose cards the composer shows and a send keeps: the core's
 * [links] for the draft (`cardLinks`, at most three, in order) less those
 * the person put away with ×.
 */
fun keptLinks(links: List<String>, dismissed: Set<String>): List<String> =
    links.filter { it !in dismissed }.take(MAX_CARDS)

/** The kept links not yet asked about: each is asked once while the composer is open. */
fun linksToAsk(links: List<String>, dismissed: Set<String>, asked: Set<String>): List<String> =
    keptLinks(links, dismissed).filter { it !in asked }

/**
 * The previews drawn over the composer: each kept link whose card has
 * come (one the page gave none for draws nothing), in the draft's order.
 */
fun <C> shownPreviews(links: List<String>, dismissed: Set<String>, previews: Map<String, C?>): List<Pair<String, C>> =
    keptLinks(links, dismissed).mapNotNull { url -> previews[url]?.let { url to it } }

// Drawing a message's attachments

/** How an attachment is drawn in a message. */
enum class MediaShape {
    /** A still or animated picture, or a video's poster, at its shape. */
    PICTURE,

    /** A row: an icon for its kind, its name and size. */
    FILE_ROW,

    /** One its server let go (spec 4.5): a row with its name and [NO_LONGER_AVAILABLE], fetching and opening nothing. */
    EXPIRED,
}

/** What an attachment its server let go says under its name. */
const val NO_LONGER_AVAILABLE = "No longer available"

/**
 * How [kind] is drawn: a picture, a GIF and a video as pictures (a video
 * its poster or the play placeholder), audio and files as rows; with
 * Preferences' Images at Link, every attachment a row, which opens the
 * viewer for a picture (spec 5); one [expired], whatever its kind, as a
 * row that opens nothing (spec 4.5).
 */
fun mediaShape(kind: String, images: Shown, expired: Boolean = false): MediaShape = when {
    expired -> MediaShape.EXPIRED
    images == Shown.LINK -> MediaShape.FILE_ROW
    kind == "image" || kind == "gif" || kind == "video" -> MediaShape.PICTURE
    else -> MediaShape.FILE_ROW
}

/**
 * A thumbnail's longest side as the core makes one (specs/media.md 3.1),
 * and so the edge a message's picture is decoded at: all of it. It was
 * 1,024 while thumbnails were 400 px and stayed so when they became 1,080,
 * which [sampleFor] halves: a phone drew them at 540.
 */
const val THUMBNAIL_EDGE = 1080

/**
 * What a picture [longest] px on its longest side is sampled down by to be
 * at most [edge] there. The platform's decoder samples by powers of two
 * only, so a picture a little over the edge comes out at about half of it:
 * an edge meant to keep a picture whole is that picture's own.
 */
fun sampleFor(longest: Int, edge: Int): Int {
    var sample = 1
    while (longest / sample > edge) sample *= 2
    return sample
}

/** Whether a message's cards are drawn: Preferences' Embeds at Link draws none. */
fun cardsDrawn(embeds: Shown): Boolean = embeds == Shown.SHOWN

/**
 * One picture's size in a column [maxWidth] wide, in dp: its own
 * dimensions taken as dp, made narrower to fit the column and shorter to
 * fit [maxHeight], its shape kept. Unknown dimensions (0) are drawn as
 * the column's width at 16:9.
 */
fun fitted(width: Int, height: Int, maxWidth: Float, maxHeight: Float = MEDIA_MAX_HEIGHT): Pair<Float, Float> {
    if (width <= 0 || height <= 0) {
        val h = minOf(maxWidth * 9f / 16f, maxHeight)
        return (h * 16f / 9f).coerceAtMost(maxWidth) to h
    }
    var w = width.toFloat()
    var h = height.toFloat()
    if (w > maxWidth) { h = h * maxWidth / w; w = maxWidth }
    if (h > maxHeight) { w = w * maxHeight / h; h = maxHeight }
    return w to h
}

/**
 * How several pictures share a message's column: rows of two, the first
 * alone when there is an odd number, so none is left alone at the end.
 * One picture is one row of one (and drawn by [fitted] instead).
 */
fun mediaRows(count: Int): List<Int> = when {
    count <= 0 -> emptyList()
    count % 2 == 1 -> listOf(1) + List((count - 1) / 2) { 2 }
    else -> List(count / 2) { 2 }
}

/** What a file row's icon shows, from its media type and name. */
enum class FileKind { IMAGE, VIDEO, AUDIO, DOCUMENT, ARCHIVE, CODE, OTHER }

fun fileKindOf(mediaType: String, name: String): FileKind {
    val ext = name.substringAfterLast('.', "").lowercase()
    return when {
        mediaType.startsWith("image/") -> FileKind.IMAGE
        mediaType.startsWith("video/") -> FileKind.VIDEO
        mediaType.startsWith("audio/") -> FileKind.AUDIO
        mediaType == "application/pdf" || mediaType.startsWith("text/plain") ||
            ext in setOf("pdf", "doc", "docx", "odt", "rtf", "txt", "md", "pages", "xls", "xlsx", "ods", "csv", "ppt", "pptx", "key") -> FileKind.DOCUMENT
        mediaType in setOf("application/zip", "application/gzip", "application/x-tar", "application/x-7z-compressed") ||
            ext in setOf("zip", "gz", "tgz", "tar", "7z", "rar", "xz", "bz2") -> FileKind.ARCHIVE
        ext in setOf("kt", "swift", "rs", "ex", "exs", "js", "ts", "tsx", "py", "rb", "go", "java", "c", "h", "cpp", "json", "toml", "yaml", "yml", "html", "css", "sh") -> FileKind.CODE
        else -> FileKind.OTHER
    }
}

/**
 * What an attachment is to a screen reader: "Photo, a cat on a step",
 * "GIF", "Video, beach.mp4, 0:12", "Audio, memo.m4a, 1:03", "File,
 * report.pdf, 2.4 MB". Alt text is a picture's label; a picture without
 * any is named by its kind alone.
 */
fun attachmentLabel(kind: String, name: String, alt: String, sizeBytes: Long, durationMs: Long): String = when (kind) {
    "image" -> listOf("Photo", alt.trim()).filter { it.isNotEmpty() }.joinToString(", ")
    "gif" -> listOf("GIF", alt.trim()).filter { it.isNotEmpty() }.joinToString(", ")
    "video" -> listOf("Video", name, durationMs.takeIf { it > 0 }?.let { lengthWords(it) } ?: "").filter { it.isNotEmpty() }.joinToString(", ")
    "audio" -> listOf("Audio", name, durationMs.takeIf { it > 0 }?.let { lengthWords(it) } ?: "").filter { it.isNotEmpty() }.joinToString(", ")
    else -> "File, $name, ${sizeWords(sizeBytes)}"
}

fun attachmentLabel(a: AttachmentSummary): String =
    if (a.expired) expiredLabel(a.kind, a.name) else attachmentLabel(a.kind, a.name, a.alt, a.sizeBytes.toLong(), a.durationMs.toLong())

/** What an attachment its server let go is to a screen reader: its kind, its name and that it is gone, "Photo, beach.jpg, No longer available". */
fun expiredLabel(kind: String, name: String): String {
    val word = when (kind) {
        "image" -> "Photo"
        "gif" -> "GIF"
        "video" -> "Video"
        "audio" -> "Audio"
        else -> "File"
    }
    return listOf(word, name, NO_LONGER_AVAILABLE).filter { it.isNotEmpty() }.joinToString(", ")
}

/** A file's size as a row shows it: "12 bytes", "340 KB", "2.4 MB", "1.2 GB" (1,024 to the step). */
fun sizeWords(bytes: Long): String {
    // In 1,000s, as the systems' own file sizes and every Lumia client say them.
    if (bytes < 1000) return if (bytes == 1L) "1 byte" else "$bytes bytes"
    val units = listOf("KB", "MB", "GB", "TB")
    var value = bytes / 1000.0
    var unit = 0
    while (value >= 999.5 && unit < units.size - 1) { value /= 1000; unit++ }
    val shown = if (value < 9.95) String.format(java.util.Locale.ROOT, "%.1f", value).removeSuffix(".0") else Math.round(value).toString()
    return "$shown ${units[unit]}"
}

/** A video's or audio's length: "0:07", "12:30", "1:02:03". */
fun lengthWords(ms: Long): String {
    val seconds = ms / 1000
    val h = seconds / 3600
    val m = (seconds % 3600) / 60
    val s = seconds % 60
    return if (h > 0) String.format(java.util.Locale.ROOT, "%d:%02d:%02d", h, m, s) else String.format(java.util.Locale.ROOT, "%d:%02d", m, s)
}

// Editing

/**
 * What saving an edit does once attachments and cards may be removed
 * with it: nothing changed closes the field; words emptied with nothing
 * left to carry asks to delete; anything else is saved.
 */
fun editWithMediaOutcome(before: String, after: String, removing: Int, keepsSomething: Boolean): EditOutcome {
    val words = after.trim()
    return when {
        words.isEmpty() && !keepsSomething -> EditOutcome.Delete
        words == before.trim() && removing == 0 -> EditOutcome.Unchanged
        else -> EditOutcome.Save(words)
    }
}
