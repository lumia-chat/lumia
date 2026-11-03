// The phone app: the Compose layout over lumiakit, the Quiet Split folded
// as the design canvas's phone boards draw it (the same screens as iOS).
plugins {
    alias(libs.plugins.android.application)
    alias(libs.plugins.kotlin.android)
    alias(libs.plugins.kotlin.compose)
}

android {
    namespace = "chat.lumia"
    compileSdk = 35
    defaultConfig {
        applicationId = "chat.lumia"
        minSdk = 28
        targetSdk = 35
        versionCode = 1
        versionName = "0.1.0"
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
}
