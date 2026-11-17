package no.navi.app

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test
import uniffi.navi.FfiVehicleLimits

/**
 * Contract for the Vehicle limits Save path: clearance profiles persist total
 * weight from the new field; a blank field clears it; height-only car saves keep
 * a previously loaded total-weight string when the field is not shown.
 */
class VehicleLimitsPersistContractTest {
    private fun buildLimits(
        clearance: Boolean,
        heightOnly: Boolean,
        axle: String,
        bogie: String,
        height: String,
        width: String,
        length: String,
        total: String,
    ): FfiVehicleLimits =
        FfiVehicleLimits(
            axleWeightKg = if (clearance) axle.toDoubleOrNull() else null,
            bogieWeightKg = if (clearance) bogie.toDoubleOrNull() else null,
            heightM = if (clearance || heightOnly) height.toDoubleOrNull() else null,
            widthM = if (clearance) width.toDoubleOrNull() else null,
            lengthM = if (clearance) length.toDoubleOrNull() else null,
            totalWeightKg = total.toDoubleOrNull(),
        )

    @Test
    fun truckSavePersistsTotalWeightFromField() {
        val lim =
            buildLimits(
                clearance = true,
                heightOnly = false,
                axle = "8000",
                bogie = "12000",
                height = "4.0",
                width = "2.55",
                length = "16.5",
                total = "18000",
            )
        assertEquals(18_000.0, lim.totalWeightKg!!, 0.01)
        assertEquals(4.0, lim.heightM!!, 0.01)
        assertEquals(8_000.0, lim.axleWeightKg!!, 0.01)
    }

    @Test
    fun blankTotalWeightClearsStoredValue() {
        val lim =
            buildLimits(
                clearance = true,
                heightOnly = false,
                axle = "",
                bogie = "",
                height = "3.5",
                width = "",
                length = "",
                total = "",
            )
        assertNull(lim.totalWeightKg)
        assertEquals(3.5, lim.heightM!!, 0.01)
    }

    @Test
    fun carHeightOnlySaveKeepsLoadedTotalWeightString() {
        // Car panel does not show total weight; Compose still holds the loaded string.
        val lim =
            buildLimits(
                clearance = false,
                heightOnly = true,
                axle = "999",
                bogie = "999",
                height = "2.0",
                width = "999",
                length = "999",
                total = "7500",
            )
        assertNull(lim.axleWeightKg)
        assertNull(lim.widthM)
        assertEquals(2.0, lim.heightM!!, 0.01)
        assertEquals(7_500.0, lim.totalWeightKg!!, 0.01)
    }
}
