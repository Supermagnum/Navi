package no.navi.app

import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performScrollTo
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import uniffi.navi.geofabrikLatestPbfUrl
import uniffi.navi.pmtilesRegionBbox
import java.io.File
import java.net.HttpURLConnection
import java.net.URL

/**
 * Tools → Download scope: seven continents, Geofabrik-sourced countries,
 * maps-only support notes, and live downloadability checks.
 */
@RunWith(AndroidJUnit4::class)
class DownloadScopeCountryHierarchyInstrumentedTest {
    @get:Rule
    val composeRule = createAndroidComposeRule<MainActivity>()

    @Before
    fun setUp() {
        NaviMapTestHooks.hideUiChrome = false
        NaviMapTestHooks.hideSearchChrome = false
    }

    @Test
    fun catalog_seven_continents_bboxes_and_prior_entries() {
        assertEquals(7, GeofabrikDownloadCatalog.continents.size)
        for (continent in GeofabrikDownloadCatalog.continents) {
            assertTrue(
                "$continent must have at least one extract",
                GeofabrikDownloadCatalog.countriesIn(continent).isNotEmpty(),
            )
        }
        assertEquals(
            GeofabrikContinent.Europe,
            GeofabrikDownloadCatalog.findByPath("russia")?.continent,
        )
        assertEquals(
            GeofabrikContinent.NorthAmerica,
            GeofabrikDownloadCatalog.findByPath("central-america/costa-rica")?.continent,
        )
        for (country in GeofabrikDownloadCatalog.countries) {
            val bbox = pmtilesRegionBbox(country.path)
            assertNotNull("missing bbox for ${country.path}", bbox)
            assertTrue("${country.path} bbox too short", (bbox?.size ?: 0) >= 4)
        }
        // Prior fully-tested entries still resolve.
        for (path in listOf(
            "europe/norway",
            "europe/sweden",
            "europe/finland",
            "europe/germany",
            "europe/france",
            "europe/switzerland",
            "europe/austria",
            "europe/great-britain",
            "north-america/us",
            "russia",
        )) {
            assertNotNull(path, GeofabrikDownloadCatalog.findByPath(path))
            assertNotNull(path, pmtilesRegionBbox(path))
        }
        val kenya = GeofabrikDownloadCatalog.findByPath("africa/kenya")!!
        assertTrue(kenya.supportNote.contains("decline (no keyed pack)"))
        val antarctica = GeofabrikDownloadCatalog.findByPath("antarctica")!!
        assertTrue(antarctica.supportNote.contains("Antarctica"))
    }

    @Test
    fun geofabrik_urls_reachable_across_continents() {
        val samples =
            listOf(
                "africa/kenya",
                "asia/japan",
                "australia-oceania/new-zealand",
                "central-america/costa-rica",
                "north-america/canada",
                "south-america/chile",
                "europe/poland",
                "antarctica",
                "russia",
                "europe/norway",
            )
        for (path in samples) {
            val url = geofabrikLatestPbfUrl(path)
            assertTrue(url, url.contains(path) && url.endsWith("-latest.osm.pbf"))
            val conn = URL(url).openConnection() as HttpURLConnection
            conn.requestMethod = "HEAD"
            conn.instanceFollowRedirects = true
            conn.connectTimeout = 30_000
            conn.readTimeout = 30_000
            try {
                val code = conn.responseCode
                assertTrue("$path HEAD $code url=$url", code in 200..399)
                val len = conn.getHeaderField("Content-Length")?.toLongOrNull() ?: 0L
                assertTrue("$path Content-Length=$len", len > 1_000_000L)
            } finally {
                conn.disconnect()
            }
        }
    }

    @Test
    fun download_antarctica_pbf_sample() {
        // Small real Geofabrik extract (~33 MB) — proves catalog path downloads.
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        val dest = File(context.cacheDir, "geofabrik_antarctica_smoke.osm.pbf")
        if (dest.exists()) dest.delete()
        val url = geofabrikLatestPbfUrl("antarctica")
        val conn = URL(url).openConnection() as HttpURLConnection
        conn.instanceFollowRedirects = true
        conn.connectTimeout = 60_000
        conn.readTimeout = 120_000
        try {
            assertEquals(200, conn.responseCode)
            conn.inputStream.use { input ->
                dest.outputStream().use { output -> input.copyTo(output) }
            }
        } finally {
            conn.disconnect()
        }
        assertTrue("antarctica download too small: ${dest.length()}", dest.length() > 5_000_000L)
        dest.delete()
    }

