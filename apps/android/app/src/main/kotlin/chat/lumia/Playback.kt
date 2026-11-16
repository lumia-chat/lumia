// What plays inside a conversation (specs/media.md 5 and 6.6, issue #21):
// an attached video in its row through Media3's player and view, with the
// platform's controls and a full-screen button; an attached recording in
// its row; a site's player, audio or post in a web view in its card's
// place. Never before a press, one attachment and one embed at most, and
// what a start stops and what leaving the screen stops are the core's
// rules (`playbackStops`, `playbackStopsOffScreen`), carried out here as
// each can be: an attachment pauses, an embed goes back to its card.
package chat.lumia

import android.annotation.SuppressLint
import android.app.Activity
import android.content.Context
import android.graphics.Color as AndroidColor
import android.net.Uri
import android.os.Handler
import android.os.Looper
import android.os.Message
import android.view.LayoutInflater
import android.view.View
import android.view.ViewGroup
import android.webkit.CookieManager
import android.webkit.JavascriptInterface
import android.webkit.RenderProcessGoneDetail
import android.webkit.WebChromeClient
import android.webkit.WebResourceRequest
import android.webkit.WebView
import android.webkit.WebViewClient
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.systemBarsPadding
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateMapOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.viewinterop.AndroidView
import androidx.compose.ui.window.Dialog
import androidx.compose.ui.window.DialogProperties
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.LifecycleEventObserver
import androidx.lifecycle.compose.LocalLifecycleOwner
import androidx.media3.common.MediaItem
import androidx.media3.common.PlaybackException
import androidx.media3.common.Player
import androidx.media3.common.util.UnstableApi
import androidx.media3.exoplayer.ExoPlayer
import androidx.media3.ui.PlayerView
import chat.lumia.kit.BlueskyLookup
import chat.lumia.kit.CardNote
import chat.lumia.kit.EMBED_BRIDGE
import chat.lumia.kit.Having
import chat.lumia.kit.LumiaSession
import chat.lumia.kit.attachmentLabel
import chat.lumia.kit.blueskyNote
import chat.lumia.kit.embedAddress
import chat.lumia.kit.embedFrameTitle
import chat.lumia.kit.embedPage
import chat.lumia.kit.isEmbedOrigin
import chat.lumia.kit.originOf
import chat.lumia.kit.playKeyIn
import chat.lumia.kit.playingOf
import java.io.File
import java.security.SecureRandom
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.withContext
import uniffi.lumia_ffi.AttachmentSummary
import uniffi.lumia_ffi.EmbedSummary
import uniffi.lumia_ffi.PlayingSummary
import uniffi.lumia_ffi.blueskyEmbedUrl
import uniffi.lumia_ffi.embedOrigins
import uniffi.lumia_ffi.playbackStops
import uniffi.lumia_ffi.playbackStopsOffScreen
import uniffi.lumia_ffi.reportedEmbedHeight

/**
 * One conversation's playing: the attachment that plays ([attachment], by
 * the kit's `attachmentPlayKey`) through the one player its attachments
 * share, and the embed shown ([embed], by its `embedPlayKey`) in its web
 * view. A post being read and an attachment playing can both be there;
 * the core says what a start stops. It outlives the activity being made
 * again (the phone turned), so a video plays on through a turn; an
 * embed's web view belongs to the activity and goes back to its card then.
 */
