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
}

dependencies {
    // UniFFI's Kotlin bindings call the native library through JNA.
    api(libs.jna) { artifact { type = "aar" } }
    implementation(libs.androidx.core.ktx)
    implementation(libs.coroutines.android)
    api(platform(libs.compose.bom))
    api(libs.compose.ui)
    api(libs.compose.foundation)
    api(libs.compose.material3)
    // src/test: the rules that are plain values (Timeline.kt), on the JVM.
    testImplementation(libs.junit)
}