    @Test
    fun tools_country_hierarchy_sets_geofabrik_path() {
        waitForToolsButton()
        composeRule
            .onNodeWithTag("btn_tools", useUnmergedTree = true)
            .performScrollTo()
            .performClick()
        composeRule.waitForIdle()
        composeRule.onNodeWithTag("tools_menu", useUnmergedTree = true).assertIsDisplayed()

        composeRule
            .onNodeWithTag("chip_download_country", useUnmergedTree = true)
            .performScrollTo()
            .performClick()
        composeRule.waitForIdle()
        Thread.sleep(500)

        for (continent in GeofabrikDownloadCatalog.continents) {
            composeRule
                .onNodeWithTag(continent.testTag, useUnmergedTree = true)
                .performScrollTo()
                .assertIsDisplayed()
        }

        // Africa → Kenya (maps-only note).
        composeRule
            .onNodeWithTag("chip_continent_africa", useUnmergedTree = true)
            .performScrollTo()
            .performClick()
        composeRule.waitForIdle()
        Thread.sleep(300)
        composeRule
            .onNodeWithTag("chip_country_africa_kenya", useUnmergedTree = true)
            .performScrollTo()
            .performClick()
        composeRule.waitForIdle()
        Thread.sleep(400)
        assertEquals("africa/kenya", NaviMapTestHooks.lastSelectedGeofabrikPath)
        composeRule
            .onNodeWithTag("country_support_note", useUnmergedTree = true)
            .performScrollTo()
            .assertIsDisplayed()
        composeRule
            .onNodeWithText("decline (no keyed pack)", substring = true, useUnmergedTree = true)
            .assertIsDisplayed()

        // Antarctica extract.
        composeRule
            .onNodeWithTag("chip_continent_antarctica", useUnmergedTree = true)
            .performScrollTo()
            .performClick()
        composeRule.waitForIdle()
        Thread.sleep(300)
        composeRule
            .onNodeWithTag("chip_country_antarctica", useUnmergedTree = true)
            .performScrollTo()
            .performClick()
        composeRule.waitForIdle()
        Thread.sleep(400)
        assertEquals("antarctica", NaviMapTestHooks.lastSelectedGeofabrikPath)

        // South America → Brazil.
        composeRule
            .onNodeWithTag("chip_continent_south_america", useUnmergedTree = true)
            .performScrollTo()
            .performClick()
        composeRule.waitForIdle()
        composeRule
            .onNodeWithTag("chip_country_south_america_brazil", useUnmergedTree = true)
            .performScrollTo()
            .performClick()
        composeRule.waitForIdle()
        Thread.sleep(400)
        assertEquals("south-america/brazil", NaviMapTestHooks.lastSelectedGeofabrikPath)

        // Asia → Japan.
        composeRule
            .onNodeWithTag("chip_continent_asia", useUnmergedTree = true)
            .performScrollTo()
            .performClick()
        composeRule.waitForIdle()
        composeRule
            .onNodeWithTag("chip_country_asia_japan", useUnmergedTree = true)
            .performScrollTo()
            .performClick()
        composeRule.waitForIdle()
        Thread.sleep(400)
        assertEquals("asia/japan", NaviMapTestHooks.lastSelectedGeofabrikPath)

        // Australia (Oceania) → New Zealand.
        composeRule
            .onNodeWithTag("chip_continent_australia_oceania", useUnmergedTree = true)
            .performScrollTo()
            .performClick()
        composeRule.waitForIdle()
        composeRule
            .onNodeWithTag("chip_country_australia_oceania_new_zealand", useUnmergedTree = true)
            .performScrollTo()
            .performClick()
        composeRule.waitForIdle()
        Thread.sleep(400)
        assertEquals("australia-oceania/new-zealand", NaviMapTestHooks.lastSelectedGeofabrikPath)

        // North America → Costa Rica (Geofabrik central-america path).
        composeRule
            .onNodeWithTag("chip_continent_north_america", useUnmergedTree = true)
            .performScrollTo()
            .performClick()
        composeRule.waitForIdle()
        composeRule
            .onNodeWithTag("chip_country_central_america_costa_rica", useUnmergedTree = true)
            .performScrollTo()
            .performClick()
        composeRule.waitForIdle()
        Thread.sleep(400)
        assertEquals("central-america/costa-rica", NaviMapTestHooks.lastSelectedGeofabrikPath)

        // Europe → Sweden + Norway landsdel regression.
        composeRule
            .onNodeWithTag("chip_continent_europe", useUnmergedTree = true)
            .performScrollTo()
            .performClick()
        composeRule.waitForIdle()
        composeRule
            .onNodeWithTag("chip_country_europe_sweden", useUnmergedTree = true)
            .performScrollTo()
            .performClick()
        composeRule.waitForIdle()
        Thread.sleep(400)
        assertEquals("europe/sweden", NaviMapTestHooks.lastSelectedGeofabrikPath)

        composeRule
            .onNodeWithTag("chip_country_europe_norway", useUnmergedTree = true)
            .performScrollTo()
            .performClick()
        composeRule.waitForIdle()
        composeRule
            .onNodeWithTag("chip_download_region", useUnmergedTree = true)
            .performScrollTo()
            .performClick()
        composeRule.waitForIdle()
        Thread.sleep(400)
        composeRule
            .onNodeWithText("Østlandet", useUnmergedTree = true)
            .performScrollTo()
            .assertIsDisplayed()
            .performClick()
        composeRule.waitForIdle()
        Thread.sleep(400)
        assertEquals("europe/norway/ostlandet", NaviMapTestHooks.lastSelectedGeofabrikPath)
    }

    private fun waitForToolsButton(timeoutMs: Long = 60_000) {
        val deadline = System.currentTimeMillis() + timeoutMs
        var last: Throwable? = null
        while (System.currentTimeMillis() < deadline) {
            try {
                composeRule.waitForIdle()
                composeRule.onNodeWithTag("btn_tools", useUnmergedTree = true).assertExists()
                return
            } catch (t: Throwable) {
                last = t
                Thread.sleep(500)
            }
        }
        throw IllegalStateException("btn_tools never appeared", last)
    }
}
