package no.navi.app

import java.io.File

/**
 * Installs Ostlandet Protomaps + Mapterhorn DEM from
 * `/data/local/tmp/navi_fixtures/` into the app [dataDir] `pmtiles/` tree and
 * registers a completed job so [BasemapStyleResolver] can attach offline 3D
 * without network.
 *
 * Uses the `test/ostlandet_fixture` region key so the truncated mz12 staging
 * fixture can register Completed for screenshots without bypassing production
 * completion validation for `europe/norway/ostlandet`.
 */
object OstlandetOfflineFixtures {
    fun ensureInstalled(
        dataDir: File,
        stagedDir: File = OfflineDataIntegrity.STAGED_FIXTURES_DIR,
    ) {
        val report =
            OfflinePmtilesBootstrap.restoreOstlandetFromStaging(
                dataDir = dataDir,
                stagedDir = stagedDir,
                forInstrumentedTests = true,
            )
        check(report.startsWith("OK:")) { report }
    }
}
