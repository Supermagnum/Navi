import org.jetbrains.kotlin.gradle.dsl.JvmTarget

plugins {
    id("com.android.application")
    id("org.jetbrains.kotlin.android")
    id("org.jetbrains.kotlin.plugin.compose")
    id("org.jlleitschuh.gradle.ktlint")
    id("io.gitlab.arturbosch.detekt")
}

kotlin {
    compilerOptions {
        jvmTarget.set(JvmTarget.JVM_17)
    }
}

// Shipped ABIs: one list shared with scripts/build-android-native.sh via
// gradle.properties `naviAbis` (override with -PnaviAbis=arm64-v8a).
val naviAllowedAbis = setOf("arm64-v8a", "x86_64")
val naviAbis: List<String> =
    providers
        .gradleProperty("naviAbis")
        .orElse("arm64-v8a,x86_64")
        .get()
        .split(',')
        .map { it.trim() }
        .filter { it.isNotEmpty() }
        .also { abis ->
            require(abis.isNotEmpty()) {
                "naviAbis must list at least one ABI (arm64-v8a and/or x86_64)"
            }
            val unknown = abis.filterNot { it in naviAllowedAbis }
            require(unknown.isEmpty()) {
                "Unsupported naviAbis entries: $unknown (allowed: $naviAllowedAbis)"
            }
        }

