package no.navi.app

import org.json.JSONArray
import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import uniffi.navi.FfiComapsLatLon
import uniffi.navi.FfiComapsRoute
import uniffi.navi.FfiComapsRouteCode

class TunnelAvoidSpeedTest {
    @Test
    fun legCacheHitsSameEndpointsAndReusesGeometry() {
        val cache = TunnelLegCache()
        val flags = flags()
        val key = tunnelLegCacheKey(59.90, 10.76, 63.42, 10.39, flags)
        val route = leg(lengthM = 1200.0, timeS = 900.0, turns = """[{"street":"Leg","dist_m":1200}]""")
        cache.put(key, route)
        val hit = cache.get(key)
        assertNotNull(hit)
        assertEquals(1200.0, hit!!.lengthM, 0.0)
        assertEquals(900.0, hit.timeS, 0.0)
        assertEquals(0.0, hit.buildTimeS, 0.0)
        assertEquals(route.turnsJson, hit.turnsJson)
        assertEquals(59.9, hit.points[0].lat, 0.0)
        assertEquals(10.7, hit.points[0].lon, 0.0)
        assertEquals(1, cache.hits)
        assertEquals(0, cache.misses)
    }

    @Test
    fun legCacheMissesWhenEndpointOrFlagChanges() {
        val cache = TunnelLegCache()
        val flags = flags()
        val key = tunnelLegCacheKey(59.90, 10.76, 63.42, 10.39, flags)
        cache.put(key, leg(lengthM = 10.0, timeS = 10.0, turns = "[]"))
        val moved =
            tunnelLegCacheKey(59.90, 10.76, 63.43, 10.39, flags)
        assertNull(cache.get(moved))
        val otherSide =
            tunnelLegCacheKey(59.91, 10.76, 63.42, 10.39, flags)
        assertNull(cache.get(otherSide))
        val ferry = tunnelLegCacheKey(59.90, 10.76, 63.42, 10.39, flags(avoidFerry = true))
        assertNull(cache.get(ferry))
        assertEquals(0, cache.hits)
        assertEquals(3, cache.misses)
        assertNotNull(cache.get(key))
        assertEquals(1, cache.hits)
    }

    @Test
    fun longestRunIsTargetedNotTheFirstAnchor() {
        val poly = southbound(startLat = 62.0, stepM = 500.0, count = 60)
        val turns =
            JSONArray()
                .put(turn(false, 2_000.0, "Surface"))
                .put(turn(true, 400.0, "Short"))
                .put(turn(false, 3_000.0, "Surface"))
                .put(turn(true, 4_850.0, "Long"))
                .put(turn(false, 5_000.0, "Surface"))
                .toString()
        val run = MwmProductionPlanner.longestTunnelRun(turns, poly)
        assertNotNull(run)
        assertEquals(4_850.0, run!!.lengthM, 0.1)
        assertEquals(5_400.0, run.startM, 0.1)
        assertEquals(3, run.anchors.size)
        assertTrue(
            "entrance ${run.anchors[0].first} should be north of exit ${run.anchors[2].first}",
            run.anchors[0].first > run.anchors[2].first,
        )
        val shortMidLat = 62.0 - ((2_000.0 + 200.0) / 111_320.0)
        assertTrue(
            "longest anchors lat=${run.anchors[1].first} shortMid=$shortMidLat",
            run.anchors[1].first < shortMidLat,
        )
    }

    @Test
    fun knownDetourFingerprintSkipsTheNativeCall() {
        val seen = HashSet<String>()
        val vias = listOf(60.79 to 11.04, 60.80 to 11.05)
        val first = detourViaFingerprint(vias)
        assertFalse(skipKnownDetour(seen, first))
        assertTrue(skipKnownDetour(seen, detourViaFingerprint(vias)))
        val moved = listOf(60.79 to 11.04, 60.85 to 11.10)
        assertFalse(skipKnownDetour(seen, detourViaFingerprint(moved)))
        assertTrue(first != detourViaFingerprint(moved))
    }

