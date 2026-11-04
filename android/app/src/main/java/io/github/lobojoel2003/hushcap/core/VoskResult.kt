package io.github.lobojoel2003.hushcap.core

import org.json.JSONObject

/** A parsed Vosk recogniser result. */
data class VoskResult(
    val text: String,
    /** Speaker x-vector, when a speaker model is loaded and the phrase was long enough. */
    val speakerVector: FloatArray?,
    /** Number of 10 ms frames the speaker vector was computed from. */
    val speakerFrames: Int,
) {
    override fun equals(other: Any?): Boolean =
        other is VoskResult && text == other.text &&
            speakerFrames == other.speakerFrames &&
            (speakerVector?.contentEquals(other.speakerVector) ?: (other.speakerVector == null))

    override fun hashCode(): Int =
        31 * (31 * text.hashCode() + (speakerVector?.contentHashCode() ?: 0)) + speakerFrames

    companion object {
        /** Parse `Recognizer.getResult()` / `getFinalResult()` JSON. */
        fun parseFinal(json: String): VoskResult {
            val obj = runCatching { JSONObject(json) }.getOrNull() ?: return VoskResult("", null, 0)
            val text = TextCleaner.clean(obj.optString("text", ""))
            val spk = obj.optJSONArray("spk")?.let { arr ->
                FloatArray(arr.length()) { arr.optDouble(it, 0.0).toFloat() }
            }
            return VoskResult(text, spk?.takeIf { it.isNotEmpty() }, obj.optInt("spk_frames", 0))
        }

        /** Parse `Recognizer.getPartialResult()` JSON. */
        fun parsePartial(json: String): String {
            val obj = runCatching { JSONObject(json) }.getOrNull() ?: return ""
            return TextCleaner.clean(obj.optString("partial", ""))
        }
    }
}

/** Tidies recogniser output for display. */
object TextCleaner {
    private val whitespace = Regex("\\s+")

    fun clean(raw: String): String {
        val collapsed = raw.replace("[unk]", " ").replace(whitespace, " ").trim()
        if (collapsed.isEmpty()) return ""
        return collapsed.replaceFirstChar { if (it.isLowerCase()) it.titlecase() else it.toString() }
    }

    /** Vosk small models emit lowercase without punctuation; finish the sentence. */
    fun asSentence(text: String): String {
        if (text.isEmpty()) return text
        val last = text.last()
        return if (last == '.' || last == '?' || last == '!' || last == '…') text else "$text."
    }
}
