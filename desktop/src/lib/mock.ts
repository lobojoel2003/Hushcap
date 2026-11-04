// Development-only mock backend. When the UI runs in a plain browser
// (`npm run dev` without Tauri), this stands in for the Rust side and plays a
// scripted conversation so the interface can be worked on and screenshotted.
// It is never included in production builds.

import { emit } from '@tauri-apps/api/event';
import { mockIPC, mockWindows } from '@tauri-apps/api/mocks';
import type {
  AudioSource,
  Caption,
  EngineEvent,
  ModelsOverview,
  ModelStatus,
  Settings,
} from './types';

const params = new URLSearchParams(location.search);

function catalogEntry(
  id: string,
  kind: 'speech' | 'speaker',
  name: string,
  description: string,
  sizeMb: number,
  extra: Partial<ModelStatus> = {},
): ModelStatus {
  return {
    id,
    kind,
    name,
    description,
    languages: kind === 'speech' ? 'English' : 'Any language',
    multilingual: kind !== 'speech',
    fileName: `${id}.bin`,
    url: 'https://example.invalid',
    sizeMb,
    sizeBytes: null,
    sha256: null,
    recommended: false,
    speed: 4,
    accuracy: 3,
    installed: false,
    path: `/models/${id}`,
    ...extra,
  };
}

const fresh = params.has('fresh');

let models: ModelsOverview = {
  dir: '/home/you/.local/share/io.github.lobojoel2003.hushcap/models',
  downloading: [],
  catalog: [
    catalogEntry(
      'whisper-base.en-q5_1',
      'speech',
      'English · Base (compact)',
      'Good accuracy and fast on most laptops. The best place to start.',
      57,
      { recommended: true, installed: !fresh },
    ),
    catalogEntry(
      'whisper-tiny.en',
      'speech',
      'English · Tiny',
      'Fastest. For older or low-power computers.',
      75,
      {
        speed: 5,
        accuracy: 2,
      },
    ),
    catalogEntry(
      'whisper-small.en-q5_1',
      'speech',
      'English · Small (compact)',
      'Noticeably more accurate. Needs a recent 4+ core CPU to keep up live.',
      181,
      { speed: 3, accuracy: 4 },
    ),
    catalogEntry(
      'whisper-base',
      'speech',
      'Multilingual · Base',
      '99 languages, detected automatically. Fast on most laptops.',
      142,
      { multilingual: true, languages: '99 languages' },
    ),
    catalogEntry(
      'speaker-campplus-voxceleb',
      'speaker',
      'Speaker labels · CAM++',
      'Tells voices apart so captions show who is speaking. Fast enough for any laptop.',
      30,
      { recommended: true, installed: !fresh },
    ),
  ],
  installed: [],
};

function syncInstalled() {
  models = {
    ...models,
    installed: models.catalog
      .filter((m) => m.installed)
      .map((m) => ({
        id: m.id,
        kind: m.kind,
        name: m.name,
        path: m.path,
        sizeBytes: m.sizeMb * 1_000_000,
        fromCatalog: true,
      })),
  };
}
syncInstalled();

let settings: Settings = {
  speechModel: fresh ? null : 'whisper-base.en-q5_1',
  speakerModel: fresh ? null : 'speaker-campplus-voxceleb',
  speakerLabels: true,
  language: 'auto',
  translate: false,
  accuracy: 'balanced',
  threads: 0,
  microphoneEnabled: true,
  microphoneDevice: null,
  systemEnabled: true,
  systemDevice: null,
  processing: {
    partialResults: true,
    vadSensitivity: 0.5,
    speakerSeparation: 0.5,
    suppressMicEcho: true,
    endSilenceMs: 650,
    maxSegmentMs: 12000,
  },
  appearance: {
    theme: (params.get('theme') as Settings['appearance']['theme']) ?? 'system',
    fontScale: 1,
    fontFamily: 'hyperlegible',
    lineSpacing: 1.45,
    showTimestamps: params.has('times'),
    alwaysOnTop: false,
    compact: params.has('compact'),
  },
  onboardingDone: !fresh,
};

const SCRIPT: [AudioSource, number, string][] = [
  ['microphone', 1, 'Hi, thanks for fitting me in today.'],
  ['microphone', 2, 'Of course. How have the headaches been since we changed the dose?'],
  ['microphone', 1, 'Much better. Maybe one or two a week, and they are milder.'],
  [
    'system',
    3,
    'This is Priya from the pharmacy joining the call. I can confirm the refill is ready.',
  ],
  ['microphone', 2, "Great. Let's keep the same dose and check in again in six weeks."],
  ['microphone', 1, 'Perfect. Could you send the appointment details by text?'],
];

