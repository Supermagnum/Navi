package no.navi.app

import com.github.luben.zstd.ZstdInputStream
import org.json.JSONArray
import org.json.JSONObject
import uniffi.navi.CorridorRouteResult
import uniffi.navi.TravelProfile
import java.io.ByteArrayInputStream
import java.io.ByteArrayOutputStream
import java.io.File
import java.io.FileInputStream
import java.nio.charset.StandardCharsets
import java.security.MessageDigest

/**
 * Named-route search stubs from `{Leaf}.navi-sidecar.v1.zst` section `named_routes` (NR01).
 *
 * Spec (sidecar-format.md §6, client-routing-hooks.md §5): the records are search stubs
 * (OSM relation id, kind, representative point, display name). They do not alter edge
 * costs. An omitted section means the leaf was not annotated.
 *
 * Kind bytes the hike toggles select:
 * - Follow pilgrim routes (hiking) → `pilgrimage` (3)
 * - Follow official networks, hiking → `hiking` (1)
 * - Follow official networks, bicycle → `bicycle` (2)
 *
 * `network` is not on the wire. Official and named-local hiking share kind `hiking`.
 */
object NamedRouteSidecar {
    const val FILE_SUFFIX = ".navi-sidecar.v1.zst"
    private const val MAGIC = "NVSC"
    private const val FORMAT_VERSION = 1
    private const val SECTION_NAMED_ROUTES = "named_routes"
    private const val SECTION_WETLAND = "wetland_edges"
    private const val SECTION_ATTRIBUTION = "attribution"
    private const val NR01 = "NR01"
    private const val MAX_LOGICAL_BYTES = 64L * 1024L * 1024L

    const val KIND_HIKING = 1
    const val KIND_BICYCLE = 2
    const val KIND_PILGRIMAGE = 3

    private val LEAF_ID = Regex("[A-Za-z0-9][A-Za-z0-9_.-]{0,80}")
    private val VERSION_DIR = Regex("\\d{1,6}")
    private val sha256Cache = java.util.concurrent.ConcurrentHashMap<String, String>()

    data class Stub(
        val osmId: Long,
        val kind: Int,
        val lat: Double,
        val lon: Double,
        val name: String,
    ) {
        val kindName: String = kindName(kind)
    }

    sealed class Load {
        data class Stubs(
            val routes: List<Stub>,
        ) : Load()

        /** Section present and `count = 0` (annotated, nothing matched at bake). */
        data object Empty : Load()

        /** File missing, or the optional section was not written. */
        data object NotAvailable : Load()

        /** Bad magic, version, truncation, checksum, or MWM fingerprint. */
        data class Rejected(
            val reason: String,
        ) : Load()
    }

    /** Kind strings from sidecar-format.md §6. Unknown bytes stay numeric. */
    fun kindName(code: Int): String =
        when (code) {
            0 -> "other"
            KIND_HIKING -> "hiking"
            KIND_BICYCLE -> "bicycle"
            KIND_PILGRIMAGE -> "pilgrimage"
            4 -> "horse"
            5 -> "running"
            6 -> "mtb"
            7 -> "superroute"
            8 -> "ski"
            9 -> "canoe"
            10 -> "inline_skates"
            else -> code.toString()
        }

    /**
     * Kinds the live toggles can select. Other NR01 kinds stay in the sidecar
     * and are not treated as a toggle this app does not have.
     */
    fun wantedKinds(
        profile: TravelProfile,
        preferOfficialNetworks: Boolean,
        preferPilgrimRoutes: Boolean,
    ): Set<Int> {
        val kinds = LinkedHashSet<Int>()
        when (profile) {
            TravelProfile.HIKING -> {
                if (preferOfficialNetworks) kinds.add(KIND_HIKING)
                if (preferPilgrimRoutes) kinds.add(KIND_PILGRIMAGE)
            }
            TravelProfile.BICYCLE, TravelProfile.BICYCLE_ELECTRIC -> {
                if (preferOfficialNetworks) kinds.add(KIND_BICYCLE)
            }
            else -> Unit
        }
        return kinds
    }

    /**
     * Append search-stub lines to [result]. Does not change [CorridorRouteResult.routePolyline]
     * or any other geometry field. Never reports that a named route was followed.
     */
    fun annotate(
        mwmRoot: File,
        leaves: Collection<String>,
        profile: TravelProfile,
        preferOfficialNetworks: Boolean,
        preferPilgrimRoutes: Boolean,
        result: CorridorRouteResult,
    ): CorridorRouteResult {
        val kinds = wantedKinds(profile, preferOfficialNetworks, preferPilgrimRoutes)
        if (kinds.isEmpty()) return result
        val note =
            try {
                reportLines(mwmRoot, leaves, kinds)
            } catch (_: Throwable) {
                notFollowedLines("not_available")
            }
        return result.copy(report = joinReport(result.report, note))
    }

