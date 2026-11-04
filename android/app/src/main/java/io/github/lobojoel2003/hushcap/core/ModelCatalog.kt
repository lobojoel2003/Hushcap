package io.github.lobojoel2003.hushcap.core

/** What a model is for. */
enum class ModelKind { SPEECH, SPEAKER }

/** A downloadable Vosk model. Downloads are the only network access Hushcap makes. */
data class ModelInfo(
    val id: String,
    val kind: ModelKind,
    val name: String,
    val language: String,
    val sizeMb: Int,
    val url: String,
    val recommended: Boolean = false,
)

object ModelCatalog {
    private const val BASE = "https://alphacephei.com/vosk/models/"

    private fun speech(id: String, name: String, language: String, sizeMb: Int, recommended: Boolean = false) =
        ModelInfo(id, ModelKind.SPEECH, name, language, sizeMb, "$BASE$id.zip", recommended)

    val models: List<ModelInfo> = listOf(
        speech("vosk-model-small-en-us-0.15", "English (US)", "en", 40, recommended = true),
        speech("vosk-model-small-en-in-0.4", "English (India)", "en", 36),
        speech("vosk-model-small-es-0.42", "Español", "es", 39),
        speech("vosk-model-small-fr-0.22", "Français", "fr", 41),
        speech("vosk-model-small-de-0.15", "Deutsch", "de", 45),
        speech("vosk-model-small-it-0.22", "Italiano", "it", 48),
        speech("vosk-model-small-pt-0.3", "Português", "pt", 31),
        speech("vosk-model-small-nl-0.22", "Nederlands", "nl", 39),
        speech("vosk-model-small-ru-0.22", "Русский", "ru", 45),
        speech("vosk-model-small-uk-v3-small", "Українська", "uk", 133),
        speech("vosk-model-small-hi-0.22", "हिन्दी", "hi", 42),
        speech("vosk-model-small-cn-0.22", "中文", "zh", 42),
        speech("vosk-model-small-ja-0.22", "日本語", "ja", 48),
        speech("vosk-model-small-ko-0.22", "한국어", "ko", 82),
        speech("vosk-model-small-tr-0.3", "Türkçe", "tr", 35),
        speech("vosk-model-small-vn-0.4", "Tiếng Việt", "vi", 32),
        ModelInfo(
            id = "vosk-model-spk-0.4",
            kind = ModelKind.SPEAKER,
            name = "Speaker labels (any language)",
            language = "*",
            sizeMb = 13,
            url = "${BASE}vosk-model-spk-0.4.zip",
            recommended = true,
        ),
    )

    val allowedHosts = setOf("alphacephei.com")

    fun find(id: String): ModelInfo? = models.firstOrNull { it.id == id }

    fun recommended(kind: ModelKind): ModelInfo = models.first { it.kind == kind && it.recommended }
}
