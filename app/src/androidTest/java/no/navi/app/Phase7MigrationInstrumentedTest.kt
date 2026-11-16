package no.navi.app

import android.util.Log
import androidx.test.platform.app.InstrumentationRegistry
import org.json.JSONArray
import org.json.JSONObject
import org.junit.Assert.assertTrue
import org.junit.Assume.assumeTrue
import org.junit.FixMethodOrder
import org.junit.Test
import org.junit.runners.MethodSorters
import uniffi.navi.defaultPackServerBaseUrl
import uniffi.navi.geofabrikLatestPbfUrl
import java.io.File

/**
 * Phase 7 device support (physical device, releasetest). Drives nothing the user could not do:
 * `a_seed_rkyv_old_path` runs the OLD download path of a flag-OFF build (the same
 * [RegionDownloadBackground.ensureStarted] call as the Tools region download), `b_state_dump` records
 * what is on disk, and the plan tests call the migration planner with detected-region lists. The
 * migration itself is driven through the app UI (adb taps) and the foreground service.
 *
 * Output: the app external files dir, phase7 (adb pull from sdcard/Android/data/no.navi.app.releasetest/files/phase7).
 */
@FixMethodOrder(MethodSorters.NAME_ASCENDING)
class Phase7MigrationInstrumentedTest {
    private val ctx get() = InstrumentationRegistry.getInstrumentation().targetContext
    private val args get() = InstrumentationRegistry.getArguments()
    private val dataDir get() = NaviAppData.resolve(ctx)

    private fun out(
        name: String,
        json: Any,
    ) {
        // App-private external dir: the app uid can write it and `adb pull` can read it.
        val dir = File(ctx.getExternalFilesDir(null), "phase7").apply { mkdirs() }
        File(dir, name).writeText(json.toString())
        Log.i(TAG, "wrote ${File(dir, name).absolutePath} bytes=${json.toString().length}")
    }

    private fun dirBytes(f: File): Long =
        when {
            !f.exists() -> 0L
            f.isFile -> f.length()
            else -> f.listFiles()?.sumOf { dirBytes(it) } ?: 0L
        }

    /** Stop the old-path queue / job (the place-index and basemap phases are not part of a migration). */
    @Test
    fun a1_cancel_old_pending() {
        RegionDownloadBackground.cancelPending(dataDir)
        Log.i(TAG, "old path queue + job cleared; running=${RegionDownloadBackground.isRunning()}")
    }

    /**
     * Old path (flag OFF): download the regions in `-e regions a,b` (default Ostlandet only) exactly like
     * the Tools region download. Run once per region: the old pipeline continues with place index and
     * basemap after the packs, so each run ends by clearing the old queue (and the process exit stops
     * the worker).
     */
    @Test
    fun a_seed_rkyv_old_path() {
        assumeTrue("needs the flag-OFF build", !MwmClientBootstrap.isEnabled())
        val regions = (args.getString("regions") ?: "europe/norway/ostlandet").split(',')
        val report = JSONObject()
        val t0 = System.currentTimeMillis()
        for (path in regions) {
            val packPath = GeofabrikDownloadCatalog.canonicalizePath(path)
            val extract = GeofabrikDownloadCatalog.extractPathForPbf(packPath)
            val leaf = extract.substringAfterLast('/')
            val start = System.currentTimeMillis()
            RegionDownloadBackground.ensureStarted(
                ctx,
                dataDir,
                geofabrikLatestPbfUrl(packPath),
                "$leaf-latest.osm.pbf",
                packPath,
                userLat = null,
                userLon = null,
            )
            // Packs land first; the later place-index / basemap phases are not part of the migration.
            val ok =
                ComapsPhase6Harness.waitFor(3 * 3600_000L, 5_000) {
                    InstalledMaps.refresh(ctx)
                    val r = InstalledMaps.region(packPath)
                    if (System.currentTimeMillis() % 60_000 < 5_000) {
                        Log.i(TAG, "seed $path status=${RegionDownloadBackground.statusLine()} installed=${r != null}")
                    }
                    r != null && r.profilesLoadable.isNotEmpty()
                }
            val sec = (System.currentTimeMillis() - start) / 1000
            InstalledMaps.refresh(ctx)
            val bytes = DownloadedRegionDelete.measureBytes(ctx, dataDir, packPath)
            report.put(path, JSONObject().put("ok", ok).put("seconds", sec).put("bytes", bytes))
            // The old pipeline would go on with the place-index extract and basemap: stop it right here.
            RegionDownloadBackground.cancelPending(dataDir)
            assertTrue("pack for $path did not install", ok)
        }
        // Stop the remaining old-path phases (place index, basemap): not part of what a migration moves.
        RegionDownloadBackground.cancelPending(dataDir)
        Thread.sleep(3000)
        report.put("total_seconds", (System.currentTimeMillis() - t0) / 1000)
        out("seed.json", report)
    }

