//! Hushcap desktop: a thin Tauri shell around `hushcap-engine`.
//!
//! The web UI talks to Rust through the commands below. Audio, recognition
//! and transcripts stay in this process; the webview has no network access
//! (see the content security policy in `tauri.conf.json`).

mod settings;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use hushcap_core::caption::{Caption, EngineEvent};
use hushcap_core::export::{self, ExportFormat, ExportOptions};
use hushcap_core::models::{self, ModelKind};
use hushcap_engine::asr::WhisperOptions;
use hushcap_engine::build_info::{self, BuildInfo};
use hushcap_engine::capture::{self, DeviceList};
use hushcap_engine::download::{self, Progress};
use hushcap_engine::storage::{InstalledModel, ModelStatus, ModelStore, APP_ID};
use hushcap_engine::{Engine, EngineConfig, SourceSettings};
use parking_lot::Mutex;
use serde::Serialize;
use settings::{Settings, SettingsStore};
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_opener::OpenerExt;

/// Errors cross the IPC boundary as plain strings.
type CmdResult<T> = Result<T, String>;

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

fn err_chain(e: anyhow::Error) -> String {
    format!("{e:#}")
}

struct AppState {
    settings: Mutex<Settings>,
    settings_store: SettingsStore,
    models: ModelStore,
    engine: Mutex<Option<Engine>>,
    /// Serialises start/stop so double clicks can't start two engines.
    lifecycle: Mutex<()>,
    downloads: Mutex<HashMap<String, Arc<AtomicBool>>>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct AppInfo {
    build: BuildInfo,
    app_id: &'static str,
    models_dir: PathBuf,
    system_audio_supported: bool,
}

#[tauri::command]
fn app_info(state: State<'_, AppState>) -> AppInfo {
    AppInfo {
        build: build_info::build_info(),
        app_id: APP_ID,
        models_dir: state.models.dir().to_path_buf(),
        system_audio_supported: capture::system_audio_supported(),
    }
}

#[tauri::command]
fn get_settings(state: State<'_, AppState>) -> Settings {
    state.settings.lock().clone()
}

#[tauri::command]
fn save_settings(state: State<'_, AppState>, settings: Settings) -> CmdResult<Settings> {
    let settings = settings.sanitized();
    state.settings_store.save(&settings).map_err(err_chain)?;
    *state.settings.lock() = settings.clone();
    Ok(settings)
}

#[tauri::command]
async fn list_devices() -> CmdResult<DeviceList> {
    tauri::async_runtime::spawn_blocking(capture::list_devices)
        .await
        .map_err(err)?
        .map_err(err_chain)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ModelsOverview {
    catalog: Vec<ModelStatus>,
    installed: Vec<InstalledModel>,
    dir: PathBuf,
    downloading: Vec<String>,
}

#[tauri::command]
fn list_models(state: State<'_, AppState>) -> ModelsOverview {
    ModelsOverview {
        catalog: state.models.catalog_status(),
        installed: state.models.installed(),
        dir: state.models.dir().to_path_buf(),
        downloading: state.downloads.lock().keys().cloned().collect(),
    }
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct DownloadEvent {
    id: String,
    #[serde(flatten)]
    progress: Option<Progress>,
    done: bool,
    error: Option<String>,
}

/// Start downloading a catalogue model. Progress arrives as `download` events.
#[tauri::command]
fn download_model(app: AppHandle, state: State<'_, AppState>, id: String) -> CmdResult<()> {
    let info = models::find(&id).ok_or_else(|| format!("unknown model '{id}'"))?;
    let cancel = Arc::new(AtomicBool::new(false));
    {
        let mut downloads = state.downloads.lock();
        if downloads.contains_key(&id) {
            return Ok(());
        }
        downloads.insert(id.clone(), cancel.clone());
    }
    let store = state.models.clone();
    std::thread::Builder::new()
        .name(format!("download-{id}"))
        .spawn(move || {
            let emit = |progress: Option<Progress>, done: bool, error: Option<String>| {
                let _ = app.emit(
                    "download",
                    DownloadEvent {
                        id: id.clone(),
                        progress,
                        done,
                        error,
                    },
                );
            };
            let result = download::download(info, &store, &cancel, |p| emit(Some(p), false, None));
            app.state::<AppState>().downloads.lock().remove(&id);
            match result {
                Ok(_) => {
                    // Adopt the first model of each kind automatically.
                    let state = app.state::<AppState>();
                    let mut settings = state.settings.lock();
                    let slot = match info.kind {
                        ModelKind::Speech => &mut settings.speech_model,
                        ModelKind::Speaker => &mut settings.speaker_model,
                    };
                    if slot.is_none() {
                        *slot = Some(info.id.to_string());
                        let _ = state.settings_store.save(&settings);
                    }
                    drop(settings);
                    emit(None, true, None);
                }
                Err(e) => emit(None, true, Some(format!("{e:#}"))),
            }
        })
        .map_err(err)?;
    Ok(())
}

#[tauri::command]
fn cancel_download(state: State<'_, AppState>, id: String) {
    if let Some(flag) = state.downloads.lock().get(&id) {
        flag.store(true, Ordering::SeqCst);
    }
}

#[tauri::command]
async fn import_model(state: State<'_, AppState>, path: PathBuf) -> CmdResult<InstalledModel> {
    let store = state.models.clone();
    let model = tauri::async_runtime::spawn_blocking(move || store.import(&path))
        .await
        .map_err(err)?
        .map_err(err_chain)?;
    let mut settings = state.settings.lock();
    let slot = match model.kind {
        ModelKind::Speech => &mut settings.speech_model,
        ModelKind::Speaker => &mut settings.speaker_model,
    };
    *slot = Some(model.id.clone());
    state.settings_store.save(&settings).map_err(err_chain)?;
    Ok(model)
}

#[tauri::command]
fn delete_model(state: State<'_, AppState>, id: String) -> CmdResult<()> {
    state.models.delete(&id).map_err(err_chain)?;
    let mut settings = state.settings.lock();
    if settings.speech_model.as_deref() == Some(id.as_str()) {
        settings.speech_model = None;
    }
    if settings.speaker_model.as_deref() == Some(id.as_str()) {
        settings.speaker_model = None;
    }
    state.settings_store.save(&settings).map_err(err_chain)
}

#[tauri::command]
fn open_models_folder(app: AppHandle, state: State<'_, AppState>) -> CmdResult<()> {
    app.opener()
        .open_path(state.models.dir().to_string_lossy(), None::<&str>)
        .map_err(err)
}

#[tauri::command]
fn open_project_page(app: AppHandle) -> CmdResult<()> {
    app.opener()
        .open_url("https://github.com/lobojoel2003/Hushcap", None::<&str>)
        .map_err(err)
}

/// Pick the speech and speaker model files for a session.
fn resolve_models(
    state: &AppState,
    settings: &Settings,
) -> Result<(PathBuf, Option<PathBuf>), String> {
    let installed = state.models.installed();
    let speech = match settings.speech_model.as_deref() {
        Some(id) => state.models.resolve(id).map_err(err_chain)?,
        None => installed
            .iter()
            .find(|m| m.kind == ModelKind::Speech)
            .map(|m| m.path.clone())
            .ok_or("Download a speech model first (Settings → Models).")?,
    };
    let speaker = if settings.speaker_labels {
        match settings.speaker_model.as_deref() {
            Some(id) => state.models.resolve(id).ok(),
            None => installed
                .iter()
                .find(|m| m.kind == ModelKind::Speaker)
                .map(|m| m.path.clone()),
        }
    } else {
        None
    };
    Ok((speech, speaker))
}

#[tauri::command]
async fn start_captions(app: AppHandle) -> CmdResult<Vec<String>> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let _guard = state.lifecycle.lock();
        if state.engine.lock().is_some() {
            return Err("Captions are already running.".to_string());
        }
        let settings = state.settings.lock().clone();
        if !settings.microphone_enabled && !settings.system_enabled {
            return Err("Turn on the microphone, system audio, or both.".into());
        }
        let (speech_model, speaker_model) = resolve_models(&state, &settings)?;
        let config = EngineConfig {
            speech_model,
            speaker_model,
            whisper: WhisperOptions {
                language: (settings.language != "auto").then(|| settings.language.clone()),
                threads: settings.threads,
                accuracy: settings.accuracy,
                translate: settings.translate,
                ..Default::default()
            },
            microphone: settings.microphone_enabled.then(|| SourceSettings {
                device: settings.microphone_device.clone(),
            }),
            system: settings.system_enabled.then(|| SourceSettings {
                device: settings.system_device.clone(),
            }),
            processing: settings.processing.clone(),
        };
        let handle = app.clone();
        let sink = Arc::new(move |event: EngineEvent| {
            let _ = handle.emit("engine", &event);
        });
        let engine = Engine::start(config, sink).map_err(err_chain)?;
        let devices = engine.devices();
        log::info!("captioning with {} on {:?}", engine.description(), devices);
        *state.engine.lock() = Some(engine);
        Ok(devices)
    })
    .await
    .map_err(err)?
}

