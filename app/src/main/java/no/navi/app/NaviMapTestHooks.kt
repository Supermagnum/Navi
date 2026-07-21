package no.navi.app

/** Hooks so instrumented tests can push a computed route / camera onto the live map UI. */
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
}
