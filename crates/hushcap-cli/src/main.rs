//! `hushcap` — private, offline live captions from the command line.

use std::collections::HashMap;
use std::io::{BufRead, IsTerminal, Write};
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use anyhow::{bail, Context, Result};
use clap::{Args, Parser, Subcommand, ValueEnum};
use hushcap_core::caption::{AudioSource, EngineEvent};
use hushcap_core::export::{self, ExportFormat, ExportOptions};
use hushcap_core::models::{self, ModelKind};
use hushcap_engine::asr::{Accuracy, WhisperOptions, WhisperRecognizer};
use hushcap_engine::storage::{default_models_dir, ModelStore};
use hushcap_engine::{build_info, capture, download, pipeline, wav};
use hushcap_engine::{Engine, EngineConfig, ProcessingSettings, SourceSettings};

#[derive(Parser)]
#[command(
    name = "hushcap",
    version = build_info::VERSION,
    long_version = long_version(),
    about = "Private, offline live captions. Audio never leaves this device.",
    propagate_version = true
)]
struct Cli {
    /// Folder holding model files (default: the Hushcap app's model folder).
    #[arg(long, global = true, env = "HUSHCAP_MODELS_DIR")]
    models_dir: Option<PathBuf>,

    /// Show debug logging.
    #[arg(short, long, global = true)]
    verbose: bool,

    #[command(subcommand)]
    command: Command,
}

fn long_version() -> &'static str {
    Box::leak(
        format!(
            "{}\nbuilt {} for {}-{}\nwhisper.cpp {}",
            build_info::version_string(),
            build_info::BUILD_DATE,
            std::env::consts::OS,
            std::env::consts::ARCH,
            build_info::build_info()
                .whisper_cpp
                .unwrap_or("not included"),
        )
        .into_boxed_str(),
    )
}

#[derive(Subcommand)]
enum Command {
    /// Caption live audio from the microphone and/or system audio.
    Live(LiveArgs),
    /// Transcribe a WAV recording.
    Transcribe(TranscribeArgs),
    /// List microphones and system audio sources.
    Devices,
    /// List, download, verify or delete models.
    Models {
        #[command(subcommand)]
        action: Option<ModelsAction>,
    },
    /// Show version and build details.
    Version,
}

#[derive(Subcommand)]
enum ModelsAction {
    /// List available and installed models (default).
    List {
        /// Print machine-readable JSON.
        #[arg(long)]
        json: bool,
    },
    /// Download a model (the only time Hushcap uses the network).
    Download {
        /// Model id, e.g. whisper-base.en-q5_1. Use "recommended" for the
        /// recommended speech and speaker models.
        id: String,
    },
    /// Check an installed model's integrity.
    Verify {
        /// Model id.
        id: String,
    },
    /// Delete an installed model.
    Delete {
        /// Model id (or file:<name> for imported models).
        id: String,
    },
    /// Copy a model file into the model folder.
    Import {
        /// Path to a .bin (speech) or .onnx (speaker) model.
        path: PathBuf,
    },
    /// Print the model folder path.
    Dir,
}

#[derive(Args, Clone)]
struct ModelArgs {
    /// Speech model: catalogue id, file:<name>, or a path. Defaults to the
    /// recommended model, or the first installed one.
    #[arg(short, long)]
    model: Option<String>,

    /// Speaker model for labels: id, path, or "none" to turn labels off.
    #[arg(long)]
    speaker_model: Option<String>,

    /// Spoken language code (en, es, fr, …) or "auto".
    #[arg(short, long, default_value = "auto")]
    language: String,

    /// Translate into English.
    #[arg(long)]
    translate: bool,

    /// CPU threads for recognition (0 = automatic).
    #[arg(long, default_value_t = 0)]
    threads: usize,

    /// Speed/accuracy trade-off.
    #[arg(long, value_enum, default_value_t = AccuracyArg::Balanced)]
    accuracy: AccuracyArg,
}

#[derive(Clone, Copy, ValueEnum)]
enum AccuracyArg {
    Fast,
    Balanced,
    Accurate,
}

impl From<AccuracyArg> for Accuracy {
    fn from(a: AccuracyArg) -> Self {
        match a {
            AccuracyArg::Fast => Accuracy::Fast,
            AccuracyArg::Balanced => Accuracy::Balanced,
            AccuracyArg::Accurate => Accuracy::Accurate,
        }
    }
}

#[derive(Args)]
struct LiveArgs {
    #[command(flatten)]
    model: ModelArgs,

    /// Caption the microphone (optionally a specific device id).
    #[arg(long, num_args = 0..=1, default_missing_value = "")]
    mic: Option<String>,

