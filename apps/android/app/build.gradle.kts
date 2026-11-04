// The phone app: the Compose layout over lumiakit, the Quiet Split folded
// as the design canvas's phone boards draw it (the same screens as iOS).
plugins {
    alias(libs.plugins.android.application)
    alias(libs.plugins.kotlin.android)
    alias(libs.plugins.kotlin.compose)
    // FCM needs the Firebase project's google-services.json beside this
    // file (not in the repo); without it the plugin is off and FCM
    // registration fails quietly, leaving UnifiedPush.
    alias(libs.plugins.google.services) apply false
}
if (file("google-services.json").exists()) {
    apply(plugin = libs.plugins.google.services.get().pluginId)
}

// A release's version is the tag's and its code the commit count, both
// passed by scripts/release.sh; the values here are the development
// build's placeholders, as tauri.conf.json's version is.
val lumiaVersion = findProperty("lumiaVersion")?.toString() ?: "0.2.1"
val lumiaBuild = findProperty("lumiaBuild")?.toString()?.toInt() ?: 3
// The release key from the environment (scripts/release.sh names the
// variables); without one the release build is unsigned.
val keystorePath = System.getenv("ANDROID_KEYSTORE_PATH")

android {
    namespace = "chat.lumia"
    compileSdk = 35
    defaultConfig {
        applicationId = "chat.lumia"
        minSdk = 28
        targetSdk = 35
        versionCode = lumiaBuild
        versionName = lumiaVersion
    }
    signingConfigs {
        if (keystorePath != null) {
            create("release") {
                storeFile = file(keystorePath)
                storePassword = System.getenv("ANDROID_KEYSTORE_PASSWORD")
                keyAlias = System.getenv("ANDROID_KEY_ALIAS")
                keyPassword = System.getenv("ANDROID_KEY_PASSWORD")
            }
        }
    }
    buildTypes {
        release {
            // R8 shrinks the Kotlin (Compose, the icon set, Firebase, Play
            // Services: the unshrunk dex is three times the stripped core);
            // proguard-rules.pro keeps the bindings and JNA, which reach the
            // native library by name.
            isMinifyEnabled = true
            isShrinkResources = true
            proguardFiles(getDefaultProguardFile("proguard-android-optimize.txt"), "proguard-rules.pro")
            // A phone is arm64; the x86_64 core is the emulator's (JNA ships
            // every ABI, this leaves its arm64 one).
            ndk { abiFilters += "arm64-v8a" }
            if (keystorePath != null) {
                signingConfig = signingConfigs.getByName("release")
            }
        }
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
        buildConfig = true
    }
    packaging {
        // JNA ships its own native library per ABI; the core's comes from lumiakit.
        jniLibs.useLegacyPackaging = false
    }
}

dependencies {
    implementation(project(":lumiakit"))
    implementation(libs.androidx.core.ktx)
    implementation(libs.androidx.activity.compose)
    implementation(libs.coroutines.android)
    implementation(libs.compose.icons.extended)
    // Push (architecture 15.5): UnifiedPush through any installed
    // distributor, FCM through the relay otherwise.
    implementation(libs.unifiedpush.connector)
    implementation(platform(libs.firebase.bom))
    implementation(libs.firebase.messaging)
    constraints {
        // Play Services base, under Firebase, carries a fragment library
        // from before the activity-result API; the release lint refuses
        // that beside registerForActivityResult, so the transitive is lifted.
        implementation(libs.androidx.fragment)
    }
}

// The UnifiedPush connector and Firebase both bring Tink; one copy.
configurations.configureEach {
    resolutionStrategy {
        force(libs.tink.android.get().toString())
        dependencySubstitution {
            substitute(module("com.google.crypto.tink:tink")).using(module(libs.tink.android.get().toString()))
        }
    }
}
