# The release build's R8 rules. The generated bindings (lumiakit's uniffi
# package) reach the core through JNA, which binds an interface's methods
# to the library's exported functions by name and builds structures by
# reflection, so neither may be renamed or dropped.
-keep class com.sun.jna.** { *; }
-keep class * implements com.sun.jna.** { *; }
-keep class uniffi.** { *; }
# The core's TLS verifier (rustls-platform-verifier) loads its Kotlin
# component through the app's class loader and calls it by name over JNI,
# which R8 cannot see; the rule is the crate's README's. Tls.initialize is
# native, and the default rules keep a native method's class and name.
-keep, includedescriptorclasses class org.rustls.platformverifier.** { *; }
-dontwarn java.awt.**
