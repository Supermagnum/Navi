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
    /** Optional instrumented-test override for hillshade-exaggeration. */
    @Volatile
    var hillshadeExaggerationOverride: Float? = null

    /**
     * When true, [LocalDemTileServer] re-encodes terrarium as Mapbox Terrain-RGB PNG
     * (legacy path). Default offline serving is raw terrarium WebP.
     */
    @Volatile
    var localDemMapboxConversion: Boolean = false

    /** @deprecated use [localDemMapboxConversion] inverted */
    @Deprecated("Use localDemMapboxConversion", ReplaceWith("!localDemMapboxConversion"))
    var localDemRawTerrarium: Boolean
        get() = !localDemMapboxConversion
        set(value) {
            localDemMapboxConversion = !value
        }

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

    /** Loopback DEM tile fetches (mirrored from [LocalDemTileServer] for instrumented tests). */
    @Volatile
    var localDemHitsOk: Long = 0

    @Volatile
    var localDemHitsMiss: Long = 0

    /** When true, [BasemapStyleResolver.resolve] skips local PMTiles (Liberty online). */
    @Volatile
    var forceOnlineBasemap: Boolean = false

    /** Triple of lat, lon, zoom. Consumed by MainActivity. */
    @Volatile
    var pendingCamera: Triple<Double, Double, Double>? = null

    /**
     * When true, live GPS never enables follow mode or retargets the map camera
     * (instrumented screenshots at a fixed [pendingCamera]).
     */
    @Volatile
    var disableGpsFollow: Boolean = false

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

    /** When true, MainActivity opens the Tools panel (test injection). */
    @Volatile
    var requestOpenTools: Boolean = false

    /** Mirrored from MainActivity: Tools sheet currently visible. */
    @Volatile
    var toolsOpen: Boolean = false

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

    /** Mirrors Tools selected Geofabrik path for instrumented assertions. */
    @Volatile
    var lastSelectedGeofabrikPath: String = ""

    /** Last missing-coverage prompt Geofabrik path (instrumented tests). */
    @Volatile
    var lastMissingCoveragePath: String = ""

    /** Last missing-coverage dialog body (instrumented tests). */
    @Volatile
    var lastMissingCoverageMessage: String = ""

    /** Whether the missing-coverage download dialog is visible. */
    @Volatile
    var missingCoveragePromptVisible: Boolean = false

    /**
     * When true, Plan route (Hiking) may load a host-staged polyline + breaks
     * (+ optional `skolla_rondvassbu.sim_samples.json` for debug simulation)
     * from `/data/local/tmp/navi_fixtures/skolla_rondvassbu.*` so instrumented
     * tests exercise search/keyboard without rebuilding the Ostlandet foot graph.
     */
    @Volatile
    var preferStagedHikingRoute: Boolean = false

    /**
     * Optional start / via / end labels for map SymbolLayer.
     * Fallback only when the corresponding live waypoint name is blank
     * (instrumented tests). [MainActivity.applyPlannedRoute] prefers
     * `fromPoint` / `toPoint` / vias over these hooks so a stale Plan-time
     * value cannot mask a live reroute start name.
     */
    @Volatile
    var routeStartLabel: String = ""

    @Volatile
    var routeViaLabel: String = ""

    @Volatile
    var routeEndLabel: String = ""

    /** Last start label actually applied to the map (after plan or reroute). */
    @Volatile
    var lastAppliedRouteStartLabel: String = ""

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

    /** Persisted "Snap rotation back to mode" preference (mirrored for tests). */
    @Volatile
    var lastSnapRotationBack: Boolean = true

    /**
     * True while a manual rotate is sticky (snap-back off) or during the brief
     * pause before snap-back reasserts the mode bearing.
     */
    @Volatile
    var manualRotationOverrideActive: Boolean = false

    /**
     * One-shot: simulate end of a user rotate gesture at this bearing (degrees).
     * MainActivity applies it to MapLibre without updating mode state, then runs
     * the same override / snap-back path as a real rotate gesture.
     */
    @Volatile
    var requestSimulateManualRotateDeg: Double? = null

    /** Count of rotate-gesture begins. */
    @Volatile
    var mapGestureRotates: Int = 0

    /** Completed 4 s map long-press events (map-mark menu). */
    @Volatile
    var mapLongPressCount: Int = 0

    @Volatile
    var lastMapLongPressLat: Double = Double.NaN

    @Volatile
    var lastMapLongPressLon: Double = Double.NaN

    /** One-shot: set snap-rotation-back preference from tests. */
    @Volatile
    var requestSnapRotationBack: Boolean? = null

    @Volatile
    var lastRoutePolylineChars: Int = 0

    /** Last successful plan report text (vehicle-limits avoidance lines, etc.). */
    @Volatile
    var lastPlanReport: String = ""

    @Volatile
    var lastPlanDistanceKm: Double = 0.0

    /** Last planned maneuvers JSON (same payload as UniFFI `maneuversJson`). */
    @Volatile
    var lastManeuversJson: String = "[]"

    /** Last planned simulation samples JSON (street labels along the corridor). */
    @Volatile
    var lastSimSamplesJson: String = "[]"

    /** Full overlay polyline from the last planned / injected route. */
    @Volatile
    var lastRoutePolyline: String = ""

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

    /**
     * Bump to wait until MapLibre reports a fully rendered frame **and** becomes
     * idle. Instrumented screenshot helpers must wait on [lastRenderSettleId]
     * before UiAutomation / shell `screencap` — capturing on a fixed sleep after
     * [styleReady] alone can freeze a mid-composite hydro soft edge that is not
     * visible during live interactive use.
     */
    @Volatile
    var renderSettleRequestId: Int = 0

    /** Last completed [renderSettleRequestId]. */
    @Volatile
    var lastRenderSettleId: Int = 0

    /** How many track features were last written into tracks-src. */
    @Volatile
    var lastTrackFeatureCount: Int = 0

    /** How many track icon bitmaps were present/registered on last apply. */
    @Volatile
    var lastTrackImagesReady: Int = 0

    /** Screen-space Compose overlay mark count (visible moving icons). */
    @Volatile
    var lastTrackOverlayCount: Int = 0

    /**
     * Sorted "id:x:y" fingerprint of screen-space overlay marks (coords rounded
     * to int). Instrumented tests wait for this to change after a track push so
     * UiAutomation screencap does not race Compose redraw.
     */
    @Volatile
    var lastOverlayScreenFingerprint: String = ""

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

    /**
     * One-shot GPS-style fix for instrumented tests (lat, lon). Consumed by
     * MainActivity → [applyFix]. Used to probe off-route behaviour — the
     * built-in simulator has no deviation-injection mode.
     */
    @Volatile
    var pendingInjectFixLatLon: Pair<Double, Double>? = null

    /**
     * When true, ignore LocationManager / other non-test providers so a real
     * device GPS fix cannot overwrite [pendingInjectFixLatLon] / sim positions
     * during off-route instrumented tests.
     */
    @Volatile
    var ignoreLiveGpsFixes: Boolean = false

    /** Last map GPS mark (after applyFix); for off-route assertions. */
    @Volatile
    var lastGpsLat: Double = Double.NaN

    @Volatile
    var lastGpsLon: Double = Double.NaN

    @Volatile
    var lastOffRoute: Boolean = false

    @Volatile
    var lastCrossTrackM: Double = 0.0

    @Volatile
    var reroutingActive: Boolean = false

    @Volatile
    var hikingReroutePromptVisible: Boolean = false

    /** Override debounce (ms) for instrumented tests; null = production default. */
    @Volatile
    var offRouteConfirmMsOverride: Long? = null

    /** Optional forced PBF path for [RouteReplan]. */
    @Volatile
    var forcePlanPbfPath: String? = null

    /** When set, [RouteReplan] returns this instead of calling native plan. */
    @Volatile
    var rerouteResultOverride: uniffi.navi.CorridorRouteResult? = null

    /** true = accept hiking prompt; false = decline; null = no request. */
    @Volatile
    var requestHikingRerouteAnswer: Boolean? = null

    @Volatile
    var autoRerouteTriggeredCount: Int = 0

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

    /** Last GPS / sim speed pushed into HUD (km/h). */
    @Volatile
    var lastGpsSpeedKmh: Double? = null

    /** Last LocationManager provider that fed applyFix (gps/network/passive/sim). */
    @Volatile
    var lastGpsProvider: String = ""

    /** Last resolved applicable speed limit (km/h). */
    @Volatile
    var lastCurrentSpeedLimitKmh: Double? = null

    /** Last bottom-HUD overspeed chrome flag (mirrored from MainActivity). */
    @Volatile
    var lastOverspeed: Boolean = false

    /**
     * Optional speed (km/h) for the next [pendingInjectFixLatLon] inject.
     * Cleared after one shot.
     */
    @Volatile
    var pendingInjectFixSpeedKmh: Double? = null

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

    /** Approach box icon key stem last applied (e.g. nav_right_1). */
    @Volatile
    var lastApproachIconKey: String? = null
}
