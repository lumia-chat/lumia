// Pictures the instance serves (a server's and a person's), fetched and
// drawn over what is drawn without one: the initials stay underneath, and
// show while the picture is on its way or when it cannot be had. Only an
// address `pictureAddress` gives is fetched. A picture's name is its
// content's hash, so a fetched one is kept for the process under its
// address and a changed picture is a new address.
package chat.lumia.kit

import android.graphics.BitmapFactory
import android.util.LruCache
import androidx.compose.foundation.Image
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.layout.ContentScale
import java.io.ByteArrayOutputStream
import java.io.InputStream
import java.net.HttpURLConnection
import java.net.URL
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Deferred
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.async
import kotlinx.coroutines.withContext
import uniffi.lumia_ffi.MemberSummary

/** The longest side a picture is decoded at: twice a rail tile on the densest screen, and the instance's square is 512. */
private const val PICTURE_SIDE = 256

/** The most a fetched picture may be; the instance's own square is far less. */
private const val PICTURE_FETCH_MAX = 4 * 1024 * 1024

/** Fetched pictures by address, up to 16 MB of them. */
private object Pictures {
    val held = object : LruCache<String, ImageBitmap>(16 * 1024 * 1024) {
        override fun sizeOf(key: String, value: ImageBitmap): Int = value.width * value.height * 4
    }

    /** Fetches under way by address, so the many avatars of one person on a screen ask once. */
    private val wanted = HashMap<String, Deferred<ImageBitmap?>>()

    /** Where fetches run: none is stopped by one of the screens waiting on it going away. */
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.IO)

    /** The picture at [address], or none when it cannot be had; one fetch however many ask at once. */
    suspend fun fetch(address: String): ImageBitmap? {
        held.get(address)?.let { return it }
        val fetching = synchronized(wanted) {
            wanted.getOrPut(address) {
                scope.async { load(address) }.also { job ->
                    job.invokeOnCompletion { synchronized(wanted) { wanted.remove(address) } }
                }
            }
        }
        return fetching.await()
    }

    /** The picture at [address] from the instance, or none when it cannot be had; nothing is said, the initials stay. */
    private suspend fun load(address: String): ImageBitmap? = withContext(Dispatchers.IO) {
        runCatching {
            val connection = URL(address).openConnection() as HttpURLConnection
            connection.connectTimeout = 10_000
            connection.readTimeout = 10_000
            try {
                if (connection.responseCode != 200) return@runCatching null
                val bytes = connection.inputStream.use { readAtMost(it, PICTURE_FETCH_MAX) } ?: return@runCatching null
                val bounds = BitmapFactory.Options().apply { inJustDecodeBounds = true }
                BitmapFactory.decodeByteArray(bytes, 0, bytes.size, bounds)
                var sample = 1
                while (maxOf(bounds.outWidth, bounds.outHeight) / (sample * 2) >= PICTURE_SIDE) sample *= 2
                BitmapFactory.decodeByteArray(bytes, 0, bytes.size, BitmapFactory.Options().apply { inSampleSize = sample })?.asImageBitmap()
            } finally {
                connection.disconnect()
            }
        }.getOrNull()?.also { held.put(address, it) }
    }
}

/**
 * The picture an account names, a path on the instance, or empty for none
 * (the desktop shell's `avatar_url_of`): this account's own ([me]) from
 * the snapshot ([ownPath]), anyone else's from the server member lists
 * ([member]), and else from the profile [lookUp] finds, which is only
 * asked for one no list holds. The path is fetched only as
 * `pictureAddress` allows.
 */
fun avatarPathOf(accountId: String, me: String, ownPath: String, member: MemberSummary?, lookUp: (String) -> MemberSummary?): String = when {
    accountId.isEmpty() -> ""
    accountId == me -> ownPath
    else -> (member ?: lookUp(accountId))?.avatarUrl ?: ""
}

/** What [input] holds, read to its end; none when that is over [max] bytes. */
fun readAtMost(input: InputStream, max: Int): ByteArray? {
    val out = ByteArrayOutputStream()
    val buffer = ByteArray(64 * 1024)
    while (true) {
        val n = input.read(buffer)
        if (n < 0) return out.toByteArray()
        out.write(buffer, 0, n)
        if (out.size() > max) return null
    }
}

/** The picture at [address] once it is here; none before, for none, or when it cannot be had. */
@Composable
fun rememberPicture(address: String?): ImageBitmap? {
    val picture = remember(address) { mutableStateOf(address?.let { Pictures.held.get(it) }) }
    LaunchedEffect(address) {
        if (address != null && picture.value == null) picture.value = Pictures.fetch(address)
    }
    return picture.value
}

/**
 * The picture at [address], filling [modifier]'s bounds and cut to them,
 * over whatever is under it; nothing until it is here. Nothing a screen
 * reader is told: what it is drawn in says what it is.
 */
@Composable
fun LumiaPicture(address: String?, modifier: Modifier = Modifier) {
    val picture = rememberPicture(address) ?: return
    Image(picture, contentDescription = null, contentScale = ContentScale.Crop, modifier = modifier)
}