    /// Caption system audio (optionally a specific device id).
    #[arg(long, num_args = 0..=1, default_missing_value = "")]
    system: Option<String>,

    /// Hide interim (in-progress) text.
    #[arg(long)]
    no_partials: bool,

    /// Also save the transcript to this file when you stop (format from extension).
    #[arg(short, long)]
    output: Option<PathBuf>,
}

#[derive(Args)]
struct TranscribeArgs {
    /// WAV file to transcribe.
    input: PathBuf,

    #[command(flatten)]
    model: ModelArgs,

    /// Output format (default: from --output extension, else txt).
    #[arg(short, long, value_enum)]
    format: Option<FormatArg>,

    /// Write to this file instead of standard output.
    #[arg(short, long)]
    output: Option<PathBuf>,
}

#[derive(Clone, Copy, ValueEnum)]
enum FormatArg {
    Txt,
    Srt,
    Vtt,
    Md,
    Json,
}

impl From<FormatArg> for ExportFormat {
    fn from(f: FormatArg) -> Self {
        match f {
            FormatArg::Txt => ExportFormat::Txt,
            FormatArg::Srt => ExportFormat::Srt,
            FormatArg::Vtt => ExportFormat::Vtt,
            FormatArg::Md => ExportFormat::Markdown,
            FormatArg::Json => ExportFormat::Json,
        }
    }
}

fn main() {
    let cli = Cli::parse();
    env_logger::Builder::new()
        .filter_level(if cli.verbose {
            log::LevelFilter::Debug
        } else {
            log::LevelFilter::Warn
        })
        // whisper.cpp is chatty at info level.
        .filter_module(
            "whisper_rs",
            if cli.verbose {
                log::LevelFilter::Info
            } else {
                log::LevelFilter::Error
            },
        )
        // Closing a PulseAudio connection on purpose is logged as an error; hide it.
        .filter_module(
            "pulseaudio",
            if cli.verbose {
                log::LevelFilter::Debug
            } else {
                log::LevelFilter::Off
            },
        )
        .parse_env("HUSHCAP_LOG")
        .init();
    if let Err(e) = run(cli) {
        eprintln!("error: {e:#}");
        std::process::exit(1);
    }
}

fn run(cli: Cli) -> Result<()> {
    let store = ModelStore::open(cli.models_dir.clone().unwrap_or_else(default_models_dir))?;
    match cli.command {
        Command::Version => {
            println!("hushcap {}", long_version());
            Ok(())
        }
        Command::Devices => devices(),
        Command::Models { action } => {
            models_cmd(&store, action.unwrap_or(ModelsAction::List { json: false }))
        }
        Command::Transcribe(args) => transcribe(&store, args),
        Command::Live(args) => live(&store, args),
    }
}

fn devices() -> Result<()> {
    let list = capture::list_devices()?;
    println!("Audio system: {}\n", list.host);
    println!("Microphones (use with --mic <id>):");
    for d in &list.microphones {
        println!(
            "  {} {}\n      id: {}",
            if d.is_default { "*" } else { " " },
            d.name,
            d.id
        );
    }
    println!();
    if list.system_capture_native {
        println!("System audio (use with --system <id>):");
    } else {
        println!(
            "System audio: not supported natively here. Choose a loopback input device instead:"
        );
    }
    for d in &list.system {
        println!(
            "  {} {}\n      id: {}",
            if d.is_default { "*" } else { " " },
            d.name,
            d.id
        );
    }
    println!("\n* = default");
    Ok(())
}

