package io.github.lobojoel2003.hushcap.asr

import io.github.lobojoel2003.hushcap.audio.SAMPLE_RATE
import io.github.lobojoel2003.hushcap.core.AudioSource
import io.github.lobojoel2003.hushcap.core.VoskResult
import kotlin.math.log10
import kotlin.math.sqrt
import org.vosk.Model
import org.vosk.Recognizer
import org.vosk.SpeakerModel

/**
 * Streams one source's audio through its own Vosk recogniser and reports
 * interim text, finished phrases (with timing and a speaker vector) and
 * input levels. Timestamps are milliseconds since [sessionStartNanos].
 */
class SourceRecognizer(
    val source: AudioSource,
    model: Model,
    speakerModel: SpeakerModel?,
    private val sessionStartNanos: Long,
    private val listener: Listener,
) {
    interface Listener {
        fun onPartial(source: AudioSource, text: String, startMs: Long)
        fun onFinal(source: AudioSource, result: VoskResult, startMs: Long, endMs: Long)
        fun onLevel(source: AudioSource, db: Float)
    }

    private val recognizer: Recognizer =
        if (speakerModel != null) Recognizer(model, SAMPLE_RATE.toFloat(), speakerModel)
        else Recognizer(model, SAMPLE_RATE.toFloat())

    private val lock = Any()
    private var utteranceStartMs: Long? = null
    private var lastPartial = ""
    private var closed = false

    private fun nowMs(): Long = (System.nanoTime() - sessionStartNanos) / 1_000_000

    fun feed(buffer: ShortArray, length: Int) {
        synchronized(lock) { feedLocked(buffer, length) }
    }

    private fun feedLocked(buffer: ShortArray, length: Int) {
        if (closed) return
        var sum = 0.0
        for (i in 0 until length) {
            val v = buffer[i] / 32768.0
            sum += v * v
        }
        val rms = sqrt(sum / length.coerceAtLeast(1))
        listener.onLevel(source, if (rms > 0) (20 * log10(rms)).toFloat().coerceAtLeast(-100f) else -100f)

        val chunkMs = length * 1000L / SAMPLE_RATE
        val bufferStart = nowMs() - chunkMs
        if (recognizer.acceptWaveForm(buffer, length)) {
            emitFinal(VoskResult.parseFinal(recognizer.result))
        } else {
            val partial = VoskResult.parsePartial(recognizer.partialResult)
            if (partial.isNotEmpty() && partial != lastPartial) {
                val start = utteranceStartMs ?: bufferStart.coerceAtLeast(0).also { utteranceStartMs = it }
                lastPartial = partial
                listener.onPartial(source, partial, start)
            }
        }
    }

    private fun emitFinal(result: VoskResult) {
        val end = nowMs()
        val start = utteranceStartMs ?: (end - result.speakerFrames * 10L).coerceAtLeast(0)
        utteranceStartMs = null
        lastPartial = ""
        if (result.text.isNotEmpty()) listener.onFinal(source, result, start, end)
    }

    /** Flush the last phrase and free the recogniser. */
    fun close() {
        synchronized(lock) {
            if (closed) return
            closed = true
            runCatching { emitFinal(VoskResult.parseFinal(recognizer.finalResult)) }
            recognizer.close()
        }
    }
}