    /** What is on disk now (sizes of the old packs and of the new map storage) + the migration state. */
    @Test
    fun b_state_dump() {
        val label = args.getString("label") ?: "state"
        val j = JSONObject()
        j.put("label", label)
        j.put("flag_enabled", MwmClientBootstrap.isEnabled())
        j.put("free_bytes", MwmMigration.freeBytes(ctx))
        val top = JSONObject()
        dataDir.listFiles()?.sortedBy { it.name }?.forEach { top.put(it.name, dirBytes(it)) }
        j.put("files_top_level_bytes", top)
        runCatching { InstalledMaps.refresh(ctx) }
        val rk = JSONArray()
        InstalledMaps.current()?.regions?.values?.filter { it.profilesLoadable.isNotEmpty() }?.forEach { r ->
            rk.put(
                JSONObject()
                    .put("region", r.regionId)
                    .put("profiles", r.profilesLoadable.joinToString(","))
                    .put("bytes", DownloadedRegionDelete.measureBytes(ctx, dataDir, r.regionId)),
            )
        }
        j.put("rkyv_regions", rk)
        val mwm = File(dataDir, "mwm")
        val m = JSONObject()
        m.put("active", File(mwm, "active.json").takeIf { it.isFile }?.readText()?.trim() ?: JSONObject.NULL)
        m.put("world_release", File(mwm, "world-release.json").takeIf { it.isFile }?.readText()?.trim() ?: JSONObject.NULL)
        m.put("total_bytes", dirBytes(mwm))
        m.put("staging_bytes", dirBytes(File(mwm, "staging")))
        m.put("releases_bytes", dirBytes(File(mwm, "releases")))
        m.put("world_bytes", dirBytes(File(mwm, "World.mwm")) + dirBytes(File(mwm, "WorldCoasts.mwm")))
        val rel = JSONObject()
        File(mwm, "releases").listFiles()?.forEach { r ->
            val leaves =
                File(r, "leaves")
                    .listFiles()
                    ?.filter { it.name.endsWith(".mwm") }
                    ?.map { it.name.removeSuffix(".mwm") }
                    ?.sorted() ?: emptyList()
            rel.put(r.name, JSONObject().put("leaves", JSONArray(leaves)).put("bytes", dirBytes(r)))
        }
        m.put("releases", rel)
        j.put("mwm", m)
        j.put("migration_state", runCatching { JSONObject(uniffi.navi.mwmMigrationState(dataDir.absolutePath)) }.getOrNull() ?: JSONObject.NULL)
        out("state-$label.json", j)
    }

    private fun planJson(
        detected: List<Pair<String, Long>>,
        free: Long,
        selected: String = "",
        excluded: String = "",
    ): JSONObject {
        val det = JSONArray().apply { detected.forEach { (id, b) -> put(JSONObject().put("region_id", id).put("old_bytes", b)) } }
        val t0 = System.currentTimeMillis()
        val res =
            JSONObject(
                uniffi.navi.mwmMigrationPlan(
                    dataDir.absolutePath,
                    defaultPackServerBaseUrl(),
                    det.toString(),
                    selected,
                    excluded,
                    free.toULong(),
                    false,
                ),
            )
        res.put("plan_ms", System.currentTimeMillis() - t0)
        return res
    }

