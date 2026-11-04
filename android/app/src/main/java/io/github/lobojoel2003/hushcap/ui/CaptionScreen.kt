package io.github.lobojoel2003.hushcap.ui

import androidx.activity.compose.BackHandler
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Delete
import androidx.compose.material.icons.filled.Lock
import androidx.compose.material.icons.filled.MoreVert
import androidx.compose.material.icons.filled.Settings
import androidx.compose.material.icons.filled.Share
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Button
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.Card
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.FilterChip
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Scaffold
import androidx.compose.material3.SnackbarHost
import androidx.compose.material3.SnackbarHostState
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.TopAppBar
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.derivedStateOf
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.luminance
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalView
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import io.github.lobojoel2003.hushcap.R
import io.github.lobojoel2003.hushcap.core.AudioSource
import io.github.lobojoel2003.hushcap.core.Caption
import io.github.lobojoel2003.hushcap.core.ExportFormat
import io.github.lobojoel2003.hushcap.core.ModelCatalog
import io.github.lobojoel2003.hushcap.core.ModelKind
import io.github.lobojoel2003.hushcap.core.clock
import io.github.lobojoel2003.hushcap.core.speakerName
import io.github.lobojoel2003.hushcap.model.ModelState
import io.github.lobojoel2003.hushcap.service.SessionState
import io.github.lobojoel2003.hushcap.service.SessionStatus
import io.github.lobojoel2003.hushcap.settings.AppSettings
import kotlinx.coroutines.launch

private enum class Screen { CAPTIONS, SETTINGS, MODELS, ABOUT }