    /** Toggle was on but the sidecar could not be used. Line is left as-is. */
    fun markNotFollowed(result: CorridorRouteResult): CorridorRouteResult {
        if (result.report.contains("named_routes_followed=")) return result
        return result.copy(report = joinReport(result.report, notFollowedLines("not_available")))
    }

    internal fun parseOuter(
        outerZstd: ByteArray,
        mwmBytes: ByteArray? = null,
    ): Load {
        val logical =
            zstdDecompress(outerZstd) ?: return Load.Rejected("zstd")
        return parseLogical(logical, mwmBytes)
    }

    internal fun parseLogical(
        logical: ByteArray,
        mwmBytes: ByteArray? = null,
    ): Load {
        if (logical.size < 8 || logical.decodeAscii(0, 4) != MAGIC) {
            return Load.Rejected("bad_magic")
        }
        val version = u32(logical, 4) ?: return Load.Rejected("truncated")
        if (version != FORMAT_VERSION) return Load.Rejected("unsupported_version")
        var off = 8
        off = skipU16String(logical, off) ?: return Load.Rejected("truncated")
        off = skipU16String(logical, off) ?: return Load.Rejected("truncated")
        if (off + 32 + 8 + 4 > logical.size) return Load.Rejected("truncated")
        val mwmSha = logical.copyOfRange(off, off + 32)
        off += 32
        val mwmSize = u64(logical, off) ?: return Load.Rejected("truncated")
        off += 8
        if (mwmBytes != null && !fingerprintMatches(mwmBytes, mwmSize, mwmSha)) {
            return Load.Rejected("fingerprint")
        }
        val sectionCount = u32(logical, off) ?: return Load.Rejected("truncated")
        off += 4
        if (sectionCount < 0 || sectionCount > 64) return Load.Rejected("section_count")

        data class Section(
            val name: String,
            val uncompressed: Long,
            val compressed: Long,
            val sha256: ByteArray,
            val offset: Long,
        )
        val sections = ArrayList<Section>(sectionCount)
        repeat(sectionCount) {
            val nameLen = u16(logical, off) ?: return Load.Rejected("truncated")
            off += 2
            if (off + nameLen + 8 + 8 + 32 + 8 > logical.size) return Load.Rejected("truncated")
            val name =
                runCatching {
                    String(logical, off, nameLen, StandardCharsets.UTF_8)
                }.getOrElse {
                    return Load.Rejected("utf8")
                }
            off += nameLen
            val uncompressed = u64(logical, off) ?: return Load.Rejected("truncated")
            off += 8
            val compressed = u64(logical, off) ?: return Load.Rejected("truncated")
            off += 8
            val sha = logical.copyOfRange(off, off + 32)
            off += 32
            val payloadOff = u64(logical, off) ?: return Load.Rejected("truncated")
            off += 8
            sections.add(Section(name, uncompressed, compressed, sha, payloadOff))
        }
        if (sections.none { it.name == SECTION_WETLAND } || sections.none { it.name == SECTION_ATTRIBUTION }) {
            return Load.Rejected("missing_required_section")
        }
        val named = sections.find { it.name == SECTION_NAMED_ROUTES } ?: return Load.NotAvailable
        val payloadStart = off
        val body =
            sectionBody(logical, payloadStart, named.offset, named.compressed, named.uncompressed, named.sha256)
                ?: return Load.Rejected("section")
        val routes =
            try {
                parseNamedRoutesBody(body)
            } catch (_: IllegalArgumentException) {
                return Load.Rejected("nr01")
            }
        return if (routes.isEmpty()) Load.Empty else Load.Stubs(routes)
    }

    internal fun parseNamedRoutesBody(body: ByteArray): List<Stub> {
        if (body.size < 8 || body.decodeAscii(0, 4) != NR01) {
            throw IllegalArgumentException("nr01")
        }
        val count = u32(body, 4) ?: throw IllegalArgumentException("truncated")
        if (count < 0 || count > 1_000_000) throw IllegalArgumentException("count")
        var off = 8
        val out = ArrayList<Stub>(count)
        repeat(count) {
            if (off + 8 + 1 + 4 + 4 + 2 > body.size) throw IllegalArgumentException("truncated")
            val osmId = u64(body, off) ?: throw IllegalArgumentException("truncated")
            off += 8
            val kind = body[off].toInt() and 0xff
            off += 1
            val latE7 = i32(body, off)
            off += 4
            val lonE7 = i32(body, off)
            off += 4
            val nameLen = u16(body, off) ?: throw IllegalArgumentException("truncated")
            off += 2
            if (off + nameLen > body.size) throw IllegalArgumentException("truncated")
            val name =
                runCatching { String(body, off, nameLen, StandardCharsets.UTF_8) }.getOrElse {
                    throw IllegalArgumentException("utf8")
                }
            off += nameLen
            out.add(
                Stub(
                    osmId = osmId,
                    kind = kind,
                    lat = latE7 / 1.0e7,
                    lon = lonE7 / 1.0e7,
                    name = name,
                ),
            )
        }
        if (off != body.size) throw IllegalArgumentException("trailing")
        return out
    }

