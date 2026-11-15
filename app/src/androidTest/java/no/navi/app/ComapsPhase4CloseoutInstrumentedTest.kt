package no.navi.app

import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import org.json.JSONArray
import org.json.JSONObject
import org.junit.Assert.assertFalse
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
import uniffi.navi.comapsSessionSetBikeWetland
import uniffi.navi.comapsSessionSetOptimize
import uniffi.navi.comapsSessionSetTurnsLocale
import java.io.File
import java.io.FileOutputStream

/**
 * Phase 4 closeout: Valdresflye error mapping, instructions nb/en + voice log,
 * elevation profile, Lillehammer→Drammen timing/mode.
 */
@RunWith(AndroidJUnit4::class)
class ComapsPhase4CloseoutInstrumentedTest {
    private lateinit var root: File
    private lateinit var outDir: File

    @Before
    fun setUp() {
        MwmClientBootstrap.ensureInitialized()
        check(comapsNativeAvailable())
        val ctx = InstrumentationRegistry.getInstrumentation().targetContext
        root = File(ctx.getExternalFilesDir(null), "navi-mwm-test")
        outDir = File(root, "out-phase4-closeout")
        outDir.mkdirs()
        File(root, "261105").mkdirs()
        for (name in ctx.assets.list("comaps-data").orEmpty()) {
            if (name == "MANIFEST.txt") continue
            val dest = File(root, name)
            if (dest.isFile && dest.length() > 0L) continue
            ctx.assets.open("comaps-data/$name").use { input ->
                FileOutputStream(dest).use { output -> input.copyTo(output) }
            }
        }
    }

    private fun assertNoInternal(code: FfiComapsRouteCode, msg: String?) {
        assertFalse(
            "INTERNAL must not reach user: $msg",
            code == FfiComapsRouteCode.INTERNAL,
        )
    }

    /** New elev JSON is `{comaps_ascent_m, points:[...]}`; legacy was a bare array. */
    private fun elevMeta(json: String): JSONObject {
        val trimmed = json.trim()
        return if (trimmed.startsWith("{")) {
            val o = JSONObject(trimmed)
            JSONObject()
                .put("comaps_ascent_m", o.optDouble("comaps_ascent_m"))
                .put("comaps_descent_m", o.optDouble("comaps_descent_m"))
                .put("ascent_m", o.optDouble("ascent_m"))
                .put("descent_m", o.optDouble("descent_m"))
                .put("points", o.optJSONArray("points")?.length() ?: 0)
                .put(
                    "sidecar_points",
                    (0 until (o.optJSONArray("points")?.length() ?: 0)).count { i ->
                        o.getJSONArray("points").optJSONObject(i)?.optString("source") == "sidecar"
                    },
                )
        } else {
            JSONObject().put("points", JSONArray(trimmed).length()).put("legacy_array", true)
        }
    }

