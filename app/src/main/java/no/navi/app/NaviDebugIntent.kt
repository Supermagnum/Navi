package no.navi.app

import android.content.Context
import android.content.Intent
import android.content.pm.ApplicationInfo
import android.util.Log
import uniffi.navi.TravelProfile
import java.util.concurrent.atomic.AtomicReference

/**
 * Debug-only adb trip intent extras (profile, vias, graph path, cabin toggles,
 * avoid-ferries, GPS pin at from). Gated like [NaviManeuverDump]: release /
 * non-debuggable APKs ignore every planning extra.
 *
 * Cabin keys supported on this build (Part B):
 * - `navi_use_networked_cabins` (membership/networked; absorbs legacy
 *   `network_hut_member` via config migration)
 * - `navi_use_unlocked_cabins`
 *
 * `navi_inject_gps` (default true): pin the map GPS mark at `navi_from_*` and
 * ignore live LocationManager fixes so emulator GPS cannot trigger off-route
 * recalculation after auto-plan.
 *
 * `navi_inject_gps` (default true): one-shot pin of the map GPS mark at
 * `navi_from_*` through plan apply; released on the first subsequent inject
 * so adb geo-fix / simulated drives can still progress.
 *
 * Settings go through the same UniFFI save/load path as the UI toggles.
 * Snapshot + restore avoids one case leaking into the next.
 */
object NaviDebugIntent {
    const val TAG = "NaviDebugIntent"

    /** pack_dir sentinel: empty pack roots → cold PBF graph build. */
    const val FORCE_PBF_PACK_DIR = "__navi_force_pbf__"

    /** Keys this build can apply (Part B cabin settings). */
    private val SUPPORTED_CABIN_KEYS =
        setOf(
            "use_networked_cabins",
            "use_unlocked_cabins",
        )

    /** No longer a separate setting — handled as networked alias. */
    private val BRANCH_ONLY_CABIN_KEYS = emptySet<String>()

    private fun classifyCabinSetting(settingKey: String): CabinSettingClass =
        when {
            settingKey in SUPPORTED_CABIN_KEYS -> CabinSettingClass.Supported
            settingKey in BRANCH_ONLY_CABIN_KEYS -> CabinSettingClass.BranchOnly
            else -> CabinSettingClass.Unknown
        }

    private enum class CabinSettingClass {
        Supported,
        BranchOnly,
        Unknown,
    }

    data class SettingsSnapshot(
        val useNetworkedCabins: Boolean,
        val useUnlockedCabins: Boolean,
        val bikeCapability: String,
    )

    data class AppliedContext(
        val profile: String,
        val bikeCapability: String?,
        val vias: List<Pair<Double, Double>>,
        val graph: String,
        val avoidFerries: Boolean?,
        val appliedSettings: Map<String, Boolean>,
        val ignoredSettings: List<String>,
        val restoreAfter: Boolean,
        val snapshot: SettingsSnapshot?,
    )

    private val lastApplied = AtomicReference<AppliedContext?>(null)
    private val pendingRestore = AtomicReference<SettingsSnapshot?>(null)

    fun debugBuild(context: Context? = null): Boolean {
        val ctx =
            context
                ?: runCatching {
                    val at = Class.forName("android.app.ActivityThread")
                    at.getMethod("currentApplication").invoke(null) as? Context
                }.getOrNull()
                ?: return false
        return (ctx.applicationInfo.flags and ApplicationInfo.FLAG_DEBUGGABLE) != 0
    }

    fun lastAppliedOrNull(): AppliedContext? = lastApplied.get()