#[tauri::command]
async fn stop_captions(app: AppHandle) -> CmdResult<()> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let _guard = state.lifecycle.lock();
        let engine = state.engine.lock().take();
        if let Some(engine) = engine {
            engine.stop();
        }
    })
    .await
    .map_err(err)
}

#[tauri::command]
fn is_running(state: State<'_, AppState>) -> bool {
    state.engine.lock().is_some()
}

#[tauri::command]
fn render_transcript(
    captions: Vec<Caption>,
    names: HashMap<u32, String>,
    format: ExportFormat,
    options: ExportOptions,
) -> String {
    export::export(&captions, &names, format, &options)
}

#[tauri::command]
async fn export_transcript(
    path: PathBuf,
    captions: Vec<Caption>,
    names: HashMap<u32, String>,
    format: Option<ExportFormat>,
    options: ExportOptions,
) -> CmdResult<()> {
    let format = format
        .or_else(|| {
            path.extension()
                .and_then(|e| e.to_str())
                .and_then(ExportFormat::from_extension)
        })
        .unwrap_or(ExportFormat::Txt);
    let text = export::export(&captions, &names, format, &options);
    tauri::async_runtime::spawn_blocking(move || std::fs::write(&path, text))
        .await
        .map_err(err)?
        .map_err(err)
}

