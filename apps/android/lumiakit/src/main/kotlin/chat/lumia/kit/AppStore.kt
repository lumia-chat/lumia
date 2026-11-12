// The app's one session and where the app is: the saved instance reopened
// at launch (state under the app's files directory), and the route the
// session's state picks. And what Preferences chose for this phone, which
// outlasts the session. The Swift AppStore's twin.
package chat.lumia.kit

import android.content.Context
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import java.io.File
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.launch

/** What the app shows. */
enum class Route { SIGN_IN, LINK, APP }

class AppStore(context: Context) {
    companion object {
        /** The activity's store while one exists, for the push receivers. */
        var current: AppStore? = null

        fun savedInstance(context: Context): String? =
            context.getSharedPreferences("lumia", Context.MODE_PRIVATE).getString("instance", null)

        fun dataDirectory(context: Context): File = File(context.filesDir, "lumia")
    }

    private val context = context.applicationContext
    private val prefs = context.getSharedPreferences("lumia", Context.MODE_PRIVATE)
    /** The store's own: what is begun in it is not cut short by the screen that asked going away. */
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate)
    val dataDirectory: File = dataDirectory(context)

    var session by mutableStateOf<LumiaSession?>(null)
        private set
    var route by mutableStateOf(Route.SIGN_IN)
    var opening by mutableStateOf(true)
        private set
    var openError by mutableStateOf<String?>(null)
        private set
    /** The `lumia:` URL the system browser came back on, for the screen that sent it (architecture 6.3). */
    var browserReturn by mutableStateOf<String?>(null)
    /** Whether the activity is resumed: then the app shows messages itself. */
    var foregrounded = false
    /** Whether a sign-out the person chose is under way: the account menu is not offered on a session that is going. */
    var signingOut by mutableStateOf(false)
        private set

    init {
        current = this
    }

    var savedInstance: String?
        get() = prefs.getString("instance", null)
        set(value) { prefs.edit().putString("instance", value).apply() }

    /**
     * Preferences' two choices: how Lumia looks on this phone. They are the
     * phone's and not the account's, kept beside the saved instance under
     * the names the desktop keeps them by and sent nowhere; signing out and
     * forgetting the instance leave them be, and a stored value this build
     * does not know reads as the default.
     */
    var appearance by mutableStateOf(Appearance.of(stored("appearance")))
        private set
    var density by mutableStateOf(Density.of(stored("density")))
        private set

    private fun stored(key: String): String? = runCatching { prefs.getString(key, null) }.getOrNull()

    /** Light, dark or as the phone is: at once, and at the next launch. */
    fun choose(appearance: Appearance) {
        this.appearance = appearance
        prefs.edit().putString("appearance", appearance.stored).apply()
    }

    /** How much room messages take: at once, and at the next launch. */
    fun choose(density: Density) {
        this.density = density
        prefs.edit().putString("density", density.stored).apply()
    }

    /**
     * A look for this launch only, which a debug build takes from the
     * launch intent's LUMIA_DEV_APPEARANCE and LUMIA_DEV_DENSITY
     * (scripts/smoke.sh draws the dark theme and the compact channel so).
     * Nothing is stored: the launch after it is as the phone chose.
     */
    fun wear(appearance: Appearance?, density: Density?) {
        if (appearance != null) this.appearance = appearance
        if (density != null) this.density = density
    }

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
        val client = LumiaClient(url, dataDirectory)
        // The phone's push subscription is kept in its preferences, which the store can reach.
        val s = LumiaSession(client) { chosen -> PushRegistrar.left(context, client, chosen) }
        session = s
        savedInstance = url
        return s
    }

    /**
     * Signs out, as the account menu's last item does: this phone's push
     * subscription is removed from the instance while the session can still
     * ask, the session is revoked, its tokens and what was held of the
     * account are forgotten (`LumiaSession.signOut`), and the sign-in
     * screen is next, with no line: the person chose this. The saved
     * instance stays, Preferences' choices stay, being the phone's, and so
     * do the identity and this device's keys.
     *
     * Returns at once and happens in the store's own scope, so the menu
     * closing or the shell going cannot stop it half-way. Once: nothing for
     * a second tap while it is under way, or with no session signed in.
     */
    fun signOut() {
        val s = session ?: return
        if (signingOut || !s.signedIn) return
        signingOut = true
        scope.launch {
            try {
                s.signOut()
                if (session === s) route = Route.SIGN_IN
            } finally {
                signingOut = false
            }
        }
    }

    fun forgetInstance() {
        session?.stop()
        session = null
        savedInstance = null
        route = Route.SIGN_IN
    }
}
