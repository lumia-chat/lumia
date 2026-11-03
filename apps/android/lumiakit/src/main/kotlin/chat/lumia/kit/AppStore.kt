// The app's one session and where the app is: the saved instance reopened
// at launch (state under the app's files directory), and the route the
// session's state picks. The Swift AppStore's twin.
package chat.lumia.kit

import android.content.Context
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import java.io.File

/** What the app shows. */
enum class Route { SIGN_IN, LINK, APP }

class AppStore(context: Context) {
    private val prefs = context.getSharedPreferences("lumia", Context.MODE_PRIVATE)
    val dataDirectory: File = File(context.filesDir, "lumia")

    var session by mutableStateOf<LumiaSession?>(null)
        private set
    var route by mutableStateOf(Route.SIGN_IN)
    var opening by mutableStateOf(true)
        private set
    var openError by mutableStateOf<String?>(null)
        private set

    var savedInstance: String?
        get() = prefs.getString("instance", null)
        set(value) { prefs.edit().putString("instance", value).apply() }

    /**
     * Reopens the saved instance at launch, straight into the app when signed
     * in. A debug build signs in from `dev` (the activity's intent extras
     * LUMIA_DEV_INSTANCE, _HANDLE, _PASSWORD, for scripts/smoke.sh).
     */
    suspend fun reopen(dev: Map<String, String> = emptyMap()) {
        try {
            val instance = dev["LUMIA_DEV_INSTANCE"]
            val handle = dev["LUMIA_DEV_HANDLE"]
            val password = dev["LUMIA_DEV_PASSWORD"]
            if (instance != null && handle != null && password != null) {
                try {
                    val s = open(instance)
                    if (!s.signedIn) s.signIn(handle, password, null)
                    route = Route.APP
                } catch (e: Exception) {
                    openError = e.lumiaMessage
                }
                return
            }
            val saved = savedInstance ?: return
            try {
                val s = open(saved)
                route = if (s.signedIn) Route.APP else Route.SIGN_IN
            } catch (e: Exception) {
                openError = e.lumiaMessage
            }
        } finally {
            opening = false
        }
    }

    fun open(url: String): LumiaSession {
        session?.stop()
        dataDirectory.mkdirs()
        val s = LumiaSession(LumiaClient(url, dataDirectory))
        session = s
        savedInstance = url
        return s
    }

    suspend fun signOut() {
        session?.signOut()
        route = Route.SIGN_IN
    }

    fun forgetInstance() {
        session?.stop()
        session = null
        savedInstance = null
        route = Route.SIGN_IN
    }
}