class Playback(private val app: Context, val session: LumiaSession) {
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate)
    private val main = Handler(Looper.getMainLooper())

    /** Why an attachment could not play, by its key, in words; said where it would have played. */
    val errors = mutableStateMapOf<String, String>()

    /** What a card says under it since its press showed no embed, by its key, and whether its next press asks again. */
    val notes = mutableStateMapOf<String, CardNote>()

    /**
     * Something is starting: what the core's rule says it stops
     * (`playbackStops`) stops as it can. A card pressed pauses the
     * attachment and puts the other embed back; an attachment started puts
     * back an embed that plays and leaves a post where it is.
     */
    private fun starting(what: PlayingSummary) {
        attachmentIs?.let { if (playbackStops(what, it)) pauseAttachment() }
        embedIs?.let { if (playbackStops(what, it)) stopEmbed() }
    }

    // Attachments

    /** The attachment playing in its row, or paused there; one at a time. */
    var attachment by mutableStateOf<String?>(null)
        private set
    private var attachmentIs: PlayingSummary? = null
    private val file = Having(scope)

    /** [attachment]'s file is still being had: its poster or its row, with a spinner, until it is. */
    val opening: Boolean get() = file.busy

    private var exo by mutableStateOf<ExoPlayer?>(null)
    val player: Player? get() = exo
    var playing by mutableStateOf(false)
        private set

    /** The video drawn full screen rather than in its row. */
    var fullScreen by mutableStateOf(false)

    private fun ensurePlayer(): ExoPlayer = exo ?: ExoPlayer.Builder(app).build().also { p ->
        exo = p
        p.addListener(object : Player.Listener {
            override fun onIsPlayingChanged(isPlaying: Boolean) { playing = isPlaying }
            override fun onPlayerError(error: PlaybackException) {
                attachment?.let { errors[it] = "This can't be played on this phone." }
                stopAttachment()
            }
        })
    }

    /** Plays an attached video or recording in its row, from its file opened into the app's cache; nothing for one that cannot play (an expired one has no blobs). */
    fun play(key: String, a: AttachmentSummary) {
        val what = playingOf(a) ?: return
        if (attachment == key) {
            if (!opening) exo?.let { p -> if (!p.isPlaying) toggle() }
            return
        }
        starting(what)
        // The one player takes this file, so whatever it held leaves its row.
        stopAttachment()
        attachment = key
        attachmentIs = what
        errors.remove(key)
        file.start {
            try {
                val bytes = session.attachmentBytes(a.main)
                val opened = withContext(Dispatchers.IO) {
                    val dir = File(app.cacheDir, "play").apply { mkdirs() }
                    // Named by the attachment and the media type's extension, never the name its sender chose.
                    val ext = a.mediaType.substringAfter('/', "").substringBefore(';').filter { it.isLetterOrDigit() }.take(5).ifEmpty { "bin" }
                    File(dir, "${a.attachmentId.filter { it.isLetterOrDigit() || it == '-' }}.$ext").apply {
                        if (!exists() || length() != bytes.size.toLong()) writeBytes(bytes)
                    }
                }
                val p = ensurePlayer()
                p.setMediaItem(MediaItem.fromUri(Uri.fromFile(opened)))
                p.prepare()
                p.play()
            } catch (e: CancellationException) {
                throw e
            } catch (e: Exception) {
                errors[key] = "This can't be played: ${e.message ?: "the file could not be had"}"
                stopAttachment()
            }
        }
    }

    /** Play or pause what the player holds; from its end, from the start again. */
    fun toggle() {
        val p = exo ?: return
        if (p.isPlaying) p.pause() else {
            if (p.playbackState == Player.STATE_ENDED) p.seekTo(0)
            p.play()
        }
    }

    // Embeds

    /** The embed shown in its card's place; one at a time. */
    var embed by mutableStateOf<String?>(null)
        private set
    private var embedIs: PlayingSummary? = null
    private val account = Having(scope)

    /** [embed] is a Bluesky post whose account is still being looked up: its card, with a spinner, until it is. */
    val resolving: Boolean get() = account.busy

    /** The address the shown embed loads, once had (at once, or after its Bluesky handle resolved). */
    var address by mutableStateOf<String?>(null)
        private set

    /** The height the shown post reported, when it grows. */
    var reported by mutableStateOf<Int?>(null)
        private set

    /** A site's own full screen (its player's button), shown over everything. */
    var customView by mutableStateOf<View?>(null)
        private set
    private var customHidden: WebChromeClient.CustomViewCallback? = null

    private var held: WebView? = null
    private var heldKey: String? = null

    /**
     * Shows a link's embed in its card's place. A Bluesky post named by
     * handle resolves it first, from Bluesky, on this press: no account for
     * it, and the card says so and opens the browser from then on; no
     * answer, and the card says that and its next press asks again
     * (`blueskyNote`).
     */
    fun show(key: String, e: EmbedSummary) {
        if (embed == key) return
        val what = playingOf(e)
        starting(what)
        // One web view: whatever it showed is back to its card.
        stopEmbed()
        embed = key
        embedIs = what
        notes.remove(key)
        val now = embedAddress(e, hostOf(session.instanceOrigin))
        if (now != null) {
            address = now
            return
        }
        account.start {
            val lookup = try {
                val url = session.resolveBlueskyHandle(e.blueskyHandle)?.let { did -> blueskyEmbedUrl(did, e.blueskyRkey) }
                if (url != null) {
                    address = url
                    BlueskyLookup.FOUND
                } else {
                    BlueskyLookup.NO_ACCOUNT
                }
            } catch (x: CancellationException) {
                throw x
            } catch (x: Exception) {
                BlueskyLookup.NO_ANSWER
            }
            blueskyNote(lookup, e.blueskyHandle)?.let { note ->
                notes[key] = note
                stopEmbed()
            }
        }
    }

    /**
     * The web view of the embed shown as [key]: the one already held (its
     * row was away while its audio played on or its post stayed shown), or
     * a new one holding the embed's page. JavaScript on, playing without another gesture (the press was
     * one), no file or content access, no third-party cookies; it goes
     * only to the embeds' origins, anything else and any new window opening
     * in the browser.
     */
    @SuppressLint("SetJavaScriptEnabled", "JavascriptInterface")
    fun embedView(context: Context, key: String, e: EmbedSummary, title: String): WebView {
        held?.takeIf { heldKey == key }?.let { view ->
            (view.parent as? ViewGroup)?.removeView(view)
            return view
        }
        val origins = embedOrigins()
        val token = ByteArray(16).also { SecureRandom().nextBytes(it) }.joinToString("") { "%02x".format(it) }
        val view = WebView(context)
        // A web view that wraps its content lays its page out 0 tall, so the iframe's 100% is nothing: it fills the room Compose gives it.
        view.layoutParams = ViewGroup.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.MATCH_PARENT)
        view.settings.apply {
            javaScriptEnabled = true
            domStorageEnabled = true
            mediaPlaybackRequiresUserGesture = false
            allowFileAccess = false
            allowContentAccess = false
            javaScriptCanOpenWindowsAutomatically = false
            setSupportMultipleWindows(true)
            setGeolocationEnabled(false)
        }
        CookieManager.getInstance().setAcceptThirdPartyCookies(view, false)
        view.setBackgroundColor(AndroidColor.TRANSPARENT)
        view.contentDescription = embedFrameTitle(e, title)
        view.webViewClient = object : WebViewClient() {
            override fun shouldOverrideUrlLoading(v: WebView, request: WebResourceRequest): Boolean {
                val url = request.url.toString()
                // The embed's own frame may go about its origins; the page holding it, and any frame elsewhere, may not.
                if (!request.isForMainFrame && isEmbedOrigin(url, origins)) return false
                if (request.isForMainFrame || request.hasGesture()) browse(context, url)
                return true
            }

            override fun onRenderProcessGone(v: WebView, detail: RenderProcessGoneDetail): Boolean {
                if (held === v) stopEmbed()
                return true
            }
        }
        view.webChromeClient = object : WebChromeClient() {
            override fun onCreateWindow(v: WebView, isDialog: Boolean, isUserGesture: Boolean, resultMsg: Message): Boolean {
                // A new window is the browser's: a throwaway view learns where it goes and hands that over.
                val opener = WebView(context)
                opener.webViewClient = object : WebViewClient() {
                    override fun shouldOverrideUrlLoading(w: WebView, request: WebResourceRequest): Boolean {
                        if (isUserGesture) browse(context, request.url.toString())
                        main.post { w.destroy() }
                        return true
                    }
                }
                (resultMsg.obj as? WebView.WebViewTransport)?.webView = opener
                resultMsg.sendToTarget()
                return true
            }

            override fun onShowCustomView(v: View, callback: CustomViewCallback) {
                customView = v
                customHidden = callback
            }

            override fun onHideCustomView() {
                customView = null
                customHidden = null
            }
        }
        if (e.resizes) {
            val provider = e.provider
            view.addJavascriptInterface(object {
                @JavascriptInterface
                fun report(from: String, data: String) {
                    if (from != token) return
                    // The core reads each site's form and holds the height in bounds.
                    val h = reportedEmbedHeight(provider, data)?.toInt() ?: return
                    main.post { if (heldKey == key) reported = h }
                }
            }, EMBED_BRIDGE)
        }
        val address = address ?: ""
        view.loadDataWithBaseURL("${session.instanceOrigin}/", embedPage(address, embedFrameTitle(e, title), token, e.resizes), "text/html", "utf-8", null)
        held = view
        heldKey = key
        return view
    }

    /** The web view's place in the list has gone: kept while it is still the embed shown (its audio playing on, its post scrolled past), else let go. */
    fun released(view: WebView) {
        if (view !== held) destroy(view)
    }

    /** Puts a site's full screen away. */
    fun hideCustomView() {
        customHidden?.onCustomViewHidden()
        customView = null
        customHidden = null
    }

    // Leaving

    /**
     * What leaving the screen stops, by the core's rule
     * (`playbackStopsOffScreen`), of [messageId]'s row, or with none of the
     * whole conversation: an attached video pauses where it is and a site's
     * player goes back to its card; audio plays on and a post stays shown.
     */
    private fun offScreen(messageId: String? = null) {
        fun left(key: String?, what: PlayingSummary?): Boolean =
            key != null && what != null && (messageId == null || playKeyIn(key, messageId)) && playbackStopsOffScreen(what)
        if (left(attachment, attachmentIs)) pauseAttachment()
        if (left(embed, embedIs)) stopEmbed()
    }

    /** A message's row has left the screen. */
    fun rowGone(messageId: String) = offScreen(messageId)

    /** The app has gone to the background. */
    fun backgrounded() = offScreen()

    /** The activity is being made again: an embed's web view is the old activity's, so it goes back to its card; the player plays on. */
    fun activityRemade() = stopEmbed()

    /** An attachment stopped as it can be: paused where it is, or, its file not yet here, left as it was before the press. */
    private fun pauseAttachment() {
        if (opening) stopAttachment() else exo?.pause()
    }

    /** The attachment leaves its row: the player emptied. */
    private fun stopAttachment() {
        file.cancel()
        exo?.let { it.stop(); it.clearMediaItems() }
        playing = false
        fullScreen = false
        attachment = null
        attachmentIs = null
    }

    /** The embed goes back to its card. */
    private fun stopEmbed() {
        account.cancel()
        if (customView != null) hideCustomView()
        held?.let { view -> if (view.parent == null) destroy(view) }
        held = null
        heldKey = null
        address = null
        reported = null
        embed = null
        embedIs = null
    }

    /** The conversation was left: everything stops. */
    fun release() {
        stopAttachment()
        stopEmbed()
        exo?.release()
        exo = null
        scope.cancel()
    }

    private fun destroy(view: WebView) {
        (view.parent as? ViewGroup)?.removeView(view)
        view.stopLoading()
        view.loadUrl("about:blank")
        view.destroy()
    }
}

