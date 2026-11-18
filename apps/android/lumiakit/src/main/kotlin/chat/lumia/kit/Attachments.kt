// What the platform does for attachments (specs/media.md 3 and 5), the
// rest being the core's: reading a chosen file, decoding a still picture
// upright with ImageDecoder (HEIC and HEIF too, which the core cannot)
// into RGBA8 pixels beside its bytes, and a video's poster frame, length
// and turned size with MediaMetadataRetriever; and the other way, drawing
// what a message carries: its placeholder (the core's ThumbHash as a few
// pixels), its thumbnail and its picture once the core has fetched and
// opened them, each checked by `attachmentDrawable` before any decoder
// sees bytes someone else chose, and kept for the process by blob.
package chat.lumia.kit

import android.content.Context
import android.graphics.Bitmap
import android.graphics.BitmapFactory
import android.graphics.ImageDecoder
import android.graphics.Matrix
import android.graphics.drawable.AnimatedImageDrawable
import android.graphics.drawable.Drawable
import android.media.MediaMetadataRetriever
import android.net.Uri
import android.provider.OpenableColumns
import android.provider.Settings
import android.util.LruCache
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.graphics.asImageBitmap
import java.nio.ByteBuffer
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import uniffi.lumia_ffi.AttachmentInput
import uniffi.lumia_ffi.BlobSummary
import uniffi.lumia_ffi.CardSummary
import uniffi.lumia_ffi.Pixels
import uniffi.lumia_ffi.attachmentDrawable
import uniffi.lumia_ffi.placeholderImage

/** The longest side a picture is handed to the core at: its display picture's (spec 3.1), so nothing larger crosses into it. */
private const val DISPLAY_EDGE = 2560

/** A video's poster frame's longest side: the core makes a thumbnail of at most 1,080 px of it. */
private const val POSTER_EDGE = 1280

/** The longest side the viewer decodes a picture at. */
private const val VIEWER_EDGE = 4096

/** The most of a file read into memory to attach: past the instance's file cap the core refuses it anyway, and a picture's original may be larger than what is sent of it. */
private const val READ_MAX = 64 * 1024 * 1024

/** Whether the system asks for no motion (Settings' "Remove animations", the animator duration scale at 0): a GIF then shows its first frame. */
fun animationsOff(context: Context): Boolean =
    Settings.Global.getFloat(context.contentResolver, Settings.Global.ANIMATOR_DURATION_SCALE, 1f) == 0f

/** What the system says of a chosen file before reading it: its name, size and type. */
data class Chosen(val uri: Uri, val name: String, val sizeBytes: Long, val mediaType: String)

fun describe(context: Context, uri: Uri): Chosen {
    var name = uri.lastPathSegment ?: "file"
    var size = -1L
    runCatching {
        context.contentResolver.query(uri, arrayOf(OpenableColumns.DISPLAY_NAME, OpenableColumns.SIZE), null, null, null)?.use { c ->
            if (c.moveToFirst()) {
                c.getColumnIndex(OpenableColumns.DISPLAY_NAME).takeIf { it >= 0 && !c.isNull(it) }?.let { name = c.getString(it) }
                c.getColumnIndex(OpenableColumns.SIZE).takeIf { it >= 0 && !c.isNull(it) }?.let { size = c.getLong(it) }
            }
        }
    }
    val type = context.contentResolver.getType(uri) ?: "application/octet-stream"
    return Chosen(uri, name, size, type)
}

/**
 * The file [chosen] made ready for the core's `attach`: its bytes, and
 * for a still picture (not a GIF, which the core keeps as it is) the
 * picture decoded upright as RGBA8, at most the display picture's size;
 * for a video its poster frame, length and size as it is shown. [asFile]
 * sends it as a file: a picture keeps its pixels and loses only its
 * metadata. Off the main thread; throws when the file cannot be read.
 */
suspend fun attachmentInput(context: Context, chosen: Chosen, asFile: Boolean): AttachmentInput = withContext(Dispatchers.IO) {
    val bytes = context.contentResolver.openInputStream(chosen.uri)?.use { readAtMost(it, READ_MAX) }
        ?: throw IllegalStateException("it is larger than this phone sends")
    val type = chosen.mediaType
    var input = AttachmentInput(bytes = bytes, name = chosen.name, asFile = asFile)
    if (!asFile && type.startsWith("image/") && type != "image/gif") {
        stillPixels(bytes)?.let { input = input.copy(pixels = it) }
    }
    if (!asFile && type.startsWith("video/")) input = withVideo(context, chosen.uri, input)
    if (type.startsWith("audio/")) lengthOf(context, chosen.uri)?.let { input = input.copy(durationMs = it) }
    input
}

