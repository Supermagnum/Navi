package no.navi.app

import java.util.concurrent.atomic.AtomicInteger
import java.util.concurrent.atomic.AtomicReference

/**
 * One place applies basemap styles. Every request gets a generation number; a
 * result for an older generation is discarded and can never replace a newer
 * one. Route / layer / style-epoch updates that keep the same source do not
 * reload the base map.
 */
object BasemapStyleApplyQueue {
    data class Request(
        val generation: Int,
        val sourceKey: String,
        val forceBaseReload: Boolean,
    )

    data class Result(
        val generation: Int,
        val sourceKey: String,
        val ok: Boolean,
        val reloadedBase: Boolean,
        val note: String? = null,
    )

    private val generation = AtomicInteger(0)
    private val appliedSource = AtomicReference<String?>(null)
    private val lastAccepted = AtomicReference<Result?>(null)
    private val lastDiscardedGeneration = AtomicInteger(0)
    private val lastNote = AtomicReference<String?>(null)

    fun resetForTests() {
        generation.set(0)
        appliedSource.set(null)
        lastAccepted.set(null)
        lastDiscardedGeneration.set(0)
        lastNote.set(null)
    }

    fun currentGeneration(): Int = generation.get()

    fun appliedSourceKey(): String? = appliedSource.get()

    fun lastAccepted(): Result? = lastAccepted.get()

    fun lastDiscardedGeneration(): Int = lastDiscardedGeneration.get()

    fun lastNote(): String? = lastNote.get()

    /** Enqueue a style request. Returns the generation assigned to it. */
    fun enqueue(
        sourceKey: String,
        forceBaseReload: Boolean = false,
    ): Request {
        val gen = generation.incrementAndGet()
        return Request(generation = gen, sourceKey = sourceKey, forceBaseReload = forceBaseReload)
    }

    /**
     * True when this request should load a new base style. False when only the
     * route / overlay layers should change.
     */
    fun shouldReloadBase(request: Request): Boolean {
        if (!isCurrent(request.generation)) return false
        val current = appliedSource.get()
        if (current == null) return true
        if (current != request.sourceKey) return true
        return false
    }

    fun isCurrent(generation: Int): Boolean = generation == this.generation.get()

    /**
     * Accept a finished apply. Returns false when [result] is stale; the live
     * map must keep whatever it already shows.
     */
    fun accept(result: Result): Boolean {
        if (!isCurrent(result.generation)) {
            lastDiscardedGeneration.set(result.generation)
            return false
        }
        if (result.ok) {
            appliedSource.set(result.sourceKey)
        } else if (result.note != null) {
            lastNote.set(result.note)
        }
        lastAccepted.set(result)
        return true
    }

    /** Keep the previous source after a failed replacement. */
    fun keepPrevious(note: String?) {
        lastNote.set(note)
    }
}
