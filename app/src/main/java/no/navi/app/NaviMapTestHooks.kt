package no.navi.app

/** One moving-icon / APRS-style tracked station for map overlay. */
data class TrackMarker(
    val id: String,
    val lat: Double,
    val lon: Double,
    val symbolKey: String,
    val label: String = id,
)

/** Hooks so instrumented tests can push a computed route / camera / tracks onto the live map UI. */
object NaviMapTestHooks {
    @Volatile
    var pendingRoute: uniffi.navi.CorridorRouteResult? = null

    @Volatile
    var pendingIconPng: ByteArray = ByteArray(0)

    @Volatile
    var lastReportedLayerCount: Int = 0

    /** Triple of lat, lon, zoom. Consumed by MainActivity. */
    @Volatile
    var pendingCamera: Triple<Double, Double, Double>? = null

    /** When true, MainActivity hides chrome so basemap/POI screenshots are unobstructed. */
    @Volatile
    var hideUiChrome: Boolean = false

    /**
     * Full replacement snapshot of moving icons (upsert semantics applied in the test /
     * TrackStore before posting). Same source id updates in place on the map.
     */
    @Volatile
    var pendingTracks: List<TrackMarker>? = null

    /** Incremented when tracks are applied; tests can wait on this. */
    @Volatile
    var tracksEpoch: Int = 0

    /** Last applied track ids (for asserting no duplicates). */
    @Volatile
    var lastTrackIds: List<String> = emptyList()

    /** Bump to request a MapLibre GL framebuffer snapshot (not UiAutomation). */
    @Volatile
    var snapshotRequestId: Int = 0

    /** Last completed snapshotRequestId. */
    @Volatile
    var lastSnapshotId: Int = 0

    /** PNG bytes from the last MapLibre map.snapshot(). */
    @Volatile
    var lastSnapshotPng: ByteArray? = null

    /** How many track features were last written into tracks-src. */
    @Volatile
    var lastTrackFeatureCount: Int = 0
}
