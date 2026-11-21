package no.navi.app

import org.json.JSONArray
import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import uniffi.navi.CorridorRouteResult
import uniffi.navi.TravelProfile
import java.io.File
import java.nio.ByteBuffer
import java.nio.ByteOrder
import java.nio.charset.StandardCharsets

/**
 * Fixture is the sidecar-format.md §6 JSONL example, wrapped in a minimal NVSC v1
 * blob (not a live MWM):
 * `{"osm_relation_id":12345,"kind":"pilgrimage","name":"St. Olavsleden","ref":"","network":"nwn","lat":61.12,"lon":10.46}`
 */
class NamedRouteSidecarTest {
    @Test
    fun specExampleDecodesPilgrimStub() {
        val load = NamedRouteSidecar.parseOuter(res("spec-example.navi-sidecar.v1.zst"), res("spec-example.mwm"))
        val stub = (load as NamedRouteSidecar.Load.Stubs).routes.single()
        assertEquals(12345L, stub.osmId)
        assertEquals(NamedRouteSidecar.KIND_PILGRIMAGE, stub.kind)
        assertEquals("pilgrimage", stub.kindName)
        assertEquals("St. Olavsleden", stub.name)
        assertEquals(61.12, stub.lat, 1e-7)
        assertEquals(10.46, stub.lon, 1e-7)
    }

    @Test
    fun specExampleMatchesHandBuiltNr01Record() {
        val name = "St. Olavsleden".toByteArray(StandardCharsets.UTF_8)
        val buf =
            ByteBuffer.allocate(4 + 4 + 8 + 1 + 4 + 4 + 2 + name.size).order(ByteOrder.LITTLE_ENDIAN)
        buf.put("NR01".toByteArray(StandardCharsets.US_ASCII))
        buf.putInt(1)
        buf.putLong(12345L)
        buf.put(NamedRouteSidecar.KIND_PILGRIMAGE.toByte())
        buf.putInt(611_200_000)
        buf.putInt(104_600_000)
        buf.putShort(name.size.toShort())
        buf.put(name)
        val fromBytes = NamedRouteSidecar.parseNamedRoutesBody(buf.array())
        val fromFile =
            NamedRouteSidecar.parseOuter(res("spec-example.navi-sidecar.v1.zst"), res("spec-example.mwm"))
                as NamedRouteSidecar.Load.Stubs
        assertEquals(fromBytes, fromFile.routes)
    }

    @Test
    fun omittedSectionIsNotAvailableAndEmptySectionIsAnnotated() {
        val omitted = NamedRouteSidecar.parseOuter(res("section-omitted.navi-sidecar.v1.zst"), res("spec-example.mwm"))
        assertTrue(omitted is NamedRouteSidecar.Load.NotAvailable)
        val empty = NamedRouteSidecar.parseOuter(res("count-zero.navi-sidecar.v1.zst"), res("spec-example.mwm"))
        assertTrue(empty is NamedRouteSidecar.Load.Empty)
    }

    @Test
    fun badMagicAndNewerVersionAreRejectedWithoutThrowing() {
        val logical = decompress(res("spec-example.navi-sidecar.v1.zst"))
        logical[0] = 'X'.code.toByte()
        val badMagic = NamedRouteSidecar.parseLogical(logical)
        assertTrue(badMagic is NamedRouteSidecar.Load.Rejected)
        logical[0] = 'N'.code.toByte()
        logical[4] = 2
        val badVersion = NamedRouteSidecar.parseLogical(logical)
        assertEquals("unsupported_version", (badVersion as NamedRouteSidecar.Load.Rejected).reason)
    }

    @Test
    fun fingerprintMismatchRejectsTheSidecar() {
        val load = NamedRouteSidecar.parseOuter(res("spec-example.navi-sidecar.v1.zst"), byteArrayOf(1, 2, 3))
        assertEquals("fingerprint", (load as NamedRouteSidecar.Load.Rejected).reason)
    }

    @Test
    fun pilgrimToggleReadsTheStubAndDoesNotChangeTheLine() {
        val root = install("spec-example.navi-sidecar.v1.zst")
        val before = corridor()
        val after =
            NamedRouteSidecar.annotate(
                mwmRoot = root,
                leaves = listOf("Norway_Example"),
                profile = TravelProfile.HIKING,
                preferOfficialNetworks = false,
                preferPilgrimRoutes = true,
                result = before,
            )
        assertEquals(before.routePolyline, after.routePolyline)
        assertEquals(before.distanceKm, after.distanceKm, 0.0)
        assertTrue(after.report.contains("named_routes=stubs\n"))
        assertTrue(after.report.contains("named_routes_followed=false\n"))
        assertFalse(after.report.contains("named_routes_followed=true"))
        val row = jsonStub(after.report)
        assertEquals(12345L, row.getLong("osm_id"))
        assertEquals("pilgrimage", row.getString("kind"))
        assertEquals("St. Olavsleden", row.getString("name"))
        assertEquals(61.12, row.getDouble("lat"), 1e-7)
        assertEquals(10.46, row.getDouble("lon"), 1e-7)
    }