    /**
     * Parse debug trip extras. Returns null when not debuggable or no from/to.
     * Side-effects: may write cabin settings via UniFFI and stash a restore snapshot.
     */
    fun consumeTripExtras(
        context: Context,
        intent: Intent,
        dataDirPath: String,
    ): NaviMapTestHooks.PendingTripPlan? {
        if (!debugBuild(context)) {
            if (hasDebugTripExtras(intent)) {
                Log.w(TAG, "ignored: non-debuggable build")
            }
            return null
        }
        val fromLat = intentDouble(intent, "navi_from_lat")
        val fromLon = intentDouble(intent, "navi_from_lon")
        val toLat = intentDouble(intent, "navi_to_lat")
        val toLon = intentDouble(intent, "navi_to_lon")
        if (fromLat.isNaN() || fromLon.isNaN() || toLat.isNaN() || toLon.isNaN()) {
            return null
        }

        val fromName =
            intent.getStringExtra("navi_from_name").orEmpty().ifBlank {
                formatCoordWaypointName(fromLat, fromLon)
            }
        val toName =
            intent.getStringExtra("navi_to_name").orEmpty().ifBlank {
                formatCoordWaypointName(toLat, toLon)
            }
        val enableLong =
            !intent.hasExtra("navi_long_trip") ||
                intent.getBooleanExtra("navi_long_trip", true)
        val autoPlan =
            !intent.hasExtra("navi_auto_plan") ||
                intent.getBooleanExtra("navi_auto_plan", true)

        val profile = parseProfile(intent.getStringExtra("navi_profile"))
        val bikeCap = parseBikeCapability(intent)
        val vias = parseVias(intent)
        val forcePbf = parseForcePbf(intent)
        val avoidFerries =
            if (intent.hasExtra("navi_avoid_ferries")) {
                intent.getBooleanExtra("navi_avoid_ferries", false)
            } else {
                null
            }
        val restoreAfter =
            !intent.hasExtra("navi_restore_settings") ||
                intent.getBooleanExtra("navi_restore_settings", true)
        // Default true: matrix / adb trips pin GPS at from so the emulator's
        // real fix cannot trigger off-route recalculation after auto-plan.
        val injectGps =
            !intent.hasExtra("navi_inject_gps") ||
                intent.getBooleanExtra("navi_inject_gps", true)

        val snapshot =
            runCatching {
                SettingsSnapshot(
                    useNetworkedCabins = uniffi.navi.loadUseNetworkedCabins(dataDirPath),
                    useUnlockedCabins = uniffi.navi.loadUseUnlockedCabins(dataDirPath),
                    bikeCapability = uniffi.navi.loadBikeCapability(dataDirPath),
                )
            }.getOrNull()

        // Always start from defaults so a killed prior case cannot leak toggles
        // into this one. Intent extras then overlay; restoreAfterPlan returns to
        // the pre-intent snapshot when the plan finishes / fails / cancels.
        resetSettingsToDefaults(dataDirPath)

        val applied = linkedMapOf<String, Boolean>()
        val ignored = mutableListOf<String>()
        applyCabinExtras(intent, dataDirPath, applied, ignored)

        if (bikeCap != null) {
            runCatching {
                uniffi.navi.saveBikeCapability(dataDirPath, bikeCap)
            }.onFailure { Log.w(TAG, "saveBikeCapability failed: ${it.message}") }
        }

        if (restoreAfter && snapshot != null) {
            pendingRestore.set(snapshot)
        } else {
            pendingRestore.set(null)
        }

        val graphLabel = if (forcePbf) "local-pbf" else "pack-hit"
        val ctx =
            AppliedContext(
                profile = profile?.name?.lowercase() ?: "default",
                bikeCapability = bikeCap,
                vias = vias.map { it.lat to it.lon },
                graph = graphLabel,
                avoidFerries = avoidFerries,
                appliedSettings = applied.toMap(),
                ignoredSettings = ignored.toList(),
                restoreAfter = restoreAfter,
                snapshot = snapshot,
            )
        lastApplied.set(ctx)
        NaviManeuverDump.noteDebugContext(ctx)

        Log.i(
            TAG,
            "applied profile=${ctx.profile} bike=${ctx.bikeCapability} " +
                "graph=${ctx.graph} vias=${ctx.vias.size} " +
                "settings=${ctx.appliedSettings} ignored=${ctx.ignoredSettings} " +
                "avoid_ferries=${ctx.avoidFerries} restore=$restoreAfter " +
                "inject_gps=$injectGps",
        )

        return NaviMapTestHooks.PendingTripPlan(
            fromName = fromName,
            fromLat = fromLat,
            fromLon = fromLon,
            toName = toName,
            toLat = toLat,
            toLon = toLon,
            enableLongTrip = enableLong,
            autoPlan = autoPlan,
            profile = profile,
            bikeCapability = bikeCap,
            vias = vias,
            forceLocalPbf = forcePbf,
            avoidFerries = avoidFerries,
            restoreSettingsAfter = restoreAfter,
            injectGpsAtFrom = injectGps,
        )
    }

