package io.github.lobojoel2003.hushcap.ui

import android.content.Intent
import android.net.Uri
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.selection.selectable
import androidx.compose.foundation.selection.toggleable
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material3.Button
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.LinearProgressIndicator
import androidx.compose.material3.ListItem
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.RadioButton
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Slider
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.TopAppBar
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import io.github.lobojoel2003.hushcap.BuildConfig
import io.github.lobojoel2003.hushcap.core.ModelCatalog
import io.github.lobojoel2003.hushcap.core.ModelInfo
import io.github.lobojoel2003.hushcap.core.ModelKind
import io.github.lobojoel2003.hushcap.model.ModelManager
import io.github.lobojoel2003.hushcap.model.ModelState
import io.github.lobojoel2003.hushcap.settings.ThemeMode

@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun Page(title: String, onBack: () -> Unit, content: @Composable () -> Unit) {
    Scaffold(
        topBar = {
            TopAppBar(
                title = { Text(title) },
                navigationIcon = {
                    IconButton(onClick = onBack) {
                        Icon(Icons.AutoMirrored.Filled.ArrowBack, contentDescription = "Back")
                    }
                },
            )
        },
    ) { padding ->
        Column(
            Modifier
                .padding(padding)
                .fillMaxSize()
                .verticalScroll(rememberScrollState())
                .padding(bottom = 24.dp),
        ) { content() }
    }
}

@Composable
private fun SectionTitle(text: String) {
    Text(
        text,
        style = MaterialTheme.typography.titleSmall,
        color = MaterialTheme.colorScheme.primary,
        modifier = Modifier
            .padding(start = 16.dp, end = 16.dp, top = 20.dp, bottom = 4.dp)
            .semantics { heading() },
    )
}

@Composable
private fun SwitchRow(title: String, subtitle: String?, checked: Boolean, onChange: (Boolean) -> Unit) {
    ListItem(
        headlineContent = { Text(title) },
        supportingContent = subtitle?.let { { Text(it) } },
        trailingContent = { Switch(checked = checked, onCheckedChange = null) },
        modifier = Modifier.toggleable(value = checked, role = Role.Switch, onValueChange = onChange),
    )
}

@Composable
fun SettingsScreen(vm: MainViewModel, onBack: () -> Unit, onOpenModels: () -> Unit, onOpenAbout: () -> Unit) {
    val s by vm.settings.collectAsStateWithLifecycle()
    Page("Settings", onBack) {
        SectionTitle("Captions")
        Column(Modifier.padding(horizontal = 16.dp)) {
            Text("Text size", style = MaterialTheme.typography.bodyLarge)
            Slider(
                value = s.textScale,
                onValueChange = { v -> vm.updateSettings { it.copy(textScale = v) } },
                valueRange = 0.8f..3f,
            )
            Text(
                "The quick brown fox",
                fontSize = (22 * s.textScale).sp,
                lineHeight = (22 * s.textScale * 1.3f).sp,
            )
        }
        SwitchRow("Show timestamps", null, s.showTimestamps) { v -> vm.updateSettings { it.copy(showTimestamps = v) } }
        SwitchRow("Keep screen on while captioning", null, s.keepScreenOn) { v ->
            vm.updateSettings { it.copy(keepScreenOn = v) }
        }

        SectionTitle("Theme")
        for (mode in ThemeMode.entries) {
            ListItem(
                headlineContent = { Text(mode.label) },
                leadingContent = { RadioButton(selected = s.theme == mode, onClick = null) },
                modifier = Modifier.selectable(
                    selected = s.theme == mode,
                    role = Role.RadioButton,
                    onClick = { vm.updateSettings { it.copy(theme = mode) } },
                ),
            )
        }

        SectionTitle("Speakers")
        SwitchRow(
            "Label speakers",
            "Tell voices apart (needs the speaker model)",
            s.speakerLabels,
        ) { v -> vm.updateSettings { it.copy(speakerLabels = v) } }
        Column(Modifier.padding(horizontal = 16.dp)) {
            Text("Speaker separation", style = MaterialTheme.typography.bodyLarge)
            Slider(
                value = s.speakerSeparation,
                onValueChange = { v -> vm.updateSettings { it.copy(speakerSeparation = v) } },
                enabled = s.speakerLabels,
            )
            Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.SpaceBetween) {
                Text("Merge similar voices", style = MaterialTheme.typography.labelSmall)
                Text("Split eagerly", style = MaterialTheme.typography.labelSmall)
            }
        }

        SectionTitle("Audio")
        SwitchRow(
            "Ignore echo of device audio",
            "When the microphone hears the phone's own speaker, show it once",
            s.suppressEcho,
        ) { v -> vm.updateSettings { it.copy(suppressEcho = v) } }

        HorizontalDivider(Modifier.padding(top = 12.dp))
        ListItem(
            headlineContent = { Text("Speech models") },
            supportingContent = { Text("Languages, downloads and imports") },
            modifier = Modifier.selectable(selected = false, onClick = onOpenModels),
        )
        ListItem(
            headlineContent = { Text("About Hushcap") },
            supportingContent = { Text("Version ${BuildConfig.VERSION_NAME} · privacy · licences") },
            modifier = Modifier.selectable(selected = false, onClick = onOpenAbout),
        )
    }
}