/// Build and run the app.
pub fn run() {
    // The PulseAudio client logs an "error" whenever a connection is closed on
    // purpose (after listing devices); it is noise, so it is off by default.
    env_logger::Builder::from_env(
        env_logger::Env::default().default_filter_or("info,whisper_rs=warn,pulseaudio=off"),
    )
    .init();

    tauri::Builder::default()
        .plugin(tauri_plugin_window_state::Builder::default().build())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let config_dir = app.path().app_config_dir()?;
            let data_dir = app.path().app_local_data_dir()?;
            let models_dir = std::env::var_os("HUSHCAP_MODELS_DIR")
                .map(PathBuf::from)
                .unwrap_or_else(|| data_dir.join("models"));
            let settings_store = SettingsStore::new(&config_dir);
            let settings = settings_store.load();
            let models = ModelStore::open(models_dir)?;
            log::info!(
                "Hushcap {} · models in {}",
                build_info::version_string(),
                models.dir().display()
            );
            app.manage(AppState {
                settings: Mutex::new(settings),
                settings_store,
                models,
                engine: Mutex::new(None),
                lifecycle: Mutex::new(()),
                downloads: Mutex::new(HashMap::new()),
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            app_info,
            get_settings,
            save_settings,
            list_devices,
            list_models,
            download_model,
            cancel_download,
            import_model,
            delete_model,
            open_models_folder,
            open_project_page,
            start_captions,
            stop_captions,
            is_running,
            render_transcript,
            export_transcript,
        ])
        .build(tauri::generate_context!())
        .expect("error while building Hushcap")
        .run(|app, event| {
            if let tauri::RunEvent::Exit = event {
                // Finish cleanly so devices are released.
                if let Some(engine) = app.state::<AppState>().engine.lock().take() {
                    engine.stop();
                }
            }
        });
}
