package no.navi.app

import org.junit.Assert.assertNotNull
import org.junit.Test

class BasemapContourLabelStyleTest {
    @Test
    fun elevationTextField_buildsForMetricAndUs() {
        assertNotNull(BasemapContourLabelStyle.elevationTextField(UnitSystem.METRIC))
        assertNotNull(BasemapContourLabelStyle.elevationTextField(UnitSystem.IMPERIAL_US))
        assertNotNull(BasemapContourLabelStyle.elevationTextField(UnitSystem.IMPERIAL_UK))
    }
}
