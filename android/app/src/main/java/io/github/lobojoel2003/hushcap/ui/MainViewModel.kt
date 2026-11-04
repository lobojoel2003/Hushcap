package io.github.lobojoel2003.hushcap.ui

import android.app.Application
import android.content.Intent
import android.net.Uri
import androidx.core.content.FileProvider
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.viewModelScope
import io.github.lobojoel2003.hushcap.HushcapApp
import io.github.lobojoel2003.hushcap.core.ExportFormat
import io.github.lobojoel2003.hushcap.core.ModelInfo
import io.github.lobojoel2003.hushcap.core.ModelKind
import io.github.lobojoel2003.hushcap.core.TranscriptExporter
import io.github.lobojoel2003.hushcap.service.CaptionRepository
import io.github.lobojoel2003.hushcap.settings.AppSettings
import java.io.File
import java.text.SimpleDateFormat
import java.util.Date
import java.util.Locale
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

class MainViewModel(application: Application) : AndroidViewModel(application) {
    private val app = application as HushcapApp
    val settings = app.settings.state
    val models = app.models.states
    val session = CaptionRepository.state
    val levels = CaptionRepository.levels
    private val downloads = mutableMapOf<String, Job>()

    fun updateSettings(transform: (AppSettings) -> AppSettings) = app.settings.update(transform)

    fun hasSpeechModel(): Boolean = app.models.installedSpeechModels().isNotEmpty()

    fun download(info: ModelInfo) {
        if (downloads[info.id]?.isActive == true) return
        downloads[info.id] = viewModelScope.launch {
            app.models.download(info)
            if (info.kind == ModelKind.SPEECH && app.models.isInstalled(info.id, info.kind)) {
                app.settings.update { it.copy(speechModelId = info.id) }
            }
        }
    }

    fun cancelDownload(id: String) {
        downloads.remove(id)?.cancel()
    }

    fun deleteModel(id: String) = app.models.delete(id)

    fun importModel(uri: Uri) {
        viewModelScope.launch {
            try {
                val kind = app.models.importModel(uri)
                if (kind == ModelKind.SPEECH) {
                    app.settings.update { it.copy(speechModelId = io.github.lobojoel2003.hushcap.model.ModelManager.IMPORTED_SPEECH) }
                }
                CaptionRepository.showMessage(
                    if (kind == ModelKind.SPEECH) "Speech model imported." else "Speaker model imported.",
                )
            } catch (e: Exception) {
                CaptionRepository.showMessage(e.message ?: "Import failed")
            }
        }
    }

    fun renameSpeaker(speaker: Int, name: String) = CaptionRepository.renameSpeaker(speaker, name)

    fun clearTranscript() = CaptionRepository.clearTranscript()

    fun dismissMessage() = CaptionRepository.showMessage(null)

    fun transcript(format: ExportFormat): String {
        val s = session.value
        return TranscriptExporter.export(
            s.captions,
            s.speakerNames,
            format,
            includeTimestamps = true,
            title = "Hushcap transcript · ${stamp("yyyy-MM-dd HH:mm")}",
        )
    }

    fun suggestedFileName(format: ExportFormat) = "hushcap-${stamp("yyyy-MM-dd-HHmm")}.${format.extension}"

    /** Write the transcript to a private cache file and return a share intent for it. */
    suspend fun shareIntent(format: ExportFormat): Intent = withContext(Dispatchers.IO) {
        val dir = File(app.cacheDir, "exports").apply { mkdirs() }
        dir.listFiles()?.forEach { it.delete() }
        val file = File(dir, suggestedFileName(format))
        file.writeText(transcript(format))
        val uri = FileProvider.getUriForFile(app, "${app.packageName}.files", file)
        Intent(Intent.ACTION_SEND)
            .setType(format.mimeType)
            .putExtra(Intent.EXTRA_STREAM, uri)
            .addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
    }

    suspend fun saveTo(uri: Uri, format: ExportFormat) = withContext(Dispatchers.IO) {
        app.contentResolver.openOutputStream(uri)?.use { it.write(transcript(format).toByteArray()) }
            ?: throw IllegalStateException("Could not write the file")
    }

    private fun stamp(pattern: String) = SimpleDateFormat(pattern, Locale.getDefault()).format(Date())
}
