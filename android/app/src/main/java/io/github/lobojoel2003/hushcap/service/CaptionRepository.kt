package io.github.lobojoel2003.hushcap.service

import io.github.lobojoel2003.hushcap.core.AudioSource
import io.github.lobojoel2003.hushcap.core.Caption
import io.github.lobojoel2003.hushcap.core.PartialCaption
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update

enum class SessionStatus { IDLE, STARTING, RUNNING, STOPPING }

data class SessionState(
    val status: SessionStatus = SessionStatus.IDLE,
    val captions: List<Caption> = emptyList(),
    val partials: Map<AudioSource, PartialCaption> = emptyMap(),
    val speakerNames: Map<Int, String> = emptyMap(),
    val activeSources: Set<AudioSource> = emptySet(),
    val message: String? = null,
)

/**
 * In-memory state shared by the caption service and the UI. Nothing here is
 * written to disk unless the user exports a transcript.
 */
object CaptionRepository {
    private val _state = MutableStateFlow(SessionState())
    val state: StateFlow<SessionState> = _state.asStateFlow()

    /** Input levels (dBFS), kept separate so meters don't redraw the transcript. */
    private val _levels = MutableStateFlow<Map<AudioSource, Float>>(emptyMap())
    val levels: StateFlow<Map<AudioSource, Float>> = _levels.asStateFlow()

    private var nextId = 1L

    fun setStatus(status: SessionStatus, sources: Set<AudioSource>? = null) = _state.update {
        it.copy(
            status = status,
            activeSources = sources ?: if (status == SessionStatus.IDLE) emptySet() else it.activeSources,
            partials = if (status == SessionStatus.IDLE) emptyMap() else it.partials,
        )
    }

    fun setPartial(source: AudioSource, text: String, startMs: Long) = _state.update {
        it.copy(partials = it.partials + (source to PartialCaption(source, text, startMs)))
    }

    fun clearPartial(source: AudioSource) = _state.update { it.copy(partials = it.partials - source) }

    fun addCaption(source: AudioSource, speaker: Int?, text: String, startMs: Long, endMs: Long) =
        _state.update {
            val caption = Caption(nextId++, source, speaker, text, startMs, endMs)
            it.copy(captions = it.captions + caption, partials = it.partials - source)
        }

    fun mergeSpeakers(from: Int, into: Int) = _state.update { s ->
        val names = if (s.speakerNames.containsKey(from) && !s.speakerNames.containsKey(into)) {
            s.speakerNames - from + (into to s.speakerNames.getValue(from))
        } else {
            s.speakerNames
        }
        s.copy(
            captions = s.captions.map { c -> if (c.speaker == from) c.copy(speaker = into) else c },
            speakerNames = names,
        )
    }

    fun renameSpeaker(speaker: Int, name: String) = _state.update {
        it.copy(speakerNames = if (name.isBlank()) it.speakerNames - speaker else it.speakerNames + (speaker to name.trim()))
    }

    fun setLevel(source: AudioSource, db: Float) = _levels.update { it + (source to db) }

    fun showMessage(message: String?) = _state.update { it.copy(message = message) }

    fun clearTranscript() = _state.update {
        it.copy(captions = emptyList(), partials = emptyMap(), speakerNames = emptyMap())
    }
}