    /** Only the two shared-leaf regions: Norway_Southern (and Denmark North) counted once. */
    @Test
    fun c_plan_shared_leaf_regions() {
        assumeTrue(MwmClientBootstrap.isEnabled())
        val r = planJson(listOf("europe/norway/ostlandet" to 2_549_326_770L, "europe/norway/sorlandet" to 331_098_569L), 18_000_000_000L)
        assertTrue(r.toString(), r.optBoolean("ok"))
        out("plan-shared.json", r)
        val plan = r.getJSONObject("plan")
        val leaves = plan.getJSONArray("leaves")
        var shared = 0
        for (i in 0 until leaves.length()) if (leaves.getJSONObject(i).getJSONArray("shared_by").length() > 1) shared++
        assertTrue("expected shared leaves", shared >= 1)
    }

    /**
     * Not enough free space: the plan says so and names the shortfall.
     *
     * On a device that already has every mapped leaf installed, `download_bytes`
     * is 0 and the planner correctly reports enough space even with a 600 MB
     * free budget (post-migration state). The shortfall assertion applies only
     * when the plan still needs to download.
     */
    @Test
    fun d_plan_not_enough_space() {
        assumeTrue(MwmClientBootstrap.isEnabled())
        val r = planJson(listOf("europe/norway/ostlandet" to 2_549_326_770L, "europe/norway/sorlandet" to 331_098_569L), 600_000_000L)
        out("plan-nospace.json", r)
        val plan = r.getJSONObject("plan")
        val download = plan.getLong("download_bytes")
        if (download == 0L) {
            assertTrue("already-installed leaves must report enough space: $plan", plan.getBoolean("enough_space"))
            return
        }
        assertTrue(
            "expected shortfall when download_bytes=$download with 600MB free: $plan",
            !plan.getBoolean("enough_space") && plan.getLong("space_short_by") > 0,
        )
    }

    /** A region without a mapping is reported and kept; nothing is selected for it. */
    @Test
    fun e_plan_unmapped_region() {
        assumeTrue(MwmClientBootstrap.isEnabled())
        val r = planJson(listOf("europe/norway/ostlandet" to 2_549_326_770L, "europe/atlantis/east" to 123_456_789L), 18_000_000_000L)
        out("plan-unmapped.json", r)
        val rows = r.getJSONObject("plan").getJSONArray("rows")
        var unmapped = 0
        for (i in 0 until rows.length()) {
            val row = rows.getJSONObject(i)
            if (!row.getBoolean("mapped")) {
                unmapped++
                assertTrue(!row.getBoolean("selected"))
            }
        }
        assertTrue(unmapped == 1)
    }

    /** Many regions at once: 40 real region ids from the mapping, planned in one call. */
    @Test
    fun f_plan_many_regions() {
        assumeTrue(MwmClientBootstrap.isEnabled())
        val ids =
            listOf(
                "europe/norway/ostlandet",
                "europe/norway/sorlandet",
                "europe/norway/vestlandet",
                "europe/norway/trondelag",
                "europe/norway/nord-norge",
                "europe/sweden/stockholm",
                "europe/sweden/vastra_gotaland",
                "europe/sweden/skane",
                "europe/denmark",
                "europe/finland",
                "europe/germany/hamburg",
                "europe/germany/niedersachsen",
                "europe/germany/schleswig-holstein",
                "europe/germany/mecklenburg-vorpommern",
                "europe/netherlands/friesland",
                "europe/netherlands/groningen",
                "europe/great-britain",
                "europe/ireland-and-northern-ireland",
                "europe/iceland",
                "europe/estonia",
                "europe/latvia",
                "europe/lithuania",
                "europe/poland",
                "europe/czech-republic",
                "europe/austria",
                "europe/switzerland",
                "europe/france",
                "europe/spain/cataluna",
                "europe/portugal",
                "europe/italy/sud",
                "europe/italy/isole",
                "europe/croatia",
                "europe/slovenia",
                "europe/hungary",
                "europe/romania",
                "europe/bulgaria",
                "europe/greece",
                "europe/turkey",
                "north-america/canada/yukon",
                "north-america/us/alaska",
            )
        val r = planJson(ids.map { it to 1_000_000_000L }, 500_000_000_000L)
        out("plan-many.json", r)
        assertTrue(r.toString().take(300), r.optBoolean("ok"))
        val plan = r.getJSONObject("plan")
        val rows = plan.getJSONArray("rows")
        var mapped = 0
        for (i in 0 until rows.length()) if (rows.getJSONObject(i).getBoolean("mapped")) mapped++
        Log.i(TAG, "many: regions=${rows.length()} mapped=$mapped leaves=${plan.getJSONArray("leaves").length()} download=${plan.getLong("download_bytes")}")
    }

