// The Android app: the Quiet Split folded for a phone (the design canvas's
// phone boards, the same screens as iOS). The saved instance is reopened
// at launch by the shared AppStore (lumiakit), and the screens are routed
// by what the session knows: no instance, not signed in, linking, or the
// app. A notification's tap arrives as the launch intent's extras (or a
// new intent, the activity being one task) and becomes the push target
// the shell opens.
package chat.lumia

import android.content.Intent
import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import chat.lumia.kit.AppStore
import chat.lumia.kit.LumiaInk
import chat.lumia.kit.LumiaTokens
import chat.lumia.kit.PushRegistrar
import chat.lumia.kit.PushTarget
import chat.lumia.kit.Route
import chat.lumia.push.PushSetup
import java.io.File

class MainActivity : ComponentActivity() {
    private lateinit var store: AppStore
    lateinit var pushSetup: PushSetup
        private set

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        enableEdgeToEdge()
        store = AppStore(applicationContext)
        pushSetup = PushSetup(this)
        PushTarget.of(intent)?.let { PushRegistrar.target = it }
        // A debug build signs in from the launch intent's extras, for the
        // smoke run (scripts/smoke.sh): LUMIA_DEV_INSTANCE, _HANDLE, _PASSWORD, _SCREEN.
        val dev = if (BuildConfig.DEBUG) {
            listOf("LUMIA_DEV_INSTANCE", "LUMIA_DEV_HANDLE", "LUMIA_DEV_PASSWORD", "LUMIA_DEV_SCREEN")
                .mapNotNull { key -> intent.getStringExtra(key)?.let { key to it } }.toMap()
        } else emptyMap()
        setContent {
            LaunchedEffect(Unit) {
                store.reopen(dev)
                if (BuildConfig.DEBUG && dev["LUMIA_DEV_SCREEN"] == "notify") notifyStandIn()
            }
            LumiaInk { Root(store, devScreen = dev["LUMIA_DEV_SCREEN"]) }
        }
    }

    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        PushTarget.of(intent)?.let { PushRegistrar.target = it }
    }

    override fun onResume() {
        super.onResume()
        if (::store.isInitialized) {
            store.foregrounded = true
            store.session?.foreground()
        }
    }

    override fun onPause() {
        super.onPause()
        if (::store.isInitialized) {
            store.foregrounded = false
            store.session?.background()
        }
    }

    /**
     * The smoke's stand-in for a push received with the app away: the same
     * call under the same lock as the receivers make, the notification
     * posted, its content written to notification.txt in the data directory.
     */
    private suspend fun notifyStandIn() {
        store.session?.background()
        store.foregrounded = false
        val content = PushRegistrar.received(applicationContext, null)
        val text = content?.let { "${it.title}\n${it.body}\n${it.channelId}\n${it.kind}\n" } ?: "none\n"
        File(store.dataDirectory, "notification.txt").writeText(text)
    }
}

@Composable
fun Root(store: AppStore, devScreen: String?) {
    Box(Modifier.fillMaxSize().background(LumiaTokens.bg.color)) {
        val session = store.session
        when {
            store.opening -> CircularProgressIndicator(Modifier.align(Alignment.Center), color = LumiaTokens.accent.color)
            devScreen == "signin" -> SignInScreen(store)
            devScreen == "link" && session != null -> LinkScreen(store, session)
            store.route == Route.APP && session != null && session.signedIn -> Shell(store, session, devScreen)
            store.route == Route.LINK && session != null -> LinkScreen(store, session)
            else -> SignInScreen(store)
        }
    }
}