android {
    namespace = "no.navi.app"
    compileSdk = 37

    defaultConfig {
        applicationId = "no.navi.app"
        minSdk = 26
        targetSdk = 36
        // F-Droid upstream CurrentVersionCode is 33 (0.3.30-beta); next is 34.
        versionCode = 36
        versionName = "0.5.1-beta"
        testInstrumentationRunner = "no.navi.app.NaviAndroidTestRunner"
        // Phase 8 / G4: MWM client always ON (no classic MapLibre / `.rkyv` product path).
        buildConfigField("boolean", "MWM_CLIENT_ENABLED", "true")
        // Restrict packaging to naviAbis so every lib/<abi>/ folder contains the
        // same .so set (libnavi + JNA + graphics.path + CoMaps when packaged).
        ndk {
            abiFilters.clear()
            abiFilters += naviAbis
        }
    }

    // Omit Google Play dependency metadata from APK/AAB (F-Droid / reproducible
    // builds stay simpler; Play App Signing still works without it).
    dependenciesInfo {
        includeInApk = false
        includeInBundle = false
    }

    signingConfigs {
        // Local upload key for AAB smoke tests (not for Play production).
        // Generated under app/keystore/ (gitignored) by scripts/make-upload-keystore.sh.
        create("upload") {
            val store = file("keystore/navi-upload.jks")
            if (store.isFile) {
                storeFile = store
                storePassword =
                    providers
                        .gradleProperty("navi.upload.storePassword")
                        .orElse("navi-upload-local")
                        .get()
                keyAlias =
                    providers
                        .gradleProperty("navi.upload.keyAlias")
                        .orElse("navi-upload")
                        .get()
                keyPassword =
                    providers
                        .gradleProperty("navi.upload.keyPassword")
                        .orElse("navi-upload-local")
                        .get()
                enableV2Signing = true
                enableV3Signing = true
            }
        }
        // Local releasetest key (gitignored). scripts/make-releasetest-keystore.sh
        create("releasetest") {
            val store = file("keystore/navi-releasetest.jks")
            if (store.isFile) {
                storeFile = store
                storePassword =
                    providers
                        .gradleProperty("navi.releasetest.storePassword")
                        .orElse("navi-releasetest-local")
                        .get()
                keyAlias =
                    providers
                        .gradleProperty("navi.releasetest.keyAlias")
                        .orElse("navi-releasetest")
                        .get()
                keyPassword =
                    providers
                        .gradleProperty("navi.releasetest.keyPassword")
                        .orElse("navi-releasetest-local")
                        .get()
                enableV2Signing = true
                enableV3Signing = true
            }
        }
    }

    buildTypes {
        release {
            isMinifyEnabled = false
            proguardFiles(
                getDefaultProguardFile("proguard-android-optimize.txt"),
                "proguard-rules.pro",
            )
            // Keep APK content free of embedded VCS stamps (reproducible / F-Droid).
            vcsInfo {
                include = false
            }
            // F-Droid and .github/workflows/release.yml need an *unsigned* APK so
            // apksigner / Binaries compare matches. Pass -Pnavi.unsignedRelease=true
            // (or omit the upload keystore) for that path. Local sideload builds may
            // still use app/keystore/navi-upload.jks when present.
            val forceUnsigned =
                providers
                    .gradleProperty("navi.unsignedRelease")
                    .orElse("false")
                    .get()
                    .equals("true", ignoreCase = true)
            val upload = signingConfigs.findByName("upload")
            if (!forceUnsigned && upload?.storeFile?.isFile == true) {
                signingConfig = upload
            }
        }
        // Sideloadable release-opt build that installs next to debug
        // (applicationId no.navi.app.releasetest). Used for Phase 5/6 device
        // measurements. Native/Rust are release; R8 matches production release
        // (minify currently off — same as `release`).
        create("releasetest") {
            initWith(getByName("release"))
            applicationIdSuffix = ".releasetest"
            versionNameSuffix = "-releasetest"
            matchingFallbacks += listOf("release")
            isDebuggable = false
            isMinifyEnabled = false
            isShrinkResources = false
            vcsInfo {
                include = false
            }
            val rt = signingConfigs.findByName("releasetest")
            if (rt?.storeFile?.isFile == true) {
                signingConfig = rt
            } else {
                logger.warn(
                    "releasetest: missing app/keystore/navi-releasetest.jks — " +
                        "run scripts/make-releasetest-keystore.sh",
                )
            }
        }
    }

    // Instrumented tests target debug by default. For releasetest:
    //   ./gradlew :app:connectedReleasetestAndroidTest -Pnavi.testBuildType=releasetest
    val testBuild =
        providers
            .gradleProperty("navi.testBuildType")
            .orElse("debug")
            .get()
    testBuildType = testBuild

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }

    buildFeatures {
        compose = true
        buildConfig = true
    }

    testOptions {
        // Host JVM unit tests: stub android.jar methods (Log.i, etc.) instead of throwing.
        unitTests.isReturnDefaultValues = true
    }

    packaging {
        jniLibs {
            useLegacyPackaging = true
        }
    }

    androidResources {
        // CoMaps' ZipFileReader reads APK entries as raw bytes (no inflate): every asset
        // type the Framework / drape reads from the APK must be stored uncompressed
        // (same list as the CoMaps app's own build.gradle.kts, plus the ICU data file).
        noCompress +=
            listOf(
                "svg",
                "svgz",
                "txt",
                "bin",
                "html",
                "png",
                "json",
                "mwm",
                "ttf",
                "sdf",
                "ui",
                "config",
                "csv",
                "spv",
                "obj",
                "dat",
            )
    }

    // Phase 6b/8: SDK-backed host code (src/drape/java) compiles with :comaps-sdk when
    // the CoMaps checkout exists. Without it the map surface shows the download placeholder.
    if (rootProject.findProject(":comaps-sdk") != null) {
        sourceSets {
            getByName("main") {
                java.srcDir("src/drape/java")
            }
        }
    }

    // Incomplete nb/nn translations must not fail the build while keys are filled
    // incrementally (see docs/i18n/missing-translations.md).
    lint {
        disable += listOf("MissingTranslation", "ExtraTranslation")
        abortOnError = false
    }
}

