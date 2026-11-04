package io.github.lobojoel2003.hushcap.audio

import android.Manifest
import android.annotation.SuppressLint
import android.content.Context
import android.content.pm.PackageManager
import android.media.AudioAttributes
import android.media.AudioFormat
import android.media.AudioPlaybackCaptureConfiguration
import android.media.AudioRecord
import android.media.MediaRecorder
import android.media.projection.MediaProjection
import android.os.Build
import android.os.Process
import androidx.annotation.RequiresApi
import androidx.core.content.ContextCompat
import io.github.lobojoel2003.hushcap.core.AudioSource

/** Sample rate every recogniser uses. */
const val SAMPLE_RATE = 16_000

/**
 * Reads 16 kHz mono 16-bit audio from an [AudioRecord] on a dedicated
 * thread and hands it to [onAudio] in 100 ms chunks.
 */
class AudioCapture private constructor(
    val source: AudioSource,
    private val record: AudioRecord,
    private val onAudio: (ShortArray, Int) -> Unit,
    private val onError: (String) -> Unit,
) {
    @Volatile
    private var running = false
    private var thread: Thread? = null

    fun start() {
        running = true
        record.startRecording()
        if (record.recordingState != AudioRecord.RECORDSTATE_RECORDING) {
            running = false
            record.release()
            throw IllegalStateException("${source.label} is in use by another app")
        }
        thread = Thread({
            Process.setThreadPriority(Process.THREAD_PRIORITY_URGENT_AUDIO)
            val buffer = ShortArray(SAMPLE_RATE / 10)
            while (running) {
                val n = record.read(buffer, 0, buffer.size)
                when {
                    n > 0 -> onAudio(buffer, n)
                    n < 0 -> {
                        if (running) onError("${source.label} stopped working (error $n)")
                        break
                    }
                }
            }
        }, "hushcap-capture-${source.name.lowercase()}").apply { start() }
    }

    /** Stop reading and release the device. Blocks until the reader thread ends. */
    fun stop() {
        running = false
        runCatching { record.stop() }
        thread?.join(2_000)
        record.release()
    }

    companion object {
        private val format: AudioFormat = AudioFormat.Builder()
            .setEncoding(AudioFormat.ENCODING_PCM_16BIT)
            .setSampleRate(SAMPLE_RATE)
            .setChannelMask(AudioFormat.CHANNEL_IN_MONO)
            .build()

        private fun bufferBytes(): Int {
            val min = AudioRecord.getMinBufferSize(
                SAMPLE_RATE,
                AudioFormat.CHANNEL_IN_MONO,
                AudioFormat.ENCODING_PCM_16BIT,
            )
            return maxOf(min, SAMPLE_RATE * 2) // at least one second
        }

        private fun requirePermission(context: Context) {
            if (ContextCompat.checkSelfPermission(context, Manifest.permission.RECORD_AUDIO) !=
                PackageManager.PERMISSION_GRANTED
            ) {
                throw SecurityException("Microphone permission is needed for captions")
            }
        }

        /** The microphone, tuned for speech recognition. */
        @SuppressLint("MissingPermission") // checked by requirePermission
        fun microphone(
            context: Context,
            onAudio: (ShortArray, Int) -> Unit,
            onError: (String) -> Unit,
        ): AudioCapture {
            requirePermission(context)
            val record = AudioRecord.Builder()
                .setAudioSource(MediaRecorder.AudioSource.VOICE_RECOGNITION)
                .setAudioFormat(format)
                .setBufferSizeInBytes(bufferBytes())
                .build()
            if (record.state != AudioRecord.STATE_INITIALIZED) {
                record.release()
                throw IllegalStateException("Could not open the microphone")
            }
            return AudioCapture(AudioSource.MICROPHONE, record, onAudio, onError)
        }

        /**
         * Audio other apps are playing (videos, podcasts, many call apps).
         * Requires Android 10 and screen-capture consent. Apps can opt out of
         * capture, and Android never allows capturing phone-call audio.
         */
        @RequiresApi(Build.VERSION_CODES.Q)
        @SuppressLint("MissingPermission") // checked by requirePermission
        fun deviceAudio(
            context: Context,
            projection: MediaProjection,
            onAudio: (ShortArray, Int) -> Unit,
            onError: (String) -> Unit,
        ): AudioCapture {
            requirePermission(context)
            val config = AudioPlaybackCaptureConfiguration.Builder(projection)
                .addMatchingUsage(AudioAttributes.USAGE_MEDIA)
                .addMatchingUsage(AudioAttributes.USAGE_GAME)
                .addMatchingUsage(AudioAttributes.USAGE_UNKNOWN)
                .build()
            val record = AudioRecord.Builder()
                .setAudioFormat(format)
                .setBufferSizeInBytes(bufferBytes())
                .setAudioPlaybackCaptureConfig(config)
                .build()
            if (record.state != AudioRecord.STATE_INITIALIZED) {
                record.release()
                throw IllegalStateException("Could not capture device audio")
            }
            return AudioCapture(AudioSource.SYSTEM, record, onAudio, onError)
        }
    }
}
