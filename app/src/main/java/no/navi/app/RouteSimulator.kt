package no.navi.app

import android.location.Location
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch
import kotlin.math.max

/**
 * Debug-only route playback: advances along [samples] at each sample's
 * posted/fallback [RouteSimSample.speedKmh], feeding the real GPS fix sink.
 *
 * [timeScale] compresses wall-clock only (instrumented tests); reported speed
 * still matches the maxspeed / highway-class table.
 */
class RouteSimulator(
    private val scope: CoroutineScope,
    private val samples: List<RouteSimSample>,
    private val onFix: (Location) -> Unit,
    private val onSample: (RouteSimSample) -> Unit = {},
    private val onFinished: () -> Unit = {},
) {
    private var job: Job? = null

    val running: Boolean get() = job?.isActive == true

    fun start(timeScale: Double = 1.0) {
        stop()
        if (samples.size < 2) {
            onFinished()
            return
        }
        val scale = max(timeScale, 0.01)
        job =
            scope.launch {
                var i = 0
                while (isActive && i < samples.lastIndex) {
                    val a = samples[i]
                    val b = samples[i + 1]
                    onSample(a)
                    val loc = locationAt(a, b)
                    onFix(loc)
                    val distM = (b.cumM - a.cumM).coerceAtLeast(0.5)
                    val speedMs = (a.speedKmh.coerceAtLeast(1.0) / 3.6)
                    val waitMs = ((distM / speedMs) * 1000.0 / scale).toLong().coerceIn(15L, 60_000L)
                    delay(waitMs)
                    i++
                }
                if (isActive) {
                    val last = samples.last()
                    onSample(last)
                    onFix(locationAt(last, last))
                    onFinished()
                }
            }
    }

    /** Jump to the sample nearest [targetCumM] (test checkpoints). */
    fun seekToCumM(targetCumM: Double) {
        if (samples.isEmpty()) return
        val idx = samples.indices.minByOrNull { kotlin.math.abs(samples[it].cumM - targetCumM) } ?: 0
        val a = samples[idx]
        val b = samples.getOrElse(idx + 1) { a }
        onSample(a)
        onFix(locationAt(a, b))
    }

    fun stop() {
        job?.cancel()
        job = null
    }

    private fun locationAt(
        a: RouteSimSample,
        b: RouteSimSample,
    ): Location {
        val loc = Location("navi-route-sim")
        loc.latitude = a.lat
        loc.longitude = a.lon
        loc.time = System.currentTimeMillis()
        loc.accuracy = 5f
        val speedMs = (a.speedKmh / 3.6).toFloat()
        loc.speed = speedMs
        if (a !== b && (a.lat != b.lat || a.lon != b.lon)) {
            loc.bearing = RouteProgressTracker.bearingDeg(a.lat, a.lon, b.lat, b.lon).toFloat()
        }
        return loc
    }
}