    @Test
    fun roundAdvancesWhenCountDropsAndFinishesOnlyAtZero() {
        assertEquals(
            TunnelRoundStep.ADVANCE_ROUND,
            tunnelRoundStep(beforeCount = 15, afterCount = 14, geometryChanged = true),
        )
        assertEquals(
            TunnelRoundStep.FINISH,
            tunnelRoundStep(beforeCount = 1, afterCount = 0, geometryChanged = true),
        )
        assertEquals(
            TunnelRoundStep.KEEP_TRYING,
            tunnelRoundStep(beforeCount = 15, afterCount = 14, geometryChanged = false),
        )
        assertEquals(
            TunnelRoundStep.KEEP_TRYING,
            tunnelRoundStep(beforeCount = 15, afterCount = 15, geometryChanged = true),
        )
        assertEquals(
            TunnelRoundStep.KEEP_TRYING,
            tunnelRoundStep(beforeCount = 15, afterCount = 16, geometryChanged = true),
        )
    }

    @Test
    fun offsetEscalationReachesWideOffsetsOnOneSide() {
        val offsets =
            listOf(1_500.0, 3_000.0, 5_000.0, 8_000.0, 12_000.0, 16_000.0, 20_000.0)
        val escalation = TunnelSideEscalation(offsets, primarySide = 1.0, otherSide = -1.0)
        val got = ArrayList<Pair<Double, Double>>()
        var step = escalation.next(lastPrimaryUnchanged = false)
        var attempts = 0
        while (step != null && attempts < 8) {
            got.add(step)
            attempts++
            step = escalation.next(lastPrimaryUnchanged = false)
        }
        assertEquals(offsets, got.map { it.first })
        assertTrue(got.all { it.second == 1.0 })
        assertTrue(attempts <= 8)
    }

    @Test
    fun otherSideRunsOnlyWhenPrimaryFingerprintStandsStill() {
        val escalation =
            TunnelSideEscalation(
                offsetsM = listOf(1_500.0, 3_000.0, 5_000.0),
                primarySide = 1.0,
                otherSide = -1.0,
            )
        val first = escalation.next(lastPrimaryUnchanged = false)
        val second = escalation.next(lastPrimaryUnchanged = true)
        val third = escalation.next(lastPrimaryUnchanged = false)
        assertEquals(1_500.0, first!!.first, 0.0)
        assertEquals(1.0, first.second, 0.0)
        assertEquals(1_500.0, second!!.first, 0.0)
        assertEquals(-1.0, second.second, 0.0)
        assertEquals(3_000.0, third!!.first, 0.0)
        assertEquals(1.0, third.second, 0.0)
    }

    @Test
    fun spliceAcceptedOnlyWhenCountDropsAndNoNewRunAppears() {
        val before =
            JSONArray()
                .put(turn(true, 100.0, "A"))
                .put(turn(false, 50.0, "Surface"))
                .put(turn(true, 4_850.0, "B"))
                .put(turn(false, 50.0, "Surface"))
                .put(turn(true, 80.0, "C"))
                .toString()
        val dropped =
            JSONArray()
                .put(turn(true, 100.0, "A"))
                .put(turn(false, 50.0, "Surface"))
                .put(turn(false, 4_200.0, "Detour"))
                .put(turn(false, 50.0, "Surface"))
                .put(turn(true, 80.0, "C"))
                .toString()
        assertTrue(acceptTunnelSplice(before, dropped))
        val newRun =
            JSONArray()
                .put(turn(true, 100.0, "A"))
                .put(turn(false, 50.0, "Surface"))
                .put(turn(true, 300.0, "NewTunnel"))
                .toString()
        assertFalse(acceptTunnelSplice(before, newRun))
        assertFalse(acceptTunnelSplice(before, before))
    }

