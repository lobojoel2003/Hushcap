package io.github.lobojoel2003.hushcap.core

import kotlin.math.sqrt

/**
 * Online speaker clustering, a port of `hushcap-core`'s diarization module.
 *
 * Each finished utterance's voice vector joins the most similar known
 * speaker when the cosine similarity is above [threshold]; otherwise it
 * starts a new speaker. Profiles are running averages, and profiles that
 * drift together are merged (reported so captions can be relabelled).
 * There is no small cap on the number of speakers.
 */
class SpeakerClusterer(
    var threshold: Float = 0.5f,
    var mergeThreshold: Float = 0.72f,
    private val minNewSpeakerMs: Long = 1200,
    private val maxSpeakers: Int = 64,
) {
    data class Assignment(
        val speaker: Int?,
        val isNew: Boolean,
        val similarity: Float,
        val merges: List<Pair<Int, Int>>,
    )

    private class Profile(val id: Int, val sum: FloatArray) {
        var centroid: FloatArray = normalized(sum)
        var count = 0

        fun add(e: FloatArray, weight: Float) {
            for (i in sum.indices) sum[i] += e[i] * weight
            count++
            centroid = normalized(sum)
        }
    }

    private val profiles = mutableListOf<Profile>()
    private var nextId = 1
    private var dim: Int? = null

    val speakerCount: Int get() = profiles.size

    fun reset() {
        profiles.clear()
        nextId = 1
        dim = null
    }

    /** Map the user-facing "speaker separation" (0..1) to thresholds. */
    fun setSeparation(separation: Float) {
        val s = separation.coerceIn(0f, 1f)
        threshold = 0.32f + 0.36f * s
        mergeThreshold = minOf(threshold + 0.22f, 0.9f)
    }

    fun assign(embedding: FloatArray, durationMs: Long): Assignment {
        val e = normalized(embedding)
        if (e.all { it == 0f } || (dim != null && dim != e.size)) {
            return Assignment(null, false, 0f, emptyList())
        }
        dim = e.size
        val weight = (durationMs / 1000f).coerceIn(0.3f, 6f)
        val best = profiles.withIndex()
            .map { (i, p) -> i to cosine(p.centroid, e) }
            .maxByOrNull { it.second }
        val short = durationMs < minNewSpeakerMs
        val full = profiles.size >= maxSpeakers

        if (best != null && (best.second >= threshold || (full && !short))) {
            val (idx, sim) = best
            val id = profiles[idx].id
            if (!short) profiles[idx].add(e, weight)
            val merges = if (short) emptyList() else mergeClose(idx)
            val speaker = merges.firstOrNull { it.first == id }?.second ?: id
            return Assignment(speaker, false, sim, merges)
        }
        if (short) {
            val speaker = best?.takeIf { it.second >= threshold * 0.75f }?.let { profiles[it.first].id }
            return Assignment(speaker, false, best?.second ?: 0f, emptyList())
        }
        val id = nextId++
        val profile = Profile(id, FloatArray(e.size))
        profile.add(e, weight)
        profiles.add(profile)
        return Assignment(id, true, best?.second ?: 0f, emptyList())
    }

    private fun mergeClose(start: Int): List<Pair<Int, Int>> {
        val merges = mutableListOf<Pair<Int, Int>>()
        var target = start
        while (true) {
            val centroid = profiles[target].centroid
            val candidate = profiles.withIndex()
                .filter { it.index != target }
                .map { it.index to cosine(it.value.centroid, centroid) }
                .filter { it.second >= mergeThreshold }
                .maxByOrNull { it.second } ?: break
            val other = candidate.first
            val (keep, gone) =
                if (profiles[other].id < profiles[target].id) other to target else target to other
            val removed = profiles[gone]
            val kept = profiles[keep]
            for (i in kept.sum.indices) kept.sum[i] += removed.sum[i]
            kept.count += removed.count
            kept.centroid = normalized(kept.sum)
            merges.add(removed.id to kept.id)
            profiles.removeAt(gone)
            target = if (keep > gone) keep - 1 else keep
        }
        return merges
    }

    companion object {
        fun cosine(a: FloatArray, b: FloatArray): Float {
            var dot = 0f
            var na = 0f
            var nb = 0f
            for (i in 0 until minOf(a.size, b.size)) {
                dot += a[i] * b[i]
                na += a[i] * a[i]
                nb += b[i] * b[i]
            }
            return if (na == 0f || nb == 0f) 0f else dot / (sqrt(na) * sqrt(nb))
        }

        fun normalized(v: FloatArray): FloatArray {
            val n = sqrt(v.fold(0f) { acc, x -> acc + x * x })
            return if (n == 0f || !n.isFinite()) FloatArray(v.size) else FloatArray(v.size) { v[it] / n }
        }
    }
}
