package no.navi.app

import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import org.json.JSONArray
import org.json.JSONObject
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith
import uniffi.navi.FfiComapsOptimize
import uniffi.navi.FfiComapsProfile
import uniffi.navi.FfiComapsRouteCode
import uniffi.navi.comapsNativeAvailable
import uniffi.navi.comapsSessionClose
import uniffi.navi.comapsSessionOpen
import uniffi.navi.comapsSessionRoute
import uniffi.navi.comapsSessionSetAvoidOptions
import uniffi.navi.comapsSessionSetBikeWetland
import uniffi.navi.comapsSessionSetOptimize
import uniffi.navi.comapsSessionSetTurnsLocale
import java.io.File
import java.io.FileOutputStream

/**
 * Phase 4 device gates: turns JSON (3 routes), optimize switches, wetland extras,
 * boardwalk candidate, mountain elevation sample routes.
 *
 * Same layout as [ComapsPhase3RouteInstrumentedTest] under navi-mwm-test/.
 */
@RunWith(AndroidJUnit4::class)
class ComapsPhase4ParityInstrumentedTest {
    private lateinit var root: File
    private lateinit var outDir: File

    @Before
    fun setUp() {
        MwmClientBootstrap.ensureInitialized()
        check(comapsNativeAvailable()) {
            "comaps-native not linked into this APK"
        }
        val ctx = InstrumentationRegistry.getInstrumentation().targetContext
        root = File(ctx.getExternalFilesDir(null), "navi-mwm-test")
        outDir = File(root, "out-phase4")
        outDir.mkdirs()
        ensureResourceData(ctx)
    }

    private fun ensureResourceData(ctx: android.content.Context) {
        root.mkdirs()
        File(root, "261105").mkdirs()
        val am = ctx.assets
        for (name in am.list("comaps-data").orEmpty()) {
            if (name == "MANIFEST.txt") continue
            val dest = File(root, name)
            if (dest.isFile && dest.length() > 0L) continue
            am.open("comaps-data/$name").use { input ->
                FileOutputStream(dest).use { output -> input.copyTo(output) }
            }
        }
        val dataDir = File(root, "data")
        if (dataDir.isDirectory) {
            for (f in dataDir.listFiles().orEmpty()) {
                if (!f.isFile) continue
                val dest = File(root, f.name)
                if (!dest.exists() || dest.length() == 0L) {
                    f.copyTo(dest, overwrite = true)
                }
            }
        }
    }

    private fun route(
        profile: FfiComapsProfile,
        fromLat: Double,
        fromLon: Double,
        toLat: Double,
        toLon: Double,
        wetland: Boolean,
    ) = comapsSessionRoute(
        profile = profile,
        fromLat = fromLat,
        fromLon = fromLon,
        toLat = toLat,
        toLon = toLon,
        wetlandHook = wetland,
    )

    private fun writeGeoJson(
        name: String,
        points: List<uniffi.navi.FfiComapsLatLon>,
    ): String {
        val coords = JSONArray()
        for (p in points) {
            coords.put(JSONArray().put(p.lon).put(p.lat))
        }
        val feat =
            JSONObject()
                .put("type", "Feature")
                .put(
                    "geometry",
                    JSONObject().put("type", "LineString").put("coordinates", coords),
                )
                .put("properties", JSONObject().put("id", name))
        val path = File(outDir, "$name.geojson")
        path.writeText(feat.toString())
        return path.absolutePath
    }

