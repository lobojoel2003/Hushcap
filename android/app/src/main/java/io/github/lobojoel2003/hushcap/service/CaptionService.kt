package io.github.lobojoel2003.hushcap.service

import android.app.Notification
import android.app.PendingIntent
import android.app.Service
import android.content.Context
import android.content.Intent
import android.content.pm.ServiceInfo
import android.media.projection.MediaProjection
import android.media.projection.MediaProjectionManager
import android.os.Build
import android.os.Handler
import android.os.IBinder
import android.os.Looper
import android.os.SystemClock
import androidx.core.app.NotificationCompat
import androidx.core.app.NotificationManagerCompat
import androidx.core.app.ServiceCompat
import androidx.core.content.ContextCompat
import androidx.core.content.IntentCompat
import io.github.lobojoel2003.hushcap.HushcapApp
import io.github.lobojoel2003.hushcap.R
import io.github.lobojoel2003.hushcap.asr.SourceRecognizer
import io.github.lobojoel2003.hushcap.audio.AudioCapture
import io.github.lobojoel2003.hushcap.core.AudioSource
import io.github.lobojoel2003.hushcap.core.ModelKind
import io.github.lobojoel2003.hushcap.core.SpeakerClusterer
import io.github.lobojoel2003.hushcap.core.TextCleaner
import io.github.lobojoel2003.hushcap.core.VoskResult
import io.github.lobojoel2003.hushcap.ui.MainActivity
import kotlin.concurrent.thread
import org.vosk.LibVosk
import org.vosk.LogLevel
import org.vosk.Model
import org.vosk.SpeakerModel

/**
 * Foreground service that captures audio and produces captions while the
 * app is in the background or the screen is off. All recognition happens
 * here, on the device.
 */
class CaptionService : Service(), SourceRecognizer.Listener {
    private val lock = Any()
    private val captures = mutableListOf<AudioCapture>()
    private val recognizers = mutableListOf<SourceRecognizer>()
    private var model: Model? = null
    private var speakerModel: SpeakerModel? = null
    private var projection: MediaProjection? = null
    private var running = false

    private val clusterer = SpeakerClusterer()
    private val lastSpeaker = mutableMapOf<AudioSource, Pair<Int?, Long>>()
    private var suppressEcho = true
    private var systemOpenSince: Long? = null
    private val systemIntervals = ArrayDeque<LongRange>()
    private var lastNotificationUpdate = 0L