@Composable
fun HushcapRoot(
    vm: MainViewModel,
    deviceAudioSupported: Boolean,
    onStart: () -> Unit,
    onStop: () -> Unit,
) {
    var screen by rememberSaveable { mutableStateOf(Screen.CAPTIONS) }
    BackHandler(enabled = screen != Screen.CAPTIONS) {
        screen = if (screen == Screen.ABOUT || screen == Screen.MODELS) Screen.SETTINGS else Screen.CAPTIONS
    }
    when (screen) {
        Screen.CAPTIONS -> CaptionScreen(
            vm = vm,
            deviceAudioSupported = deviceAudioSupported,
            onStart = onStart,
            onStop = onStop,
            onOpenSettings = { screen = Screen.SETTINGS },
            onOpenModels = { screen = Screen.MODELS },
        )
        Screen.SETTINGS -> SettingsScreen(
            vm = vm,
            onBack = { screen = Screen.CAPTIONS },
            onOpenModels = { screen = Screen.MODELS },
            onOpenAbout = { screen = Screen.ABOUT },
        )
        Screen.MODELS -> ModelsScreen(vm = vm, onBack = { screen = Screen.SETTINGS })
        Screen.ABOUT -> AboutScreen(onBack = { screen = Screen.SETTINGS })
    }
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun CaptionScreen(
    vm: MainViewModel,
    deviceAudioSupported: Boolean,
    onStart: () -> Unit,
    onStop: () -> Unit,
    onOpenSettings: () -> Unit,
    onOpenModels: () -> Unit,
) {
    val settings by vm.settings.collectAsStateWithLifecycle()
    val session by vm.session.collectAsStateWithLifecycle()
    val models by vm.models.collectAsStateWithLifecycle()
    val context = LocalContext.current
    val scope = rememberCoroutineScope()
    val snackbar = remember { SnackbarHostState() }
    var menuOpen by remember { mutableStateOf(false) }
    var saveFormat by remember { mutableStateOf(ExportFormat.TXT) }
    var renaming by remember { mutableStateOf<Int?>(null) }
    val hasModel = models.any { (id, state) ->
        state is ModelState.Installed &&
            (ModelCatalog.find(id)?.kind == ModelKind.SPEECH || id == io.github.lobojoel2003.hushcap.model.ModelManager.IMPORTED_SPEECH)
    }

    // Keep the screen awake while captioning, if asked.
    val view = LocalView.current
    val keepOn = settings.keepScreenOn && session.status == SessionStatus.RUNNING
    DisposableEffect(keepOn) {
        view.keepScreenOn = keepOn
        onDispose { view.keepScreenOn = false }
    }

    LaunchedEffect(session.message) {
        session.message?.let {
            snackbar.showSnackbar(it)
            vm.dismissMessage()
        }
    }

    val saveLauncher = rememberLauncherForActivityResult(
        ActivityResultContracts.CreateDocument("text/plain"),
    ) { uri ->
        if (uri != null) {
            scope.launch {
                runCatching { vm.saveTo(uri, saveFormat) }
                    .onSuccess { snackbar.showSnackbar("Transcript saved.") }
                    .onFailure { snackbar.showSnackbar(it.message ?: "Could not save") }
            }
        }
    }

    Scaffold(
        topBar = {
            TopAppBar(
                title = { Text("Hushcap", fontWeight = FontWeight.Bold) },
                actions = {
                    IconButton(
                        enabled = session.captions.isNotEmpty(),
                        onClick = {
                            scope.launch {
                                val intent = vm.shareIntent(ExportFormat.TXT)
                                context.startActivity(android.content.Intent.createChooser(intent, "Share transcript"))
                            }
                        },
                    ) { Icon(Icons.Filled.Share, contentDescription = "Share transcript") }
                    IconButton(onClick = onOpenSettings) {
                        Icon(Icons.Filled.Settings, contentDescription = "Settings")
                    }
                    Box {
                        IconButton(onClick = { menuOpen = true }) {
                            Icon(Icons.Filled.MoreVert, contentDescription = "More options")
                        }
                        DropdownMenu(expanded = menuOpen, onDismissRequest = { menuOpen = false }) {
                            for (format in ExportFormat.entries) {
                                DropdownMenuItem(
                                    enabled = session.captions.isNotEmpty(),
                                    text = { Text("Save as .${format.extension}") },
                                    onClick = {
                                        menuOpen = false
                                        saveFormat = format
                                        saveLauncher.launch(vm.suggestedFileName(format))
                                    },
                                )
                            }
                            DropdownMenuItem(
                                enabled = session.captions.isNotEmpty(),
                                text = { Text("Clear transcript") },
                                leadingIcon = { Icon(Icons.Filled.Delete, contentDescription = null) },
                                onClick = {
                                    menuOpen = false
                                    vm.clearTranscript()
                                },
                            )
                        }
                    }
                },
            )
        },
        snackbarHost = { SnackbarHost(snackbar) },
        bottomBar = {
            ControlBar(
                vm = vm,
                settings = settings,
                status = session.status,
                deviceAudioSupported = deviceAudioSupported,
                hasModel = hasModel,
                onStart = onStart,
                onStop = onStop,
            )
        },
    ) { padding ->
        Column(
            Modifier
                .padding(padding)
                .fillMaxSize(),
        ) {
            PrivacyBanner()
            if (!hasModel) {
                Onboarding(vm = vm, models = models, onOpenModels = onOpenModels)
            } else {
                CaptionList(
                    session = session,
                    settings = settings,
                    onSpeakerClick = { renaming = it },
                    modifier = Modifier.weight(1f),
                )
            }
        }
    }

    renaming?.let { speaker ->
        RenameDialog(
            current = session.speakerNames[speaker] ?: "",
            speaker = speaker,
            onDismiss = { renaming = null },
            onSave = {
                vm.renameSpeaker(speaker, it)
                renaming = null
            },
        )
    }
}

@Composable
private fun PrivacyBanner() {
    Row(
        Modifier
            .fillMaxWidth()
            .padding(horizontal = 16.dp, vertical = 4.dp)
            .semantics(mergeDescendants = true) {},
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Icon(
            Icons.Filled.Lock,
            contentDescription = null,
            tint = MaterialTheme.colorScheme.secondary,
            modifier = Modifier.size(16.dp),
        )
        Spacer(Modifier.width(6.dp))
        Text(
            "On-device · works offline · nothing is uploaded",
            style = MaterialTheme.typography.labelMedium,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
    }
}

@Composable
private fun Onboarding(vm: MainViewModel, models: Map<String, ModelState>, onOpenModels: () -> Unit) {
    val speech = ModelCatalog.recommended(ModelKind.SPEECH)
    val speaker = ModelCatalog.recommended(ModelKind.SPEAKER)
    Card(Modifier.padding(16.dp)) {
        Column(Modifier.padding(20.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
            Text(
                "Welcome to Hushcap",
                style = MaterialTheme.typography.headlineSmall,
                modifier = Modifier.semantics { heading() },
            )
            Text(
                "Live captions that never leave your phone. Hushcap needs a speech model " +
                    "(about ${speech.sizeMb + speaker.sizeMb} MB) once. After that it works in airplane mode, " +
                    "on the bus, anywhere.",
                style = MaterialTheme.typography.bodyLarge,
            )
            when (val state = models[speech.id]) {
                is ModelState.Downloading -> {
                    Text("Downloading… %.0f MB".format(state.megabytes))
                    val f = state.fraction
                    if (f != null) {
                        androidx.compose.material3.LinearProgressIndicator(
                            progress = { f },
                            modifier = Modifier.fillMaxWidth(),
                        )
                    } else {
                        androidx.compose.material3.LinearProgressIndicator(Modifier.fillMaxWidth())
                    }
                }
                is ModelState.Failed -> Text(
                    "Download failed: ${state.message}",
                    color = MaterialTheme.colorScheme.error,
                )
                else -> {}
            }
            Button(
                onClick = {
                    vm.download(speech)
                    vm.download(speaker)
                },
                enabled = models[speech.id] !is ModelState.Downloading,
                modifier = Modifier.fillMaxWidth(),
            ) { Text("Download English model and speaker labels") }
            OutlinedButton(onClick = onOpenModels, modifier = Modifier.fillMaxWidth()) {
                Text("Other languages or import a file")
            }
        }
    }
}

@Composable
private fun CaptionList(
    session: SessionState,
    settings: AppSettings,
    onSpeakerClick: (Int) -> Unit,
    modifier: Modifier = Modifier,
) {
    val listState = rememberLazyListState()
    val partials = session.partials.values.sortedBy { it.startMs }
    val total = session.captions.size + partials.size
    val atBottom by remember {
        derivedStateOf {
            val info = listState.layoutInfo
            val last = info.visibleItemsInfo.lastOrNull()?.index ?: 0
            info.totalItemsCount == 0 || last >= info.totalItemsCount - 2
        }
    }
    LaunchedEffect(total, partials.lastOrNull()?.text) {
        if (total > 0 && atBottom) listState.animateScrollToItem(total - 1)
    }
    val dark = MaterialTheme.colorScheme.background.luminance() < 0.5f
    val textSize = (22 * settings.textScale).sp

    if (total == 0) {
        Box(modifier.fillMaxSize().padding(24.dp), contentAlignment = Alignment.Center) {
            Text(
                when (session.status) {
                    SessionStatus.RUNNING -> "Listening… captions appear here."
                    SessionStatus.STARTING -> "Loading the speech model…"
                    else -> "Press Start to caption what's said around you."
                },
                style = MaterialTheme.typography.titleMedium,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }
        return
    }

    LazyColumn(
        state = listState,
        modifier = modifier.fillMaxWidth(),
        verticalArrangement = Arrangement.spacedBy(14.dp),
        contentPadding = androidx.compose.foundation.layout.PaddingValues(16.dp),
    ) {
        items(session.captions, key = { it.id }) { caption ->
            CaptionRow(
                caption = caption,
                names = session.speakerNames,
                settings = settings,
                dark = dark,
                textSize = textSize,
                isLatest = caption.id == session.captions.last().id,
                onSpeakerClick = onSpeakerClick,
            )
        }
        items(partials, key = { "partial-${it.source}" }) { partial ->
            Column {
                if (partial.source == AudioSource.SYSTEM) SourceTag()
                Text(
                    partial.text,
                    fontSize = textSize,
                    lineHeight = textSize * 1.3f,
                    fontStyle = FontStyle.Italic,
                    color = MaterialTheme.colorScheme.onSurface.copy(alpha = 0.65f),
                )
            }
        }
    }
}

@Composable
private fun SourceTag() {
    Row(verticalAlignment = Alignment.CenterVertically) {
        Icon(
            painterResource(R.drawable.ic_device_audio),
            contentDescription = null,
            modifier = Modifier.size(14.dp),
            tint = MaterialTheme.colorScheme.onSurfaceVariant,
        )
        Spacer(Modifier.width(4.dp))
        Text("Device audio", style = MaterialTheme.typography.labelSmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
    }
}

@Composable
private fun CaptionRow(
    caption: Caption,
    names: Map<Int, String>,
    settings: AppSettings,
    dark: Boolean,
    textSize: androidx.compose.ui.unit.TextUnit,
    isLatest: Boolean,
    onSpeakerClick: (Int) -> Unit,
) {
    Column(
        Modifier.semantics {
            if (isLatest) liveRegion = LiveRegionMode.Polite
        },
    ) {
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            caption.speaker?.let { s ->
                val name = speakerName(s, names)
                Text(
                    name,
                    style = MaterialTheme.typography.labelLarge,
                    fontWeight = FontWeight.Bold,
                    color = speakerColor(s, dark),
                    modifier = Modifier
                        .clip(RoundedCornerShape(6.dp))
                        .clickable { onSpeakerClick(s) }
                        .padding(vertical = 2.dp)
                        .semantics { contentDescription = "$name. Tap to rename." },
                )
            }
            if (caption.source == AudioSource.SYSTEM) SourceTag()
            if (settings.showTimestamps) {
                Text(
                    clock(caption.startMs),
                    style = MaterialTheme.typography.labelSmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
            }
        }
        Text(caption.text, fontSize = textSize, lineHeight = textSize * 1.3f)
    }
}

@Composable
private fun ControlBar(
    vm: MainViewModel,
    settings: AppSettings,
    status: SessionStatus,
    deviceAudioSupported: Boolean,
    hasModel: Boolean,
    onStart: () -> Unit,
    onStop: () -> Unit,
) {
    val levels by vm.levels.collectAsStateWithLifecycle()
    val running = status == SessionStatus.RUNNING
    val busy = status == SessionStatus.STARTING || status == SessionStatus.STOPPING
    Surface(tonalElevation = 3.dp) {
        Row(
            Modifier
                .fillMaxWidth()
                .padding(horizontal = 12.dp, vertical = 10.dp)
                .padding(bottom = 8.dp),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(4.dp)) {
                SourceChip(
                    label = "Microphone",
                    icon = R.drawable.ic_mic,
                    selected = settings.useMicrophone,
                    enabled = status == SessionStatus.IDLE,
                    level = levels[AudioSource.MICROPHONE].takeIf { running },
                    onClick = { vm.updateSettings { it.copy(useMicrophone = !it.useMicrophone) } },
                )
                if (deviceAudioSupported) {
                    SourceChip(
                        label = "Device audio",
                        icon = R.drawable.ic_device_audio,
                        selected = settings.useDeviceAudio,
                        enabled = status == SessionStatus.IDLE,
                        level = levels[AudioSource.SYSTEM].takeIf { running },
                        onClick = { vm.updateSettings { it.copy(useDeviceAudio = !it.useDeviceAudio) } },
                    )
                }
            }
            Button(
                onClick = { if (running) onStop() else onStart() },
                enabled = hasModel && !busy && (running || settings.useMicrophone || settings.useDeviceAudio),
                modifier = Modifier.height(64.dp),
                colors = if (running) {
                    ButtonDefaults.buttonColors(containerColor = MaterialTheme.colorScheme.error)
                } else {
                    ButtonDefaults.buttonColors()
                },
            ) {
                if (busy) {
                    CircularProgressIndicator(Modifier.size(22.dp), strokeWidth = 2.dp)
                } else {
                    Icon(
                        painterResource(if (running) R.drawable.ic_stop else R.drawable.ic_mic),
                        contentDescription = null,
                    )
                }
                Spacer(Modifier.width(8.dp))
                Text(if (running) "Stop" else "Start", fontSize = 20.sp)
            }
        }
    }
}

@Composable
private fun SourceChip(
    label: String,
    icon: Int,
    selected: Boolean,
    enabled: Boolean,
    level: Float?,
    onClick: () -> Unit,
) {
    FilterChip(
        selected = selected,
        enabled = enabled,
        onClick = onClick,
        label = {
            Row(verticalAlignment = Alignment.CenterVertically) {
                Text(label)
                if (level != null && selected) {
                    Spacer(Modifier.width(8.dp))
                    LevelMeter(level)
                }
            }
        },
        leadingIcon = { Icon(painterResource(icon), contentDescription = null, modifier = Modifier.size(18.dp)) },
    )
}

/** A small bar showing that sound is being picked up. */
@Composable
private fun LevelMeter(db: Float) {
    val fraction = ((db + 60f) / 60f).coerceIn(0f, 1f)
    Box(
        Modifier
            .width(40.dp)
            .height(6.dp)
            .clip(RoundedCornerShape(3.dp))
            .background(MaterialTheme.colorScheme.outline.copy(alpha = 0.3f))
            .semantics { contentDescription = "Input level" },
    ) {
        Box(
            Modifier
                .fillMaxHeight()
                .fillMaxWidth(fraction)
                .background(MaterialTheme.colorScheme.primary),
        )
    }
}

@Composable
private fun RenameDialog(current: String, speaker: Int, onDismiss: () -> Unit, onSave: (String) -> Unit) {
    var name by remember { mutableStateOf(current) }
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text("Name Speaker $speaker") },
        text = {
            OutlinedTextField(
                value = name,
                onValueChange = { name = it },
                singleLine = true,
                label = { Text("Name") },
            )
        },
        confirmButton = { TextButton(onClick = { onSave(name) }) { Text("Save") } },
        dismissButton = { TextButton(onClick = onDismiss) { Text("Cancel") } },
    )
}
