<script lang="ts">
  import { ask, save } from '@tauri-apps/plugin-dialog';
  import { api, errorMessage } from '../api';
  import { app } from '../state.svelte';
  import type { ExportFormat } from '../types';
  import Icon from './Icon.svelte';

  let menuOpen = $state(false);
  const empty = $derived(app.transcript.captions.length === 0);
  const appearance = $derived(app.settings?.appearance);

  const FORMATS: { format: ExportFormat; label: string; ext: string }[] = [
    { format: 'txt', label: 'Plain text', ext: 'txt' },
    { format: 'srt', label: 'Subtitles (SRT)', ext: 'srt' },
    { format: 'vtt', label: 'Web subtitles (VTT)', ext: 'vtt' },
    { format: 'markdown', label: 'Markdown', ext: 'md' },
    { format: 'json', label: 'JSON', ext: 'json' },
  ];

  function stamp() {
    const d = new Date();
    const pad = (n: number) => String(n).padStart(2, '0');
    return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}-${pad(d.getHours())}${pad(d.getMinutes())}`;
  }

  const options = () => ({
    includeSpeakers: app.settings?.speakerLabels ?? true,
    includeTimestamps: true,
    title: `Hushcap transcript · ${new Date().toLocaleString()}`,
  });

  export async function saveAs(format: ExportFormat = 'txt') {
    menuOpen = false;
    if (empty) return;
    const f = FORMATS.find((x) => x.format === format)!;
    const path = await save({
      defaultPath: `hushcap-${stamp()}.${f.ext}`,
      filters: [{ name: f.label, extensions: [f.ext] }],
    });
    if (!path) return;
    try {
      await api.exportTranscript(
        path,
        $state.snapshot(app.transcript.captions),
        $state.snapshot(app.transcript.names),
        format,
        options(),
      );
      app.toast('info', 'Transcript saved.');
    } catch (e) {
      app.toast('error', `Could not save: ${errorMessage(e)}`);
    }
  }

  export async function copyAll() {
    if (empty) return;
    try {
      const text = await api.renderTranscript(
        $state.snapshot(app.transcript.captions),
        $state.snapshot(app.transcript.names),
        'txt',
        { ...options(), includeTimestamps: false },
      );
      await navigator.clipboard.writeText(text);
      app.toast('info', 'Transcript copied.');
    } catch (e) {
      app.toast('error', `Could not copy: ${errorMessage(e)}`);
    }
  }

  export async function clear() {
    if (empty) return;
    const ok = await ask('Clear the transcript? This cannot be undone.', {
      title: 'Clear transcript',
      kind: 'warning',
      okLabel: 'Clear',
      cancelLabel: 'Keep',
    });
    if (ok) app.clearTranscript();
  }

  function onWindowClick(e: MouseEvent) {
    if (menuOpen && !(e.target as HTMLElement).closest('.menu-wrap')) menuOpen = false;
  }
</script>

<svelte:window onclick={onWindowClick} />

<header class="header">
  <div class="brand">
    <svg class="logo" viewBox="0 0 64 64" aria-hidden="true">
      <rect x="2" y="2" width="60" height="60" rx="14" fill="#122039" />
      <path
        fill="#fff"
        d="M17 15h31a5.5 5.5 0 0 1 5.5 5.5v17A5.5 5.5 0 0 1 48 43H31.5l-8 7.2V43H17a5.5 5.5 0 0 1-5.5-5.5v-17A5.5 5.5 0 0 1 17 15z"
      />
      <rect x="17" y="22" width="22" height="4" rx="2" fill="#2a6fdb" />
      <rect x="17" y="30" width="15" height="4" rx="2" fill="#2a6fdb" />
      <rect x="38.5" y="32" width="13" height="10.5" rx="2.2" fill="#f5b841" />
      <path
        d="M41.3 32.5v-2.8a3.7 3.7 0 0 1 7.4 0v2.8"
        fill="none"
        stroke="#f5b841"
        stroke-width="2.4"
      />
    </svg>
    <h1>Hushcap</h1>
    <span class="badge" title="Speech recognition runs on this computer. No audio is uploaded.">
      <Icon name="lock" size={14} /> On this device · offline
    </span>
  </div>

  <div class="actions">
    <button
      class="icon-btn"
      onclick={copyAll}
      disabled={empty}
      title="Copy transcript (Ctrl+Shift+C)"
    >
      <Icon name="copy" label="Copy transcript" />
    </button>
    <div class="menu-wrap">
      <button
        class="icon-btn"
        aria-haspopup="menu"
        aria-expanded={menuOpen}
        disabled={empty}
        onclick={() => (menuOpen = !menuOpen)}
        title="Save transcript (Ctrl+S)"
      >
        <Icon name="save" label="Save transcript" />
      </button>
      {#if menuOpen}
        <div class="menu" role="menu">
          {#each FORMATS as f (f.format)}
            <button role="menuitem" onclick={() => saveAs(f.format)}>
              {f.label} <span class="ext">.{f.ext}</span>
            </button>
          {/each}
        </div>
      {/if}
    </div>
    <button class="icon-btn" onclick={clear} disabled={empty} title="Clear transcript (Ctrl+L)">
      <Icon name="trash" label="Clear transcript" />
    </button>
    <span class="sep"></span>
    <button
      class="icon-btn"
      aria-pressed={appearance?.alwaysOnTop ? 'true' : 'false'}
      onclick={() => app.update((s) => (s.appearance.alwaysOnTop = !s.appearance.alwaysOnTop))}
      title="Keep window on top (Ctrl+T)"
    >
      <Icon name="pin" label="Keep on top" />
    </button>
    <button
      class="icon-btn"
      onclick={() => app.update((s) => (s.appearance.compact = true))}
      title="Captions only (Ctrl+M)"
    >
      <Icon name="compact" label="Captions only" />
    </button>
    <button
      class="icon-btn"
      onclick={() => (app.settingsOpen = true)}
      title="Settings (Ctrl+,)"
      aria-keyshortcuts="Control+Comma"
    >
      <Icon name="sliders" label="Settings" />
    </button>
  </div>
</header>

<style>
  .header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    padding: 8px 12px 8px 16px;
    border-bottom: 1px solid var(--border);
    background: var(--surface);
  }

  .brand {
    display: flex;
    align-items: center;
    gap: 10px;
    min-width: 0;
  }

  .logo {
    width: 28px;
    height: 28px;
    flex: none;
  }

  h1 {
    margin: 0;
    font-size: 18px;
    letter-spacing: -0.01em;
  }

  .badge {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 4px 10px;
    border-radius: 999px;
    background: var(--surface-2);
    color: var(--muted);
    font-size: 12.5px;
    font-weight: 600;
    white-space: nowrap;
  }

  .actions {
    display: flex;
    align-items: center;
    gap: 2px;
  }

  .sep {
    width: 1px;
    height: 22px;
    background: var(--border);
    margin: 0 6px;
  }

  .menu-wrap {
    position: relative;
  }

  .menu {
    position: absolute;
    right: 0;
    top: calc(100% + 6px);
    z-index: 20;
    display: grid;
    min-width: 210px;
    padding: 6px;
    border-radius: var(--radius);
    border: 1px solid var(--border);
    background: var(--surface);
    box-shadow: var(--shadow);
  }

  .menu button {
    display: flex;
    justify-content: space-between;
    gap: 16px;
    padding: 9px 10px;
    border: 0;
    border-radius: 6px;
    background: none;
    text-align: left;
  }

  .menu button:hover {
    background: var(--surface-2);
  }

  .ext {
    color: var(--muted);
    font-family: ui-monospace, monospace;
    font-size: 12px;
  }

  @media (max-width: 620px) {
    .badge {
      display: none;
    }
  }
</style>
