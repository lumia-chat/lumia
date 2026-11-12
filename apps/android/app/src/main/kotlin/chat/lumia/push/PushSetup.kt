// Registration (architecture 15.5): once the session is signed in, the
// notification permission is asked for, then a UnifiedPush distributor is
// used when one is installed (the saved one, else the system's default
// through its picker), and FCM otherwise, when the build carries a
// Firebase project. Each path ends in PushRegistrar.subscribe. Once per
// sign-in, and given up when the session is left (`unregister`): the
// phone unregisters from the distributor and deletes FCM's token, so the
// next sign-in registers afresh and is handed an endpoint or token of its
// own for PushRegistrar to send.
package chat.lumia.push

import android.Manifest
import android.content.Context
import android.content.pm.PackageManager
import android.os.Build
import androidx.activity.ComponentActivity
import androidx.activity.result.contract.ActivityResultContracts
import chat.lumia.kit.PushRegistrar
import com.google.firebase.messaging.FirebaseMessaging
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import org.unifiedpush.android.connector.UnifiedPush

/** The activity's part: the permission's question needs one, and so does the distributor's picker. */
class PushSetup(private val activity: ComponentActivity) {
    companion object {
        private val scope = CoroutineScope(Dispatchers.IO)

        /**
         * Whether the sign-in the process holds has registered. The
         * process's and not the activity's: an activity made again (the
         * phone turned) does not register a second time, and a session
         * left with no activity there is still one the next registers after.
         */
        @Volatile
        private var registered = false

        /**
         * An endpoint a debug launch names (LUMIA_DEV_PUSH), standing in
         * for the distributor: registering hands it over as a distributor
         * would its own, with no permission asked and no distributor or
         * FCM approached, so scripts/smoke.sh can listen where the
         * instance posts and see each sign-in subscribe.
         */
        @Volatile
        var standIn: String? = null

        /**
         * The session was left, signed out of or ended by the instance:
         * the phone's own registrations go with it (`PushRegistrar.left`
         * calls this, through `PushRegistrar.unregister`). The distributor
         * is told to drop this app's registration, which also forgets the
         * distributor chosen and the keys made for the endpoint, and FCM's
         * token is deleted where the build has FCM (without a Firebase
         * project the call throws, as the token call does). Each quietly:
         * neither failing stops the leaving, and the instance has dropped
         * or will refuse the subscription either way. The next sign-in
         * registers again.
         */
        suspend fun unregister(context: Context) {
            registered = false
            withContext(Dispatchers.IO) {
                runCatching { UnifiedPush.unregister(context) }
                runCatching { FirebaseMessaging.getInstance().deleteToken() }
            }
        }
    }

    private val permission = activity.registerForActivityResult(ActivityResultContracts.RequestPermission()) { registerTransport() }

    /** Asks for permission (once per sign-in) and registers a transport; best effort. */
    fun register() {
        if (registered) return
        registered = true
        val context = activity.applicationContext
        standIn?.let { endpoint ->
            scope.launch { PushRegistrar.subscribe(context, "unified_push", endpoint) }
            return
        }
        if (Build.VERSION.SDK_INT >= 33 &&
            activity.checkSelfPermission(Manifest.permission.POST_NOTIFICATIONS) != PackageManager.PERMISSION_GRANTED
        ) {
            permission.launch(Manifest.permission.POST_NOTIFICATIONS)
        } else {
            registerTransport()
        }
    }

    private fun registerTransport() {
        if (UnifiedPush.getDistributors(activity).isNotEmpty()) {
            UnifiedPush.tryUseCurrentOrDefaultDistributor(activity) { chosen ->
                if (chosen) UnifiedPush.register(activity) else registerFcm()
            }
        } else {
            registerFcm()
        }
    }

    private fun registerFcm() {
        val context = activity.applicationContext
        // Without google-services.json there is no Firebase app, and the
        // token call throws: no transport then, and messages arrive when
        // the app next opens.
        runCatching {
            FirebaseMessaging.getInstance().token.addOnSuccessListener { token ->
                scope.launch { PushRegistrar.subscribe(context, "fcm", token) }
            }
        }
    }
}
