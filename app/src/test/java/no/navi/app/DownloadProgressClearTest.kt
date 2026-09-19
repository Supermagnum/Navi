package no.navi.app

import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class DownloadProgressClearTest {
    @Test
    fun shouldClear_only_when_both_idle() {
        assertTrue(
            DownloadProgressClear.shouldClear(
                regionRunning = false,
                placeIndexRunning = false,
            ),
        )
        assertFalse(
            DownloadProgressClear.shouldClear(
                regionRunning = true,
                placeIndexRunning = false,
            ),
        )
        assertFalse(
            DownloadProgressClear.shouldClear(
                regionRunning = false,
                placeIndexRunning = true,
            ),
        )
        assertFalse(
            DownloadProgressClear.shouldClear(
                regionRunning = true,
                placeIndexRunning = true,
            ),
        )
    }
}
