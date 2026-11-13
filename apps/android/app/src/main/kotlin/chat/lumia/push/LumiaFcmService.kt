// FCM through the relay (architecture 15.5): the token goes to the
// instance as the `fcm` subscription (kept as `relay_fcm` there), and a
// data message from the relay names the instance to sync. Live only in a
// build with the Firebase project's google-services.json.
package chat.lumia.push

import chat.lumia.kit.PushRegistrar
import chat.lumia.kit.pushBadge
import com.google.firebase.messaging.FirebaseMessagingService
import com.google.firebase.messaging.RemoteMessage
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch

class LumiaFcmService : FirebaseMessagingService() {
    private val scope = CoroutineScope(Dispatchers.IO)

    override fun onNewToken(token: String) {
        val context = applicationContext
        scope.launch { PushRegistrar.subscribe(context, "fcm", token) }
    }

    override fun onMessageReceived(message: RemoteMessage) {
        val context = applicationContext
        val host = message.data["instance"]
        // Every notice carries the account's badge; a read (`kind=read`) carries only that, and shows nothing.
        val badge = pushBadge(message.data["badge"])
        scope.launch {
            if (message.data["kind"] == "read") PushRegistrar.readElsewhere(context, host, badge)
            else PushRegistrar.received(context, host, badge)
        }
    }
}
