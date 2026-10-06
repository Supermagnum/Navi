package no.navi.app

import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder
import java.io.File

class PlanReportStoreTest {
    @get:Rule
    val tmp = TemporaryFolder()

    @Test
    fun internal_dir_is_long_trip_ui_report_under_files() {
        val dataDir = tmp.newFolder("files")
        val dir = PlanReportStore.internalDir(dataDir)
        assertEquals(File(dataDir, "long-trip-ui-report").absolutePath, dir.absolutePath)
        PlanReportStore.writeText(dir, PlanReportStore.POLYLINE_NAME, "53.0,10.0;54.0,11.0")
        PlanReportStore.writeText(
            dir,
            PlanReportStore.LOG_NAME,
            "hop_result=success km=1.0\nplan_summary km=1.0 eta_min=2.0 route_ferry_legs=0 terminate=found hops=1\n",
        )
        assertTrue(File(dir, "route-polyline.txt").isFile)
        assertTrue(File(dir, "routing-plan.log").readText().contains("hop_result=success"))
        assertTrue(File(dir, "routing-plan.log").readText().contains("plan_summary"))
    }

    @Test
    fun publish_writes_polyline_beside_log_not_a_second_root() {
        val dataDir = tmp.newFolder("files")
        // No Android Context: exercise the same filenames native uses.
        val internal = PlanReportStore.internalDir(dataDir).also { it.mkdirs() }
        File(internal, PlanReportStore.LOG_NAME).writeText("hop_result=success i=1 km=10\n")
        PlanReportStore.writeText(internal, PlanReportStore.POLYLINE_NAME, "a;b")
        PlanReportStore.writeText(internal, PlanReportStore.HOPS_NAME, """{"hops":[]}""")
        val names = internal.list()?.sorted()?.joinToString()
        assertEquals("hops.json, route-polyline.txt, routing-plan.log", names)
    }
}
