package io.github.lobojoel2003.hushcap.ui

import android.Manifest
import android.content.pm.PackageManager
import android.media.projection.MediaProjectionManager
import android.os.Build
import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.activity.result.contract.ActivityResultContracts
import androidx.activity.viewModels
import androidx.compose.runtime.getValue
import androidx.core.content.ContextCompat
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import io.github.lobojoel2003.hushcap.service.CaptionRepository
import io.github.lobojoel2003.hushcap.service.CaptionService

class MainActivity : ComponentActivity() {
    private val vm: MainViewModel by viewModels()

    private val permissions = registerForActivityResult(
        ActivityResultContracts.RequestMultiplePermissions(),
    ) { granted ->
        if (granted[Manifest.permission.RECORD_AUDIO] == true || hasMicPermission()) {
            continueStart()
        } else {
            CaptionRepository.showMessage(
                "Hushcap needs microphone permission to hear audio. It never leaves your phone.",
            )
        }
    }

    private val projectionConsent = registerForActivityResult(
        ActivityResultContracts.StartActivityForResult(),
    ) { result ->
        val data = result.data
        if (result.resultCode == RESULT_OK && data != null) {
            CaptionService.start(this, vm.settings.value.useMicrophone, result.resultCode, data)
        } else if (vm.settings.value.useMicrophone) {
            CaptionRepository.showMessage("Device audio was not allowed; captioning the microphone only.")
            CaptionService.start(this, useMic = true)
        } else {
            CaptionRepository.showMessage("Device audio needs your permission to be captioned.")
        }
    }

    override fun onCreate(savedInstanceState: Bundle?) {
        enableEdgeToEdge()
        super.onCreate(savedInstanceState)
        setContent {
            val settings by vm.settings.collectAsStateWithLifecycle()
            HushcapTheme(settings.theme) {
                HushcapRoot(
                    vm = vm,
                    deviceAudioSupported = Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q,
                    onStart = ::requestStart,
                    onStop = { CaptionService.stop(this) },
                )
            }
        }
    }

    private fun hasMicPermission() =
        ContextCompat.checkSelfPermission(this, Manifest.permission.RECORD_AUDIO) ==
            PackageManager.PERMISSION_GRANTED

    private fun requestStart() {
        val needed = buildList {
            if (!hasMicPermission()) add(Manifest.permission.RECORD_AUDIO)
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU &&
                ContextCompat.checkSelfPermission(this@MainActivity, Manifest.permission.POST_NOTIFICATIONS) !=
                PackageManager.PERMISSION_GRANTED
            ) {
                add(Manifest.permission.POST_NOTIFICATIONS)
            }
        }
        if (needed.isEmpty()) continueStart() else permissions.launch(needed.toTypedArray())
    }

    private fun continueStart() {
        val s = vm.settings.value
        if (s.useDeviceAudio && Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) {
            val manager = getSystemService(MediaProjectionManager::class.java)
            projectionConsent.launch(manager.createScreenCaptureIntent())
        } else {
            CaptionService.start(this, useMic = true)
        }
    }
}