fn models_cmd(store: &ModelStore, action: ModelsAction) -> Result<()> {
    match action {
        ModelsAction::Dir => println!("{}", store.dir().display()),
        ModelsAction::List { json: true } => {
            println!("{}", serde_json::to_string_pretty(&store.catalog_status())?);
        }
        ModelsAction::List { json: false } => {
            println!("Models folder: {}\n", store.dir().display());
            for kind in [ModelKind::Speech, ModelKind::Speaker] {
                println!(
                    "{}:",
                    match kind {
                        ModelKind::Speech => "Speech models",
                        ModelKind::Speaker => "Speaker label models",
                    }
                );
                for s in store
                    .catalog_status()
                    .iter()
                    .filter(|s| s.info.kind == kind)
                {
                    println!(
                        "  [{}] {:<30} {:>4} MB  {}{}",
                        if s.installed { "x" } else { " " },
                        s.info.id,
                        s.info.size_mb,
                        s.info.name,
                        if s.info.recommended {
                            "  (recommended)"
                        } else {
                            ""
                        }
                    );
                }
                println!();
            }
            let imported: Vec<_> = store
                .installed()
                .into_iter()
                .filter(|m| !m.from_catalog)
                .collect();
            if !imported.is_empty() {
                println!("Imported:");
                for m in imported {
                    println!("  [x] {:<30} {:>4} MB", m.id, m.size_bytes / 1_000_000);
                }
            }
        }
        ModelsAction::Download { id } => {
            let ids: Vec<&str> = if id == "recommended" {
                vec![
                    models::recommended(ModelKind::Speech).id,
                    models::recommended(ModelKind::Speaker).id,
                ]
            } else {
                vec![id.as_str()]
            };
            for id in ids {
                let info = models::find(id)
                    .with_context(|| format!("unknown model '{id}'; see `hushcap models`"))?;
                if store.is_installed(info) {
                    println!("{} is already installed.", info.id);
                    continue;
                }
                eprintln!(
                    "Downloading {} ({} MB) from {}",
                    info.name, info.size_mb, info.url
                );
                let cancel = AtomicBool::new(false);
                let tty = std::io::stderr().is_terminal();
                let path = download::download(info, store, &cancel, |p| {
                    if tty {
                        let pct = p
                            .total
                            .map(|t| p.downloaded as f64 / t as f64 * 100.0)
                            .unwrap_or(0.0);
                        eprint!(
                            "\r  {:>5.1}%  {:>6.1} MB  {:>5.1} MB/s ",
                            pct,
                            p.downloaded as f64 / 1e6,
                            p.bytes_per_second / 1e6
                        );
                    }
                })?;
                if tty {
                    eprintln!();
                }
                println!("Installed {}", path.display());
            }
        }
        ModelsAction::Verify { id } => {
            let info = models::find(&id).with_context(|| format!("unknown model '{id}'"))?;
            if download::verify_installed(info, store)? {
                println!("{id}: OK");
            } else {
                bail!("{id}: checksum mismatch — delete and download it again");
            }
        }
        ModelsAction::Delete { id } => {
            store.delete(&id)?;
            println!("Deleted {id}");
        }
        ModelsAction::Import { path } => {
            let m = store.import(&path)?;
            println!("Imported {} as {}", path.display(), m.id);
        }
    }
    Ok(())
}

fn pick_speech_model(store: &ModelStore, requested: Option<&str>) -> Result<PathBuf> {
    if let Some(r) = requested {
        return store.resolve(r);
    }
    let recommended = models::recommended(ModelKind::Speech);
    if store.is_installed(recommended) {
        return Ok(store.path_for(recommended));
    }
    store
        .installed()
        .into_iter()
        .find(|m| m.kind == ModelKind::Speech)
        .map(|m| m.path)
        .with_context(|| {
            format!(
                "no speech model installed. Download one with:\n    hushcap models download {}",
                recommended.id
            )
        })
}

fn pick_speaker_model(store: &ModelStore, requested: Option<&str>) -> Result<Option<PathBuf>> {
    match requested {
        Some("none" | "off") => Ok(None),
        Some(r) => store.resolve(r).map(Some),
        None => Ok(store
            .installed()
            .into_iter()
            .filter(|m| m.kind == ModelKind::Speaker)
            .min_by_key(|m| !(m.id == models::recommended(ModelKind::Speaker).id))
            .map(|m| m.path)),
    }
}

fn whisper_options(args: &ModelArgs) -> WhisperOptions {
    WhisperOptions {
        language: (args.language != "auto").then(|| args.language.clone()),
        threads: args.threads,
        accuracy: args.accuracy.into(),
        translate: args.translate,
        ..Default::default()
    }
}

fn transcribe(store: &ModelStore, args: TranscribeArgs) -> Result<()> {
    let speech = pick_speech_model(store, args.model.model.as_deref())?;
    let speaker = pick_speaker_model(store, args.model.speaker_model.as_deref())?;
    let format = args
        .format
        .map(ExportFormat::from)
        .or_else(|| {
            args.output
                .as_ref()
                .and_then(|p| p.extension())
                .and_then(|e| e.to_str())
                .and_then(ExportFormat::from_extension)
        })
        .unwrap_or(ExportFormat::Txt);

    let audio = wav::read_wav_16k_mono(&args.input)?;
    eprintln!(
        "Transcribing {:.1} s of audio with {}{}",
        audio.len() as f32 / 16_000.0,
        speech.file_name().unwrap_or_default().to_string_lossy(),
        if speaker.is_some() {
            " (with speaker labels)"
        } else {
            ""
        }
    );
    let recognizer = WhisperRecognizer::load(&speech, whisper_options(&args.model))?;
    let captions = pipeline::transcribe_audio(
        &audio,
        Box::new(recognizer),
        speaker,
        ProcessingSettings::default(),
        |e| {
            if let EngineEvent::Warning { message } = e {
                eprintln!("warning: {message}");
            }
        },
    )?;
    let rendered = export::export(
        &captions,
        &HashMap::new(),
        format,
        &ExportOptions::default(),
    );
    match args.output {
        Some(path) => {
            std::fs::write(&path, rendered)
                .with_context(|| format!("could not write {}", path.display()))?;
            eprintln!("Saved {} captions to {}", captions.len(), path.display());
        }
        None => print!("{rendered}"),
    }
    Ok(())
}

