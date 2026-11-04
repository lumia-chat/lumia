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

android {
    namespace = "chat.lumia"
    compileSdk = 35
    defaultConfig {
        applicationId = "chat.lumia"
        minSdk = 28
        targetSdk = 35
        versionCode = 3
        versionName = "0.2.1"
    }
    buildTypes {
        release {
            isMinifyEnabled = false
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
