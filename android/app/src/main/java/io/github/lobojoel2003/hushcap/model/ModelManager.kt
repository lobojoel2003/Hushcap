package io.github.lobojoel2003.hushcap.model

import android.content.Context
import android.net.Uri
import io.github.lobojoel2003.hushcap.core.ModelCatalog
import io.github.lobojoel2003.hushcap.core.ModelInfo
import io.github.lobojoel2003.hushcap.core.ModelKind
import io.github.lobojoel2003.hushcap.core.ZipInstaller
import java.io.File
import java.io.FilterInputStream
import java.io.IOException
import java.io.InputStream
import java.net.HttpURLConnection
import java.net.URL
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.currentCoroutineContext
import kotlinx.coroutines.ensureActive
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.withContext

sealed interface ModelState {
    data object NotInstalled : ModelState
    data class Downloading(val fraction: Float?, val megabytes: Float) : ModelState
    data object Installed : ModelState
    data class Failed(val message: String) : ModelState
}

/**
 * Downloads, imports and deletes models. Models live in the app's private
 * no-backup folder. Downloads go only to [ModelCatalog.allowedHosts] over
 * HTTPS and are the only network traffic Hushcap makes.
 */
class ModelManager(private val context: Context) {
    private val root = File(context.noBackupFilesDir, "models").apply { mkdirs() }
    private val _states = MutableStateFlow(scan())
    val states: StateFlow<Map<String, ModelState>> = _states.asStateFlow()

    fun dirFor(id: String): File = File(root, id)

    private fun markers(kind: ModelKind) =
        if (kind == ModelKind.SPEECH) ZipInstaller.SPEECH_MODEL_MARKERS else ZipInstaller.SPEAKER_MODEL_MARKERS

    fun isInstalled(id: String, kind: ModelKind): Boolean =
        ZipInstaller.looksInstalled(dirFor(id), markers(kind))

    private fun scan(): Map<String, ModelState> {
        val map = ModelCatalog.models.associate { info ->
            info.id to if (isInstalled(info.id, info.kind)) ModelState.Installed else ModelState.NotInstalled
        }.toMutableMap()
        for ((id, kind) in listOf(IMPORTED_SPEECH to ModelKind.SPEECH, IMPORTED_SPEAKER to ModelKind.SPEAKER)) {
            if (isInstalled(id, kind)) map[id] = ModelState.Installed
        }
        return map
    }

    fun refresh() {
        val current = _states.value
        _states.value = scan().mapValues { (id, state) ->
            if (current[id] is ModelState.Downloading) current.getValue(id) else state
        }
    }

    /** Installed speech model ids, catalogue order, imported last. */
    fun installedSpeechModels(): List<String> =
        ModelCatalog.models.filter { it.kind == ModelKind.SPEECH && isInstalled(it.id, it.kind) }.map { it.id } +
            listOfNotNull(IMPORTED_SPEECH.takeIf { isInstalled(it, ModelKind.SPEECH) })

    fun speakerModelDir(): File? {
        val catalog = ModelCatalog.recommended(ModelKind.SPEAKER)
        return when {
            isInstalled(IMPORTED_SPEAKER, ModelKind.SPEAKER) -> dirFor(IMPORTED_SPEAKER)
            isInstalled(catalog.id, ModelKind.SPEAKER) -> dirFor(catalog.id)
            else -> null
        }
    }

    suspend fun download(info: ModelInfo) = withContext(Dispatchers.IO) {
        setState(info.id, ModelState.Downloading(0f, 0f))
        try {
            var url = URL(info.url)
            var connection: HttpURLConnection
            var redirects = 0
            while (true) {
                if (url.protocol != "https" || url.host !in ModelCatalog.allowedHosts) {
                    throw IOException("Refusing to download from ${url.host}")
                }
                connection = (url.openConnection() as HttpURLConnection).apply {
                    instanceFollowRedirects = false
                    connectTimeout = 20_000
                    readTimeout = 60_000
                    setRequestProperty("User-Agent", "Hushcap-Android")
                }
                val code = connection.responseCode
                if (code in 300..399 && redirects < 5) {
                    val location = connection.getHeaderField("Location") ?: throw IOException("Bad redirect")
                    connection.disconnect()
                    url = URL(url, location)
                    redirects++
                    continue
                }
                if (code != HttpURLConnection.HTTP_OK) throw IOException("Server answered $code")
                break
            }
            val total = connection.contentLengthLong.takeIf { it > 0 }
            val scope = currentCoroutineContext()
            connection.inputStream.use { raw ->
                val counting = CountingStream(raw) { bytes ->
                    scope.ensureActive()
                    setState(
                        info.id,
                        ModelState.Downloading(total?.let { bytes.toFloat() / it }, bytes / 1_000_000f),
                    )
                }
                ZipInstaller.install(counting, dirFor(info.id))
            }
            connection.disconnect()
            if (!isInstalled(info.id, info.kind)) {
                dirFor(info.id).deleteRecursively()
                throw IOException("The download did not contain a usable model")
            }
            setState(info.id, ModelState.Installed)
        } catch (e: kotlinx.coroutines.CancellationException) {
            setState(info.id, ModelState.NotInstalled)
            throw e
        } catch (e: Exception) {
            setState(info.id, ModelState.Failed(e.message ?: "Download failed"))
        }
    }

    /** Import a model zip picked by the user. Detects whether it is a speech or speaker model. */
    suspend fun importModel(uri: Uri): ModelKind = withContext(Dispatchers.IO) {
        val staging = dirFor("import-staging")
        try {
            val input = context.contentResolver.openInputStream(uri) ?: throw IOException("Could not open the file")
            input.use { ZipInstaller.install(it, staging) }
            val kind = when {
                ZipInstaller.looksInstalled(staging, ZipInstaller.SPEECH_MODEL_MARKERS) -> ModelKind.SPEECH
                ZipInstaller.looksInstalled(staging, ZipInstaller.SPEAKER_MODEL_MARKERS) -> ModelKind.SPEAKER
                else -> throw IOException("This file is not a Vosk speech or speaker model")
            }
            val target = dirFor(if (kind == ModelKind.SPEECH) IMPORTED_SPEECH else IMPORTED_SPEAKER)
            target.deleteRecursively()
            if (!staging.renameTo(target)) {
                staging.copyRecursively(target, overwrite = true)
            }
            refresh()
            kind
        } finally {
            staging.deleteRecursively()
        }
    }

    fun delete(id: String) {
        dirFor(id).deleteRecursively()
        refresh()
    }

    private fun setState(id: String, state: ModelState) {
        _states.update { it + (id to state) }
    }

    private class CountingStream(input: InputStream, val onBytes: (Long) -> Unit) : FilterInputStream(input) {
        private var count = 0L
        private var lastReport = 0L

        override fun read(): Int = super.read().also { if (it >= 0) advance(1) }

        override fun read(b: ByteArray, off: Int, len: Int): Int =
            super.read(b, off, len).also { if (it > 0) advance(it.toLong()) }

        private fun advance(n: Long) {
            count += n
            if (count - lastReport >= 256 * 1024) {
                lastReport = count
                onBytes(count)
            }
        }
    }

    companion object {
        const val IMPORTED_SPEECH = "imported-speech"
        const val IMPORTED_SPEAKER = "imported-speaker"
    }
}
