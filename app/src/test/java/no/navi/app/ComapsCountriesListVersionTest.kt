package no.navi.app

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder
import java.io.File

/**
 * CoMaps Storage / drape scan `{writable}/{countries.txt "v"}/` for leaf MWMs.
 * Active release id may differ; only the countries-list version dir is linked.
 */
class ComapsCountriesListVersionTest {
    @get:Rule
    val tmp = TemporaryFolder()

    @Test
    fun reads_top_level_v_from_countries_txt() {
        File(tmp.root, "countries.txt").writeText("""{"id":"Countries","v":261105,"g":[]}""")
        assertEquals("261105", ComapsSessionBootstrap.readCountriesListVersion(tmp.root))
    }

    @Test
    fun missing_or_invalid_returns_null() {
        File(tmp.root, "countries.txt").writeText("{}")
        assertNull(ComapsSessionBootstrap.readCountriesListVersion(tmp.root))
        assertNull(ComapsSessionBootstrap.readCountriesListVersion(File(tmp.root, "absent_dir")))
    }
}
