// A server's emoji drawn as their pictures (specs/emoji.md 6 and 10),
// fetched by address as people's pictures are (`Pictures.kt`, the same
// rule of whose address is fetched) and kept for the process: a still one
// (PNG) as a bitmap, an animated one (GIF) as Android's own
// `AnimatedImageDrawable`, which plays, looping, wherever it is drawn. With
// animations off in the system's settings (Developer options' animator
// scale, and Accessibility's "Remove animations", which sets it) an
// animated one is its first frame, still. A picture the server no longer
// lists is dropped (`forgetEmojiPictures`), and every one at sign-out.
package chat.lumia.kit

import android.graphics.BitmapFactory
import android.graphics.ImageDecoder
import android.graphics.drawable.AnimatedImageDrawable
import android.graphics.drawable.Drawable
import android.os.Handler
import android.os.Looper
import android.util.LruCache
import androidx.compose.foundation.Image
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.MutableIntState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.graphics.drawscope.DrawScope
import androidx.compose.ui.graphics.drawscope.drawIntoCanvas
import androidx.compose.ui.graphics.nativeCanvas
import androidx.compose.ui.graphics.painter.BitmapPainter
import androidx.compose.ui.graphics.painter.Painter
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.platform.LocalContext
import java.net.HttpURLConnection
import java.net.URL
import java.nio.ByteBuffer
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Deferred
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.async
import kotlinx.coroutines.withContext

/** The most an emoji's picture may be: the instance makes none over 512 KiB. */
private const val EMOJI_FETCH_MAX = 1024 * 1024

/** One emoji's picture, ready to draw. */
private sealed class Held {
    class Still(val bitmap: ImageBitmap) : Held()

    /**
     * An animated one: the drawable, at its own size for good (each place
     * scales the canvas to its own), and what tells Compose a new frame is
     * due. A drawable holds its callback weakly, so it is kept here.
     */
    class Moving(val drawable: AnimatedImageDrawable, val frame: MutableIntState, val ticker: Ticker) : Held()
}

/** The frames of an animated picture, told to Compose and timed on the main thread. */
private class Ticker(private val frame: MutableIntState) : Drawable.Callback {
    private val handler = Handler(Looper.getMainLooper())
    override fun invalidateDrawable(who: Drawable) { frame.intValue++ }
    override fun scheduleDrawable(who: Drawable, what: Runnable, `when`: Long) { handler.postAtTime(what, who, `when`) }
    override fun unscheduleDrawable(who: Drawable, what: Runnable) { handler.removeCallbacks(what, who) }
}

private object EmojiPictures {
    /** The bytes fetched, by address: a server's emoji are few and small. */
    val bytes = object : LruCache<String, ByteArray>(8 * 1024 * 1024) {
        override fun sizeOf(key: String, value: ByteArray): Int = value.size
    }

    /** What is drawn, by address and by whether it plays. */
    val held = LruCache<String, Held>(256)

