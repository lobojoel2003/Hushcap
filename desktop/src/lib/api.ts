// Typed wrappers around Tauri commands and events.

import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import type {
  AppInfo,
  Caption,
  DeviceList,
  DownloadEvent,
  EngineEvent,
  ExportFormat,
  ExportOptions,
  InstalledModel,
  ModelsOverview,
  Settings,
} from './types';

export const api = {
  appInfo: () => invoke<AppInfo>('app_info'),
  getSettings: () => invoke<Settings>('get_settings'),
  saveSettings: (settings: Settings) => invoke<Settings>('save_settings', { settings }),
  listDevices: () => invoke<DeviceList>('list_devices'),
  listModels: () => invoke<ModelsOverview>('list_models'),
  downloadModel: (id: string) => invoke<void>('download_model', { id }),
  cancelDownload: (id: string) => invoke<void>('cancel_download', { id }),
  importModel: (path: string) => invoke<InstalledModel>('import_model', { path }),
  deleteModel: (id: string) => invoke<void>('delete_model', { id }),
  openModelsFolder: () => invoke<void>('open_models_folder'),
  openProjectPage: () => invoke<void>('open_project_page'),
  startCaptions: () => invoke<string[]>('start_captions'),
  stopCaptions: () => invoke<void>('stop_captions'),
  isRunning: () => invoke<boolean>('is_running'),
  renderTranscript: (
    captions: Caption[],
    names: Record<number, string>,
    format: ExportFormat,
    options: ExportOptions,
  ) => invoke<string>('render_transcript', { captions, names, format, options }),
  exportTranscript: (
    path: string,
    captions: Caption[],
    names: Record<number, string>,
    format: ExportFormat | null,
    options: ExportOptions,
  ) => invoke<void>('export_transcript', { path, captions, names, format, options }),
};

export function onEngineEvent(handler: (e: EngineEvent) => void): Promise<UnlistenFn> {
  return listen<EngineEvent>('engine', (e) => handler(e.payload));
}

export function onDownload(handler: (e: DownloadEvent) => void): Promise<UnlistenFn> {
  return listen<DownloadEvent>('download', (e) => handler(e.payload));
}

/** Error messages from commands arrive as strings. */
export function errorMessage(e: unknown): string {
  if (typeof e === 'string') return e;
  if (e instanceof Error) return e.message;
  return String(e);
}