    internal fun sidecarFile(
        root: File,
        version: String,
        leaf: String,
    ): File? {
        if (!VERSION_DIR.matches(version) || !LEAF_ID.matches(leaf)) return null
        val name = leaf + FILE_SUFFIX
        return listOf(
            File(root, "releases/$version/sidecars/$name"),
            File(root, "releases/$version/leaves/$name"),
            File(root, "$version/$name"),
        ).firstOrNull { it.isFile }
    }

    internal fun mwmFile(
        root: File,
        version: String,
        leaf: String,
    ): File? {
        if (!VERSION_DIR.matches(version) || !LEAF_ID.matches(leaf)) return null
        val name = "$leaf.mwm"
        return listOf(
            File(root, "releases/$version/leaves/$name"),
            File(root, "$version/$name"),
        ).firstOrNull { it.isFile }
    }

    private fun reportLines(
        mwmRoot: File,
        leaves: Collection<String>,
        kinds: Set<Int>,
    ): String {
        val version = activeVersion(mwmRoot)
        if (version == null) return notFollowedLines("not_available")
        val matched = LinkedHashMap<Long, Stub>()
        val unavailable = ArrayList<String>()
        var sawSection = false
        val leafIds = leaves.filter { it.isNotBlank() }
        if (leafIds.isEmpty()) return notFollowedLines("not_available")
        for (leaf in leafIds) {
            val side = sidecarFile(mwmRoot, version, leaf)
            val mwm = mwmFile(mwmRoot, version, leaf)
            if (side == null || mwm == null) {
                unavailable.add(leaf)
                continue
            }
            val load =
                try {
                    parseFile(side, mwm)
                } catch (_: Throwable) {
                    Load.Rejected("io")
                }
            when (load) {
                is Load.Stubs -> {
                    sawSection = true
                    for (stub in load.routes) {
                        if (stub.kind in kinds) matched.putIfAbsent(stub.osmId, stub)
                    }
                }
                Load.Empty -> sawSection = true
                Load.NotAvailable, is Load.Rejected -> unavailable.add(leaf)
            }
        }
        val status =
            when {
                matched.isNotEmpty() -> "stubs"
                sawSection -> "none"
                else -> "not_available"
            }
        return buildString {
            append(notFollowedLines(status))
            if (unavailable.isNotEmpty()) {
                append("named_routes_unavailable=")
                append(unavailable.joinToString(","))
                append('\n')
            }
            if (matched.isNotEmpty()) {
                val arr = JSONArray()
                for (stub in matched.values.sortedBy { it.osmId }) {
                    arr.put(
                        JSONObject()
                            .put("osm_id", stub.osmId)
                            .put("kind", stub.kindName)
                            .put("lat", stub.lat)
                            .put("lon", stub.lon)
                            .put("name", stub.name),
                    )
                }
                append("named_routes_json=")
                append(arr.toString())
                append('\n')
            }
        }
    }

    private fun parseFile(
        sidecar: File,
        mwm: File,
    ): Load {
        val logical = zstdDecompress(sidecar.readBytes()) ?: return Load.Rejected("zstd")
        val header = peekFingerprint(logical) ?: return Load.Rejected("fingerprint")
        if (mwm.length() != header.size) return Load.Rejected("fingerprint")
        val digest = sha256Hex(mwm)
        if (!digest.equals(header.sha256Hex, ignoreCase = true)) {
            return Load.Rejected("fingerprint")
        }
        return parseLogical(logical, mwmBytes = null)
    }

    private data class Fingerprint(
        val size: Long,
        val sha256Hex: String,
    )

    private fun peekFingerprint(logical: ByteArray): Fingerprint? {
        if (logical.decodeAscii(0, 4) != MAGIC) return null
        var off = 8
        off = skipU16String(logical, off) ?: return null
        off = skipU16String(logical, off) ?: return null
        if (off + 32 + 8 > logical.size) return null
        val sha = logical.copyOfRange(off, off + 32).toHex()
        off += 32
        val size = u64(logical, off) ?: return null
        return Fingerprint(size, sha)
    }

