pluginManagement {
    repositories {
        google()
        mavenCentral()
        gradlePluginPortal()
    }
}
dependencyResolutionManagement {
    repositoriesMode.set(RepositoriesMode.FAIL_ON_PROJECT_REPOS)
    repositories {
        google()
        mavenCentral()
        // The Kotlin half of the core's TLS verifier (rustls-platform-verifier):
        // its makers publish the AAR as a Maven repository on a branch of their
        // repository, and only that group is looked for there.
        maven("https://github.com/rustls/rustls-platform-verifier/raw/maven-archive/android-release-support/maven/") {
            content { includeGroup("org.rustls") }
        }
    }
}
rootProject.name = "Lumia"
include(":lumiakit", ":app")
