// A message body as the phone draws it (protocol 14, issue #18), as plain
// values: a paragraph of the core's blocks (`parseBody`) as styled text,
// its mentions by name in their pills and its inline code in chips; when a
// long body collapses; and the composer's counter, from the core's
// `messageRoom`, and what it says. The core reads the Markdown; nothing
// here parses it.
package chat.lumia.kit

import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.LinkAnnotation
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.TextLinkStyles
import uniffi.lumia_ffi.BodyBlock
import uniffi.lumia_ffi.BodyInline
import uniffi.lumia_ffi.BodyStyle
import uniffi.lumia_ffi.EmojiRefSummary
import uniffi.lumia_ffi.MentionSummary
import uniffi.lumia_ffi.MessageRoom

// Collapsing

/** A body drawn taller than this many lines collapses. */
const val COLLAPSE_OVER = 14

/** How many lines a collapsed body shows. */
const val COLLAPSED_LINES = 10

/** Whether a body of [lines], as drawn at the width and size it has, collapses to [COLLAPSED_LINES]. */
fun collapses(lines: Int): Boolean = lines > COLLAPSE_OVER

/** What the button under a collapsible body says. */
fun showWords(expanded: Boolean): String = if (expanded) "Show less" else "Show more"

// The composer's counter

/**
 * Whether a draft of [length] UTF-16 units could be near enough to [max]
 * bytes for its counter to show (from 90% of the limit): a unit is at most
 * three bytes of UTF-8, so one shorter than this is not, and the core need
 * not be asked on every keystroke of a short draft.
 */
fun mayNearLimit(length: Int, max: UInt): Boolean = length.toLong() * 3 * 10 >= max.toLong() * 9

/** Whether a draft with [room] is over the limit, and Send and Save refuse it. */
fun isOver(room: MessageRoom?): Boolean = room != null && room.left < 0

/** What the counter says: "1,204 characters left", "1 character too long". */
fun roomWords(left: Long): String {
    val n = if (left < 0) -left else left
    val characters = if (n == 1L) "character" else "characters"
    return if (left < 0) "${grouped(n)} $characters too long" else "${grouped(n)} $characters left"
}

/** [n] with a comma between each three digits: "16,384". */
fun grouped(n: Long): String {
    val digits = n.toString()
    val head = digits.length % 3
    return buildString {
        for ((i, c) in digits.withIndex()) {
            if (i > 0 && (i - head) % 3 == 0) append(',')
            append(c)
        }
    }
}

// A paragraph

/** How each of a paragraph's runs is drawn, in the theme and at the size of the text around it. */
data class InlineLook(
    val emphasis: SpanStyle,
    val strong: SpanStyle,
    val strike: SpanStyle,
    val code: SpanStyle,
    val mention: SpanStyle,
    val link: SpanStyle,
    val edited: SpanStyle,
    /** Whether a link opens when pressed; else it is only drawn as one (Preferences' preview, `LocalPressable`). */
    val pressable: Boolean = true,
)

/**
 * A paragraph as drawn: its text with its styles and links, where its
 * mention pills and inline code chips are (each with the thin spaces that
 * pad it), the server's emoji drawn as pictures in it ([emoji], each by
 * the id its inline content is found by), and its [words] as a screen
 * reader has them, without the thin spaces.
 */
data class StyledInline(val text: AnnotatedString, val pills: List<IntRange>, val chips: List<IntRange>, val words: String, val emoji: List<DrawnEmoji> = emptyList())

/**
 * The tag Compose's text finds inline content by (what
 * `androidx.compose.foundation.text.appendInlineContent` writes, which
 * appends where this marks text already there).
 */
const val INLINE_CONTENT_TAG = "androidx.compose.foundation.text.inlineContent"

/** The id an emoji's picture is put in a paragraph by. */
fun emojiContentId(emoji: DrawnEmoji): String = "emoji ${emoji.id}"

/** The room inside a mention's pill or a code chip, each side of its words. */
const val THIN_SPACE = "\u2009"

/**
 * [inline] as drawn with [look]: each mention run's text, the mention as
 * typed, replaced by its [words] (`mentions` the message's, by the run's
 * index; the text as typed for one past them), padded by a thin space each
 * side inside its pill, as is inline code inside its chip; every range
 * moved with the text before it. The core's ranges are UTF-16 units, as
 * `AnnotatedString` counts. [edited] appends "(edited)" to the words.
 *
 * A run of one of the server's emoji ([emoji] the message's refs, by the
 * run's index) whose emoji the phone holds ([emojiOf], by id) is its
 * present `:name:`, marked as inline content to draw its picture in place
 * of those words, and read out as them (specs/emoji.md 6); one it does not
 * hold (removed, or not yet known) is the text as typed. One held whose
 * picture could not be had ([pictureMissing], `EmojiMisses`) is its present
 * `:name:` as words, not a place its picture would fill.
 */