    @Test
    fun phase4CloseoutValdresflyeInstructionsElevationPerf() {
        val open =
            comapsSessionOpen(
                storagePath = root.absolutePath,
                writablePath = root.absolutePath,
                sidecarDir = File(root, "sidecars").absolutePath,
                loadWetlandSidecars = true,
            )
        assertTrue("session: ${open.errorMessage}", open.ok)
        comapsSessionSetTurnsLocale("nb")
        comapsSessionSetBikeWetland(false)
        comapsSessionSetOptimize(FfiComapsOptimize.TIME)

        val summary = JSONObject()
        val spoken = JSONArray()

        // 1) Valdresflye car — must not be INTERNAL
        val vf =
            comapsSessionRoute(
                FfiComapsProfile.CAR,
                61.39,
                8.80,
                61.45,
                8.95,
                wetlandHook = false,
            )
        assertNoInternal(vf.code, vf.errorMessage)
        summary.put(
            "valdresflye_car",
            JSONObject()
                .put("code", vf.code.name)
                .put("error", vf.errorMessage)
                .put("length_m", vf.lengthM)
                .put("ascent_m", vf.ascentM)
                .put("descent_m", vf.descentM)
                .put("router_mode", vf.routerMode),
        )

        // Try a car-road corridor on Valdresflye (Beitostølen / Rv51) if first failed
        val vf2 =
            comapsSessionRoute(
                FfiComapsProfile.CAR,
                61.2440,
                8.9200,
                61.3860,
                8.8120,
                wetlandHook = true,
            )
        assertNoInternal(vf2.code, vf2.errorMessage)
        summary.put(
            "valdresflye_rv51_beitostolen",
            JSONObject()
                .put("code", vf2.code.name)
                .put("error", vf2.errorMessage)
                .put("length_m", vf2.lengthM)
                .put("ascent_m", vf2.ascentM)
                .put("descent_m", vf2.descentM)
                .put("ele_meta", elevMeta(vf2.elevationJson)),
        )

        // 2) Turn-by-turn + ferry: instructions_json must have text_nb/text_en/voice
        data class Tbt(
            val id: String,
            val profile: FfiComapsProfile,
            val la0: Double,
            val lo0: Double,
            val la1: Double,
            val lo1: Double,
        )
        val tbts =
            listOf(
                Tbt("hamar_elverum", FfiComapsProfile.CAR, 60.7945, 11.0680, 60.8825, 11.5623),
                Tbt("oslo_ring", FfiComapsProfile.CAR, 59.9200, 10.7400, 59.9400, 10.7600),
                Tbt("moss_horten_ferry", FfiComapsProfile.CAR, 59.4358, 10.6600, 59.4130, 10.4800),
            )
        val tbtReport = JSONArray()
        for (t in tbts) {
            val r =
                comapsSessionRoute(t.profile, t.la0, t.lo0, t.la1, t.lo1, wetlandHook = false)
            assertTrue("${t.id} ${r.code} ${r.errorMessage}", r.code == FfiComapsRouteCode.OK)
            assertNoInternal(r.code, r.errorMessage)
            val instr = JSONArray(r.instructionsJson)
            assertTrue("${t.id}: empty instructions", instr.length() > 0)
            var ferrySteps = 0
            for (i in 0 until instr.length()) {
                val step = instr.getJSONObject(i)
                assertTrue(step.optString("text_nb").isNotBlank())
                assertTrue(step.optString("text_en").isNotBlank())
                assertTrue(
                    step.optString("voice_nb").isNotBlank() || step.optString("voice_en").isNotBlank(),
                )
                if (step.optBoolean("is_ferry")) {
                    ferrySteps++
                    assertTrue(
                        "${t.id}: ferry text",
                        step.optString("text_en").contains("ferry", ignoreCase = true) ||
                            step.optString("text_nb").contains("ferge", ignoreCase = true),
                    )
                }
                spoken.put(
                    JSONObject()
                        .put("route", t.id)
                        .put("seq", step.optInt("sequence"))
                        .put("voice_nb", step.optString("voice_nb"))
                        .put("voice_en", step.optString("voice_en"))
                        .put("is_ferry", step.optBoolean("is_ferry")),
                )
            }
            if (t.id.contains("ferry")) {
                assertTrue("${t.id}: expected ≥1 ferry step", ferrySteps >= 1)
            }
            File(outDir, "${t.id}.instructions.json").writeText(r.instructionsJson)
            tbtReport.put(
                JSONObject()
                    .put("id", t.id)
                    .put("steps", instr.length())
                    .put("ascent_m", r.ascentM)
                    .put("descent_m", r.descentM)
                    .put("router_mode", r.routerMode),
            )
        }
        summary.put("turn_by_turn", tbtReport)

        // 3) Lillehammer→Drammen timing + mode
        val ld =
            comapsSessionRoute(
                FfiComapsProfile.CAR,
                61.1153,
                10.4662,
                59.7440,
                10.2045,
                wetlandHook = false,
            )
        assertTrue(ld.code == FfiComapsRouteCode.OK)
        summary.put(
            "lillehammer_drammen",
            JSONObject()
                .put("code", ld.code.name)
                .put("length_m", ld.lengthM)
                .put("build_s", ld.buildTimeS)
                .put("router_mode", ld.routerMode)
                .put("ascent_m", ld.ascentM)
                .put("descent_m", ld.descentM)
                .put("under_3s", ld.buildTimeS < 3.0),
        )

        // 4) Mountain elevation with/without wetland hook (sidecar elev)
        val elevCases =
            listOf(
                Triple("rv7_hallingdal", 60.63 to 8.56, 60.70 to 8.80),
                Triple("rv27", 61.90 to 10.20, 62.00 to 10.40),
            )
        val elevReport = JSONArray()
        for ((id, from, to) in elevCases) {
            val plain =
                comapsSessionRoute(
                    FfiComapsProfile.CAR,
                    from.first,
                    from.second,
                    to.first,
                    to.second,
                    wetlandHook = false,
                )
            val hooked =
                comapsSessionRoute(
                    FfiComapsProfile.CAR,
                    from.first,
                    from.second,
                    to.first,
                    to.second,
                    wetlandHook = true,
                )
            elevReport.put(
                JSONObject()
                    .put("id", id)
                    .put("plain_code", plain.code.name)
                    .put("plain_ascent", plain.ascentM)
                    .put("plain_descent", plain.descentM)
                    .put("plain_elev", elevMeta(plain.elevationJson))
                    .put("hook_code", hooked.code.name)
                    .put("hook_ascent", hooked.ascentM)
                    .put("hook_descent", hooked.descentM)
                    .put("hook_elev", elevMeta(hooked.elevationJson)),
            )
            File(outDir, "$id.elevation.plain.json").writeText(plain.elevationJson)
            File(outDir, "$id.elevation.hook.json").writeText(hooked.elevationJson)
        }
        summary.put("elevation", elevReport)
        summary.put("spoken_prompts", spoken)

        File(outDir, "phase4-closeout-summary.json").writeText(summary.toString(2))
        File(outDir, "spoken-prompts.log").writeText(
            buildString {
                for (i in 0 until spoken.length()) {
                    val s = spoken.getJSONObject(i)
                    appendLine(
                        "[${s.getString("route")}#${s.getInt("seq")}] nb=${s.optString("voice_nb")} | en=${s.optString("voice_en")}",
                    )
                }
            },
        )
        comapsSessionClose()
    }
}
