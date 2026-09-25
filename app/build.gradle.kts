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

android {
    namespace = "no.navi.app"
    compileSdk = 37

    defaultConfig {
        applicationId = "no.navi.app"
        minSdk = 26
        targetSdk = 36
        versionCode = 9
        versionName = "0.3.4-beta"
        testInstrumentationRunner = "no.navi.app.NaviAndroidTestRunner"
        // Ship only 64-bit ABIs used by device (arm64) and emulator (x86_64).
        // Dropping armeabi-v7a / x86 / mips MapLibre+JNI copies keeps the
        // committed debug APK under GitHub's 50 MB soft-size advisory.
        ndk {
            abiFilters += listOf("arm64-v8a", "x86_64")
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
            val upload = signingConfigs.findByName("upload")
            if (upload?.storeFile?.isFile == true) {
                signingConfig = upload
            }
        }
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }

    buildFeatures {
        compose = true
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
        noCompress += listOf("svg", "svgz")
    }
}

dependencies {
    val composeBom = platform("androidx.compose:compose-bom:2026.09.00")
    implementation(composeBom)
    androidTestImplementation(composeBom)

    implementation("androidx.activity:activity-compose:1.9.3")
    implementation("androidx.core:core-ktx:1.19.0")
    implementation("androidx.core:core-splashscreen:1.2.0")
    implementation("androidx.compose.ui:ui")
    implementation("androidx.compose.ui:ui-tooling-preview")
    implementation("androidx.compose.material3:material3")
    implementation("org.jetbrains.kotlinx:kotlinx-coroutines-android:1.11.0")
    implementation("net.java.dev.jna:jna:5.19.1@aar")
    // Default (finalized): MapLibre GLES android-sdk. Prefer 11.13.5 over 11.8.8
    // (Maven has GLES 11.13.5 — keep version, change renderer from android-sdk-vulkan).
    // Evidence 2026-07-31: AAOS BearingCrashIsolationTest PASS (no SIGSEGV);
    // SM-P613 online/offline 3D wash cleared (demHitsOk>=1).
    implementation("org.maplibre.gl:android-sdk:13.6.1")
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