    override fun onBind(intent: Intent?): IBinder? = null

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        when (intent?.action) {
            ACTION_STOP -> stopCaptions()
            ACTION_START -> {
                val useMic = intent.getBooleanExtra(EXTRA_MIC, true)
                val resultCode = intent.getIntExtra(EXTRA_PROJECTION_RESULT, 0)
                val data = IntentCompat.getParcelableExtra(intent, EXTRA_PROJECTION_DATA, Intent::class.java)
                var types = ServiceInfo.FOREGROUND_SERVICE_TYPE_MICROPHONE
                if (data != null && Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) {
                    types = types or ServiceInfo.FOREGROUND_SERVICE_TYPE_MEDIA_PROJECTION
                }
                try {
                    ServiceCompat.startForeground(this, NOTIFICATION_ID, notification(null), types)
                } catch (e: Exception) {
                    CaptionRepository.showMessage("Android did not allow captions to start: ${e.message}")
                    CaptionRepository.setStatus(SessionStatus.IDLE)
                    stopSelf()
                    return START_NOT_STICKY
                }
                startCaptions(useMic, resultCode, data)
            }
        }
        return START_NOT_STICKY
    }

    private fun startCaptions(useMic: Boolean, resultCode: Int, projectionData: Intent?) {
        synchronized(lock) {
            if (running) return
            running = true
        }
        CaptionRepository.setStatus(SessionStatus.STARTING)
        CaptionRepository.showMessage(null)
        thread(name = "hushcap-start") {
            try {
                LibVosk.setLogLevel(LogLevel.WARNINGS)
                val app = application as HushcapApp
                val settings = app.settings.current
                val models = app.models
                val speechId = settings.speechModelId.takeIf { models.isInstalled(it, ModelKind.SPEECH) }
                    ?: models.installedSpeechModels().firstOrNull()
                    ?: throw IllegalStateException("Download a speech model first (Settings → Models).")
                val m = Model(models.dirFor(speechId).absolutePath)
                val spk = if (settings.speakerLabels) {
                    models.speakerModelDir()?.let { SpeakerModel(it.absolutePath) }
                } else {
                    null
                }
                clusterer.reset()
                clusterer.setSeparation(settings.speakerSeparation)
                lastSpeaker.clear()
                systemIntervals.clear()
                systemOpenSince = null
                suppressEcho = settings.suppressEcho

                val start = System.nanoTime()
                val sources = mutableSetOf<AudioSource>()
                synchronized(lock) {
                    model = m
                    speakerModel = spk
                    if (useMic) {
                        val rec = SourceRecognizer(AudioSource.MICROPHONE, m, spk, start, this)
                        recognizers += rec
                        captures += AudioCapture.microphone(this, rec::feed, ::onCaptureError)
                        sources += AudioSource.MICROPHONE
                    }
                    if (projectionData != null && Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) {
                        val manager = getSystemService(MediaProjectionManager::class.java)
                        val p = manager.getMediaProjection(resultCode, projectionData)
                            ?: throw IllegalStateException("Screen-audio permission was not granted")
                        p.registerCallback(object : MediaProjection.Callback() {
                            override fun onStop() {
                                CaptionRepository.showMessage("Device audio capture was stopped by Android.")
                            }
                        }, Handler(Looper.getMainLooper()))
                        projection = p
                        val rec = SourceRecognizer(AudioSource.SYSTEM, m, spk, start, this)
                        recognizers += rec
                        captures += AudioCapture.deviceAudio(this, p, rec::feed, ::onCaptureError)
                        sources += AudioSource.SYSTEM
                    }
                    if (captures.isEmpty()) throw IllegalStateException("Turn on the microphone or device audio.")
                    captures.forEach { it.start() }
                }
                CaptionRepository.setStatus(SessionStatus.RUNNING, sources)
            } catch (e: Throwable) {
                CaptionRepository.showMessage(e.message ?: "Captions could not start")
                stopCaptions()
            }
        }
    }

    private fun onCaptureError(message: String) {
        CaptionRepository.showMessage(message)
    }

    /** Stop everything; safe to call from any thread, more than once. */
    private fun stopCaptions() {
        CaptionRepository.setStatus(SessionStatus.STOPPING)
        thread(name = "hushcap-stop") {
            synchronized(lock) {
                captures.forEach { runCatching { it.stop() } }
                captures.clear()
                recognizers.forEach { runCatching { it.close() } }
                recognizers.clear()
                runCatching { projection?.stop() }
                projection = null
                runCatching { model?.close() }
                runCatching { speakerModel?.close() }
                model = null
                speakerModel = null
                running = false
            }
            CaptionRepository.setStatus(SessionStatus.IDLE)
            Handler(Looper.getMainLooper()).post {
                ServiceCompat.stopForeground(this, ServiceCompat.STOP_FOREGROUND_REMOVE)
                stopSelf()
            }
        }
    }

    override fun onDestroy() {
        if (running) stopCaptions()
        super.onDestroy()
    }

    // --- SourceRecognizer.Listener (called on capture threads) ---

    override fun onLevel(source: AudioSource, db: Float) = CaptionRepository.setLevel(source, db)

    override fun onPartial(source: AudioSource, text: String, startMs: Long) {
        if (source == AudioSource.SYSTEM) {
            synchronized(systemIntervals) { if (systemOpenSince == null) systemOpenSince = startMs }
        } else if (suppressEcho && echoOverlap(startMs, startMs + 1) > 0.5f) {
            return
        }
        CaptionRepository.setPartial(source, text, startMs)
    }

    override fun onFinal(source: AudioSource, result: VoskResult, startMs: Long, endMs: Long) {
        if (source == AudioSource.SYSTEM) {
            synchronized(systemIntervals) {
                systemOpenSince = null
                systemIntervals.addLast(startMs..endMs)
                while (systemIntervals.size > 64) systemIntervals.removeFirst()
            }
        } else if (suppressEcho && echoOverlap(startMs, endMs) > 0.5f) {
            CaptionRepository.clearPartial(source)
            return
        }

        var speaker: Int? = null
        result.speakerVector?.let { vector ->
            val assignment = synchronized(clusterer) {
                clusterer.assign(vector, result.speakerFrames * 10L)
            }
            assignment.merges.forEach { (from, into) -> CaptionRepository.mergeSpeakers(from, into) }
            speaker = assignment.speaker
        }
        if (speaker == null) {
            // Too short to tell: assume the previous speaker kept talking.
            lastSpeaker[source]?.let { (prev, endedAt) -> if (startMs - endedAt < 2_000) speaker = prev }
        }
        lastSpeaker[source] = speaker to endMs

        val text = TextCleaner.asSentence(result.text)
        CaptionRepository.addCaption(source, speaker, text, startMs, endMs)
        updateNotification(text)
    }

    private fun echoOverlap(start: Long, end: Long): Float {
        if (end <= start) return 0f
        var covered = 0L
        synchronized(systemIntervals) {
            val ranges = systemIntervals + listOfNotNull(systemOpenSince?.let { it..Long.MAX_VALUE })
            for (r in ranges) {
                val lo = maxOf(r.first, start)
                val hi = minOf(r.last, end)
                if (hi > lo) covered += hi - lo
            }
        }
        return covered.toFloat() / (end - start)
    }

    // --- Notification ---

    private fun notification(latest: String?): Notification {
        val open = PendingIntent.getActivity(
            this,
            0,
            Intent(this, MainActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_SINGLE_TOP),
            PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT,
        )
        val stop = PendingIntent.getService(
            this,
            1,
            Intent(this, CaptionService::class.java).setAction(ACTION_STOP),
            PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT,
        )
        val base = NotificationCompat.Builder(this, HushcapApp.CHANNEL_CAPTIONS)
            .setSmallIcon(R.drawable.ic_notification)
            .setContentTitle(getString(R.string.notification_title))
            .setContentIntent(open)
            .setOngoing(true)
            .setOnlyAlertOnce(true)
            .setSilent(true)
            .setCategory(NotificationCompat.CATEGORY_SERVICE)
            .setForegroundServiceBehavior(NotificationCompat.FOREGROUND_SERVICE_IMMEDIATE)
            .addAction(R.drawable.ic_stop, getString(R.string.notification_stop), stop)
        // The lock screen never shows what was said.
        val public = NotificationCompat.Builder(this, HushcapApp.CHANNEL_CAPTIONS)
            .setSmallIcon(R.drawable.ic_notification)
            .setContentTitle(getString(R.string.notification_title))
            .build()
        return base
            .setContentText(latest ?: getString(R.string.notification_text))
            .setStyle(latest?.let { NotificationCompat.BigTextStyle().bigText(it) })
            .setVisibility(NotificationCompat.VISIBILITY_PRIVATE)
            .setPublicVersion(public)
            .build()
    }

    private fun updateNotification(latest: String) {
        val now = SystemClock.elapsedRealtime()
        if (now - lastNotificationUpdate < 1_000) return
        lastNotificationUpdate = now
        val manager = NotificationManagerCompat.from(this)
        if (manager.areNotificationsEnabled()) {
            try {
                manager.notify(NOTIFICATION_ID, notification(latest))
            } catch (_: SecurityException) {
                // Notification permission revoked mid-session; captions keep working.
            }
        }
    }

    companion object {
        private const val NOTIFICATION_ID = 1
        private const val ACTION_START = "io.github.lobojoel2003.hushcap.START"
        private const val ACTION_STOP = "io.github.lobojoel2003.hushcap.STOP"
        private const val EXTRA_MIC = "mic"
        private const val EXTRA_PROJECTION_RESULT = "projection_result"
        private const val EXTRA_PROJECTION_DATA = "projection_data"

        fun start(context: Context, useMic: Boolean, projectionResult: Int = 0, projectionData: Intent? = null) {
            val intent = Intent(context, CaptionService::class.java)
                .setAction(ACTION_START)
                .putExtra(EXTRA_MIC, useMic)
                .putExtra(EXTRA_PROJECTION_RESULT, projectionResult)
            if (projectionData != null) intent.putExtra(EXTRA_PROJECTION_DATA, projectionData)
            CaptionRepository.setStatus(SessionStatus.STARTING)
            ContextCompat.startForegroundService(context, intent)
        }

        fun stop(context: Context) {
            context.startService(Intent(context, CaptionService::class.java).setAction(ACTION_STOP))
        }
    }
}
