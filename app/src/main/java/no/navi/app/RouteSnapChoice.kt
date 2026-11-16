package no.navi.app

import uniffi.navi.FfiComapsProfile
import uniffi.navi.FfiComapsRouteCode
import uniffi.navi.FfiSnapSuggestion
import uniffi.navi.TravelProfile

/** Which endpoint CoMaps could not match to a road. */
enum class NotOnRoadRole { START, END }

/** Proposed endpoint on the nearest drivable road. Never applied without a confirm. */
data class SnapOffer(
    val role: NotOnRoadRole,
    val originalLat: Double,
    val originalLon: Double,
    val snapLat: Double,
    val snapLon: Double,
    val distanceM: Double,
    val mwmName: String? = null,
)

/**
 * User choice flow for START_NOT_ON_ROAD / END_NOT_ON_ROAD (Phase 5, A2).
 *
 * The router never snaps an endpoint by itself. The only path from a failed
 * route to a snapped route is
 * `NotOnRoad -> RequestingSnap -> Preview -> confirm()`; every other transition
 * ends without an endpoint to route to. Pure logic, no Android or native
 * dependencies, so it is unit tested on the JVM.
 */
class SnapChoiceFlow {
    sealed interface State {
        data object Idle : State

        /** Route failed; show the message and the optional snap button. */
        data class NotOnRoad(
            val role: NotOnRoadRole,
            val lat: Double,
            val lon: Double,
        ) : State

        /** Snap lookup in flight. */
        data class RequestingSnap(
            val role: NotOnRoadRole,
            val lat: Double,
            val lon: Double,
        ) : State

        /** Snap point and distance are shown; waiting for confirm or cancel. */
        data class Preview(
            val offer: SnapOffer,
        ) : State

        /** No drivable road inside the lookup radius. */
        data class NoRoadNearby(
            val role: NotOnRoadRole,
        ) : State
    }

    var state: State = State.Idle
        private set

    /**
     * A route finished with [failure]. Only not-on-road failures open the flow;
     * [lat]/[lon] is the endpoint that failed.
     */
    fun onRouteFailed(
        failure: NotOnRoadRole?,
        lat: Double,
        lon: Double,
    ) {
        state = if (failure == null) State.Idle else State.NotOnRoad(failure, lat, lon)
    }

    /** User tapped "Route to nearest drivable road". Returns the lookup to run, or null. */
    fun requestSnap(): State.RequestingSnap? {
        val s = state as? State.NotOnRoad ?: return null
        val next = State.RequestingSnap(s.role, s.lat, s.lon)
        state = next
        return next
    }

    /** Result of the snap lookup; a null or non-matching offer means no road nearby. */
    fun onSnapResult(offer: SnapOffer?) {
        val s = state as? State.RequestingSnap ?: return
        state = if (offer != null && offer.role == s.role) State.Preview(offer) else State.NoRoadNearby(s.role)
    }

    /**
     * User confirmed the preview. Returns the offer to route to; null in every
     * other state, so nothing is routed without passing through [State.Preview].
     */
    fun confirm(): SnapOffer? {
        val s = state as? State.Preview ?: return null
        state = State.Idle
        return s.offer
    }

    /** Cancel from any state; the original endpoint stays as the user set it. */
    fun dismiss() {
        state = State.Idle
    }

    companion object {
        /** Not-on-road role for a CoMaps route code; null for every other code. */
        fun roleFor(code: FfiComapsRouteCode): NotOnRoadRole? =
            when (code) {
                FfiComapsRouteCode.START_NOT_ON_ROAD -> NotOnRoadRole.START
                FfiComapsRouteCode.END_NOT_ON_ROAD -> NotOnRoadRole.END
                else -> null
            }

        private const val START_MESSAGE = "Start is not on a routable road"
        private const val END_MESSAGE = "Destination is not on a routable road"

        /**
         * Not-on-road role from a plan report. Recognises the explicit
         * `comaps_route_code=` token and the shim's error messages.
         */
        fun roleFromReport(report: String): NotOnRoadRole? {
            val token =
                Regex("""comaps_route_code=([A-Za-z_]+)""")
                    .find(report)
                    ?.groupValues
                    ?.getOrNull(1)
                    ?.replace("_", "")
                    ?.lowercase()
            when (token) {
                "startnotonroad" -> return NotOnRoadRole.START
                "endnotonroad" -> return NotOnRoadRole.END
            }
            return when {
                report.contains(START_MESSAGE, ignoreCase = true) -> NotOnRoadRole.START
                report.contains(END_MESSAGE, ignoreCase = true) -> NotOnRoadRole.END
                else -> null
            }
        }

        /** Routing profile CoMaps uses for the nearest-road lookup. */
        fun comapsProfileFor(profile: TravelProfile): FfiComapsProfile =
            when (profile) {
                TravelProfile.HIKING -> FfiComapsProfile.PEDESTRIAN
                TravelProfile.BICYCLE, TravelProfile.BICYCLE_ELECTRIC -> FfiComapsProfile.BICYCLE
                else -> FfiComapsProfile.CAR
            }

        /** Convert the native suggestion; null when the engine offered nothing. */
        fun offerFrom(s: FfiSnapSuggestion): SnapOffer? =
            if (!s.found) {
                null
            } else {
                SnapOffer(
                    role = if (s.isStart) NotOnRoadRole.START else NotOnRoadRole.END,
                    originalLat = s.originalLat,
                    originalLon = s.originalLon,
                    snapLat = s.snapLat,
                    snapLon = s.snapLon,
                    distanceM = s.distanceM,
                    mwmName = s.mwmName,
                )
            }

        /** `85 m` / `1.4 km`, same rule as the Rust `format_snap_distance`. */
        fun formatDistance(distanceM: Double): String =
            if (distanceM < 1000.0) {
                "${Math.round(distanceM)} m"
            } else {
                String.format(java.util.Locale.US, "%.1f km", distanceM / 1000.0)
            }
    }
}
