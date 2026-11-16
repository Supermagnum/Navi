# The native library (liborganicmaps.so) looks Java classes and methods up by name via JNI.
-keep class app.organicmaps.sdk.** { *; }
-keepclasseswithmembernames class * {
    native <methods>;
}
