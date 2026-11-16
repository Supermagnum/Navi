package no.navi.app

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import uniffi.navi.FfiComapsRouteCode
import uniffi.navi.FfiSnapSuggestion

/** JVM tests for the START/END_NOT_ON_ROAD choice flow (never auto-snap). */
class RouteSnapChoiceTest {
    private fun offer(
        role: NotOnRoadRole = NotOnRoadRole.END,
        distanceM: Double = 1_200.0,
    ) = SnapOffer(
        role = role,
        originalLat = 61.45,
        originalLon = 8.95,
        snapLat = 61.386,
        snapLon = 8.812,
        distanceM = distanceM,
        mwmName = "Norway_Oppland",
    )

    @Test
    fun onlyNotOnRoadCodesOpenTheFlow() {
        assertEquals(NotOnRoadRole.START, SnapChoiceFlow.roleFor(FfiComapsRouteCode.START_NOT_ON_ROAD))
        assertEquals(NotOnRoadRole.END, SnapChoiceFlow.roleFor(FfiComapsRouteCode.END_NOT_ON_ROAD))
        for (code in FfiComapsRouteCode.values()) {
            if (code == FfiComapsRouteCode.START_NOT_ON_ROAD || code == FfiComapsRouteCode.END_NOT_ON_ROAD) continue
            assertNull("$code", SnapChoiceFlow.roleFor(code))
        }
    }

    @Test
    fun roleFromReport_tokenAndShimMessages() {
        assertEquals(
            NotOnRoadRole.END,
            SnapChoiceFlow.roleFromReport("FAIL: Destination is not on a routable road for this profile\n"),
        )
        assertEquals(
            NotOnRoadRole.START,
            SnapChoiceFlow.roleFromReport("FAIL: Start is not on a routable road for this profile\n"),
        )
        assertEquals(NotOnRoadRole.END, SnapChoiceFlow.roleFromReport("comaps_route_code=END_NOT_ON_ROAD"))
        assertEquals(NotOnRoadRole.START, SnapChoiceFlow.roleFromReport("comaps_route_code=StartNotOnRoad"))
        assertNull(SnapChoiceFlow.roleFromReport("FAIL: No route found"))
        assertNull(SnapChoiceFlow.roleFromReport(""))
    }

    @Test
    fun failureShowsMessageButNeverAutoSnaps() {
        val flow = SnapChoiceFlow()
        flow.onRouteFailed(NotOnRoadRole.END, 61.45, 8.95)
        assertTrue(flow.state is SnapChoiceFlow.State.NotOnRoad)
        // Nothing to route to until the user asks and confirms.
        assertNull(flow.confirm())
        assertTrue(flow.state is SnapChoiceFlow.State.NotOnRoad)
    }

    @Test
    fun otherFailuresLeaveFlowIdle() {
        val flow = SnapChoiceFlow()
        flow.onRouteFailed(null, 61.0, 9.0)
        assertTrue(flow.state is SnapChoiceFlow.State.Idle)
        assertNull(flow.requestSnap())
    }

    @Test
    fun happyPath_request_preview_confirm() {
        val flow = SnapChoiceFlow()
        flow.onRouteFailed(NotOnRoadRole.END, 61.45, 8.95)

        val lookup = flow.requestSnap()
        assertNotNull(lookup)
        assertEquals(61.45, lookup!!.lat, 0.0)
        assertTrue(flow.state is SnapChoiceFlow.State.RequestingSnap)
        // Confirm is still refused while the lookup runs.
        assertNull(flow.confirm())

        flow.onSnapResult(offer())
        val preview = flow.state as SnapChoiceFlow.State.Preview
        assertEquals(1_200.0, preview.offer.distanceM, 0.0)

        val confirmed = flow.confirm()
        assertNotNull(confirmed)
        assertEquals(61.386, confirmed!!.snapLat, 0.0)
        assertTrue(flow.state is SnapChoiceFlow.State.Idle)
        // A second confirm cannot reuse the offer.
        assertNull(flow.confirm())
    }

    @Test
    fun cancelFromPreviewRoutesNothing() {
        val flow = SnapChoiceFlow()
        flow.onRouteFailed(NotOnRoadRole.START, 61.0, 9.0)
        flow.requestSnap()
        flow.onSnapResult(offer(role = NotOnRoadRole.START))
        flow.dismiss()
        assertTrue(flow.state is SnapChoiceFlow.State.Idle)
        assertNull(flow.confirm())
    }

    @Test
    fun noRoadNearbyOrWrongRoleDoesNotPreview() {
        val none = SnapChoiceFlow()
        none.onRouteFailed(NotOnRoadRole.END, 61.45, 8.95)
        none.requestSnap()
        none.onSnapResult(null)
        assertTrue(none.state is SnapChoiceFlow.State.NoRoadNearby)
        assertNull(none.confirm())

        // A start offer cannot answer an end lookup.
        val mismatch = SnapChoiceFlow()
        mismatch.onRouteFailed(NotOnRoadRole.END, 61.45, 8.95)
        mismatch.requestSnap()
        mismatch.onSnapResult(offer(role = NotOnRoadRole.START))
        assertTrue(mismatch.state is SnapChoiceFlow.State.NoRoadNearby)
    }

    @Test
    fun snapResultIgnoredWhenNoLookupPending() {
        val flow = SnapChoiceFlow()
        flow.onSnapResult(offer())
        assertTrue(flow.state is SnapChoiceFlow.State.Idle)
    }

    @Test
    fun offerFromNativeSuggestion() {
        val found =
            FfiSnapSuggestion(
                found = true,
                isStart = false,
                originalLat = 61.45,
                originalLon = 8.95,
                snapLat = 61.386,
                snapLon = 8.812,
                distanceM = 85.0,
                featureId = 7u,
                segmentIdx = 2u,
                mwmName = "Norway_Oppland",
                message = "m",
                errorMessage = null,
            )
        val o = SnapChoiceFlow.offerFrom(found)
        assertNotNull(o)
        assertEquals(NotOnRoadRole.END, o!!.role)
        assertNull(SnapChoiceFlow.offerFrom(found.copy(found = false)))
    }

    @Test
    fun distanceFormatting() {
        assertEquals("85 m", SnapChoiceFlow.formatDistance(84.6))
        assertEquals("999 m", SnapChoiceFlow.formatDistance(999.0))
        assertEquals("1.4 km", SnapChoiceFlow.formatDistance(1_449.0))
    }
}
