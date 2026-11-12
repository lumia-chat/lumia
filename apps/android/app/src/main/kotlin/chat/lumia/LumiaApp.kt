// The application: what outlives a screen. Android makes the activity
// again when the phone is turned, the window resized, the font scaled or
// the language changed, and the app's one session must not be made again
// with it: the store is the process's, made when the first activity asks
// for it and kept until the process ends. The activity keeps what is an
// activity's (the window, the permission's question, the intent it was
// launched with).
package chat.lumia

import android.app.Application
import chat.lumia.kit.AppStore
import chat.lumia.kit.PushRegistrar
import chat.lumia.push.PushSetup

class LumiaApp : Application() {
    /** The one store, and through it the one session. Not made for a push received with the app away, which opens a client of its own. */
    val store: AppStore by lazy { AppStore(this) }

    override fun onCreate() {
        super.onCreate()
        // Leaving a session gives up the phone's own push registrations, which only the app's libraries can.
        PushRegistrar.unregister = PushSetup::unregister
    }
}