fun styledInline(
    inline: BodyInline,
    look: InlineLook,
    mentions: List<MentionSummary>,
    words: (MentionSummary, String) -> String,
    edited: Boolean = false,
    emoji: List<EmojiRefSummary> = emptyList(),
    emojiOf: (String) -> DrawnEmoji? = { null },
    pictureMissing: (DrawnEmoji) -> Boolean = { false },
): StyledInline {
    val src = inline.text
    // What changes in the text: a mention replaced by its words, a thin space put in either side of code.
    class Edit(val at: Int, val end: Int, val insert: String, val spoken: String)
    val edits = mutableListOf<Edit>()
    // The emoji drawn, by where their runs start in the text as typed.
    val pictured = mutableMapOf<Int, DrawnEmoji>()
    for (run in inline.runs) {
        val start = run.start.toInt().coerceIn(0, src.length)
        val end = run.end.toInt().coerceIn(start, src.length)
        when (val style = run.style) {
            is BodyStyle.Mention -> {
                val typed = src.substring(start, end)
                val said = mentions.getOrNull(style.index.toInt())?.let { words(it, typed) } ?: typed
                edits += Edit(start, end, THIN_SPACE + said + THIN_SPACE, said)
            }
            BodyStyle.Code -> {
                edits += Edit(start, start, THIN_SPACE, "")
                edits += Edit(end, end, THIN_SPACE, "")
            }
            is BodyStyle.Emoji -> {
                val held = emoji.getOrNull(style.index.toInt())?.let { emojiOf(it.emojiId) }
                if (held != null && end > start) {
                    // Its present name, whatever it was typed as (a rename since rewrites nothing).
                    edits += Edit(start, end, held.label, held.label)
                    if (!pictureMissing(held)) pictured[start] = held
                }
            }
            else -> {}
        }
    }
    // In order along the text; at one place, what is put in before what replaces.
    edits.sortWith(compareBy<Edit>({ it.at }, { it.end }))
    val delta = { e: Edit -> e.insert.length - (e.end - e.at) }
    // Where a range that starts at [o] starts now: after whatever was before it, not what was put in at it.
    fun startOf(o: Int) = o + edits.filter { it.end < o || (it.end == o && it.at < o) }.sumOf(delta)
    // Where a range that ends at [o] ends now: after whatever was put in at it.
    fun endOf(o: Int) = o + edits.filter { it.end <= o }.sumOf(delta)

    val text = StringBuilder()
    val spoken = StringBuilder()
    var at = 0
    for (e in edits) {
        if (e.at < at) continue
        text.append(src, at, e.at)
        spoken.append(src, at, e.at)
        text.append(e.insert)
        spoken.append(e.spoken)
        at = e.end
    }
    text.append(src, at, src.length)
    spoken.append(src, at, src.length)

    val pills = mutableListOf<IntRange>()
    val chips = mutableListOf<IntRange>()
    val builder = AnnotatedString.Builder(text.toString())
    for ((at, held) in pictured) {
        val start = startOf(at)
        builder.addStringAnnotation(INLINE_CONTENT_TAG, emojiContentId(held), start, start + held.label.length)
    }
    // Links first, so the styles of what is in a link draw over its own.
    for (link in inline.links) {
        val start = startOf(link.start.toInt())
        val end = endOf(link.end.toInt())
        if (start >= end) continue
        if (look.pressable) builder.addLink(LinkAnnotation.Url(link.url, TextLinkStyles(style = look.link)), start, end)
        else builder.addStyle(look.link, start, end)
    }
    for (run in inline.runs) {
        val start = startOf(run.start.toInt())
        val end = endOf(run.end.toInt())
        if (start >= end) continue
        val style = when (run.style) {
            BodyStyle.Emphasis -> look.emphasis
            BodyStyle.Strong -> look.strong
            BodyStyle.Strike -> look.strike
            BodyStyle.Code -> look.code.also { chips += start until end }
            is BodyStyle.Mention -> look.mention.also { pills += start until end }
            // Drawn as its picture, or as the words as typed: nothing of the text's styles.
            is BodyStyle.Emoji -> null
        } ?: continue
        builder.addStyle(style, start, end)
    }
    if (edited) {
        builder.append(" ")
        val from = builder.length
        builder.append(EDITED)
        builder.addStyle(look.edited, from, builder.length)
        spoken.append(", $EDITED_LABEL")
    }
    return StyledInline(builder.toAnnotatedString(), pills, chips, spoken.toString(), pictured.values.distinctBy { it.id })
}

/** Whether "(edited)" follows a body's last words: when it ends in a paragraph, and not in code, a quote or a list, after which it is a line of its own. */
fun editedInline(blocks: List<BodyBlock>): Boolean = blocks.lastOrNull() is BodyBlock.Para

// A code block

/** Its button, what the button says for a moment once pressed, and what it is to a screen reader. */
const val COPY = "Copy"
const val COPIED = "Copied"
const val COPY_CODE = "Copy code"

/** The marker of a list's item at [index]: "•", or its number counted from [start]. */
fun listMarker(start: ULong?, index: Int): String = if (start == null) "•" else "${start + index.toULong()}."
