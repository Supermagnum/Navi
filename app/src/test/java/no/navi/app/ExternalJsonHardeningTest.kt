package no.navi.app

import org.json.JSONException
import org.json.JSONObject
import org.junit.Assert.assertTrue
import org.junit.Test
import kotlin.random.Random

/**
 * Property-style checks for Kotlin-side JSON that mirrors external / on-disk
 * inputs (active.json, world-release.json, prefs blobs). Malformed input must
 * surface as a caught exception, never an uncaught crash in the test process.
 *
 * Full Jazzer instrumentation is not wired yet; this is the lightweight gate
 * that runs in `:app:testDebugUnitTest` / kotlin-fast CI.
 */
class ExternalJsonHardeningTest {
    @Test
    fun malformedJsonObjectIsCaught() {
        val samples =
            listOf(
                "",
                "{",
                "{]",
                "null",
                "[]",
                "{\"version\":",
                "{\"version\":null}",
                ByteArray(64) { 0xFF.toByte() }.toString(Charsets.ISO_8859_1),
            )
        for (s in samples) {
            val outcome =
                runCatching {
                    JSONObject(s).optString("version")
                }
            // Either parses or throws JSONException / IllegalArgument — never escapes.
            assertTrue(
                "unexpected failure for sample=${s.take(32)}: ${outcome.exceptionOrNull()}",
                outcome.isSuccess ||
                    outcome.exceptionOrNull() is JSONException ||
                    outcome.exceptionOrNull() is IllegalArgumentException,
            )
        }
    }

    @Test
    fun randomByteBlobsDoNotCrashProcess() {
        val rnd = Random(167L)
        repeat(200) {
            val n = rnd.nextInt(4096)
            val bytes = ByteArray(n) { rnd.nextBits(8).toByte() }
            val text = bytes.toString(Charsets.ISO_8859_1)
            runCatching { JSONObject(text) }
            runCatching { JSONObject(text).optString("version") }
        }
    }
}