let running = false;
let timers: ReturnType<typeof setTimeout>[] = [];

function send(event: EngineEvent) {
  void emit('engine', event);
}

function playScript() {
  let t = 300;
  let clock = 2000;
  SCRIPT.forEach(([source, speaker, text], i) => {
    const id = i + 1;
    const words = text.split(' ');
    const startMs = clock;
    timers.push(setTimeout(() => send({ type: 'speechStarted', id, source, startMs }), t));
    t += 350;
    for (let w = 2; w < words.length; w += 3) {
      const partial = words.slice(0, w).join(' ');
      timers.push(
        setTimeout(() => send({ type: 'partial', id, source, text: partial, startMs }), t),
      );
      t += 280;
    }
    const caption: Caption = {
      id,
      source,
      speaker,
      text,
      startMs,
      endMs: startMs + words.length * 320,
      language: 'en',
    };
    timers.push(setTimeout(() => send({ type: 'final', caption }), t));
    t += 500;
    clock += words.length * 320 + 1400;
  });
  const meter = setInterval(() => {
    if (!running) return clearInterval(meter);
    send({ type: 'level', source: 'microphone', db: -32 + Math.random() * 14, speaking: true });
    send({ type: 'level', source: 'system', db: -48 + Math.random() * 8, speaking: false });
  }, 100);
  timers.push(setTimeout(() => send({ type: 'stats', realtimeFactor: 0.31, queued: 0 }), t));
}

export function installMockBackend() {
  mockWindows('main');
  mockIPC(
    (cmd, args) => {
      const a = (args ?? {}) as Record<string, unknown>;
      switch (cmd) {
        case 'app_info':
          return {
            build: {
              version: '0.1.0',
              buildNumber: '0',
              gitSha: 'dev',
              buildDate: '2026-11-04',
              os: 'linux',
              arch: 'x86_64',
              whisperCpp: '1.8.3',
            },
            appId: 'io.github.lobojoel2003.hushcap',
            modelsDir: models.dir,
            systemAudioSupported: true,
          };
        case 'get_settings':
          return settings;
        case 'save_settings':
          settings = a.settings as Settings;
          return settings;
        case 'list_devices':
          return {
            host: 'PulseAudio',
            systemCaptureNative: true,
            microphones: [
              { id: 'pulseaudio:mic', name: 'Built-in Microphone', kind: 'input', isDefault: true },
              { id: 'pulseaudio:usb', name: 'USB Headset', kind: 'input', isDefault: false },
            ],
            system: [
              {
                id: 'pulseaudio:speakers.monitor',
                name: 'Monitor of Built-in Speakers',
                kind: 'loopback',
                isDefault: true,
              },
            ],
          };
        case 'list_models':
          return models;
        case 'download_model': {
          const id = a.id as string;
          let done = 0;
          const total = (models.catalog.find((m) => m.id === id)?.sizeMb ?? 50) * 1_000_000;
          const tick = setInterval(() => {
            done = Math.min(total, done + total / 12);
            void emit('download', {
              id,
              downloaded: done,
              total,
              bytesPerSecond: 8_000_000,
              done: false,
              error: null,
            });
            if (done >= total) {
              clearInterval(tick);
              models = {
                ...models,
                catalog: models.catalog.map((m) => (m.id === id ? { ...m, installed: true } : m)),
              };
              syncInstalled();
              const kind = models.catalog.find((m) => m.id === id)?.kind;
              if (kind === 'speech' && !settings.speechModel) settings.speechModel = id;
              if (kind === 'speaker' && !settings.speakerModel) settings.speakerModel = id;
              void emit('download', { id, done: true, error: null });
            }
          }, 150);
          return null;
        }
        case 'start_captions':
          running = true;
          send({ type: 'state', state: 'running' });
          playScript();
          return ['Built-in Microphone', 'Monitor of Built-in Speakers'];
        case 'stop_captions':
          running = false;
          timers.forEach(clearTimeout);
          timers = [];
          send({ type: 'state', state: 'stopped' });
          return null;
        case 'is_running':
          return running;
        case 'render_transcript':
          return (a.captions as Caption[]).map((c) => c.text).join('\n');
        default:
          return null;
      }
    },
    { shouldMockEvents: true },
  );
  if (params.has('autostart')) {
    setTimeout(() => {
      document.querySelector<HTMLButtonElement>('.bar .big')?.click();
    }, 400);
  }
}
