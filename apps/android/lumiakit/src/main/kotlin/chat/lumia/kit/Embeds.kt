// What plays inside a conversation (specs/media.md 5 and 6.6), the rules a
// screen plays by, as plain values `src/test` checks on the JVM: what an
// attachment or a link's embed is while it plays, one at a time, and what
// its row leaving the screen does to it; an embed's address (Twitch's
// parent), the origins its web view may go to, its size in a column, the
// height a post reports by `postMessage`, the page that holds its iframe,
// and its words. Which link has an embed is the core's (`embedFor`), never
// the card's.
package chat.lumia.kit

import java.net.URI
import uniffi.lumia_ffi.AttachmentSummary
import uniffi.lumia_ffi.EmbedSummary

/** What is playing or shown in a conversation; one at a time, embeds included. */
enum class PlayKind {
    /** An attached video, inline in its row. */
    VIDEO,

    /** An attached recording, inline in its row. */
    AUDIO,

    /** A site's video player (YouTube, Vimeo, Twitch, TikTok, Facebook's videos). */
    EMBED_PLAYER,

    /** A site's audio player (SoundCloud, Spotify). */
    EMBED_AUDIO,

    /** A site's post (X, Bluesky, Facebook, Instagram, Reddit). */
    EMBED_POST,
    ;

    val isEmbed: Boolean get() = this == EMBED_PLAYER || this == EMBED_AUDIO || this == EMBED_POST
}

/** What happens to what plays when its row leaves the screen. */
enum class OffScreen {
    /** An attached video pauses where it is. */
    PAUSE,

    /** A site's player or post goes back to its card: no site's player can be paused from outside it. */
    BACK_TO_CARD,

    /** Audio, attached or a site's, plays on until another starts or the conversation is left. */
    PLAYS_ON,
}

/** What its row scrolling away does to [kind] (specs/media.md 5 and 6.6). */
fun offScreen(kind: PlayKind): OffScreen = when (kind) {
    PlayKind.VIDEO -> OffScreen.PAUSE
    PlayKind.EMBED_PLAYER, PlayKind.EMBED_POST -> OffScreen.BACK_TO_CARD
    PlayKind.AUDIO, PlayKind.EMBED_AUDIO -> OffScreen.PLAYS_ON
}

/** How an attachment plays, or none: a video or a recording that its server still holds. One with `expired` set has no blobs and never plays. */
fun playKindOf(a: AttachmentSummary): PlayKind? = when {
    a.expired -> null
    a.kind == "video" -> PlayKind.VIDEO
    a.kind == "audio" -> PlayKind.AUDIO
    else -> null
}

/** How an embed plays, by the core's kind (`player`, `audio`, `post`). */
fun playKindOf(e: EmbedSummary): PlayKind = when (e.kind) {
    "audio" -> PlayKind.EMBED_AUDIO
    "post" -> PlayKind.EMBED_POST
    else -> PlayKind.EMBED_PLAYER
}

/** What one thing playing is called in its conversation: an attachment of a message, or a link of one (the same link in two messages is two embeds). */
fun attachmentPlayKey(messageId: String, attachmentId: String): String = "a:$messageId:$attachmentId"

fun embedPlayKey(messageId: String, url: String): String = "e:$messageId:$url"

// Embeds

/** The tallest a site's player is drawn, as a picture is (spec 6.6). */
const val EMBED_MAX_HEIGHT = MEDIA_MAX_HEIGHT

/** The tallest a post that grows is let grow, in dp: a bound on what an embed may claim. */
const val EMBED_MAX_GROWN = 4000

/** The name the page holding an embed calls this app by, in that web view only. */
const val EMBED_BRIDGE = "LumiaEmbed"

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
 * one that `resizes` the height it [reported] once it has.
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

private val heightField = Regex("\"height\"\\s*:\\s*(\\d+(?:\\.\\d+)?)")
private val dataField = Regex("\"data\"\\s*:\\s*(\\d+(?:\\.\\d+)?)")

/**
 * The height a post's embed reported to the page holding it, from the
 * message's [data] as JSON text, by its site's own form: Bluesky's
 * `{"height": 412, "id": …}`, X's `twttr.private.resize` with a height in
 * its params, Reddit's `{"type": "resize.embed", "data": 412}`; none for any
 * other message, site or a height out of bounds. The page passes on only
 * messages from the embed's own window and origin.
 */
fun reportedHeight(provider: String, data: String): Int? {
    val found = when (provider) {
        "bluesky" -> heightField.find(data)
        "x" -> if ("twttr.private.resize" in data) heightField.find(data) else null
        "reddit" -> if ("\"resize.embed\"" in data) dataField.find(data) else null
        else -> null
    } ?: return null
    val h = found.groupValues[1].toDoubleOrNull() ?: return null
    return Math.round(h).toInt().takeIf { it in 1..EMBED_MAX_GROWN }
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
 * as JSON text, to [EMBED_BRIDGE] with [token], which a frame of another
 * origin cannot read from this page; nothing else reaches the app.
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
    if (typeof data === 'string' && data.length <= 8192) $EMBED_BRIDGE.report('$token', data);
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