    private val fakeStems =
        mapOf(
            "unmapped" to listOf("europe/atlantis/east" to "east-latest"),
            "many" to
                listOf(
                    "europe/norway/vestlandet",
                    "europe/norway/trondelag",
                    "europe/norway/nord-norge",
                    "europe/sweden/stockholm",
                    "europe/sweden/vastra_gotaland",
                    "europe/sweden/skane",
                    "europe/denmark",
                    "europe/finland",
                    "europe/germany/hamburg",
                    "europe/germany/niedersachsen",
                    "europe/germany/schleswig-holstein",
                    "europe/netherlands/friesland",
                    "europe/netherlands/groningen",
                    "europe/great-britain",
                    "europe/iceland",
                    "europe/estonia",
                    "europe/latvia",
                    "europe/lithuania",
                    "europe/poland",
                    "europe/czech-republic",
                    "europe/austria",
                    "europe/switzerland",
                    "europe/france",
                    "europe/spain/cataluna",
                    "europe/portugal",
                    "europe/italy/sud",
                    "europe/croatia",
                    "europe/slovenia",
                    "europe/hungary",
                    "europe/atlantis/east",
                ).map { it to it.substringAfterLast('/') + "-latest" },
        )

    /**
     * Test-only: create tiny stand-in `.rkyv` regions (manifest + install stamp + one graph file per profile,
     * 4 kB each) so the migration offer can be shown with an unmapped region (`mode=unmapped`) or with 30
     * regions (`mode=many`). Nothing is downloaded; `h_fake_regions_remove` deletes exactly these files.
     */
    @Test
    fun h_fake_regions_create() {
        val mode = args.getString("mode") ?: "unmapped"
        val made = JSONArray()
        for ((id, stem) in fakeStems.getValue(mode)) {
            File(dataDir, "$stem.navi-manifest.json").writeText("{\"graph_format_version\":9}")
            File(dataDir, "$stem.navi-server-install.json").writeText("{\"region_id\":\"$id\",\"generation\":\"phase7-fake\"}")
            for (p in listOf("car", "foot")) File(dataDir, "$stem.navi-graph-$p.t0_0.rkyv").writeBytes(ByteArray(4096))
            made.put(id)
        }
        out("fake-$mode.json", made)
    }

    @Test
    fun h_fake_regions_remove() {
        var n = 0
        // "atlantis-east-latest" is the stem of the first version of the unmapped fake.
        dataDir.listFiles()?.filter { it.name.startsWith("atlantis-east-latest.navi-") }?.forEach { if (it.delete()) n++ }
        for (list in fakeStems.values) {
            for ((_, stem) in list) {
                dataDir.listFiles()?.filter { it.name.startsWith("$stem.navi-") }?.forEach {
                    if (it.delete()) n++
                }
            }
        }
        Log.i(TAG, "removed $n fake region files")
    }

