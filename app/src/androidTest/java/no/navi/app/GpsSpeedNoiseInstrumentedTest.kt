package no.navi.app

import android.Manifest
import android.content.pm.PackageManager
import android.location.Location
import android.location.LocationListener
import android.location.LocationManager
import android.os.Bundle
import android.os.Handler
import android.os.Looper
import android.util.Log
import androidx.core.content.ContextCompat
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import org.junit.Assert.assertTrue
import org.junit.Assume.assumeTrue
import org.junit.Test
import org.junit.runner.RunWith
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit
import kotlin.math.max
import kotlin.math.sqrt

/**
 * Samples **live** [LocationManager.GPS_PROVIDER] speed (not the route simulator)
 * to size [OverspeedHud.MARGIN_KMH] against real consumer GPS noise.
 *
 * Best run outdoors / near a window with a clear sky view. Stationary samples
 * measure spurious reported speed when true speed is ~0 — the flicker risk when
 * driving just under a posted limit is at least this large (plus Doppler noise
 * while moving).
 */
@RunWith(AndroidJUnit4::class)
class GpsSpeedNoiseInstrumentedTest {
    @Test
    fun sampleLiveGpsSpeedNoise_forOverspeedMargin() {
        val ctx = InstrumentationRegistry.getInstrumentation().targetContext
        assumeTrue(
            "ACCESS_FINE_LOCATION required",
            ContextCompat.checkSelfPermission(ctx, Manifest.permission.ACCESS_FINE_LOCATION) ==
                PackageManager.PERMISSION_GRANTED,
        )
        val lm = ctx.getSystemService(LocationManager::class.java)
        assumeTrue("GPS provider disabled", lm.isProviderEnabled(LocationManager.GPS_PROVIDER))

        val samples = mutableListOf<Sample>()
        val latch = CountDownLatch(1)
        val main = Handler(Looper.getMainLooper())
        val listener =
            object : LocationListener {
                override fun onLocationChanged(location: Location) {
                    if (location.provider != LocationManager.GPS_PROVIDER &&
                        location.provider != LocationManager.FUSED_PROVIDER
                    ) {
                        // Prefer GNSS; still record fused if it carries Doppler speed.
                        if (!location.hasSpeed()) return
                    }
                    if (!location.hasSpeed()) return
                    val speedKmh = location.speed * 3.6
                    val accKmh =
                        if (location.hasSpeedAccuracy()) {
                            location.speedAccuracyMetersPerSecond * 3.6
                        } else {
                            null
                        }
                    val hAcc = if (location.hasAccuracy()) location.accuracy else null
                    synchronized(samples) {
                        samples.add(
                            Sample(
                                provider = location.provider.orEmpty(),
                                speedKmh = speedKmh,
                                speedAccKmh = accKmh,
                                hAccM = hAcc,
                            ),
                        )
                    }
                }

                @Deprecated("Deprecated in API")
                override fun onStatusChanged(
                    provider: String?,
                    status: Int,
                    extras: Bundle?,
                ) {}

                override fun onProviderEnabled(provider: String) {}

                override fun onProviderDisabled(provider: String) {}
            }

        InstrumentationRegistry.getInstrumentation().runOnMainSync {
            lm.requestLocationUpdates(
                LocationManager.GPS_PROVIDER,
                500L,
                0f,
                listener,
                Looper.getMainLooper(),
            )
            // Also fuse — some devices only attach Doppler speed on fused.
            runCatching {
                lm.requestLocationUpdates(
                    LocationManager.FUSED_PROVIDER,
                    500L,
                    0f,
                    listener,
                    Looper.getMainLooper(),
                )
            }
            main.postDelayed({ latch.countDown() }, COLLECT_MS)
        }

        val got = latch.await(COLLECT_MS + 5_000L, TimeUnit.MILLISECONDS)
        InstrumentationRegistry.getInstrumentation().runOnMainSync {
            lm.removeUpdates(listener)
        }
        assertTrue("collection wait interrupted", got)

        val snapshot = synchronized(samples) { samples.toList() }
        val gpsOnly = snapshot.filter { it.provider == LocationManager.GPS_PROVIDER }
        val withSpeed = snapshot.ifEmpty { emptyList() }

        Log.i(TAG, "collected=${snapshot.size} gps=${gpsOnly.size} margin=${OverspeedHud.MARGIN_KMH}")
        for (s in snapshot.takeLast(20)) {
            Log.i(
                TAG,
                "sample provider=${s.provider} speed=${"%.2f".format(s.speedKmh)} " +
                    "acc=${s.speedAccKmh?.let { "%.2f".format(it) } ?: "n/a"} " +
                    "hAcc=${s.hAccM?.let { "%.1f".format(it) } ?: "n/a"}",
            )
        }

        assumeTrue(
            "No live GPS/fused speed samples in ${COLLECT_MS / 1000}s " +
                "(need outdoor/window sky view — not simulator). Re-run outdoors.",
            withSpeed.isNotEmpty(),
        )

        val speeds = withSpeed.map { it.speedKmh }
        val maxSpeed = speeds.maxOrNull()!!
        val p95 = percentile(speeds, 0.95)
        val mean = speeds.average()
        val std =
            sqrt(speeds.map { (it - mean) * (it - mean) }.average())
        val medianAcc =
            withSpeed.mapNotNull { it.speedAccKmh }.sorted().let { accs ->
                if (accs.isEmpty()) null else accs[accs.size / 2]
            }

        Log.i(
            TAG,
            "stats n=${speeds.size} mean=${"%.2f".format(mean)} std=${"%.2f".format(std)} " +
                "p95=${"%.2f".format(p95)} max=${"%.2f".format(maxSpeed)} " +
                "medianSpeedAccKmh=${medianAcc?.let { "%.2f".format(it) } ?: "n/a"} " +
                "margin=${OverspeedHud.MARGIN_KMH}",
        )

        // Stationary (or near-stationary) spurious speed must stay under the HUD
        // margin, else the indicator flickers when truly at/under the limit.
        // Allow a small slack so a single outlier GNSS spike does not force an
        // unbounded margin; p95 is the primary noise metric.
        assertTrue(
            "p95 live GPS speed ${"%.2f".format(p95)} km/h exceeds HUD margin " +
                "${OverspeedHud.MARGIN_KMH} — widen OverspeedHud.MARGIN_KMH " +
                "(observed max=${"%.2f".format(maxSpeed)}, medianAcc=$medianAcc)",
            p95 <= OverspeedHud.MARGIN_KMH + 0.25,
        )
        if (medianAcc != null) {
            assertTrue(
                "margin ${OverspeedHud.MARGIN_KMH} < median reported speed accuracy " +
                    "${"%.2f".format(medianAcc)} km/h (68% band)",
                OverspeedHud.MARGIN_KMH + 1e-6 >= medianAcc,
            )
        }
    }

    private data class Sample(
        val provider: String,
        val speedKmh: Double,
        val speedAccKmh: Double?,
        val hAccM: Float?,
    )

    companion object {
        private const val TAG = "NaviGpsSpeedNoise"
        private const val COLLECT_MS = 45_000L

        private fun percentile(
            values: List<Double>,
            p: Double,
        ): Double {
            val sorted = values.sorted()
            if (sorted.isEmpty()) return Double.NaN
            val idx = max(0, ((sorted.size - 1) * p).toInt())
            return sorted[idx]
        }
    }
}
