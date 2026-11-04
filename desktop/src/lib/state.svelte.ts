// Application state (Svelte 5 runes). One instance, shared by all components.

import { getCurrentWindow } from '@tauri-apps/api/window';
import { api, errorMessage, onDownload, onEngineEvent } from './api';
import { applyEvent, emptyTranscript, renameSpeaker, type TranscriptState } from './transcript';
import type {
  AppInfo,
  AudioSource,
  DeviceList,
  DownloadEvent,
  EngineEvent,
  EngineState,
  ModelsOverview,
  Settings,
} from './types';

export interface Toast {
  id: number;
  kind: 'info' | 'warning' | 'error';
  message: string;
}

export interface Level {
  db: number;
  speaking: boolean;
}

class AppStore {
  ready = $state(false);
  info = $state<AppInfo | null>(null);
  settings = $state<Settings | null>(null);
  devices = $state<DeviceList | null>(null);
  models = $state<ModelsOverview | null>(null);
  downloads = $state<Record<string, DownloadEvent>>({});
  engine = $state<EngineState>('stopped');
  transcript = $state<TranscriptState>(emptyTranscript());
  levels = $state<Record<AudioSource, Level>>({
    microphone: { db: -100, speaking: false },
    system: { db: -100, speaking: false },
  });
  stats = $state<{ realtimeFactor: number; queued: number } | null>(null);
  toasts = $state<Toast[]>([]);
  /** Whether the effective theme is dark (drives speaker colours). */
  dark = $state(false);
  /** Settings changed that only apply after captions restart. */
  pendingRestart = $state(false);
  settingsOpen = $state(false);
  settingsTab = $state<'captions' | 'audio' | 'speech' | 'speakers' | 'models' | 'about'>(
    'captions',
  );

  private nextToast = 1;

  get running(): boolean {
    return this.engine === 'running';
  }

  get busy(): boolean {
    return this.engine === 'starting' || this.engine === 'stopping';
  }

  get hasSpeechModel(): boolean {
    return !!this.models?.installed.some((m) => m.kind === 'speech');
  }

  get hasSpeakerModel(): boolean {
    return !!this.models?.installed.some((m) => m.kind === 'speaker');
  }

  async init() {
    await onEngineEvent((e) => this.handleEngineEvent(e));
    await onDownload((e) => this.handleDownload(e));
    const [info, settings, models, running] = await Promise.all([
      api.appInfo(),
      api.getSettings(),
      api.listModels(),
      api.isRunning(),
    ]);
    this.info = info;
    this.settings = settings;
    this.models = models;
    this.engine = running ? 'running' : 'stopped';
    this.ready = true;
    void this.refreshDevices();
    if (settings.appearance.alwaysOnTop) {
      void getCurrentWindow().setAlwaysOnTop(true);
    }
  }

  async refreshDevices() {
    try {
      this.devices = await api.listDevices();
    } catch (e) {
      this.toast('warning', `Could not list audio devices: ${errorMessage(e)}`);
    }
  }

  async refreshModels() {
    this.models = await api.listModels();
    // The backend may have adopted a freshly downloaded model.
    this.settings = await api.getSettings();
  }

  /**
   * Apply a change to settings and persist it. `affectsEngine` marks changes
   * (models, devices, tuning) that take effect when captions restart.
   */
  async update(change: (s: Settings) => void, affectsEngine = false) {
    if (!this.settings) return;
    if (affectsEngine && this.running) this.pendingRestart = true;
    const next: Settings = $state.snapshot(this.settings);
    change(next);
    const onTopChanged = next.appearance.alwaysOnTop !== this.settings.appearance.alwaysOnTop;
    this.settings = next;
    try {
      this.settings = await api.saveSettings(next);
      if (onTopChanged) await getCurrentWindow().setAlwaysOnTop(next.appearance.alwaysOnTop);
    } catch (e) {
      this.toast('error', `Settings were not saved: ${errorMessage(e)}`);
    }
  }

  async start() {
    if (this.running || this.busy) return;
    this.engine = 'starting';
    this.stats = null;
    this.pendingRestart = false;
    try {
      await api.startCaptions();
      this.engine = 'running';
    } catch (e) {
      this.engine = 'stopped';
      this.toast('error', errorMessage(e));
    }
  }

  async stop() {
    if (!this.running) return;
    this.engine = 'stopping';
    try {
      await api.stopCaptions();
    } catch (e) {
      this.toast('error', errorMessage(e));
    } finally {
      this.engine = 'stopped';
      this.levels.microphone = { db: -100, speaking: false };
      this.levels.system = { db: -100, speaking: false };
    }
  }

  /** Apply changes that need a restart while captions are running. */
  async restartIfRunning() {
    if (!this.running) return;
    await this.stop();
    await this.start();
  }

  toggle() {
    return this.running ? this.stop() : this.start();
  }

  clearTranscript() {
    this.transcript = emptyTranscript();
  }

  rename(speaker: number, name: string) {
    this.transcript = renameSpeaker(this.transcript, speaker, name);
  }

  async download(id: string) {
    this.downloads[id] = { id, done: false, error: null, downloaded: 0 };
    try {
      await api.downloadModel(id);
    } catch (e) {
      this.downloads[id] = { id, done: true, error: errorMessage(e) };
    }
  }

  async cancelDownload(id: string) {
    await api.cancelDownload(id);
  }

  async deleteModel(id: string) {
    try {
      await api.deleteModel(id);
      await this.refreshModels();
    } catch (e) {
      this.toast('error', errorMessage(e));
    }
  }

  async importModel(path: string) {
    try {
      const m = await api.importModel(path);
      await this.refreshModels();
      this.toast('info', `Imported ${m.name}.`);
    } catch (e) {
      this.toast('error', errorMessage(e));
    }
  }

  toast(kind: Toast['kind'], message: string) {
    const id = this.nextToast++;
    this.toasts = [...this.toasts.slice(-3), { id, kind, message }];
    const ttl = kind === 'error' ? 12_000 : 6_000;
    setTimeout(() => this.dismiss(id), ttl);
  }

  dismiss(id: number) {
    this.toasts = this.toasts.filter((t) => t.id !== id);
  }

  private handleEngineEvent(e: EngineEvent) {
    switch (e.type) {
      case 'level':
        this.levels[e.source] = { db: e.db, speaking: e.speaking };
        return;
      case 'stats':
        this.stats = { realtimeFactor: e.realtimeFactor, queued: e.queued };
        return;
      case 'state':
        this.engine = e.state;
        break;
      case 'warning':
        this.toast('warning', e.message);
        return;
      case 'error':
        this.toast('error', e.message);
        return;
    }
    this.transcript = applyEvent(this.transcript, e);
  }

  private handleDownload(e: DownloadEvent) {
    this.downloads[e.id] = { ...this.downloads[e.id], ...e };
    if (e.done) {
      if (e.error) {
        this.toast('error', `Download failed: ${e.error}`);
      } else {
        const name = this.models?.catalog.find((m) => m.id === e.id)?.name ?? e.id;
        this.toast('info', `${name} is ready.`);
      }
      void this.refreshModels();
      setTimeout(() => {
        if (this.downloads[e.id]?.done) delete this.downloads[e.id];
      }, 4000);
    }
  }
}

export const app = new AppStore();
