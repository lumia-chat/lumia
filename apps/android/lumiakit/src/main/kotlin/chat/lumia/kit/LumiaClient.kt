// The client as the app uses it: the UniFFI `Client` (blocking calls, a
// listener called from a runtime thread) behind suspend functions on the
// IO dispatcher, with the listener turned into a Flow of events. One per
// instance. The screens hold a `LumiaSession` rather than this directly.
package chat.lumia.kit

import java.io.File
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.channels.awaitClose
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.callbackFlow
import kotlinx.coroutines.withContext
import uniffi.lumia_ffi.Client
import uniffi.lumia_ffi.ClientEvent
import uniffi.lumia_ffi.Listener
import uniffi.lumia_ffi.LumiaException

typealias LumiaEvent = ClientEvent

/** One instance's client, off the main thread. */
class LumiaClient(val baseUrl: String, dataDirectory: File) {
    private val inner: Client = Client(baseUrl, dataDirectory.absolutePath)

    val host: String get() = inner.host()
    val deviceId: String get() = inner.deviceId()
    val signedIn: Boolean get() = inner.signedIn()
    val hasIdentity: Boolean get() = inner.hasIdentity()
    val dmAvailable: Boolean get() = inner.dmAvailable()

    /** Runs one blocking call of the client off the main thread. */
    suspend fun <T> call(block: Client.() -> T): T = withContext(Dispatchers.IO) { inner.block() }

    /**
     * One of the client's calls that are values only, reaching nothing and
     * waiting on nothing (the rail's `rail…` edits and words, a draft's
     * `messageRoom`), where it is asked: a screen needs the answer as it
     * draws.
     */
    fun <T> now(block: Client.() -> T): T = inner.block()

    /**
     * For a push receiver: what the notification should say, after syncing
     * and decrypting under the exclusive writer lock (tried for `timeoutMs`);
     * null when the app holds the lock or nothing new was found.
     */
    suspend fun notificationContent(timeoutMs: ULong) = call { notificationContent(timeoutMs) }

    /** Starts the gateway loop; the flow carries every event in order until the loop closes or `stop()`. */
    fun start(channelIds: List<String>): Flow<LumiaEvent> = callbackFlow {
        inner.start(channelIds, object : Listener {
            override fun onEvent(event: ClientEvent) {
                trySend(event)
                if (event is ClientEvent.Closed) close()
            }
        })
        awaitClose { }
    }

    fun stop() = inner.stop()
}

/** The API error's code, when the failure was one (`UNAUTHENTICATED`, `NOT_FOUND`, ...). */
val Throwable.lumiaCode: String?
    get() = (this as? LumiaException.Api)?.code

/** The API error's `details`, as the protocol documents them per code (`mfa`, `state`, ...). */
val Throwable.lumiaDetails: Map<String, String>
    get() = (this as? LumiaException.Api)?.details ?: emptyMap()

/**
 * Whether the failure says this session is over (`sessionOver`): the
 * instance's refusal by its code, or the core saying so itself, with no
 * tokens left to send or this device revoked in the roster it holds.
 */
val Throwable.endsSession: Boolean
    get() = when (this) {
        is LumiaException.Api -> sessionOver(code)
        is LumiaException.DeviceRevoked, is LumiaException.NotSignedIn -> true
        else -> false
    }

/** What to show for the failure. */
val Throwable.lumiaMessage: String
    get() = when (this) {
        is LumiaException.Api -> detail
        is LumiaException.DeviceRevoked -> "This device was revoked (roster v$rosterVersion)."
        is LumiaException.NotSignedIn -> "Not signed in."
        is LumiaException.Other -> detail
        else -> message ?: toString()
    }