fn live(store: &ModelStore, args: LiveArgs) -> Result<()> {
    let speech = pick_speech_model(store, args.model.model.as_deref())?;
    let speaker = pick_speaker_model(store, args.model.speaker_model.as_deref())?;
    let (mic, system) = match (&args.mic, &args.system) {
        (None, None) => (Some(String::new()), None),
        (m, s) => (m.clone(), s.clone()),
    };
    let to_settings = |d: Option<String>| {
        d.map(|d| SourceSettings {
            device: (!d.is_empty()).then_some(d),
        })
    };
    let config = EngineConfig {
        speech_model: speech,
        speaker_model: speaker,
        whisper: whisper_options(&args.model),
        microphone: to_settings(mic),
        system: to_settings(system),
        processing: ProcessingSettings {
            partial_results: !args.no_partials,
            ..Default::default()
        },
    };

    let tty = std::io::stdout().is_terminal();
    let captions = Arc::new(parking_lot_free::Captions::default());
    let sink_captions = captions.clone();
    let sink = Arc::new(move |event: EngineEvent| {
        let mut out = std::io::stdout().lock();
        match event {
            EngineEvent::Partial { text, source, .. } if tty => {
                let _ = write!(
                    out,
                    "\r\x1b[2K\x1b[2m{}{}\x1b[0m",
                    prefix(source, None),
                    text
                );
            }
            EngineEvent::Final { caption } => {
                let line = format!(
                    "{}{}",
                    prefix(caption.source, caption.speaker),
                    caption.text
                );
                if tty {
                    let _ = writeln!(out, "\r\x1b[2K{line}");
                } else {
                    let _ = writeln!(out, "{line}");
                }
                sink_captions.push(caption);
            }
            EngineEvent::Dropped { .. } if tty => {
                let _ = write!(out, "\r\x1b[2K");
            }
            EngineEvent::SpeakersMerged { from, into } => {
                sink_captions.relabel(from, into);
                if tty {
                    let _ = writeln!(out, "\r\x1b[2K\x1b[2m(Speaker {from} is the same person as Speaker {into})\x1b[0m");
                }
            }
            EngineEvent::Warning { message } => eprintln!("\nwarning: {message}"),
            EngineEvent::Error { message } => eprintln!("\nerror: {message}"),
            _ => {}
        }
        let _ = out.flush();
    });

    eprintln!("Loading model…");
    let engine = Engine::start(config, sink)?;
    eprintln!(
        "Listening on {} — audio stays on this device. Press Enter to stop.\n",
        engine.devices().join(" + ")
    );
    let mut line = String::new();
    let _ = std::io::stdin().lock().read_line(&mut line);
    eprintln!("Finishing…");
    engine.stop();

    if let Some(path) = args.output {
        let format = path
            .extension()
            .and_then(|e| e.to_str())
            .and_then(ExportFormat::from_extension)
            .unwrap_or(ExportFormat::Txt);
        let rendered = export::export(
            &captions.all(),
            &HashMap::new(),
            format,
            &ExportOptions::default(),
        );
        std::fs::write(&path, rendered)?;
        eprintln!("Saved transcript to {}", path.display());
    }
    Ok(())
}

fn prefix(source: AudioSource, speaker: Option<u32>) -> String {
    let src = match source {
        AudioSource::Microphone => "",
        AudioSource::System => "[system] ",
    };
    match speaker {
        Some(s) => format!("{src}Speaker {s}: "),
        None => src.to_string(),
    }
}

/// Small thread-safe caption store for the live command.
mod parking_lot_free {
    use hushcap_core::caption::Caption;
    use std::sync::Mutex;

    #[derive(Default)]
    pub struct Captions(Mutex<Vec<Caption>>);

    impl Captions {
        pub fn push(&self, c: Caption) {
            self.0.lock().unwrap_or_else(|e| e.into_inner()).push(c);
        }
        pub fn relabel(&self, from: u32, into: u32) {
            for c in self.0.lock().unwrap_or_else(|e| e.into_inner()).iter_mut() {
                if c.speaker == Some(from) {
                    c.speaker = Some(into);
                }
            }
        }
        pub fn all(&self) -> Vec<Caption> {
            self.0.lock().unwrap_or_else(|e| e.into_inner()).clone()
        }
    }
}