    @Test
    fun spliceReplacesTheTunnelWindowAndKeepsTheEnds() {
        val poly = southbound(startLat = 62.0, stepM = 500.0, count = 12)
        val turns =
            JSONArray()
                .put(turn(false, 1_000.0, "Surface"))
                .put(turn(true, 2_000.0, "OldTunnel"))
                .put(turn(false, 2_000.0, "Surface"))
                .toString()
        val windowPoly = "11.2,61.99;11.2,61.98"
        val windowTurns = JSONArray().put(turn(false, 1_800.0, "Detour")).toString()
        val spliced =
            spliceTunnelWindow(
                polyline = poly,
                turnsJson = turns,
                windowStartM = 1_000.0,
                windowEndM = 3_000.0,
                windowPolyline = windowPoly,
                windowTurnsJson = windowTurns,
                windowLengthM = 1_800.0,
            )
        assertNotNull(spliced)
        assertTrue(spliced!!.polyline.contains("11.2,61.99"))
        assertTrue(spliced.polyline.startsWith("10.0,62.0"))
        assertEquals(0, MwmProductionPlanner.tunnelRunCount(spliced.turnsJson))
        assertTrue(acceptTunnelSplice(turns, spliced.turnsJson))
        val rejected =
            spliceTunnelWindow(
                polyline = poly,
                turnsJson = turns,
                windowStartM = 1_000.0,
                windowEndM = 3_000.0,
                windowPolyline = windowPoly,
                windowTurnsJson = JSONArray().put(turn(true, 400.0, "BrandNew")).toString(),
                windowLengthM = 400.0,
            )
        assertNotNull(rejected)
        assertFalse(acceptTunnelSplice(turns, rejected!!.turnsJson))
        assertEquals(1, MwmProductionPlanner.tunnelRunCount(turns))
    }

    @Test
    fun spliceWindowStaysOnOneLeg() {
        val open =
            spliceWindowMeters(
                runStartM = 100_000.0,
                runEndM = 104_850.0,
                routeLengthM = 515_000.0,
                viaAlongM = listOf(2_000.0, 120_000.0, 200_000.0),
            )
        assertNotNull(open)
        assertTrue(open!!.second - open.first < 15_000.0)
        assertNull(
            spliceWindowMeters(
                runStartM = 100_000.0,
                runEndM = 104_850.0,
                routeLengthM = 515_000.0,
                viaAlongM = listOf(102_000.0),
            ),
        )
    }

    private fun flags(avoidFerry: Boolean = false): TunnelLegFlags =
        TunnelLegFlags(
            profile = "PEDESTRIAN",
            avoidToll = false,
            avoidMotorway = false,
            avoidFerry = avoidFerry,
            avoidDirty = false,
            optimize = "time",
            wetlandHook = true,
            turnsLocale = "nb",
            vehicleKey = "none",
        )

    private fun leg(
        lengthM: Double,
        timeS: Double,
        turns: String,
    ): FfiComapsRoute =
        FfiComapsRoute(
            code = FfiComapsRouteCode.OK,
            lengthM = lengthM,
            timeS = timeS,
            buildTimeS = 3.0,
            points = listOf(FfiComapsLatLon(59.9, 10.7), FfiComapsLatLon(63.4, 10.4)),
            errorMessage = null,
            turnsJson = turns,
            instructionsJson = "[]",
            ascentM = 1.0,
            descentM = 1.0,
            elevationJson = "[]",
            routerMode = "pedestrian",
            speedLimitsJson = "[]",
        )

    private fun turn(
        tunnel: Boolean,
        distM: Double,
        street: String,
    ): JSONObject =
        JSONObject()
            .put("is_tunnel", tunnel)
            .put("dist_m", distM)
            .put("street", street)

    private fun southbound(
        startLat: Double,
        stepM: Double,
        count: Int,
    ): String {
        val dLat = stepM / 111_320.0
        return buildString {
            for (i in 0 until count) {
                if (i > 0) append(';')
                append("10.0,${startLat - i * dLat}")
            }
        }
    }
}
