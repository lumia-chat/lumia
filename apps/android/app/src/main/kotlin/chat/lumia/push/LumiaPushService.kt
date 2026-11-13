// UnifiedPush (architecture 15.5): the distributor the person installed
// (ntfy and the like) hands this app an endpoint, which goes to the
// instance as the `unified_push` subscription, and delivers each push the
// instance posts there, the generic notice that PushRegistrar turns into
// the message.
package chat.lumia.push

import chat.lumia.kit.PushRegistrar
import chat.lumia.kit.pushBadge
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import org.json.JSONObject
import org.unifiedpush.android.connector.FailedReason
import org.unifiedpush.android.connector.PushService
import org.unifiedpush.android.connector.data.PushEndpoint
import org.unifiedpush.android.connector.data.PushMessage

class LumiaPushService : PushService() {
    private val scope = CoroutineScope(Dispatchers.IO)

    override fun onNewEndpoint(endpoint: PushEndpoint, instance: String) {
        val context = applicationContext
        // The keys the connector made for this endpoint: the instance
        // encrypts the notice under them (Web Push, RFC 8291), so the
        // distributor and its server carry ciphertext.
        val keys = endpoint.pubKeySet
        scope.launch {
            PushRegistrar.subscribe(context, "unified_push", endpoint.url, keys?.pubKey ?: "", keys?.auth ?: "")
        }
    }

    override fun onMessage(message: PushMessage, instance: String) {
        val context = applicationContext
        val notice = runCatching { JSONObject(String(message.content)) }.getOrNull()
        val host = notice?.optString("instance")
        // Every notice carries the account's badge; a read (`"kind": "read"`) carries only that, and shows nothing.
        val badge = pushBadge(notice?.opt("badge"))
        scope.launch {
            if (notice?.optString("kind") == "read") PushRegistrar.readElsewhere(context, host, badge)
            else PushRegistrar.received(context, host, badge)
        }
    }

    override fun onRegistrationFailed(reason: FailedReason, instance: String) {
        // No distributor answered: FCM, when the build has it, is the fallback
        // PushSetup tries next time.
    }

    override fun onUnregistered(instance: String) {
        // The distributor dropped the registration by itself (it was removed,
        // or the person took this app off it); the instance's subscription
        // answers 410 on the next push and is forgotten there. Leaving a
        // session unregisters from this side (`PushSetup.unregister`), and
        // the instance was told or has dropped the subscription already.
    }
}
