// The core's TLS on Android: its HTTPS client trusts the system's
// certificates through rustls-platform-verifier, which asks Android's trust
// manager over JNI (its Kotlin component is lumiakit's dependency) and must
// be handed the application's Context once, before the first request;
// without it every HTTPS request fails. LumiaApp calls `initialize` at the
// process's start, before any client is made (a push received with the app
// away starts the process through the application too).
package chat.lumia.kit

import android.content.Context

object Tls {
    init {
        // UniFFI's bindings open the core through JNA, which the JVM does
        // not search for a native method; this loads the same library for
        // `initialize` (lumia-ffi's src/android.rs).
        System.loadLibrary("lumia_ffi")
    }

    /**
     * Hands the core's verifier the application's Context (an Application,
     * which lives as long as the process). Once is enough; later calls
     * change nothing. Throws a RuntimeException if the JVM refuses.
     */
    external fun initialize(context: Context)
}