    /** Restore UniFFI cabin/bike settings snapshotted before the last debug intent. */
    fun restoreAfterPlan(dataDirPath: String) {
        val snap = pendingRestore.getAndSet(null) ?: return
        runCatching {
            uniffi.navi.saveUseNetworkedCabins(dataDirPath, snap.useNetworkedCabins)
            uniffi.navi.saveUseUnlockedCabins(dataDirPath, snap.useUnlockedCabins)
            uniffi.navi.saveBikeCapability(dataDirPath, snap.bikeCapability)
            // Release GPS pin so interactive use is not stuck ignoring live fixes.
            NaviMapTestHooks.ignoreLiveGpsFixes = false
            NaviMapTestHooks.pinGpsAfterPlanLatLon = null
            Log.i(
                TAG,
                "restored use_networked_cabins=${snap.useNetworkedCabins} " +
                    "use_unlocked_cabins=${snap.useUnlockedCabins} " +
                    "bike_capability=${snap.bikeCapability}",
            )
        }.onFailure { Log.w(TAG, "restore failed: ${it.message}") }
    }

    /** Defaults match first-run UI (networked/unlocked off, trekking bike). */
    private fun resetSettingsToDefaults(dataDirPath: String) {
        runCatching {
            uniffi.navi.saveUseNetworkedCabins(dataDirPath, false)
            uniffi.navi.saveUseUnlockedCabins(dataDirPath, false)
            uniffi.navi.saveBikeCapability(dataDirPath, "trekking")
            Log.i(
                TAG,
                "reset to defaults use_networked_cabins=false " +
                    "use_unlocked_cabins=false bike_capability=trekking",
            )
        }.onFailure { Log.w(TAG, "reset to defaults failed: ${it.message}") }
    }

    private fun hasDebugTripExtras(intent: Intent): Boolean {
        val keys =
            listOf(
                "navi_from_lat",
                "navi_profile",
                "navi_graph",
                "navi_via1_lat",
                "navi_use_networked_cabins",
                "navi_network_hut_member",
                "navi_use_unlocked_cabins",
                "navi_avoid_ferries",
                "navi_bike_capability",
                "navi_bike_mode",
            )
        return keys.any { intent.hasExtra(it) }
    }

    private fun applyCabinExtras(
        intent: Intent,
        dataDirPath: String,
        applied: MutableMap<String, Boolean>,
        ignored: MutableList<String>,
    ) {
        fun handle(
            extraKey: String,
            settingKey: String,
        ) {
            if (!intent.hasExtra(extraKey)) return
            val value = intent.getBooleanExtra(extraKey, false)
            when (classifyCabinSetting(settingKey)) {
                CabinSettingClass.Supported -> {
                    when (settingKey) {
                        "use_networked_cabins" ->
                            uniffi.navi.saveUseNetworkedCabins(dataDirPath, value)
                        "use_unlocked_cabins" ->
                            uniffi.navi.saveUseUnlockedCabins(dataDirPath, value)
                    }
                    applied[settingKey] = value
                }
                CabinSettingClass.BranchOnly -> {
                    ignored.add(settingKey)
                    Log.w(
                        TAG,
                        "setting not supported in this build (ignored): $settingKey=$value",
                    )
                }
                CabinSettingClass.Unknown -> {
                    ignored.add(settingKey)
                    Log.w(TAG, "unknown setting ignored: $settingKey=$value")
                }
            }
        }
        handle("navi_use_networked_cabins", "use_networked_cabins")
        handle("navi_use_unlocked_cabins", "use_unlocked_cabins")
        // Legacy extra: Part B migrated network_hut_member into use_networked_cabins.
        if (intent.hasExtra("navi_network_hut_member")) {
            val value = intent.getBooleanExtra("navi_network_hut_member", false)
            if (!intent.hasExtra("navi_use_networked_cabins")) {
                uniffi.navi.saveUseNetworkedCabins(dataDirPath, value)
                applied["use_networked_cabins"] = value
                Log.i(TAG, "legacy navi_network_hut_member aliased to use_networked_cabins=$value")
            } else {
                Log.w(TAG, "navi_network_hut_member ignored (navi_use_networked_cabins present)")
            }
        }
    }

