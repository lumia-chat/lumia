# The release build's R8 rules. The generated bindings (lumiakit's uniffi
# package) reach the core through JNA, which binds an interface's methods
# to the library's exported functions by name and builds structures by
# reflection, so neither may be renamed or dropped.
-keep class com.sun.jna.** { *; }
-keep class * implements com.sun.jna.** { *; }
-keep class uniffi.** { *; }
-dontwarn java.awt.**
