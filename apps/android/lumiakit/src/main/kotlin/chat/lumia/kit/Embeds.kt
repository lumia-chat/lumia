// What plays inside a conversation (specs/media.md 5 and 6.6), what a
// screen plays by, as plain values `src/test` checks on the JVM: what an
// attachment or a link's embed is to the core's rules of what playing
// stops (`playbackStops`, `playbackStopsOffScreen`; the rules are the
// core's, never copied here), what each is called, and what is being had
// for the one pressed last; an embed's address (Twitch's parent), the
// origins its web view may go to, its size in a column, the page that
// holds its iframe, and its words. Which link has an embed is the core's
// (`embedFor`), never the card's, and so is the height a post reports
// (`reportedEmbedHeight`).
package chat.lumia.kit

import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import java.net.URI
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.launch
import uniffi.lumia_ffi.AttachmentSummary
import uniffi.lumia_ffi.EmbedSummary
import uniffi.lumia_ffi.PlayingSummary

/**
 * What an attachment is to the core's rules of what playing stops, or
 * none: a video or a recording that its server still holds. One with
 * `expired` set has no blobs and never plays.
 */
fun playingOf(a: AttachmentSummary): PlayingSummary? = when {
    a.expired -> null
    a.kind == "video" || a.kind == "audio" -> PlayingSummary(embed = false, kind = a.kind)
    else -> null
}

/** What an embed is to them, by the core's kind (`player`, `audio`, `post`). */
fun playingOf(e: EmbedSummary): PlayingSummary = PlayingSummary(embed = true, kind = e.kind)

/** What one thing playing is called in its conversation: an attachment of a message, or a link of one (the same link in two messages is two embeds). */
fun attachmentPlayKey(messageId: String, attachmentId: String): String = "a:$messageId:$attachmentId"

fun embedPlayKey(messageId: String, url: String): String = "e:$messageId:$url"

/** Whether what [key] names is in [messageId]'s row: one of its attachments or its links. */
fun playKeyIn(key: String, messageId: String): Boolean = key.startsWith("a:$messageId:") || key.startsWith("e:$messageId:")

/**
 * What is being had for something pressed (an attachment's file, a
 * Bluesky post's account), the latest only: [busy] from [start] until its
 * work ends, or until [cancel]. Work that was cancelled does not always
 * end at once (a call the core is in the middle of ends when the core
 * answers), and when it does end it changes nothing here: [busy] is the
 * latest work's alone, so one press ending never takes away the spinner
 * of the press after it.
 */
class Having(private val scope: CoroutineScope) {
    var busy by mutableStateOf(false)
        private set
    private var job: Job? = null
    private var turn = 0

    /** Starts [work] in place of any before it. */
    fun start(work: suspend () -> Unit) {
        cancel()
        val mine = turn
        busy = true
        job = scope.launch {
            try {
                work()
            } finally {
                if (turn == mine) busy = false
            }
        }
    }

    /** Nothing is being had any more; work under way is cancelled. */
    fun cancel() {
        turn++
        job?.cancel()
        job = null
        busy = false
    }
}

// Embeds

/** The tallest a site's player is drawn, as a picture is (spec 6.6). */
const val EMBED_MAX_HEIGHT = MEDIA_MAX_HEIGHT

/** The name the page holding an embed calls this app by, in that web view only. */
const val EMBED_BRIDGE = "LumiaEmbed"

/** The longest message the page hands on: the core reads none longer (`REPORTED_MESSAGE_MAX`), so none longer is copied across. */
const val EMBED_MESSAGE_MAX = 8192

/**
 * The address an embed loads: the core's, and for Twitch, which refuses an
 * embed that does not name the page holding it, `parent=` and [host] (the
 * instance's, the host of the page the web view holds). None while a
 * Bluesky handle is still to be resolved (the address is then the core's
 * `blueskyEmbedUrl`).
 */
fun embedAddress(e: EmbedSummary, host: String): String? {
    if (e.url.isEmpty()) return null
    if (!e.needsParent) return e.url
    return e.url + (if ('?' in e.url) "&" else "?") + "parent=" + host
}

/** A web address's origin, `https://host` (with its port when it names one), lowercased; none for anything not `http` or `https`. */
fun originOf(url: String): String? {
    // Only the scheme and authority are read: a query a URI parser refuses does not hide the origin.
    val head = Regex("^([A-Za-z][A-Za-z0-9+.-]*)://([^/?#]*)").find(url) ?: return null
    if ('@' in head.groupValues[2]) return null
    val uri = runCatching { URI(head.value) }.getOrNull() ?: return null
    val scheme = uri.scheme?.lowercase() ?: return null
    if (scheme != "http" && scheme != "https") return null
    val host = uri.host?.lowercase()?.ifEmpty { null } ?: return null
    return if (uri.port == -1) "$scheme://$host" else "$scheme://$host:${uri.port}"
}

