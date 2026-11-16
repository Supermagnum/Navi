package no.navi.app

import uniffi.navi.PlaceHit

/** Result of a background From/Via/To place query (offline FTS ± online). */
internal data class PlaceSearchOutcome(
    val hits: List<PlaceHit>,
    val hasEntries: Boolean,
    val usedOnline: Boolean,
    val onlineOk: Boolean,
    val errorLine: String? = null,
    /** CoMaps readiness wait before the request id was accepted (-1 if unused). */
    val comapsReadyMs: Long = -1L,
    /** Time from search start to first non-empty CoMaps batch (-1 if none). */
    val comapsFirstMs: Long = -1L,
    /** Time from search start to final CoMaps batch (-1 if unused). */
    val comapsTotalMs: Long = -1L,
    /**
     * True when CoMaps search stayed unavailable until the readiness timeout
     * (not when the user cancelled by clearing the field).
     */
    val comapsNotReady: Boolean = false,
)
