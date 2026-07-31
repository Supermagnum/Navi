package no.navi.app

import java.io.File

/**
 * Installs Ostlandet Protomaps + Mapterhorn DEM from
 * `/data/local/tmp/navi_fixtures/` into the app [dataDir] `pmtiles/` tree and
 * registers a completed job so [BasemapStyleResolver] can attach offline 3D
 * without network.
 *
 * Without this, opt-in 3D falls through to online Mapterhorn TileJSON and the
 * status chip shows “3D terrain needs network” when Wi‑Fi is off.
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
            )
        check(report.startsWith("OK:")) { report }
    }
}