    @Test
    fun phase4TurnsOptimizeWetlandBoardwalkElevation() {
        val sidecarDir = File(root, "sidecars")
        assertTrue("classificator missing at ${root.absolutePath}", File(root, "classificator.txt").isFile)
        assertTrue("261105 missing", File(root, "261105").isDirectory)

        val open =
            comapsSessionOpen(
                storagePath = root.absolutePath,
                writablePath = root.absolutePath,
                sidecarDir = sidecarDir.absolutePath,
                loadWetlandSidecars = true,
            )
        assertTrue("session open: ${open.errorMessage}", open.ok)

        comapsSessionSetTurnsLocale("nb")
        comapsSessionSetBikeWetland(false)
        comapsSessionSetOptimize(FfiComapsOptimize.TIME)
        comapsSessionSetAvoidOptions(
            avoidToll = false,
            avoidMotorway = false,
            avoidFerry = false,
            avoidDirty = false,
        )

        val summary = JSONObject()
        val routes = JSONArray()
        val turnsReport = JSONArray()

        // --- Turn-by-turn on 3 routes (Norwegian locale) ---
        data class TurnCase(
            val id: String,
            val profile: FfiComapsProfile,
            val fromLat: Double,
            val fromLon: Double,
            val toLat: Double,
            val toLon: Double,
            val notes: String,
        )
        val turnCases =
            listOf(
                TurnCase(
                    "tbt_hamar_elverum",
                    FfiComapsProfile.CAR,
                    60.7945,
                    11.0680,
                    60.8825,
                    11.5623,
                    "street names / turns nb",
                ),
                TurnCase(
                    "tbt_oslo_ring",
                    FfiComapsProfile.CAR,
                    59.9200,
                    10.7400,
                    59.9400,
                    10.7600,
                    "urban roundabout / turns",
                ),
                TurnCase(
                    "tbt_moss_horten_ferry",
                    FfiComapsProfile.CAR,
                    59.4358,
                    10.6600,
                    59.4130,
                    10.4800,
                    "ferry leg + Norwegian names",
                ),
            )
        for (tc in turnCases) {
            val r =
                route(tc.profile, tc.fromLat, tc.fromLon, tc.toLat, tc.toLon, wetland = false)
            assertTrue("${tc.id} code=${r.code} ${r.errorMessage}", r.code == FfiComapsRouteCode.OK)
            val turns = JSONArray(r.turnsJson)
            File(outDir, "${tc.id}.turns.json").writeText(r.turnsJson)
            writeGeoJson(tc.id, r.points)
            assertTrue("${tc.id}: expected non-empty turns", turns.length() > 0)
            val sampleStreet =
                (0 until turns.length())
                    .mapNotNull { turns.optJSONObject(it)?.optString("street") }
                    .firstOrNull { it.isNotBlank() }
            turnsReport.put(
                JSONObject()
                    .put("id", tc.id)
                    .put("notes", tc.notes)
                    .put("turn_count", turns.length())
                    .put("length_m", r.lengthM)
                    .put("time_s", r.timeS)
                    .put("sample_street", sampleStreet)
                    .put("sample", turns.optJSONObject(0)),
            )
            routes.put(
                JSONObject()
                    .put("id", tc.id)
                    .put("code", r.code.name)
                    .put("length_m", r.lengthM)
                    .put("turn_count", turns.length()),
            )
        }

        // --- Lillehammer→Drammen: Time vs PreferNonMotorway ---
        comapsSessionSetOptimize(FfiComapsOptimize.TIME)
        val fastest =
            route(FfiComapsProfile.CAR, 61.1153, 10.4662, 59.7440, 10.2045, wetland = false)
        writeGeoJson("lillehammer_drammen_time", fastest.points)
        comapsSessionSetOptimize(FfiComapsOptimize.PREFER_NON_MOTORWAY)
        val nonMw =
            route(FfiComapsProfile.CAR, 61.1153, 10.4662, 59.7440, 10.2045, wetland = false)
        writeGeoJson("lillehammer_drammen_prefer_non_motorway", nonMw.points)
        comapsSessionSetOptimize(FfiComapsOptimize.TIME)
        assertTrue(fastest.code == FfiComapsRouteCode.OK)
        assertTrue(nonMw.code == FfiComapsRouteCode.OK)
        routes.put(
            JSONObject()
                .put("id", "lillehammer_drammen_time")
                .put("code", fastest.code.name)
                .put("length_m", fastest.lengthM)
                .put("time_s", fastest.timeS)
                .put("build_s", fastest.buildTimeS),
        )
        routes.put(
            JSONObject()
                .put("id", "lillehammer_drammen_prefer_non_motorway")
                .put("code", nonMw.code.name)
                .put("length_m", nonMw.lengthM)
                .put("time_s", nonMw.timeS)
                .put("build_s", nonMw.buildTimeS)
                .put("length_delta_m", nonMw.lengthM - fastest.lengthM)
                .put("time_delta_s", nonMw.timeS - fastest.timeS),
        )

        // Distance optimize must be Unsupported
        comapsSessionSetOptimize(FfiComapsOptimize.DISTANCE)
        val dist =
            route(FfiComapsProfile.CAR, 60.7945, 11.0680, 60.8825, 11.5623, wetland = false)
        assertTrue(
            "distance optimize should be Unsupported, got ${dist.code}",
            dist.code == FfiComapsRouteCode.UNSUPPORTED,
        )
        comapsSessionSetOptimize(FfiComapsOptimize.TIME)
        routes.put(JSONObject().put("id", "distance_unsupported").put("code", dist.code.name))

        // --- Foot wetland detour (Phase 3 pin) + extras ---
        val footPlain =
            route(FfiComapsProfile.PEDESTRIAN, 61.0500, 11.4700, 61.0800, 11.5200, wetland = false)
        val footWet =
            route(FfiComapsProfile.PEDESTRIAN, 61.0500, 11.4700, 61.0800, 11.5200, wetland = true)
        assertTrue(kotlin.math.abs(footPlain.lengthM - 13867.0) < 50.0)
        assertTrue(kotlin.math.abs(footWet.lengthM - 16245.0) < 50.0)
        routes.put(
            JSONObject()
                .put("id", "foot_wetland_phase3")
                .put("plain_m", footPlain.lengthM)
                .put("wetland_m", footWet.lengthM),
        )

        val w2Plain = route(FfiComapsProfile.PEDESTRIAN, 61.02, 11.40, 61.06, 11.45, false)
        val w2Wet = route(FfiComapsProfile.PEDESTRIAN, 61.02, 11.40, 61.06, 11.45, true)
        assertTrue(w2Plain.code == FfiComapsRouteCode.OK)
        assertTrue(w2Wet.code == FfiComapsRouteCode.OK)
        routes.put(
            JSONObject()
                .put("id", "hedmark_wetland_2")
                .put("plain_m", w2Plain.lengthM)
                .put("wetland_m", w2Wet.lengthM)
                .put("delta_m", w2Wet.lengthM - w2Plain.lengthM),
        )

        // Boardwalk candidate: hook on may stay closer to plain if proxy exempts
        val bwPlain = route(FfiComapsProfile.PEDESTRIAN, 61.04, 11.46, 61.055, 11.49, false)
        val bwWet = route(FfiComapsProfile.PEDESTRIAN, 61.04, 11.46, 61.055, 11.49, true)
        writeGeoJson("boardwalk_proxy_plain", bwPlain.points)
        writeGeoJson("boardwalk_proxy_wetland", bwWet.points)
        assertTrue(bwPlain.code == FfiComapsRouteCode.OK)
        assertTrue(bwWet.code == FfiComapsRouteCode.OK)
        routes.put(
            JSONObject()
                .put("id", "boardwalk_proxy_hedmark")
                .put("plain_m", bwPlain.lengthM)
                .put("wetland_m", bwWet.lengthM)
                .put("delta_m", bwWet.lengthM - bwPlain.lengthM)
                .put(
                    "note",
                    "boardwalk proxy ~25% recall; small delta suggests exemption or no Soft/Hard on path",
                ),
        )

        // Bike wetland option (default off): same endpoints as foot pin
        comapsSessionSetBikeWetland(false)
        val bikeOff = route(FfiComapsProfile.BICYCLE, 61.05, 11.47, 61.08, 11.52, wetland = true)
        comapsSessionSetBikeWetland(true)
        val bikeOn = route(FfiComapsProfile.BICYCLE, 61.05, 11.47, 61.08, 11.52, wetland = true)
        comapsSessionSetBikeWetland(false)
        assertTrue(bikeOff.code == FfiComapsRouteCode.OK)
        assertTrue(bikeOn.code == FfiComapsRouteCode.OK)
        routes.put(
            JSONObject()
                .put("id", "bike_wetland_option")
                .put("off_m", bikeOff.lengthM)
                .put("on_m", bikeOn.lengthM)
                .put("delta_m", bikeOn.lengthM - bikeOff.lengthM),
        )

        // Mountain elevation sample corridors (ascent/time with hook; sidecar vs CoMaps)
        data class ElevCase(val id: String, val lat0: Double, val lon0: Double, val lat1: Double, val lon1: Double)
        val elevCases =
            listOf(
                ElevCase("valdresflye_car", 61.39, 8.80, 61.45, 8.95),
                ElevCase("rv7_hallingdal", 60.63, 8.56, 60.70, 8.80),
                ElevCase("winter_mountain_rv27", 61.90, 10.20, 62.00, 10.40),
            )
        val elevReport = JSONArray()
        for (ec in elevCases) {
            val plain = route(FfiComapsProfile.CAR, ec.lat0, ec.lon0, ec.lat1, ec.lon1, false)
            val wet = route(FfiComapsProfile.CAR, ec.lat0, ec.lon0, ec.lat1, ec.lon1, true)
            elevReport.put(
                JSONObject()
                    .put("id", ec.id)
                    .put("plain_code", plain.code.name)
                    .put("plain_m", plain.lengthM)
                    .put("plain_time_s", plain.timeS)
                    .put("hook_code", wet.code.name)
                    .put("hook_m", wet.lengthM)
                    .put("hook_time_s", wet.timeS)
                    .put(
                        "note",
                        "time/length with wetland+elevation hook; NeedMoreMaps if leaf absent",
                    ),
            )
            routes.put(
                JSONObject()
                    .put("id", ec.id)
                    .put("plain_code", plain.code.name)
                    .put("hook_code", wet.code.name)
                    .put("plain_m", plain.lengthM)
                    .put("hook_m", wet.lengthM),
            )
        }

        summary.put("routes", routes)
        summary.put("turns", turnsReport)
        summary.put("elevation", elevReport)
        summary.put(
            "product_options",
            JSONObject()
                .put("optimize_default_recommendation", "Time")
                .put("distance_supported", false)
                .put("prefer_non_motorway_demo", "lillehammer_drammen_*")
                .put("bike_wetland_default_recommendation", "off"),
        )
        File(outDir, "phase4-summary.json").writeText(summary.toString(2))
        comapsSessionClose()
    }
}
