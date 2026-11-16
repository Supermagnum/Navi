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
    }
}

rootProject.name = "Navi"
include(":app")

// CoMaps drape (Phase 6): the :comaps-sdk library packages the Java/res/assets of
// the pinned CoMaps checkout (android/sdk) WITHOUT that module's CMake -- Navi
// already stages liborganicmaps.so under app/src/main/jniLibs. The module is
// only included when the gitignored checkout exists (same lookup order as
// scripts/stage-comaps-data-assets.sh); otherwise the app builds without drape
// and falls back to MapLibre. Disable explicitly with -Pnavi.comapsDrape=false.
run {
    val enabled =
        providers
            .gradleProperty("navi.comapsDrape")
            .orElse("true")
            .get()
            .equals("true", ignoreCase = true)
    if (!enabled) return@run
    val root = rootDir
    val envSrc = System.getenv("NAVI_COMAPS_SRC")
    val pinSha =
        runCatching {
            Regex("\"sha\"\\s*:\\s*\"([0-9a-f]+)\"")
                .find(File(root, "third_party/comaps.pin.json").readText())
                ?.groupValues
                ?.get(1)
                ?.take(8)
        }.getOrNull()
    val candidates =
        listOfNotNull(
            envSrc?.let { File(it) },
            File(root, "third_party/comaps"),
            pinSha?.let { File(root.parentFile, "Navi-local-artifacts/comaps-$it") },
        )
    val sdkDir =
        candidates
            .map { File(it, "android/sdk") }
            .firstOrNull { File(it, "src/main/java/app/organicmaps/sdk/Framework.java").isFile }
    if (sdkDir != null) {
        include(":comaps-sdk")
        gradle.extra["naviComapsSdkDir"] = sdkDir.absolutePath
    }
}
