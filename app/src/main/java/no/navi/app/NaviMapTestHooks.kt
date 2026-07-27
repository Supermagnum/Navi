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

    /**
     * Direct apply callback registered by the live MainActivity composition.
     * Instrumented tests should prefer this over [pendingRoute] when a system
     * permission dialog has paused the activity (pendingRoute only consumes while
     * RESUMED).
     */
    @Volatile
    var applyRouteHandler: ((uniffi.navi.CorridorRouteResult) -> Unit)? = null

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

    /** When true, MainActivity closes tools / route panels (test injection). */
    @Volatile
    var requestCloseTools: Boolean = false

    /** True after MapLibre style finished loading (basemap ready for overlays). */
    @Volatile
    var styleReady: Boolean = false

    /**
     * When true, [CorridorMapView] pauses/stops the MapView but skips `onDestroy`
     * and retains the instance. Instrumented suites on the AAOS emulator otherwise
     * SIGSEGV in `AndroidVulkanRendererBackend::~` on the FinalizerDaemon when
     * ActivityTestRule tears down activities (MapLibre 11.8.8 Vulkan).
     */
    @Volatile
    var deferMapViewDestroy: Boolean = false

    /** Strong refs so deferred MapViews are not GC-finalized mid-suite. */
    val retainedMapViews: MutableList<Any> =
        java.util.Collections.synchronizedList(mutableListOf())

    /** Last basemap kind from [BasemapStyleResolver] (OnlineLiberty / Online3d / OfflineProtomaps). */
    @Volatile
    var lastBasemapKind: String = ""

    /** Last MapLibre style load failure message, if any. */
    @Volatile
    var lastStyleLoadError: String? = null

    /** True when Mapterhorn hillshade attach succeeded for the current style. */
    @Volatile
    var lastTerrainAttached: Boolean = false

    /** Last MapLibre camera pitch/tilt (degrees); viewing aid with hillshade 3D. */
    @Volatile
    var lastCameraPitch: Double = 0.0

    /** When set, Tools uses this Geofabrik path on the next composition pass. */
    @Volatile
    var pendingGeofabrikPath: String? = null

    /**
     * When true, Plan route (Hiking) may load a host-staged polyline + breaks
     * from `/data/local/tmp/navi_fixtures/skolla_rondvassbu.*` so instrumented
     * tests exercise search/keyboard without rebuilding the Ostlandet foot graph.
     */
    @Volatile
    var preferStagedHikingRoute: Boolean = false

    /** Optional start / via / end labels for map SymbolLayer (tests + search). */
    @Volatile
    var routeStartLabel: String = ""

    @Volatile
    var routeViaLabel: String = ""

    @Volatile
    var routeEndLabel: String = ""

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

    /**
     * Whether the map camera is following live GPS (true) or the user has
     * manually panned away (false). Mirrored from MainActivity for tests.
     */
    @Volatile
    var followGps: Boolean = true

    /** One-shot: re-enable GPS follow and recenter the camera (test helper). */
    @Volatile
    var requestRecenterGps: Boolean = false

    /** Last rotation mode selected in the HUD. */
    @Volatile
    var lastRotationMode: MapRotationMode = MapRotationMode.NorthUp

    @Volatile
    var lastRoutePolylineChars: Int = 0

    @Volatile
    var lastBreakPoiCount: Int = 0

    @Volatile
    var lastSearchHitCount: Int = 0

    @Volatile
    var lastSearchQuery: String = ""

    @Volatile
    var lastSearchHitNames: List<String> = emptyList()

    /** When true, MainActivity clears the search query field (test helper). */
    @Volatile
    var requestClearSearch: Boolean = false

    /** When set, MainActivity applies this place hit as From/To/Via (test helper). */
    @Volatile
    var pendingApplyHit: uniffi.navi.PlaceHit? = null

    /** When set, MainActivity assigns the search query and runs search (test helper). */
    @Volatile
    var requestSearchQuery: String? = null

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

    /** Optional test injection for 3D hillshade (null = no request). */
    @Volatile
    var requestOptIn3d: Boolean? = null

    /** Optional test injection for camera tilt degrees (null = no request). */
    @Volatile
    var requestCameraTiltDeg: Double? = null

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

    /** Last integrated planned driving hours since route start (break countdown input). */
    @Volatile
    var lastElapsedDrivingHours: Double? = null

    /** True when the bottom HUD currently renders a break-info line. */
    @Volatile
    var lastBreakHudVisible: Boolean = false

    /** When true, clear the active corridor polyline and break countdown. */
    @Volatile
    var requestClearRoute: Boolean = false

    /** When set, MainActivity switches travel profile (test injection). */
    @Volatile
    var requestTravelProfile: uniffi.navi.TravelProfile? = null

    /** Optional: set break display mode (null = no request). */
    @Volatile
    var requestBreakAsDistance: Boolean? = null

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

    /**
     * Injected approach / next-maneuver guidance (shared publisher for UI + voice).
     * Null = leave current Compose state unchanged.
     */
    @Volatile
    var pendingApproachGuidance: ApproachGuidanceState? = null

    /** Last approach phase applied (for assertions). */
    @Volatile
    var lastApproachPhase: ApproachUiPhase = ApproachUiPhase.Hidden

    /** Request start of debug route simulation (consumed by MainActivity). */
    @Volatile
    var requestStartRouteSimulation: Boolean = false

    /** Request stop of debug route simulation. */
    @Volatile
    var requestStopRouteSimulation: Boolean = false

    /** Rebuild progress tracker and ensure a simulator instance exists (no auto-start). */
    @Volatile
    var requestPrepareRouteSimulation: Boolean = false

    /** Optional From / Via / To injection before [pendingRoute] (test helper). */
    @Volatile
    var pendingFromPoint: Waypoint? = null

    @Volatile
    var pendingViaPoints: List<Waypoint>? = null

    @Volatile
    var pendingToPoint: Waypoint? = null

    /**
     * Wall-clock compression for simulation (1.0 = realtime). Speeds still follow
     * maxspeed / highway fallback; only wait times shrink. Used by instrumented tests.
     */
    @Volatile
    var simulationTimeScale: Double = 1.0

    /** When set, simulator seeks to this cumulative metres then continues. */
    @Volatile
    var requestSimSeekCumM: Double? = null

    @Volatile
    var simulatingActive: Boolean = false

    @Volatile
    var lastSimSpeedKmh: Double? = null

    @Volatile
    var lastSimHighway: String? = null

    @Volatile
    var lastSimMaxspeedPosted: Boolean = false

    /** Last bottom-HUD current-street label (name/ref/class). */
    @Volatile
    var lastCurrentStreet: String? = null

    /**
     * When set, MainActivity applies this as [DriveHudState.currentStreet] once
     * (instrumented UTF-8 / layout checks without a full corridor rebuild).
     */
    @Volatile
    var pendingCurrentStreet: String? = null

    @Volatile
    var lastDistanceToManeuverM: Double? = null

    @Volatile
    var lastViaIndex: Int = -1

    @Volatile
    var lastSimAlongM: Double = 0.0

    @Volatile
    var lastArrivedAtEnd: Boolean = false

    @Volatile
    var lastManeuverKind: String? = null
}
