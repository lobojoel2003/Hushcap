package io.github.lobojoel2003.hushcap.settings

import android.content.Context
import io.github.lobojoel2003.hushcap.core.ModelCatalog
import io.github.lobojoel2003.hushcap.core.ModelKind
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow

enum class ThemeMode(val label: String) {
    SYSTEM("Match system"),
    LIGHT("Light"),
    DARK("Dark"),
    HIGH_CONTRAST("High contrast"),
}

/** Everything people can change. Stored only on this device. */
data class AppSettings(
    val speechModelId: String = ModelCatalog.recommended(ModelKind.SPEECH).id,
    val speakerLabels: Boolean = true,
    val useMicrophone: Boolean = true,
    val useDeviceAudio: Boolean = false,
    val textScale: Float = 1.25f,
    val theme: ThemeMode = ThemeMode.SYSTEM,
    val speakerSeparation: Float = 0.5f,
    val suppressEcho: Boolean = true,
    val keepScreenOn: Boolean = true,
    val showTimestamps: Boolean = false,
    val onboardingDone: Boolean = false,
)

class Settings(context: Context) {
    private val prefs = context.getSharedPreferences("settings", Context.MODE_PRIVATE)
    private val _state = MutableStateFlow(load())
    val state: StateFlow<AppSettings> = _state.asStateFlow()

    val current: AppSettings get() = _state.value

    fun update(transform: (AppSettings) -> AppSettings) {
        val next = transform(_state.value)
        _state.value = next
        save(next)
    }

    private fun load(): AppSettings {
        val d = AppSettings()
        return AppSettings(
            speechModelId = prefs.getString("speechModelId", d.speechModelId) ?: d.speechModelId,
            speakerLabels = prefs.getBoolean("speakerLabels", d.speakerLabels),
            useMicrophone = prefs.getBoolean("useMicrophone", d.useMicrophone),
            useDeviceAudio = prefs.getBoolean("useDeviceAudio", d.useDeviceAudio),
            textScale = prefs.getFloat("textScale", d.textScale),
            theme = runCatching { ThemeMode.valueOf(prefs.getString("theme", d.theme.name)!!) }
                .getOrDefault(d.theme),
            speakerSeparation = prefs.getFloat("speakerSeparation", d.speakerSeparation),
            suppressEcho = prefs.getBoolean("suppressEcho", d.suppressEcho),
            keepScreenOn = prefs.getBoolean("keepScreenOn", d.keepScreenOn),
            showTimestamps = prefs.getBoolean("showTimestamps", d.showTimestamps),
            onboardingDone = prefs.getBoolean("onboardingDone", d.onboardingDone),
        )
    }

    private fun save(s: AppSettings) {
        prefs.edit()
            .putString("speechModelId", s.speechModelId)
            .putBoolean("speakerLabels", s.speakerLabels)
            .putBoolean("useMicrophone", s.useMicrophone)
            .putBoolean("useDeviceAudio", s.useDeviceAudio)
            .putFloat("textScale", s.textScale)
            .putString("theme", s.theme.name)
            .putFloat("speakerSeparation", s.speakerSeparation)
            .putBoolean("suppressEcho", s.suppressEcho)
            .putBoolean("keepScreenOn", s.keepScreenOn)
            .putBoolean("showTimestamps", s.showTimestamps)
            .putBoolean("onboardingDone", s.onboardingDone)
            .apply()
    }
}
