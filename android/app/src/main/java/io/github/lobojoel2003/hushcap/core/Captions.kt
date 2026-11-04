package io.github.lobojoel2003.hushcap.core

/** Where audio comes from. */
enum class AudioSource(val label: String) {
    MICROPHONE("Microphone"),
    SYSTEM("Device audio"),
}

/** A finished caption line. */
data class Caption(
    val id: Long,
    val source: AudioSource,
    val speaker: Int?,
    val text: String,
    val startMs: Long,
    val endMs: Long,
)

/** Interim text for an utterance still being spoken. */
data class PartialCaption(
    val source: AudioSource,
    val text: String,
    val startMs: Long,
)

/** Transcript formats people can save or share. */
enum class ExportFormat(val extension: String, val mimeType: String) {
    TXT("txt", "text/plain"),
    SRT("srt", "application/x-subrip"),
    VTT("vtt", "text/vtt"),
    MARKDOWN("md", "text/markdown"),
}

/** Display name for a speaker: a custom name if set, else "Speaker N". */
fun speakerName(speaker: Int, names: Map<Int, String>): String =
    names[speaker]?.trim()?.takeIf { it.isNotEmpty() } ?: "Speaker $speaker"

/** `HH:MM:SS<sep>mmm` */
fun timestamp(ms: Long, separator: Char): String {
    val h = ms / 3_600_000
    val m = ms / 60_000 % 60
    val s = ms / 1000 % 60
    val frac = ms % 1000
    return "%02d:%02d:%02d%c%03d".format(h, m, s, separator, frac)
}

/** `M:SS` or `H:MM:SS` */
fun clock(ms: Long): String {
    val h = ms / 3_600_000
    val m = ms / 60_000 % 60
    val s = ms / 1000 % 60
    return if (h > 0) "%d:%02d:%02d".format(h, m, s) else "%d:%02d".format(m, s)
}

/** Render a transcript. Mirrors the desktop app's export formats. */
object TranscriptExporter {
    fun export(
        captions: List<Caption>,
        names: Map<Int, String>,
        format: ExportFormat,
        includeSpeakers: Boolean = true,
        includeTimestamps: Boolean = true,
        title: String = "Transcript",
    ): String {
        fun who(c: Caption): String? =
            if (includeSpeakers) c.speaker?.let { speakerName(it, names) } else null

        val out = StringBuilder()
        when (format) {
            ExportFormat.TXT -> captions.forEach { c ->
                if (includeTimestamps) out.append('[').append(clock(c.startMs)).append("] ")
                who(c)?.let { out.append(it).append(": ") }
                out.append(c.text).append('\n')
            }
            ExportFormat.SRT -> captions.forEachIndexed { i, c ->
                out.append(i + 1).append('\n')
                out.append(timestamp(c.startMs, ',')).append(" --> ")
                    .append(timestamp(maxOf(c.endMs, c.startMs + 1), ',')).append('\n')
                who(c)?.let { out.append(it).append(": ") }
                out.append(c.text).append("\n\n")
            }
            ExportFormat.VTT -> {
                out.append("WEBVTT\n\n")
                captions.forEach { c ->
                    out.append(timestamp(c.startMs, '.')).append(" --> ")
                        .append(timestamp(maxOf(c.endMs, c.startMs + 1), '.')).append('\n')
                    val text = vttEscape(c.text)
                    val name = who(c)
                    if (name != null) out.append("<v ").append(vttEscape(name)).append('>')
                    out.append(text).append("\n\n")
                }
            }
            ExportFormat.MARKDOWN -> {
                out.append("# ").append(title).append("\n\n")
                captions.forEach { c ->
                    val prefix = buildString {
                        who(c)?.let { append("**").append(it).append("**") }
                        if (includeTimestamps) {
                            if (isNotEmpty()) append(' ')
                            append('_').append(clock(c.startMs)).append('_')
                        }
                    }
                    if (prefix.isEmpty()) out.append(c.text).append("\n\n")
                    else out.append(prefix).append(": ").append(c.text).append("\n\n")
                }
            }
        }
        return out.toString()
    }

    private fun vttEscape(s: String): String =
        s.replace("&", "&amp;").replace("<", "&lt;").replace(">", "&gt;")
}
