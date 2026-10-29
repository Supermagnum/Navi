package no.navi.app

import android.content.ContentProvider
import android.content.ContentValues
import android.database.Cursor
import android.net.Uri

/**
 * Debug-only installer: wires [NaviDebugIntent] into [DebugTripHooks] before
 * [MainActivity] runs. Absent from release APKs (debug source set).
 */
class DebugTripInitProvider : ContentProvider() {
    override fun onCreate(): Boolean {
        DebugTripHooks.backend =
            object : DebugTripHooks.Backend {
                override fun consumeTripExtras(
                    context: android.content.Context,
                    intent: android.content.Intent,
                    dataDirPath: String,
                ) = NaviDebugIntent.consumeTripExtras(context, intent, dataDirPath)

                override fun consumePendingTripFile(
                    context: android.content.Context,
                    dataDirPath: String,
                ) = NaviDebugIntent.consumePendingTripFile(context, dataDirPath)

                override fun restoreAfterPlan(dataDirPath: String) {
                    NaviDebugIntent.restoreAfterPlan(dataDirPath)
                }

                override fun debugBuild(context: android.content.Context?): Boolean = NaviDebugIntent.debugBuild(context)
            }
        return true
    }

    override fun query(
        uri: Uri,
        projection: Array<out String>?,
        selection: String?,
        selectionArgs: Array<out String>?,
        sortOrder: String?,
    ): Cursor? = null

    override fun getType(uri: Uri): String? = null

    override fun insert(
        uri: Uri,
        values: ContentValues?,
    ): Uri? = null

    override fun delete(
        uri: Uri,
        selection: String?,
        selectionArgs: Array<out String>?,
    ): Int = 0

    override fun update(
        uri: Uri,
        values: ContentValues?,
        selection: String?,
        selectionArgs: Array<out String>?,
    ): Int = 0
}