/** A link from inside an embed, opened in the browser: only the web's own schemes. */
private fun browse(context: Context, url: String) {
    if (originOf(url) == null) return
    runCatching { openInBrowser(context, url) }
}

private fun hostOf(origin: String): String = Uri.parse(origin).host ?: origin

/** The conversations' playbacks, by channel, kept while an activity is made again. */
private object Playbacks {
    val held = mutableMapOf<String, Playback>()
}

/**
 * A conversation's playback: kept through the activity being made again,
 * let go (everything stopping) when the conversation is left; when the
 * app goes to the background, what the core says stops off screen does (a
 * video pauses, a site's player goes back to its card).
 */
@Composable
fun rememberPlayback(session: LumiaSession, channelId: String): Playback {
    val context = LocalContext.current
    val activity = context as? Activity
    val playback = remember(session, channelId) {
        Playbacks.held[channelId]?.takeIf { it.session === session } ?: Playback(context.applicationContext, session).also {
            Playbacks.held.remove(channelId)?.release()
            Playbacks.held[channelId] = it
        }
    }
    val lifecycle = LocalLifecycleOwner.current.lifecycle
    DisposableEffect(playback, lifecycle) {
        val observer = LifecycleEventObserver { _, event ->
            if (event == Lifecycle.Event.ON_STOP && activity?.isChangingConfigurations != true) playback.backgrounded()
        }
        lifecycle.addObserver(observer)
        onDispose {
            lifecycle.removeObserver(observer)
            if (activity?.isChangingConfigurations == true) {
                playback.activityRemade()
            } else {
                if (Playbacks.held[channelId] === playback) Playbacks.held.remove(channelId)
                playback.release()
            }
        }
    }
    PlaybackOverlays(playback)
    return playback
}