    @Test
    fun officialNetworksToggleDoesNotTreatPilgrimKindAsAMatch() {
        val root = install("spec-example.navi-sidecar.v1.zst")
        val after =
            NamedRouteSidecar.annotate(
                mwmRoot = root,
                leaves = listOf("Norway_Example"),
                profile = TravelProfile.HIKING,
                preferOfficialNetworks = true,
                preferPilgrimRoutes = false,
                result = corridor(),
            )
        assertTrue(after.report.contains("named_routes=none\n"))
        assertTrue(after.report.contains("named_routes_followed=false\n"))
        assertFalse(after.report.contains("named_routes_json="))
        assertEquals(PEDESTRIAN_LINE, after.routePolyline)
    }

    @Test
    fun bothHikingTogglesKeepThePilgrimStub() {
        val root = install("spec-example.navi-sidecar.v1.zst")
        val after =
            NamedRouteSidecar.annotate(
                mwmRoot = root,
                leaves = listOf("Norway_Example"),
                profile = TravelProfile.HIKING,
                preferOfficialNetworks = true,
                preferPilgrimRoutes = true,
                result = corridor(),
            )
        assertEquals("pilgrimage", jsonStub(after.report).getString("kind"))
        assertEquals(PEDESTRIAN_LINE, after.routePolyline)
    }

    @Test
    fun missingSidecarDoesNotPretendARouteWasFollowed() {
        val root = install(sidecar = null)
        val before = corridor()
        val after =
            NamedRouteSidecar.annotate(
                mwmRoot = root,
                leaves = listOf("Norway_Example"),
                profile = TravelProfile.HIKING,
                preferOfficialNetworks = false,
                preferPilgrimRoutes = true,
                result = before,
            )
        assertEquals(before.routePolyline, after.routePolyline)
        assertTrue(after.report.contains("named_routes=not_available\n"))
        assertTrue(after.report.contains("named_routes_followed=false\n"))
        assertTrue(after.report.contains("named_routes_unavailable=Norway_Example\n"))
        assertFalse(after.report.contains("St. Olavsleden"))
    }

    @Test
    fun omittedSectionAndEmptySectionStayOffThePedestrianLine() {
        val omitted = annotateInstalled("section-omitted.navi-sidecar.v1.zst", pilgrim = true, official = false)
        assertTrue(omitted.report.contains("named_routes=not_available\n"))
        assertEquals(PEDESTRIAN_LINE, omitted.routePolyline)
        val empty = annotateInstalled("count-zero.navi-sidecar.v1.zst", pilgrim = true, official = true)
        assertTrue(empty.report.contains("named_routes=none\n"))
        assertTrue(empty.report.contains("named_routes_followed=false\n"))
        assertEquals(PEDESTRIAN_LINE, empty.routePolyline)
    }

    @Test
    fun sidecarBesideTheMwmIsFound() {
        val root = install("spec-example.navi-sidecar.v1.zst", besideMwm = true)
        val after =
            NamedRouteSidecar.annotate(
                mwmRoot = root,
                leaves = listOf("Norway_Example"),
                profile = TravelProfile.HIKING,
                preferOfficialNetworks = false,
                preferPilgrimRoutes = true,
                result = corridor(),
            )
        assertEquals("St. Olavsleden", jsonStub(after.report).getString("name"))
    }

    @Test
    fun wrongMwmFingerprintDoesNotApplyTheStub() {
        val root = install("spec-example.navi-sidecar.v1.zst")
        File(root, "releases/261105/leaves/Norway_Example.mwm").writeBytes(byteArrayOf(9, 9, 9))
        val after =
            NamedRouteSidecar.annotate(
                mwmRoot = root,
                leaves = listOf("Norway_Example"),
                profile = TravelProfile.HIKING,
                preferOfficialNetworks = false,
                preferPilgrimRoutes = true,
                result = corridor(),
            )
        assertTrue(after.report.contains("named_routes=not_available\n"))
        assertFalse(after.report.contains("St. Olavsleden"))
        assertEquals(PEDESTRIAN_LINE, after.routePolyline)
    }