/** Whether the web view may go to [url] itself: one of the embeds' origins (the core's `embedOrigins`); anything else opens in the browser. */
fun isEmbedOrigin(url: String, origins: List<String>): Boolean {
    val origin = originOf(url) ?: return false
    return origins.any { it.equals(origin, ignoreCase = true) }
}

/**
 * An embed's size in a column [column] dp wide, in dp (spec 6.6): a player
 * at the column's width at its shape and at most [EMBED_MAX_HEIGHT] tall
 * (narrower to keep its shape); audio at the column's width and its site's
 * height; a post at most its `maxWidth` wide and its site's height, or for
 * one that `resizes` the height it [reported] once it has (the core's
 * `reportedEmbedHeight`, which holds it in bounds).
 */
fun embedSize(e: EmbedSummary, column: Float, reported: Int? = null): Pair<Float, Float> {
    if (e.aspectWidth > 0u && e.aspectHeight > 0u) {
        val ratio = e.aspectHeight.toFloat() / e.aspectWidth.toFloat()
        val h = column * ratio
        return if (h <= EMBED_MAX_HEIGHT) column to h else EMBED_MAX_HEIGHT / ratio to EMBED_MAX_HEIGHT
    }
    val w = if (e.maxWidth > 0u) minOf(column, e.maxWidth.toFloat()) else column
    val h = if (e.resizes && reported != null) reported.toFloat() else e.fixedHeight.toFloat()
    return w to h
}

/** What the press on a card with an embed is to a screen reader: the core's label and the card's title after a comma, "Play video from YouTube, <title>". */
fun embedPressLabel(e: EmbedSummary, title: String): String =
    listOf(e.label, title.trim()).filter { it.isNotEmpty() }.joinToString(", ")

/** The iframe's title: the card's title, or the site's name when there is none. */
fun embedFrameTitle(e: EmbedSummary, title: String): String = title.trim().ifEmpty { e.providerName }

/** What a card says when its Bluesky post could not be found (a handle that does not resolve): its press then opens the browser. */
const val BLUESKY_NOT_FOUND = "This post couldn't be shown. Tap it again to open it in your browser."

private fun escapeHtml(s: String): String = buildString {
    for (c in s) when (c) {
        '&' -> append("&amp;")
        '<' -> append("&lt;")
        '>' -> append("&gt;")
        '"' -> append("&quot;")
        '\'' -> append("&#39;")
        else -> append(c)
    }
}

/**
 * The page a web view holds an embed in, loaded with the instance's origin
 * as its base (so the site sees it as the referrer, which YouTube needs):
 * the embed's iframe alone, filling the view, allowed to autoplay (the
 * press was the gesture) and go full screen. For a post that grows, its
 * script passes on each message from that iframe's own window and origin,
 * as text (a string as posted, an object as its JSON, which is what the
 * core's `reportedEmbedHeight` reads), to [EMBED_BRIDGE] with [token],
 * which a frame of another origin cannot read from this page; nothing else
 * reaches the app, and nothing longer than the core reads
 * ([EMBED_MESSAGE_MAX]) crosses the bridge.
 */
fun embedPage(address: String, frameTitle: String, token: String, forwards: Boolean): String {
    val origin = originOf(address) ?: ""
    val script = if (!forwards) "" else """
<script>
(function () {
  var frame = document.getElementById('embed');
  window.addEventListener('message', function (event) {
    if (event.source !== frame.contentWindow || event.origin !== '$origin') return;
    var data = event.data;
    if (typeof data !== 'string') { try { data = JSON.stringify(data); } catch (e) { return; } }
    if (typeof data === 'string' && data.length <= $EMBED_MESSAGE_MAX) $EMBED_BRIDGE.report('$token', data);
  });
})();
</script>"""
    return """<!doctype html>
<html><head><meta charset="utf-8"><meta name="viewport" content="width=device-width, initial-scale=1">
<meta name="referrer" content="strict-origin-when-cross-origin">
<style>html, body { margin: 0; padding: 0; width: 100%; height: 100%; overflow: hidden; background: transparent; } iframe { display: block; border: 0; width: 100%; height: 100%; }</style>
</head><body>
<iframe id="embed" src="${escapeHtml(address)}" title="${escapeHtml(frameTitle)}" allow="autoplay; encrypted-media; fullscreen; picture-in-picture" allowfullscreen referrerpolicy="strict-origin-when-cross-origin"></iframe>$script
</body></html>"""
}