    /**
     * Decision 15b on the device: the drape position puck in its three states (blue GPS, amber estimated, grey
     * lost: sprite when there is no heading, 3D arrow colour while moving) and the four APRS sprites from the
     * generated atlas. Espa is held as the GPS fix. Screenshots land in the `-e out_dir` directory.
     */
    @Test
    fun n_puck_and_aprs_visual() {
        assumeTrue(MwmClientBootstrap.isEnabled())
        val h = ComapsPhase6Harness
        h.ensureShellDirs()
        androidx.test.core.app.ActivityScenario.launch(MainActivity::class.java).use {
            NaviMapTestHooks.hideSearchChrome = true
            NaviMapTestHooks.hideUiChrome = true
            assertTrue("drape ready", h.waitFor(120_000, 500) { NaviMapTestHooks.drapeState == "READY" })
            val lat = 60.562616550309635
            val lon = 11.2546040956274
            NaviMapTestHooks.holdDebugGps(lat, lon)
            assertTrue("gps", h.waitFor(20_000, 250) { Math.abs(NaviMapTestHooks.lastGpsLat - lat) < 1e-4 })
            assertTrue("camera", h.gotoCamera(lat, lon, 17.0))
            val ui = android.os.Handler(android.os.Looper.getMainLooper())
            for ((state, name) in listOf(0 to "gps", 1 to "estimated", 2 to "lost")) {
                ui.post { ComapsDrape.backend?.setPuckState(state) }
                Thread.sleep(2500)
                h.screenshot("puck_$name", settleMs = 8_000)
            }
            ui.post { ComapsDrape.backend?.setPuckState(0) }
            val dlat = 0.0007
            NaviMapTestHooks.pendingTracks =
                listOf(
                    TrackMarker("LA1CAR-9", lat + dlat, lon - 0.0014, "aprs_car"),
                    TrackMarker("LA1DIG-1", lat + dlat, lon - 0.0005, "aprs_digi"),
                    TrackMarker("LA1HOM-5", lat + dlat, lon + 0.0005, "aprs_house"),
                    TrackMarker("LA1HUM-7", lat + dlat, lon + 0.0014, "aprs_human"),
                )
            Thread.sleep(4000)
            assertTrue("camera 2", h.gotoCamera(lat + dlat, lon, 17.0))
            h.screenshot("aprs_sprites", settleMs = 10_000)
            out("puck-aprs.json", JSONObject().put("last_puck_state", NaviMapTestHooks.lastPuckState).put("tracks", NaviMapTestHooks.lastTrackIds.size))
        }
    }

    /** Restore the Ostlandet leaf set (the Map storage delete test removed one): installs only what is missing. */
    @Test
    fun k_reinstall_ostlandet_leaves() {
        assumeTrue(MwmClientBootstrap.isEnabled())
        val regions = uniffi.navi.mwmListRegions(defaultPackServerBaseUrl())
        val leaves = regions.regions.first { it.id == "europe_norway_ostlandet" }.leafIds
        val t0 = System.currentTimeMillis()
        val res = uniffi.navi.mwmInstallLeaves(dataDir.absolutePath, defaultPackServerBaseUrl(), leaves)
        out(
            "reinstall.json",
            JSONObject()
                .put("ok", res.ok)
                .put("downloaded", res.downloadedLeaves.toInt())
                .put("already", res.alreadyInstalledLeaves.toInt())
                .put("seconds", (System.currentTimeMillis() - t0) / 1000)
                .put("error", res.error ?: JSONObject.NULL),
        )
        assertTrue(res.error ?: "install failed", res.ok)
        ComapsSessionBootstrap.ensureOpen(ctx, force = true)
    }

    /**
     * Hold Espa as the GPS fix (Navi's own debug GPS hold) for as long as this instrumentation runs; the app is
     * then driven through the UI with adb. Used by the GPX and the puck checks.
     */
    @Test
    fun j_hold_session() {
        assumeTrue(MwmClientBootstrap.isEnabled())
        // MainActivity.onCreate clears debug GPS pins, so hold only once the activity (drape) is up.
        ComapsPhase6Harness.waitFor(600_000, 500) { NaviMapTestHooks.drapeState == "READY" }
        NaviMapTestHooks.holdDebugGps(60.562616550309635, 11.2546040956274)
        Log.i(TAG, "hold session: Espa held as GPS")
        Thread.sleep((args.getString("hold_s") ?: "7200").toLong() * 1000)
    }

    /**
     * Point the navi-server origin at a host proxy (`adb reverse tcp:18080 tcp:18080`,
     * scripts/phase7-origin-proxy.py) for as long as this instrumentation runs, then wait. The app
     * under test shares this process, so launching it with `am start` uses the proxy. Stop it by
     * force-stopping the app or `am instrument` with a new test.
     */
    @Test
    fun g_proxy_origin_hold() {
        val port = args.getString("port") ?: "18080"
        android.system.Os.setenv("NAVI_PACK_SERVER_BASE_URL", "http://127.0.0.1:$port", true)
        Log.i(TAG, "origin proxy set: ${defaultPackServerBaseUrl()}")
        Thread.sleep((args.getString("hold_s") ?: "7200").toLong() * 1000)
    }

    companion object {
        private const val TAG = "Phase7"
    }
}
