// Mirrors the Rust types sent over IPC (camelCase via serde).

export type AudioSource = 'microphone' | 'system';
export type EngineState = 'stopped' | 'starting' | 'running' | 'stopping';
export type Accuracy = 'fast' | 'balanced' | 'accurate';
export type ExportFormat = 'txt' | 'srt' | 'vtt' | 'markdown' | 'json';
export type ModelKind = 'speech' | 'speaker';

export interface Caption {
  id: number;
  source: AudioSource;
  speaker: number | null;
  text: string;
  startMs: number;
  endMs: number;
  language?: string | null;
}

export type EngineEvent =
  | { type: 'state'; state: EngineState }
  | { type: 'speechStarted'; id: number; source: AudioSource; startMs: number }
  | { type: 'partial'; id: number; source: AudioSource; text: string; startMs: number }
  | { type: 'final'; caption: Caption }
  | { type: 'dropped'; id: number }
  | { type: 'speakersMerged'; from: number; into: number }
  | { type: 'level'; source: AudioSource; db: number; speaking: boolean }
  | { type: 'stats'; realtimeFactor: number; queued: number }
  | { type: 'warning'; message: string }
  | { type: 'error'; message: string };

export interface ProcessingSettings {
  partialResults: boolean;
  vadSensitivity: number;
  speakerSeparation: number;
  suppressMicEcho: boolean;
  endSilenceMs: number;
  maxSegmentMs: number;
}

export type Theme = 'system' | 'light' | 'dark' | 'contrast';
export type FontFamily = 'hyperlegible' | 'system' | 'dyslexic' | 'mono';

export interface Appearance {
  theme: Theme;
  fontScale: number;
  fontFamily: FontFamily;
  lineSpacing: number;
  showTimestamps: boolean;
  alwaysOnTop: boolean;
  compact: boolean;
}

export interface Settings {
  speechModel: string | null;
  speakerModel: string | null;
  speakerLabels: boolean;
  language: string;
  translate: boolean;
  accuracy: Accuracy;
  threads: number;
  microphoneEnabled: boolean;
  microphoneDevice: string | null;
  systemEnabled: boolean;
  systemDevice: string | null;
  processing: ProcessingSettings;
  appearance: Appearance;
  onboardingDone: boolean;
}

export interface AudioDevice {
  id: string;
  name: string;
  kind: 'input' | 'loopback';
  isDefault: boolean;
}

export interface DeviceList {
  host: string;
  microphones: AudioDevice[];
  system: AudioDevice[];
  systemCaptureNative: boolean;
}

export interface ModelStatus {
  id: string;
  kind: ModelKind;
  name: string;
  description: string;
  languages: string;
  multilingual: boolean;
  fileName: string;
  url: string;
  sizeMb: number;
  sizeBytes: number | null;
  sha256: string | null;
  recommended: boolean;
  speed: number;
  accuracy: number;
  installed: boolean;
  path: string;
}

export interface InstalledModel {
  id: string;
  kind: ModelKind;
  name: string;
  path: string;
  sizeBytes: number;
  fromCatalog: boolean;
}

export interface ModelsOverview {
  catalog: ModelStatus[];
  installed: InstalledModel[];
  dir: string;
  downloading: string[];
}

export interface DownloadEvent {
  id: string;
  downloaded?: number;
  total?: number | null;
  bytesPerSecond?: number;
  done: boolean;
  error: string | null;
}

export interface BuildInfo {
  version: string;
  buildNumber: string;
  gitSha: string;
  buildDate: string;
  os: string;
  arch: string;
  whisperCpp: string | null;
}

export interface AppInfo {
  build: BuildInfo;
  appId: string;
  modelsDir: string;
  systemAudioSupported: boolean;
}

export interface ExportOptions {
  includeSpeakers: boolean;
  includeTimestamps: boolean;
  title: string | null;
}
