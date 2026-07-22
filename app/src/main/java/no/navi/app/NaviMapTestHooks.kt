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
     * When true (and hideUiChrome is false), hide the search / tools panels but keep
     * the top and bottom drive HUDs visible for HUD-focused tests.
     */
    @Volatile
    var hideSearchChrome: Boolean = false

    /** True after MapLibre style finished loading (basemap ready for overlays). */
    @Volatile
    var styleReady: Boolean = false

    /**
     * Synthetic magnetic heading (degrees clockwise from north). Used when rotation
     * mode is Compass. Null = leave bearing unchanged from this source.
     */
    @Volatile
    var magneticHeadingDeg: Double? = null

    /**
     * Synthetic GPS course / direction-of-travel bearing (degrees). Used when
     * rotation mode is DirectionOfTravel.
     */
    @Volatile
    var gpsBearingDeg: Double? = null

    /**
     * Synthetic GPS altitude in meters (WGS84). When non-null, overrides the
     * live LocationManager fix for the top HUD altitude readout.
     */
    @Volatile
    var gpsAltitudeM: Double? = null

    /** Last altitude shown on the top HUD (for test assertions). */
    @Volatile
    var lastHudAltitudeM: Double? = null

    /** Last camera bearing applied to the map (for test assertions). */
    @Volatile
    var lastCameraBearing: Double = 0.0

    /** Last camera zoom applied to the map (for test assertions). */
    @Volatile
    var lastCameraZoom: Double = 12.0

    /** Last camera target latitude (updated on camera idle). */
    @Volatile
    var lastCameraLat: Double = 0.0

    /** Last camera target longitude (updated on camera idle). */
    @Volatile
    var lastCameraLon: Double = 0.0

    /** Last rotation mode selected in the HUD. */
    @Volatile
    var lastRotationMode: MapRotationMode = MapRotationMode.NorthUp

    /** When true, MainActivity opens the drive settings sheet (test injection). */
    @Volatile
    var requestOpenDriveSettings: Boolean = false

    /** Whether the drive settings sheet is currently open. */
    @Volatile
    var driveSettingsOpen: Boolean = false

    /** When true, MainActivity opens the map / display settings sheet. */
    @Volatile
    var requestOpenMapSettings: Boolean = false

    /** Whether the map settings sheet is currently open. */
    @Volatile
    var mapSettingsOpen: Boolean = false

    /** Optional test injection for Trip ETA toggle (null = no request). */
    @Volatile
    var requestShowTripEta: Boolean? = null

    /** Optional test injection for break-reminder toggle (null = no request). */
    @Volatile
    var requestBreakReminders: Boolean? = null

    /** Mirror of HUD break-reminder toggle (for test assertions). */
    @Volatile
    var lastBreakRemindersEnabled: Boolean = true

    /** Mirror of Trip ETA toggle. */
    @Volatile
    var lastShowTripEta: Boolean = false

    /** Mirror of minutes-to-break shown on the bottom HUD. */
    @Volatile
    var lastMinutesToBreak: Double? = null

    /**
     * Full replacement snapshot of moving icons (upsert semantics applied in the test /
     * TrackStore before posting). Same source id updates in place on the map.
     */
    @Volatile
    var pendingTracks: List<TrackMarker>? = null

    /** Incremented when tracks are applied to Compose state; tests can wait on this. */
    @Volatile
    var tracksEpoch: Int = 0

    /** Updated when tracks have been written into the live MapLibre style. */
    @Volatile
    var tracksAppliedEpoch: Int = 0

    /** Last applied track ids (for asserting no duplicates). */
    @Volatile
    var lastTrackIds: List<String> = emptyList()

    /** When true, CorridorMapView pauses the MapLibre GL surface (safer screenshots). */
    @Volatile
    var pauseMapForScreenshot: Boolean = false

    /**
     * When false, heading hooks update [lastCameraBearing] / mode only and do not
     * call MapLibre moveCamera. Kept for optional hook-only assertions; with the
     * Vulkan MapLibre SDK, non-zero bearings are safe to apply on this AVD.
     */
    @Volatile
    var applyBearingToMap: Boolean = true

    /** One-shot camera bearing degrees (applied by MainActivity poll). */
    @Volatile
    var pendingBearing: Double? = null

    /** One-shot rotation mode request (applied by MainActivity poll). */
    @Volatile
    var requestRotationMode: MapRotationMode? = null

    /** Optional pause/resume callback registered by CorridorMapView. */
    @Volatile
    var mapPauseHandler: ((Boolean) -> Unit)? = null

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

    /** How many track icon bitmaps were present/registered on last apply. */
    @Volatile
    var lastTrackImagesReady: Int = 0

    /** Screen-space Compose overlay mark count (visible moving icons). */
    @Volatile
    var lastTrackOverlayCount: Int = 0

    /** Count of MapLibre move-gesture begins (pan). */
    @Volatile
    var mapGestureMoves: Int = 0

    /** Count of MapLibre scale-gesture begins (pinch). */
    @Volatile
    var mapGestureScales: Int = 0

    /** Dispatches a MotionEvent to the live MapView (UI thread). */
    @Volatile
    var mapViewTouch: ((android.view.MotionEvent) -> Boolean)? = null
}