    @Test
    fun togglesOffOrWrongProfileDoNotTouchTheReport() {
        val root = install("spec-example.navi-sidecar.v1.zst")
        val before = corridor()
        val off =
            NamedRouteSidecar.annotate(
                mwmRoot = root,
                leaves = listOf("Norway_Example"),
                profile = TravelProfile.HIKING,
                preferOfficialNetworks = false,
                preferPilgrimRoutes = false,
                result = before,
            )
        assertEquals(before.report, off.report)
        val car =
            NamedRouteSidecar.annotate(
                mwmRoot = root,
                leaves = listOf("Norway_Example"),
                profile = TravelProfile.CAR,
                preferOfficialNetworks = true,
                preferPilgrimRoutes = true,
                result = before,
            )
        assertEquals(before.report, car.report)
        assertEquals(
            setOf(NamedRouteSidecar.KIND_BICYCLE),
            NamedRouteSidecar.wantedKinds(TravelProfile.BICYCLE, preferOfficialNetworks = true, preferPilgrimRoutes = true),
        )
        assertEquals(
            setOf(NamedRouteSidecar.KIND_HIKING, NamedRouteSidecar.KIND_PILGRIMAGE),
            NamedRouteSidecar.wantedKinds(TravelProfile.HIKING, preferOfficialNetworks = true, preferPilgrimRoutes = true),
        )
    }

    private fun annotateInstalled(
        sidecar: String,
        pilgrim: Boolean,
        official: Boolean,
    ): CorridorRouteResult =
        NamedRouteSidecar.annotate(
            mwmRoot = install(sidecar),
            leaves = listOf("Norway_Example"),
            profile = TravelProfile.HIKING,
            preferOfficialNetworks = official,
            preferPilgrimRoutes = pilgrim,
            result = corridor(),
        )

    private fun install(
        sidecar: String?,
        besideMwm: Boolean = false,
    ): File {
        val root = File(System.getProperty("java.io.tmpdir"), "named-routes-" + System.nanoTime())
        root.mkdirs()
        File(root, "active.json").writeText("""{"version":"261105"}""")
        val leaves = File(root, "releases/261105/leaves").apply { mkdirs() }
        File(leaves, "Norway_Example.mwm").writeBytes(res("spec-example.mwm"))
        if (sidecar != null) {
            val destDir =
                if (besideMwm) {
                    leaves
                } else {
                    File(root, "releases/261105/sidecars").apply { mkdirs() }
                }
            File(destDir, "Norway_Example.navi-sidecar.v1.zst").writeBytes(res(sidecar))
        }
        return root
    }

    private fun jsonStub(report: String): JSONObject {
        val line = report.lineSequence().first { it.startsWith("named_routes_json=") }
        return JSONArray(line.removePrefix("named_routes_json=")).getJSONObject(0)
    }

    private fun res(name: String): ByteArray {
        val stream =
            checkNotNull(javaClass.classLoader).getResourceAsStream("named-routes/$name")
                ?: error("missing fixture $name")
        return stream.use { it.readBytes() }
    }

    private fun decompress(src: ByteArray): ByteArray {
        java.io.ByteArrayInputStream(src).use { raw ->
            com.github.luben.zstd.ZstdInputStream(raw).use { zin ->
                return zin.readBytes()
            }
        }
    }

    private fun corridor(): CorridorRouteResult =
        CorridorRouteResult(
            report = "TEST_KIND=PLAN_MWM_ROUTE\nPASS\nPLANNER=mwm\n",
            distanceKm = 4.2,
            etaMinutes = 70.0,
            cacheHit = false,
            coldBuildS = 0.0,
            warmLoadS = 0.0,
            routePolyline = PEDESTRIAN_LINE,
            poiLat = 61.12,
            poiLon = 10.46,
            poiName = "end",
            poiIconKey = "",
            breakPoisJson = "[]",
            daysJson = "[]",
            simSamplesJson = "[]",
            maneuversJson = "[]",
            priorityPathSharePct = 0.0,
            routeSegmentsJson = "[]",
            offTrailAdvisory = "",
            tollPolicy = "allow",
            padAttemptsJson = "[]",
            searchExpansions = 0u,
            searchTerminateReason = "ok",
            tollAvoidanceIncomplete = false,
            routeUsesTolls = false,
        )

    private companion object {
        const val PEDESTRIAN_LINE = "10.4600,61.1200;10.4700,61.1300"
    }
}