    private val wanted = HashMap<String, Deferred<ByteArray?>>()
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.IO)

    fun key(address: String, moving: Boolean) = if (moving) "moving $address" else "still $address"

    /** The picture at [address], played when [moving], or none when it cannot be had; one fetch however many ask at once. */
    suspend fun picture(address: String, moving: Boolean): Held? {
        held.get(key(address, moving))?.let { return it }
        val data = fetch(address) ?: return null
        val made = withContext(Dispatchers.Default) { runCatching { decode(data, moving) }.getOrNull() } ?: return null
        // Made on the main thread's terms: the drawable starts there.
        return withContext(Dispatchers.Main) {
            held.get(key(address, moving)) ?: made.also {
                if (it is Held.Moving) it.drawable.start()
                held.put(key(address, moving), it)
            }
        }
    }

    private fun decode(data: ByteArray, moving: Boolean): Held? {
        if (moving) {
            val drawable = ImageDecoder.decodeDrawable(ImageDecoder.createSource(ByteBuffer.wrap(data)))
            if (drawable is AnimatedImageDrawable) {
                drawable.setBounds(0, 0, drawable.intrinsicWidth.coerceAtLeast(1), drawable.intrinsicHeight.coerceAtLeast(1))
                drawable.repeatCount = AnimatedImageDrawable.REPEAT_INFINITE
                val frame = mutableIntStateOf(0)
                val ticker = Ticker(frame)
                drawable.callback = ticker
                return Held.Moving(drawable, frame, ticker)
            }
        }
        // Still, or an animated one under reduced motion: its first frame.
        return BitmapFactory.decodeByteArray(data, 0, data.size)?.asImageBitmap()?.let { Held.Still(it) }
    }

    private suspend fun fetch(address: String): ByteArray? {
        bytes.get(address)?.let { return it }
        val fetching = synchronized(wanted) {
            wanted.getOrPut(address) {
                scope.async { load(address) }.also { job -> job.invokeOnCompletion { synchronized(wanted) { wanted.remove(address) } } }
            }
        }
        return fetching.await()
    }

    private suspend fun load(address: String): ByteArray? = withContext(Dispatchers.IO) {
        runCatching {
            val connection = URL(address).openConnection() as HttpURLConnection
            connection.connectTimeout = 10_000
            connection.readTimeout = 10_000
            try {
                if (connection.responseCode != 200) return@runCatching null
                connection.inputStream.use { readAtMost(it, EMOJI_FETCH_MAX) }
            } finally {
                connection.disconnect()
            }
        }.getOrNull()?.also { bytes.put(address, it) }
    }

    fun forget(keep: (String) -> Boolean) {
        for (key in bytes.snapshot().keys) if (!keep(key)) bytes.remove(key)
        for ((key, value) in held.snapshot()) {
            if (!keep(key.substringAfter(' '))) {
                held.remove(key)
                if (value is Held.Moving) value.drawable.stop()
            }
        }
    }
}

/** Every emoji picture held but those at the addresses [keep] says to keep: what a server no longer lists goes (specs/emoji.md 7). */
fun forgetEmojiPictures(keep: (String) -> Boolean = { false }) = EmojiPictures.forget(keep)

/** An animated picture drawn at whatever size it is given: the canvas scaled, its own bounds left as they are, so one drawable serves every place. */
private class MovingPainter(private val held: Held.Moving) : Painter() {
    private val drawable = held.drawable
    override val intrinsicSize: Size = Size(drawable.intrinsicWidth.toFloat(), drawable.intrinsicHeight.toFloat())

    override fun DrawScope.onDraw() {
        // Read, so a new frame draws this again.
        held.frame.intValue
        val w = drawable.bounds.width().coerceAtLeast(1)
        val h = drawable.bounds.height().coerceAtLeast(1)
        drawIntoCanvas { canvas ->
            canvas.save()
            canvas.scale(size.width / w, size.height / h)
            drawable.draw(canvas.nativeCanvas)
            canvas.restore()
        }
    }
}

/**
 * One of a server's emoji as its picture, fitted inside [modifier]'s
 * bounds at its own shape, played when it is animated and the system plays
 * animations; nothing until it is here or when it cannot be had. Nothing
 * a screen reader is told: what it is drawn in names it (`:name:`).
 */
@Composable
fun LumiaEmoji(emoji: DrawnEmoji, modifier: Modifier = Modifier) {
    // Still where the system asks for no motion, as a GIF attachment is (`animationsOff`).
    val moving = emoji.animated && !animationsOff(LocalContext.current)
    var held by remember(emoji.address, moving) { mutableStateOf(EmojiPictures.held.get(EmojiPictures.key(emoji.address, moving))) }
    LaunchedEffect(emoji.address, moving) {
        if (held == null) held = EmojiPictures.picture(emoji.address, moving)
    }
    val painter: Painter = when (val h = held) {
        is Held.Still -> remember(h) { BitmapPainter(h.bitmap) }
        is Held.Moving -> remember(h) { MovingPainter(h) }
        null -> return
    }
    Image(painter, contentDescription = null, contentScale = ContentScale.Fit, modifier = modifier)
}
