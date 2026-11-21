package no.navi.app

import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder
import java.io.File

/**
 * VFAT/exFAT cannot symlink `{mwm}/{ver}` → `releases/{ver}/leaves`. The
 * bootstrap mirrors leaf `.mwm` files into a real `{ver}/` directory instead.
 */
class ComapsSessionBootstrapMirrorTest {
    @get:Rule
    val tmp = TemporaryFolder()

    @Test
    fun mirrorLeavesInto_copiesMissingLeavesAndSkipsSameSize() {
        val srcDir = tmp.newFolder("leaves")
        val destDir = tmp.newFolder("261105")
        val a = File(srcDir, "Norway_Hedmark.mwm").also { it.writeBytes(ByteArray(32) { 1 }) }
        val b = File(srcDir, "Norway_Oppland.mwm").also { it.writeBytes(ByteArray(48) { 2 }) }
        // Pre-existing same-size file must be kept (skip).
        File(destDir, a.name).writeBytes(ByteArray(32) { 1 })
        assertTrue(ComapsSessionBootstrap.mirrorLeavesInto(destDir, arrayOf(a, b)))
        assertEquals(32, File(destDir, a.name).length())
        assertEquals(48, File(destDir, b.name).length())
        assertEquals(2, destDir.listFiles { f -> f.name.endsWith(".mwm") }!!.size)
    }

    @Test
    fun alignCountriesListVersion_raisesStaleVAndLeavesTheTree() {
        val root = tmp.newFolder("mwm2")
        val file = File(root, "countries.txt")
        file.writeText("""{ "id": "Countries", "v": 261105, "g": [ { "id": "Norway" } ] }""")
        ComapsSessionBootstrap.alignCountriesListVersion(root, "261115")
        val text = file.readText()
        assertTrue(text.contains("\"v\": 261115"))
        assertTrue(text.contains("Norway"))
        ComapsSessionBootstrap.alignCountriesListVersion(root, "261105")
        assertTrue(file.readText().contains("\"v\": 261115"))
    }

    @Test
    fun pruneStaleReleaseDirs_removesOlderVersionDirAndKeepsActive() {
        val root = tmp.newFolder("mwm")
        val stale = File(root, "261105").also { it.mkdirs() }
        File(stale, "Norway_Western.mwm").writeBytes(ByteArray(16) { 4 })
        val active = File(root, "261115").also { it.mkdirs() }
        File(active, "Norway_Hedmark.mwm").writeBytes(ByteArray(16) { 5 })
        File(root, "World.mwm").writeBytes(ByteArray(8))
        ComapsSessionBootstrap.pruneStaleReleaseDirs(root, "261115", keepVersion = "261105")
        assertTrue(stale.exists())
        ComapsSessionBootstrap.pruneStaleReleaseDirs(root, "261115")
        assertTrue(!stale.exists())
        assertTrue(File(active, "Norway_Hedmark.mwm").isFile)
        assertTrue(File(root, "World.mwm").isFile)
    }

    @Test
    fun mirrorLeavesInto_removesStaleLeafNames() {
        val srcDir = tmp.newFolder("leaves2")
        val destDir = tmp.newFolder("261106")
        val keep = File(srcDir, "Keep.mwm").also { it.writeBytes(ByteArray(8) { 3 }) }
        File(destDir, "Stale.mwm").writeBytes(ByteArray(8) { 9 })
        assertTrue(ComapsSessionBootstrap.mirrorLeavesInto(destDir, arrayOf(keep)))
        assertTrue(File(destDir, "Keep.mwm").isFile)
        assertTrue(!File(destDir, "Stale.mwm").exists())
    }
}