    private fun parseForcePbf(intent: Intent): Boolean {
        val g = intent.getStringExtra("navi_graph")?.trim()?.lowercase().orEmpty()
        return when (g) {
            "pbf", "local-pbf", "local_pbf", "local" -> true
            "pack", "pack-hit", "pack_hit", "" -> false
            else -> {
                if (g.isNotEmpty()) {
                    Log.w(TAG, "unknown navi_graph=$g (default pack-hit)")
                }
                false
            }
        }
    }

    private fun parseBikeCapability(intent: Intent): String? {
        val raw =
            intent.getStringExtra("navi_bike_capability")
                ?: intent.getStringExtra("navi_bike_mode")
                ?: return null
        return when (raw.trim().lowercase()) {
            "road" -> "road"
            "trekking", "gravel" -> "trekking"
            "mountain", "mtb" -> "mountain"
            else -> {
                Log.w(TAG, "unknown bike capability '$raw' (ignored)")
                null
            }
        }
    }

    private fun parseProfile(raw: String?): TravelProfile? {
        if (raw.isNullOrBlank()) return null
        return when (raw.trim().lowercase().replace('-', '_')) {
            "car" -> TravelProfile.CAR
            "car_electric", "ev" -> TravelProfile.CAR_ELECTRIC
            "truck" -> TravelProfile.TRUCK
            "truck_electric" -> TravelProfile.TRUCK_ELECTRIC
            "mobile_home", "mobilehome", "motorhome" -> TravelProfile.MOBILE_HOME
            "bicycle", "bike" -> TravelProfile.BICYCLE
            "bicycle_electric", "ebike", "e_bike" -> TravelProfile.BICYCLE_ELECTRIC
            "hiking", "foot", "walk" -> TravelProfile.HIKING
            "motorcycle", "moto" -> TravelProfile.MOTORCYCLE
            "motorcycle_electric" -> TravelProfile.MOTORCYCLE_ELECTRIC
            else -> {
                Log.w(TAG, "unknown navi_profile='$raw' (ignored)")
                null
            }
        }
    }

    private fun parseVias(intent: Intent): List<Waypoint> {
        val out = ArrayList<Waypoint>(4)
        for (i in 1..4) {
            val lat = intentDouble(intent, "navi_via${i}_lat")
            val lon = intentDouble(intent, "navi_via${i}_lon")
            if (lat.isNaN() || lon.isNaN()) continue
            val name =
                intent.getStringExtra("navi_via${i}_name").orEmpty().ifBlank {
                    formatCoordWaypointName(lat, lon)
                }
            out.add(Waypoint(name = name, lat = lat, lon = lon))
        }
        return out
    }

    private fun intentDouble(
        intent: Intent,
        key: String,
    ): Double {
        val extras = intent.extras ?: return Double.NaN
        if (!extras.containsKey(key)) return Double.NaN
        when (val v = extras.get(key)) {
            is Double -> return v
            is Float -> return v.toDouble()
            is Int -> return v.toDouble()
            is Long -> return v.toDouble()
            is String -> return v.toDoubleOrNull() ?: Double.NaN
            is Number -> return v.toDouble()
        }
        val asDouble = extras.getDouble(key, Double.NaN)
        if (!asDouble.isNaN()) return asDouble
        val asFloat = extras.getFloat(key, Float.NaN)
        if (!asFloat.isNaN()) return asFloat.toDouble()
        return Double.NaN
    }
}
