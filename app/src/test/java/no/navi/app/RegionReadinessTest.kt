package no.navi.app

import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder
import java.io.File

class RegionReadinessTest {
    @get:Rule
    val tmp = TemporaryFolder()

    @Test
    fun skeleton_artifacts_present_detects_bin() {
        val dir = tmp.newFolder("packs")
        val stem = "ostlandet-latest"
        assertFalse(RegionReadiness.skeletonArtifactsPresent(dir, stem))
        File(dir, "$stem.navi-corridor-skeleton.bin").writeBytes(ByteArray(4))
        assertTrue(RegionReadiness.skeletonArtifactsPresent(dir, stem))
    }

    @Test
    fun car_skeleton_ready_falls_back_to_artifacts_without_native() {
        val dir = tmp.newFolder("packs2")
        val stem = "vestlandet-latest"
        File(dir, "$stem.navi-corridor-skeleton.bin").writeBytes(ByteArray(4))
        assertTrue(RegionReadiness.carSkeletonReady(dir, stem))
    }
}