    private fun fingerprintMatches(
        mwmBytes: ByteArray,
        size: Long,
        sha: ByteArray,
    ): Boolean {
        if (mwmBytes.size.toLong() != size) return false
        val digest = MessageDigest.getInstance("SHA-256").digest(mwmBytes)
        return digest.contentEquals(sha)
    }

    private fun activeVersion(root: File): String? {
        val active = File(root, "active.json")
        if (!active.isFile) return null
        return runCatching {
            JSONObject(active.readText()).optString("version").takeIf { VERSION_DIR.matches(it) }
        }.getOrNull()
    }

    private fun sectionBody(
        logical: ByteArray,
        payloadStart: Int,
        offset: Long,
        compressed: Long,
        uncompressed: Long,
        sha256: ByteArray,
    ): ByteArray? {
        if (offset < 0 || compressed < 0 || uncompressed < 0) return null
        if (compressed > MAX_LOGICAL_BYTES || uncompressed > MAX_LOGICAL_BYTES) return null
        val start = payloadStart.toLong() + offset
        val end = start + compressed
        if (start > logical.size || end > logical.size || start > Int.MAX_VALUE || end > Int.MAX_VALUE) {
            return null
        }
        val frame = logical.copyOfRange(start.toInt(), end.toInt())
        val raw = zstdDecompress(frame) ?: return null
        if (raw.size.toLong() != uncompressed) return null
        val got = MessageDigest.getInstance("SHA-256").digest(raw)
        if (!got.contentEquals(sha256)) return null
        return raw
    }

    private fun zstdDecompress(src: ByteArray): ByteArray? {
        if (src.isEmpty()) return null
        return try {
            ByteArrayInputStream(src).use { raw ->
                ZstdInputStream(raw).use { zin ->
                    val out = ByteArrayOutputStream()
                    val buf = ByteArray(8192)
                    while (true) {
                        val n = zin.read(buf)
                        if (n < 0) break
                        if (out.size() + n > MAX_LOGICAL_BYTES) return null
                        if (n > 0) out.write(buf, 0, n)
                    }
                    out.toByteArray()
                }
            }
        } catch (_: Throwable) {
            null
        }
    }

    private fun sha256Hex(file: File): String {
        val key = file.absolutePath + ":" + file.length() + ":" + file.lastModified()
        return sha256Cache.computeIfAbsent(key) {
            val md = MessageDigest.getInstance("SHA-256")
            FileInputStream(file).use { input ->
                val buf = ByteArray(1024 * 1024)
                while (true) {
                    val n = input.read(buf)
                    if (n < 0) break
                    if (n > 0) md.update(buf, 0, n)
                }
            }
            md.digest().toHex()
        }
    }

    private fun notFollowedLines(status: String): String = "named_routes=$status\nnamed_routes_followed=false\n"

    private fun joinReport(
        report: String,
        extra: String,
    ): String = if (report.endsWith("\n") || report.isEmpty()) report + extra else report + "\n" + extra

    private fun skipU16String(
        b: ByteArray,
        off: Int,
    ): Int? {
        val n = u16(b, off) ?: return null
        val next = off + 2 + n
        return if (next <= b.size) next else null
    }

    private fun u16(
        b: ByteArray,
        off: Int,
    ): Int? {
        if (off + 2 > b.size) return null
        return (b[off].toInt() and 0xff) or ((b[off + 1].toInt() and 0xff) shl 8)
    }

    private fun u32(
        b: ByteArray,
        off: Int,
    ): Int? {
        if (off + 4 > b.size) return null
        return (b[off].toInt() and 0xff) or
            ((b[off + 1].toInt() and 0xff) shl 8) or
            ((b[off + 2].toInt() and 0xff) shl 16) or
            ((b[off + 3].toInt() and 0xff) shl 24)
    }

    private fun i32(
        b: ByteArray,
        off: Int,
    ): Int = u32(b, off) ?: throw IllegalArgumentException("truncated")

    private fun u64(
        b: ByteArray,
        off: Int,
    ): Long? {
        if (off + 8 > b.size) return null
        var v = 0L
        for (s in 0 until 8) {
            v = v or ((b[off + s].toLong() and 0xff) shl (8 * s))
        }
        return v
    }

    private fun ByteArray.decodeAscii(
        off: Int,
        len: Int,
    ): String? {
        if (off + len > size) return null
        return String(this, off, len, StandardCharsets.US_ASCII)
    }

    private fun ByteArray.toHex(): String = joinToString("") { "%02x".format(it) }
}