/** A still picture decoded by the platform, upright, unpremultiplied RGBA8, its longest side at most [DISPLAY_EDGE]; none when the platform cannot decode it (the core then sends it as it can). */
private fun stillPixels(bytes: ByteArray): Pixels? = runCatching {
    val source = ImageDecoder.createSource(ByteBuffer.wrap(bytes))
    val bitmap = ImageDecoder.decodeBitmap(source) { decoder, info, _ ->
        decoder.allocator = ImageDecoder.ALLOCATOR_SOFTWARE
        decoder.isUnpremultipliedRequired = true
        val w = info.size.width
        val h = info.size.height
        val longest = maxOf(w, h)
        if (longest > DISPLAY_EDGE) decoder.setTargetSize((w.toLong() * DISPLAY_EDGE / longest).toInt().coerceAtLeast(1), (h.toLong() * DISPLAY_EDGE / longest).toInt().coerceAtLeast(1))
    }
    pixelsOf(bitmap)
}.getOrNull()

/** A bitmap's pixels as the core takes them: RGBA8, row by row. */
private fun pixelsOf(bitmap: Bitmap): Pixels {
    val rgba = if (bitmap.config == Bitmap.Config.ARGB_8888) bitmap else bitmap.copy(Bitmap.Config.ARGB_8888, false)
    val buffer = ByteBuffer.allocate(rgba.byteCount)
    rgba.copyPixelsToBuffer(buffer)
    return Pixels(buffer.array(), rgba.width.toUInt(), rgba.height.toUInt())
}

/** A video's poster frame (the first, turned as it is shown), its length, and its size as it is shown. */
private fun withVideo(context: Context, uri: Uri, input: AttachmentInput): AttachmentInput {
    val retriever = MediaMetadataRetriever()
    return try {
        retriever.setDataSource(context, uri)
        val rotation = retriever.extractMetadata(MediaMetadataRetriever.METADATA_KEY_VIDEO_ROTATION)?.toIntOrNull() ?: 0
        val w = retriever.extractMetadata(MediaMetadataRetriever.METADATA_KEY_VIDEO_WIDTH)?.toIntOrNull() ?: 0
        val h = retriever.extractMetadata(MediaMetadataRetriever.METADATA_KEY_VIDEO_HEIGHT)?.toIntOrNull() ?: 0
        val turned = rotation == 90 || rotation == 270
        val shownW = if (turned) h else w
        val shownH = if (turned) w else h
        val length = retriever.extractMetadata(MediaMetadataRetriever.METADATA_KEY_DURATION)?.toLongOrNull()
        val frame = runCatching { retriever.getScaledFrameAtTime(0, MediaMetadataRetriever.OPTION_CLOSEST_SYNC, POSTER_EDGE, POSTER_EDGE) }.getOrNull()
        // Some decoders hand back the frame as stored: turned here to stand as it is shown.
        val poster = frame?.let { f ->
            if (shownW > 0 && shownH > 0 && (f.width > f.height) != (shownW > shownH) && rotation != 0) {
                Bitmap.createBitmap(f, 0, 0, f.width, f.height, Matrix().apply { postRotate(rotation.toFloat()) }, true)
            } else f
        }
        input.copy(
            poster = poster?.let { pixelsOf(it) },
            durationMs = length?.coerceIn(0, UInt.MAX_VALUE.toLong())?.toUInt(),
            videoWidth = shownW.coerceAtLeast(0).toUInt(),
            videoHeight = shownH.coerceAtLeast(0).toUInt(),
        )
    } catch (e: Exception) {
        input
    } finally {
        runCatching { retriever.release() }
    }
}

private fun lengthOf(context: Context, uri: Uri): UInt? {
    val retriever = MediaMetadataRetriever()
    return try {
        retriever.setDataSource(context, uri)
        retriever.extractMetadata(MediaMetadataRetriever.METADATA_KEY_DURATION)?.toLongOrNull()?.coerceIn(0, UInt.MAX_VALUE.toLong())?.toUInt()
    } catch (e: Exception) {
        null
    } finally {
        runCatching { retriever.release() }
    }
}

// Drawing what a message carries

/** Pictures decoded, by blob id or card address, up to 64 MB of them (a dozen display pictures at [TILE_EDGE]); placeholders by their bytes. */
private object MediaPictures {
    val held = object : LruCache<String, ImageBitmap>(64 * 1024 * 1024) {
        override fun sizeOf(key: String, value: ImageBitmap): Int = value.width * value.height * 4
    }
}

/** Every picture of a message decoded, forgotten: with the device's erasure, as the avatars are. */
fun forgetMediaPictures() = MediaPictures.held.evictAll()

/** A placeholder (ThumbHash) as the few pixels it stands for, scaled up where it is drawn; none for an empty or malformed one. */
fun placeholderPicture(placeholder: ByteArray): ImageBitmap? {
    if (placeholder.isEmpty()) return null
    val key = "placeholder:" + placeholder.joinToString("") { "%02x".format(it) }
    MediaPictures.held.get(key)?.let { return it }
    val pixels = runCatching { placeholderImage(placeholder) }.getOrNull() ?: return null
    val bitmap = Bitmap.createBitmap(pixels.width.toInt(), pixels.height.toInt(), Bitmap.Config.ARGB_8888)
    bitmap.copyPixelsFromBuffer(ByteBuffer.wrap(pixels.rgba))
    return bitmap.asImageBitmap().also { MediaPictures.held.put(key, it) }
}

