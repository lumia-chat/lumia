// The Android app (architecture section 13): lumiakit, the library the
// screens share (the generated Kotlin bindings over the core, the client,
// the session, tokens and components), and app, the Compose layout.
plugins {
    alias(libs.plugins.android.application) apply false
    alias(libs.plugins.android.library) apply false
    alias(libs.plugins.kotlin.android) apply false
    alias(libs.plugins.kotlin.compose) apply false
}
