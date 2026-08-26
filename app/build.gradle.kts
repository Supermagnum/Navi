plugins {
    id("com.android.application")
    id("org.jetbrains.kotlin.android")
    id("org.jetbrains.kotlin.plugin.compose")
    id("org.jlleitschuh.gradle.ktlint")
    id("io.gitlab.arturbosch.detekt")
}

android {
    namespace = "no.navi.app"
    compileSdk = 36

    defaultConfig {
        applicationId = "no.navi.app"
        minSdk = 26
        targetSdk = 36
        versionCode = 2
        versionName = "0.2.0"
        testInstrumentationRunner = "no.navi.app.NaviAndroidTestRunner"
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

    kotlinOptions {
        jvmTarget = "17"
    }

    buildFeatures {
        compose = true
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
    val composeBom = platform("androidx.compose:compose-bom:2024.10.01")
    implementation(composeBom)
    androidTestImplementation(composeBom)

    implementation("androidx.activity:activity-compose:1.9.3")
    implementation("androidx.core:core-ktx:1.15.0")
    implementation("androidx.core:core-splashscreen:1.0.1")
    implementation("androidx.compose.ui:ui")
    implementation("androidx.compose.ui:ui-tooling-preview")
    implementation("androidx.compose.material3:material3")
    implementation("org.jetbrains.kotlinx:kotlinx-coroutines-android:1.9.0")
    implementation("net.java.dev.jna:jna:5.15.0@aar")
    // Default (finalized): MapLibre GLES android-sdk. Prefer 11.13.5 over 11.8.8
    // (Maven has GLES 11.13.5 — keep version, change renderer from android-sdk-vulkan).
    // Evidence 2026-07-31: AAOS BearingCrashIsolationTest PASS (no SIGSEGV);
    // SM-P613 online/offline 3D wash cleared (demHitsOk>=1).
    implementation("org.maplibre.gl:android-sdk:11.13.5")
    debugImplementation("androidx.compose.ui:ui-tooling")

    testImplementation("junit:junit:4.13.2")

    // androidx.test 1.7 / Espresso 3.7: API 37 removes InputManager.getInstance();
    // Espresso 3.6.1 still reflected it (Compose ui-test → Espresso.onIdle crash).
    // Release notes (Espresso 3.7.0, 2025-07-30): use getSystemService instead.
    // App compileSdk/targetSdk stay 36 — device API 37 only needs newer *test* libs.
    androidTestImplementation("androidx.test.ext:junit:1.3.0")
    androidTestImplementation("androidx.test:runner:1.7.0")
    androidTestImplementation("androidx.test:rules:1.7.0")
    androidTestImplementation("androidx.test.espresso:espresso-core:3.7.0")
    androidTestImplementation("androidx.compose.ui:ui-test-junit4")
    androidTestImplementation("androidx.test.uiautomator:uiautomator:2.3.0")
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