@Composable
fun ModelsScreen(vm: MainViewModel, onBack: () -> Unit) {
    val settings by vm.settings.collectAsStateWithLifecycle()
    val states by vm.models.collectAsStateWithLifecycle()
    val importLauncher = rememberLauncherForActivityResult(ActivityResultContracts.OpenDocument()) { uri: Uri? ->
        if (uri != null) vm.importModel(uri)
    }
    Page("Speech models", onBack) {
        Text(
            "Downloading a model is the only time Hushcap uses the internet. Afterwards captions work " +
                "with no connection at all. Models come from alphacephei.com (Vosk).",
            style = MaterialTheme.typography.bodyMedium,
            modifier = Modifier.padding(16.dp),
        )
        SectionTitle("Speech")
        for (info in ModelCatalog.models.filter { it.kind == ModelKind.SPEECH }) {
            ModelRow(
                info = info,
                state = states[info.id] ?: ModelState.NotInstalled,
                inUse = settings.speechModelId == info.id,
                onUse = { vm.updateSettings { it.copy(speechModelId = info.id) } },
                vm = vm,
            )
        }
        if (states[ModelManager.IMPORTED_SPEECH] == ModelState.Installed) {
            ListItem(
                headlineContent = { Text("Imported speech model") },
                supportingContent = { Text(if (settings.speechModelId == ModelManager.IMPORTED_SPEECH) "In use" else "Installed") },
                trailingContent = {
                    Row {
                        if (settings.speechModelId != ModelManager.IMPORTED_SPEECH) {
                            TextButton(onClick = {
                                vm.updateSettings { it.copy(speechModelId = ModelManager.IMPORTED_SPEECH) }
                            }) { Text("Use") }
                        }
                        TextButton(onClick = { vm.deleteModel(ModelManager.IMPORTED_SPEECH) }) { Text("Delete") }
                    }
                },
            )
        }
        SectionTitle("Speaker labels")
        for (info in ModelCatalog.models.filter { it.kind == ModelKind.SPEAKER }) {
            ModelRow(info, states[info.id] ?: ModelState.NotInstalled, inUse = false, onUse = null, vm = vm)
        }
        SectionTitle("Offline install")
        Text(
            "No internet on this phone? Download a Vosk model .zip on another device, copy it over, " +
                "and import it here.",
            style = MaterialTheme.typography.bodyMedium,
            modifier = Modifier.padding(horizontal = 16.dp),
        )
        OutlinedButton(
            onClick = { importLauncher.launch(arrayOf("application/zip", "application/x-zip-compressed", "application/octet-stream")) },
            modifier = Modifier
                .padding(16.dp)
                .fillMaxWidth(),
        ) { Text("Import model file (.zip)") }
    }
}

@Composable
private fun ModelRow(info: ModelInfo, state: ModelState, inUse: Boolean, onUse: (() -> Unit)?, vm: MainViewModel) {
    ListItem(
        headlineContent = { Text(info.name + if (info.recommended && info.kind == ModelKind.SPEECH) " (recommended)" else "") },
        supportingContent = {
            Column {
                when (state) {
                    is ModelState.Downloading -> {
                        Text("Downloading… %.0f / %d MB".format(state.megabytes, info.sizeMb))
                        val f = state.fraction
                        if (f != null) {
                            LinearProgressIndicator(progress = { f }, modifier = Modifier.fillMaxWidth())
                        } else {
                            LinearProgressIndicator(Modifier.fillMaxWidth())
                        }
                    }
                    is ModelState.Failed -> Text("Failed: ${state.message}", color = MaterialTheme.colorScheme.error)
                    ModelState.Installed -> Text(if (inUse) "In use" else "Installed · ${info.sizeMb} MB")
                    ModelState.NotInstalled -> Text("${info.sizeMb} MB")
                }
            }
        },
        trailingContent = {
            Row(verticalAlignment = Alignment.CenterVertically) {
                when (state) {
                    is ModelState.Downloading -> TextButton(onClick = { vm.cancelDownload(info.id) }) { Text("Cancel") }
                    ModelState.Installed -> {
                        if (onUse != null && !inUse) TextButton(onClick = onUse) { Text("Use") }
                        TextButton(onClick = { vm.deleteModel(info.id) }) { Text("Delete") }
                    }
                    else -> Button(onClick = { vm.download(info) }) {
                        Text(if (state is ModelState.Failed) "Retry" else "Download")
                    }
                }
            }
        },
    )
}

@Composable
fun AboutScreen(onBack: () -> Unit) {
    val context = LocalContext.current
    Page("About", onBack) {
        Column(Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
            Text("Hushcap", style = MaterialTheme.typography.headlineMedium)
            Text(
                "Version ${BuildConfig.VERSION_NAME} (build ${BuildConfig.BUILD_NUMBER}, ${BuildConfig.GIT_SHA})",
                style = MaterialTheme.typography.bodyMedium,
            )
            Text("Private, offline live captions.", style = MaterialTheme.typography.titleMedium)
            Text(
                "Speech is turned into text on this phone. Audio and transcripts are never uploaded, " +
                    "there are no accounts, ads, analytics or trackers, and nothing is backed up to the " +
                    "cloud. Transcripts stay in memory until you clear them or close the app, unless you " +
                    "choose to save or share them.",
                style = MaterialTheme.typography.bodyMedium,
            )
            Text(
                "Hushcap is free software under the GNU GPL v3 or later. Speech recognition by Vosk " +
                    "(Apache 2.0), which uses Kaldi (Apache 2.0) and JNA (Apache 2.0 / LGPL 2.1).",
                style = MaterialTheme.typography.bodyMedium,
            )
            OutlinedButton(onClick = {
                context.startActivity(Intent(Intent.ACTION_VIEW, Uri.parse("https://github.com/lobojoel2003/Hushcap")))
            }) { Text("Source code and help") }
        }
    }
}
