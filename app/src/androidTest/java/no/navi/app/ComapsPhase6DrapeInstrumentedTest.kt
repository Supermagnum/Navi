package no.navi.app

import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.rule.ActivityTestRule
import org.junit.Assert.assertTrue
import org.junit.Assume.assumeTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith

/**
 * Phase 6 acceptance, step 1: stage the production MWM layout and bring the hybrid
 * drape up inside Navi (flag ON, headless session first, then Framework).
 */
@RunWith(AndroidJUnit4::class)
class ComapsPhase6DrapeInstrumentedTest {
    @get:Rule
    val activityRule = ActivityTestRule(MainActivity::class.java, false, false)

    @Test
    fun a_stage_mwm_root_and_open_session() {
        MwmClientBootstrap.ensureInitialized()
        assumeTrue("MWM flag off", MwmClientBootstrap.isEnabled())
        val summary = ComapsPhase6Harness.stageProductionMwmRoot()
        android.util.Log.i(ComapsPhase6Harness.TAG, "staged $summary")
        val ctx = ComapsPhase6Harness.context
        assertTrue("session must open", ComapsSessionBootstrap.ensureOpen(ctx, force = true))
    }

    @Test
    fun b_drape_comes_up_behind_maplibre() {
        MwmClientBootstrap.ensureInitialized()
        assumeTrue("MWM flag off", MwmClientBootstrap.isEnabled())
        ComapsPhase6Harness.ensureShellDirs()
        ComapsPhase6Harness.stageProductionMwmRoot()
        assertTrue(ComapsSessionBootstrap.ensureOpen(ComapsPhase6Harness.context, force = false))
        NaviMapTestHooks.hideSearchChrome = true
        activityRule.launchActivity(null)
        val ready =
            ComapsPhase6Harness.waitFor(90_000, 500) {
                NaviMapTestHooks.drapeState == "READY" || NaviMapTestHooks.drapeState == "FAILED"
            }
        android.util.Log.i(
            ComapsPhase6Harness.TAG,
            "drape state=${NaviMapTestHooks.drapeState} err=${NaviMapTestHooks.drapeError} waited=$ready",
        )
        assertTrue("drape init did not finish", ready)
        ComapsPhase6Harness.gotoCamera(61.0, 10.5, 12.0)
        Thread.sleep(8_000)
        ComapsPhase6Harness.screenshot("00_bringup_drape")
        android.util.Log.i(
            ComapsPhase6Harness.TAG,
            "bringup basemapDrape=${NaviMapTestHooks.drapeBasemapVisible} syncs=${NaviMapTestHooks.drapeViewportSyncs}",
        )
    }
}
