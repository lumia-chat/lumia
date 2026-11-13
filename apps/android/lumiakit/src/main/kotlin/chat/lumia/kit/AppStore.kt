// The app's one session and where the app is: the saved instance reopened
// at launch (state under the app's files directory), and the route the
// session's state picks. And what Preferences chose for this phone, which
// outlasts the session. The Swift AppStore's twin.
//
// One for the process, held by the application and not by the activity:
// Android makes the activity again when the phone is turned (and on every
// other change of configuration the manifest does not hand it), and a
// second store would open the session a second time, with a gateway of
// its own, under whatever the first was still doing. So what a screen
// going away must not cut short is done in the store's scope: reopening,
// signing in, signing out.
package chat.lumia.kit

import android.content.Context
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import java.io.File
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.async
import kotlinx.coroutines.launch

/** What the app shows. */
enum class Route { SIGN_IN, LINK, APP }

class AppStore(context: Context) {
    companion object {
        /** The process's store, once the app has been opened in it, for the push receivers. */
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
     * Preferences' choices: how Lumia looks on this phone, and how it draws
     * a conversation's messages. They are the phone's and not the
     * account's, kept beside the saved instance under the names every
     * client keeps them by and sent nowhere; signing out and forgetting the
     * instance leave them be, and a stored value this build does not know
     * reads as the default, each setting on its own.
     */
    var appearance by mutableStateOf(Appearance.of(stored("appearance")))
        private set
    var display by mutableStateOf(Display.read(::stored))
        private set

    private fun stored(key: String): String? = runCatching { prefs.getString(key, null) }.getOrNull()

    /** Light, dark or as the phone is: at once, and at the next launch. */
    fun choose(appearance: Appearance) {
        this.appearance = appearance
        prefs.edit().putString("appearance", appearance.stored).apply()
    }

    /**
     * What this phone has reacted with, newest first, for the picker's
     * "Frequently used" (architecture 6.6): this phone's own, kept in the
     * `lumia` preferences as `recent_emoji`, one a line, and sent nowhere.
     */
    var recentEmoji by mutableStateOf(stored("recent_emoji")?.lines()?.filter { it.isNotEmpty() } ?: emptyList())
        private set

    /** An emoji reacted with: first in "Frequently used" from now on. */
    fun used(emoji: String) {
        recentEmoji = usedEmoji(recentEmoji, emoji)
        prefs.edit().putString("recent_emoji", recentEmoji.joinToString("\n")).apply()
    }

    /**
     * How messages are drawn, a setting or a preset's values: at once, and
     * at the next launch. Every setting is written, a preset being only the
     * values it wrote.
     */
    fun choose(display: Display) {
        this.display = display
        prefs.edit().apply { for ((key, value) in display.written()) putString(key, value) }.apply()
    }

    /**
     * A look for this launch only, which a debug build takes from the
     * launch intent's LUMIA_DEV_APPEARANCE, LUMIA_DEV_PRESET and
     * LUMIA_DEV_FONT_SIZE (scripts/smoke.sh draws the dark theme, the
     * channel in Minimal and its text at 20 so). Nothing is stored: the
     * launch after it is as the phone chose.
     */
    fun wear(appearance: Appearance?, preset: Preset?, fontSize: Int?) {
        if (appearance != null) this.appearance = appearance
        if (preset != null) display = display.with(preset)
        if (fontSize != null) display = display.copy(fontSize = fontSize)
    }

    /**
     * Reopens the saved instance at launch, straight into the app when signed
     * in. A debug build signs in from `dev` (the activity's intent extras
     * LUMIA_DEV_INSTANCE, _HANDLE, _PASSWORD, for scripts/smoke.sh).
     *
     * Once for the store, in its own scope: an activity made again finds
     * the session as it is, waits here for a reopening still under way,
     * and stops nothing by going away while it waits.
     */
    suspend fun reopen(dev: Map<String, String> = emptyMap()) {
        val job = reopening ?: scope.launch { reopenOnce(dev) }.also { reopening = it }
        job.join()
    }

    private var reopening: Job? = null

    private suspend fun reopenOnce(dev: Map<String, String>) {
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
        // A conversation read, here or on another device: its delivered notifications go, and the rest carry the badge.
        s.readsChanged = { states, badge -> PushRegistrar.settle(context, states, badge) }
        session = s
        savedInstance = url
        return s
    }

    /**
     * Signs in, by whichever way the screen asked ([how], on the session
     * held), and opens the app. In the store's own scope, as signing out
     * is: a sign-in the instance has answered is taken whole, the session
     * signed in and the app opened, even when the screen that asked is
     * made again meanwhile (the phone turned). A failure is the caller's
     * to show, if it is still there to show it.
     */
    suspend fun signIn(how: suspend LumiaSession.() -> Unit) {
        val s = session ?: return
        scope.async {
            s.how()
            if (session === s) route = Route.APP
        }.await()
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
