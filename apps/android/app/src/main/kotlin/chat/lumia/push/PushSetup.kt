// Registration (architecture 15.5): once the session is signed in, the
// notification permission is asked for, then a UnifiedPush distributor is
// used when one is installed (the saved one, else the system's default
// through its picker), and FCM otherwise, when the build carries a
// Firebase project. Each path ends in PushRegistrar.subscribe.
package chat.lumia.push

import android.Manifest
import android.content.pm.PackageManager
import android.os.Build
import androidx.activity.ComponentActivity
import androidx.activity.result.contract.ActivityResultContracts
import chat.lumia.kit.PushRegistrar
import com.google.firebase.messaging.FirebaseMessaging
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import org.unifiedpush.android.connector.UnifiedPush

class PushSetup(private val activity: ComponentActivity) {
    private val scope = CoroutineScope(Dispatchers.IO)
    private val permission = activity.registerForActivityResult(ActivityResultContracts.RequestPermission()) { registerTransport() }
    private var registered = false

    /** Asks for permission (once) and registers a transport; best effort. */
    fun register() {
        if (registered) return
        registered = true
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
