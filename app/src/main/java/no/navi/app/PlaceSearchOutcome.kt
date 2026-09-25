package no.navi.app

import uniffi.navi.PlaceHit

/** Result of a background From/Via/To place query (offline FTS ± online). */
internal data class PlaceSearchOutcome(
    val hits: List<PlaceHit>,
    val hasEntries: Boolean,
    val usedOnline: Boolean,
    val onlineOk: Boolean,
)