/** Bytes someone sent decoded as a picture, its longest side at most [edge]; none when they are not within the bounds (`attachmentDrawable`) or will not decode. */
fun decodePicture(bytes: ByteArray, edge: Int = VIEWER_EDGE): ImageBitmap? {
    if (!runCatching { attachmentDrawable(bytes) }.getOrDefault(false)) return null
    return runCatching {
        val bounds = BitmapFactory.Options().apply { inJustDecodeBounds = true }
        BitmapFactory.decodeByteArray(bytes, 0, bytes.size, bounds)
        var sample = 1
        while (maxOf(bounds.outWidth, bounds.outHeight) / sample > edge) sample *= 2
        BitmapFactory.decodeByteArray(bytes, 0, bytes.size, BitmapFactory.Options().apply { inSampleSize = sample })?.asImageBitmap()
    }.getOrNull()
}

/** A GIF someone sent as a drawable that animates, once its bounds are checked; none when it will not decode. */
fun decodeAnimated(bytes: ByteArray): Drawable? {
    if (!runCatching { attachmentDrawable(bytes) }.getOrDefault(false)) return null
    return runCatching { ImageDecoder.decodeDrawable(ImageDecoder.createSource(ByteBuffer.wrap(bytes))) }.getOrNull()
        ?.also { d -> (d as? AnimatedImageDrawable)?.repeatCount = AnimatedImageDrawable.REPEAT_INFINITE }
}

/** The state of a picture asked for: on its way, here, or not to be had. */
sealed class Fetched<out T> {
    data object Waiting : Fetched<Nothing>()
    data class Here<T>(val value: T) : Fetched<T>()
    data object Failed : Fetched<Nothing>()
}

/** An attachment's blob as a picture (a message's still, a personal card's picture): fetched and opened by the core, decoded once and kept by blob id. */
@Composable
fun rememberBlobPicture(session: LumiaSession, blob: BlobSummary?, edge: Int = TILE_EDGE): Fetched<ImageBitmap> {
    val key = blob?.let { "blob:${it.blobId}:$edge" }
    val state = remember(key) { mutableStateOf<Fetched<ImageBitmap>>(key?.let { MediaPictures.held.get(it) }?.let { Fetched.Here(it) } ?: Fetched.Waiting) }
    LaunchedEffect(key) {
        if (blob == null) { state.value = Fetched.Failed; return@LaunchedEffect }
        if (state.value is Fetched.Here) return@LaunchedEffect
        val picture = runCatching { session.attachmentBytes(blob) }.getOrNull()?.let { withContext(Dispatchers.Default) { decodePicture(it, edge) } }
        state.value = picture?.let { MediaPictures.held.put(key!!, it); Fetched.Here(it) } ?: Fetched.Failed
    }
    return state.value
}

/** A card's picture: the instance's by its address for a server card, the blob for a personal one. */
@Composable
fun rememberCardPicture(session: LumiaSession, card: CardSummary): Fetched<ImageBitmap> {
    val image = card.image
    if (image != null) return rememberBlobPicture(session, image, 800)
    val key = card.imageUrl.takeIf { it.isNotEmpty() }?.let { "card:$it" }
    val state = remember(key) { mutableStateOf<Fetched<ImageBitmap>>(key?.let { MediaPictures.held.get(it) }?.let { Fetched.Here(it) } ?: Fetched.Waiting) }
    LaunchedEffect(key) {
        if (key == null) { state.value = Fetched.Failed; return@LaunchedEffect }
        if (state.value is Fetched.Here) return@LaunchedEffect
        val picture = runCatching { session.cardImage(card.imageUrl) }.getOrNull()?.let { withContext(Dispatchers.Default) { decodePicture(it, 800) } }
        state.value = picture?.let { MediaPictures.held.put(key, it); Fetched.Here(it) } ?: Fetched.Failed
    }
    return state.value
}

/** An attachment's whole blob, as bytes, for the viewer, a player, Save and Share; fetched once a screen asks. */
@Composable
fun rememberBlobBytes(session: LumiaSession, blob: BlobSummary?): Fetched<ByteArray> {
    val state = remember(blob?.blobId) { mutableStateOf<Fetched<ByteArray>>(Fetched.Waiting) }
    LaunchedEffect(blob?.blobId) {
        if (blob == null) { state.value = Fetched.Failed; return@LaunchedEffect }
        state.value = runCatching { session.attachmentBytes(blob) }.getOrNull()?.let { Fetched.Here(it) } ?: Fetched.Failed
    }
    return state.value
}
