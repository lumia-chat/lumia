// The library the screens share: the generated Kotlin bindings over the
// core (uniffi/, scripts/core.sh), the native library per ABI (jniLibs/,
// the same script), the client and the session, the tokens (generated)
// and the small components.
plugins {
    alias(libs.plugins.android.library)
    alias(libs.plugins.kotlin.android)
    alias(libs.plugins.kotlin.compose)
}

android {
    namespace = "chat.lumia.kit"
    compileSdk = 35
    defaultConfig {
        minSdk = 28
    }
    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }
    kotlinOptions {
        jvmTarget = "17"
    }
    buildFeatures {
        compose = true
    }
    testOptions {
        // A unit test that asks the core (ViewerTest) loads it through JNA,
        // which looks here for the library scripts/core.sh built for this
        // machine: jniLibs holds Android's only.
        unitTests.all { it.systemProperty("jna.library.path", rootProject.file(".build/host").absolutePath) }
    }
}

// The core's TLS verifier (rustls-platform-verifier, Tls.kt) calls a Kotlin
// component of its own, which must be the version its crate expects: the
// version of `rustls-platform-verifier-android` in the core's Cargo.lock,
// read at configuration (and again when the lock changes), as the crate's
// README has it. The repository is settings.gradle.kts's.
abstract class RustlsVersion : ValueSource<String, RustlsVersion.Params> {
    interface Params : ValueSourceParameters {
        val lockFile: RegularFileProperty
    }

    companion object {
        const val CRATE_NAME = "rustls-platform-verifier-android"
    }

    override fun obtain(): String {
        val lines = parameters.lockFile.get().asFile.readLines()
        val at = lines.indexOfFirst { it.trim() == "name = \"$CRATE_NAME\"" }
        val version = if (at < 0) null else lines.drop(at + 1)
            .firstOrNull { it.trimStart().startsWith("version = ") }
            ?.substringAfter('"', "")
            ?.substringBefore('"', "")
            ?.takeIf { it.isNotEmpty() }
        return version ?: error("$CRATE_NAME not found in Cargo.lock")
    }
}

val rustlsPlatformVerifierVersion = providers.of(RustlsVersion::class.java) {
    parameters.lockFile.set(layout.projectDirectory.file("../../core/Cargo.lock"))
}

dependencies {
    // UniFFI's Kotlin bindings call the native library through JNA.
    api(libs.jna) { artifact { type = "aar" } }
    // The TLS verifier's Kotlin component at the Cargo.lock's version (above),
    // named with it here rather than by a resolution rule, so the app's
    // classpath, which takes it through this project, has it too.
    implementation(rustlsPlatformVerifierVersion.map { "org.rustls:rustls-platform-verifier:$it" })
    implementation(libs.androidx.core.ktx)
    implementation(libs.coroutines.android)
    api(platform(libs.compose.bom))
    api(libs.compose.ui)
    api(libs.compose.foundation)
    api(libs.compose.material3)
    // src/test: the rules that are plain values (Timeline.kt), on the JVM.
    testImplementation(libs.junit)
    // And JNA as a jar, which has its own native half for the machine the
    // tests run on; the AAR above has Android's only.
    testImplementation(libs.jna)
}