/** What a conversation's playback draws over everything: the video full screen, and a site's own full screen. */
@androidx.annotation.OptIn(UnstableApi::class)
@Composable
private fun PlaybackOverlays(playback: Playback) {
    val player = playback.player
    if (playback.fullScreen && player != null) {
        Dialog(onDismissRequest = { playback.fullScreen = false }, properties = DialogProperties(usePlatformDefaultWidth = false, decorFitsSystemWindows = false)) {
            Box(Modifier.fillMaxSize().background(Color.Black).systemBarsPadding()) {
                AndroidView(
                    factory = { c ->
                        playerView(c, inline = false).apply {
                            this.player = player
                            setFullscreenButtonState(true)
                            setFullscreenButtonClickListener { playback.fullScreen = false }
                            showController()
                        }
                    },
                    onRelease = { it.player = null },
                    modifier = Modifier.fillMaxSize(),
                )
            }
        }
    }
    val custom = playback.customView
    if (custom != null) {
        Dialog(onDismissRequest = { playback.hideCustomView() }, properties = DialogProperties(usePlatformDefaultWidth = false, decorFitsSystemWindows = false)) {
            Box(Modifier.fillMaxSize().background(Color.Black).systemBarsPadding()) {
                AndroidView(
                    factory = { c -> android.widget.FrameLayout(c).apply { (custom.parent as? ViewGroup)?.removeView(custom); addView(custom) } },
                    onRelease = { it.removeAllViews() },
                    modifier = Modifier.fillMaxSize(),
                )
            }
        }
    }
}

/**
 * Media3's view with the platform's controls: in a row ([inline]) drawn on
 * a texture so it scrolls and clips with it, full screen on a surface.
 */
@androidx.annotation.OptIn(UnstableApi::class)
fun playerView(context: Context, inline: Boolean = true): PlayerView =
    (if (inline) LayoutInflater.from(context).inflate(R.layout.inline_player, null, false) as PlayerView else PlayerView(context)).apply {
        useController = true
        setShowNextButton(false)
        setShowPreviousButton(false)
        setShowSubtitleButton(false)
        setShowShuffleButton(false)
        setShowVrButton(false)
        setShutterBackgroundColor(AndroidColor.TRANSPARENT)
        setKeepContentOnPlayerReset(true)
        layoutParams = ViewGroup.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.MATCH_PARENT)
    }

/** What the inline player is to a screen reader: the attachment's own words. */
fun describePlayer(view: PlayerView, a: AttachmentSummary) {
    view.contentDescription = attachmentLabel(a)
}
