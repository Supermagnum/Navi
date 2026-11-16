// Java / resources / assets of the pinned CoMaps Android SDK (app.organicmaps.sdk).
//
// Deliberately NO externalNativeBuild: liborganicmaps.so is built by
// scripts/build-comaps-android.sh and staged under app/src/main/jniLibs, so this
// module only supplies the Java classes the native library's JNI expects
// (Framework, Map, MapView, OrganicMaps, ...). Sources are read in place from the
// gitignored checkout located by settings.gradle.kts; nothing is copied into git.
plugins {
    id("com.android.library")
}

val sdkDir =
    File(
        (gradle.extra["naviComapsSdkDir"] as? String)
            ?: error("naviComapsSdkDir not set (settings.gradle.kts did not find the CoMaps checkout)"),
    )

// The Framework constructor reads classificator / drules / countries / fonts /
// symbols from the APK (assets/ root) before the writable directory is switched to
// {filesDir}/mwm, so the complete SDK asset set is packaged (it partly duplicates
// assets/comaps-data, which the headless session reads from the writable dir).
// SDK assets are symlinks into the checkout's data/ directory; Sync dereferences them.
val stagedSdkAssets = layout.buildDirectory.dir("comaps-sdk-assets").get().asFile

val stageComapsSdkAssets =
    tasks.register<Sync>("stageComapsSdkAssets") {
        from(File(sdkDir, "src/main/assets"))
        into(stagedSdkAssets)
    }

android {
    namespace = "app.organicmaps.sdk"
    compileSdk = 37

    defaultConfig {
        minSdk = 26
        consumerProguardFiles("consumer-rules.pro")
    }

    buildFeatures {
        buildConfig = true
    }

    sourceSets {
        getByName("main") {
            java.srcDir(File(sdkDir, "src/main/java"))
            res.srcDir(File(sdkDir, "src/main/res"))
            assets.srcDir(stagedSdkAssets)
        }
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }

    lint {
        abortOnError = false
        disable += listOf("MissingTranslation", "ExtraTranslation", "MissingQuantity", "UnusedQuantity")
    }

    androidResources {
        ignoreAssetsPatterns += "!design"
    }
}

// The staged assets dir is a plain source dir, so AGP tasks that read it (asset merge,
// lint model / vital analysis) need an explicit ordering edge.
tasks
    .matching {
        it.name != "stageComapsSdkAssets" &&
            (it.name.contains("Assets", ignoreCase = true) || it.name.contains("Lint", ignoreCase = true))
    }
    .configureEach { dependsOn(stageComapsSdkAssets) }

dependencies {
    // Versions follow third_party/comaps/android/gradle/libs.versions.toml
    // (minSdk 26 here, so the newer androidx lines are fine).
    implementation("androidx.core:core:1.17.0")
    implementation("androidx.core:core-ktx:1.17.0")
    implementation("androidx.media:media:1.7.1")
    implementation("com.google.android.material:material:1.12.0")
    implementation("androidx.fragment:fragment:1.8.9")
    implementation("androidx.annotation:annotation:1.9.1")
    implementation("androidx.customview:customview:1.2.0")
    implementation("androidx.preference:preference:1.2.1")
    implementation("androidx.recyclerview:recyclerview:1.4.0")
    implementation("androidx.documentfile:documentfile:1.1.0")
    implementation("androidx.lifecycle:lifecycle-process:2.9.4")
}