dependencies {
    val composeBom = platform("androidx.compose:compose-bom:2026.09.00")
    implementation(composeBom)
    androidTestImplementation(composeBom)

    implementation("androidx.activity:activity-compose:1.9.3")
    implementation("androidx.appcompat:appcompat:1.7.1")
    implementation("androidx.core:core-ktx:1.19.1")
    implementation("androidx.core:core-splashscreen:1.2.0")
    implementation("androidx.compose.ui:ui")
    implementation("androidx.compose.ui:ui-tooling-preview")
    implementation("androidx.compose.material3:material3")
    implementation("org.jetbrains.kotlinx:kotlinx-coroutines-android:1.11.0")
    // NR01 named-route stubs are zstd section bodies inside `{leaf}.navi-sidecar.v1.zst`.
    implementation("com.github.luben:zstd-jni:1.5.7-28")
    implementation("net.java.dev.jna:jna:5.19.1@aar")
    // MapLibre still packaged until Phase 8 deletion completes (dead on MWM-only path).
    implementation("org.maplibre.gl:android-sdk:13.6.1")
    // CoMaps drape (Java/res/assets of the pinned checkout, no CMake): see comaps-sdk/.
    if (rootProject.findProject(":comaps-sdk") != null) {
        implementation(project(":comaps-sdk"))
        // androidTest compiles headless CoMaps suites that import app.organicmaps.sdk.*;
        // keep the SDK on the instrumented classpath even when AGP does not expose it.
        androidTestImplementation(project(":comaps-sdk"))
    }
    debugImplementation("androidx.compose.ui:ui-tooling")

    testImplementation("junit:junit:4.13.2")
    // Real org.json for JVM unit tests (Android stubs throw "not mocked").
    testImplementation("org.json:json:20260814")

    // androidx.test 1.7 / Espresso 3.7: API 37 removes InputManager.getInstance();
    // Espresso 3.6.1 still reflected it (Compose ui-test → Espresso.onIdle crash).
    // Release notes (Espresso 3.7.0, 2025-07-30): use getSystemService instead.
    // App compileSdk/targetSdk stay 36 — device API 37 only needs newer *test* libs.
    androidTestImplementation("androidx.test.ext:junit:1.3.0")
    androidTestImplementation("androidx.test:runner:1.7.0")
    androidTestImplementation("androidx.test:rules:1.7.0")
    androidTestImplementation("androidx.test.espresso:espresso-core:3.7.0")
    androidTestImplementation("androidx.compose.ui:ui-test-junit4")
    androidTestImplementation("androidx.test.uiautomator:uiautomator:2.4.0")
    debugImplementation("androidx.compose.ui:ui-test-manifest")
}

ktlint {
    version.set("1.5.0")
    android.set(true)
    ignoreFailures.set(false)
    filter {
        exclude("**/uniffi/**")
        exclude("**/generated/**")
    }
}

detekt {
    buildUponDefaultConfig = true
    allRules = false
    config.setFrom(files("$rootDir/config/detekt/detekt.yml"))
    source.setFrom(
        "src/main/java",
        "src/drape/java",
        "src/test/java",
        "src/androidTest/java",
    )
}

tasks.withType<io.gitlab.arturbosch.detekt.Detekt>().configureEach {
    exclude("**/uniffi/**")
    exclude("**/generated/**")
    reports {
        html.required.set(true)
        xml.required.set(false)
        txt.required.set(false)
        sarif.required.set(false)
        md.required.set(false)
    }
}

// F-Droid / reproducible: stage wasm guests from source (no committed binaries).
// Hook assemble/bundle only — not preBuild — so JVM unit tests do not compile wasm.
tasks.register<Exec>("buildPluginWasm") {
    workingDir = rootProject.projectDir
    commandLine("bash", "scripts/build-plugin-wasm.sh")
}

// Stage CoMaps classificator / countries / packed_polygons from the pinned
// source tree into APK assets (gitignored; no prebuilt World.mwm here).
//
// With :comaps-sdk (the drape build) the same files already ship as the SDK's APK root assets and
// ComapsSessionBootstrap stages the headless router's copy from there, so nothing is staged a second
// time and a stale staged dir is removed. Only SDK-less builds still stage `assets/comaps-data`.
val comapsSdkPresent = rootProject.findProject(":comaps-sdk") != null

tasks.register<Delete>("removeStagedComapsData") {
    delete("src/main/assets/comaps-data")
}

tasks.register<Exec>("stageComapsDataAssets") {
    workingDir = rootProject.projectDir
    commandLine("bash", "scripts/stage-comaps-data-assets.sh")
    onlyIf {
        // Skip when the pin clone / local-artifacts cache is unavailable (CI
        // host unit tests); device / F-Droid builds must have the tree.
        !comapsSdkPresent &&
            file("${rootProject.projectDir}/scripts/stage-comaps-data-assets.sh").exists()
    }
}

if (comapsSdkPresent) {
    tasks
        .matching { it.name.startsWith("merge") && it.name.endsWith("Assets") }
        .configureEach { dependsOn("removeStagedComapsData") }
}

val packageTasks =
    setOf(
        "assemble",
        "assembleDebug",
        "assembleRelease",
        "bundle",
        "bundleDebug",
        "bundleRelease",
    )
tasks.matching { it.name in packageTasks }.configureEach {
    dependsOn("buildPluginWasm")
    dependsOn("stageComapsDataAssets")
}
