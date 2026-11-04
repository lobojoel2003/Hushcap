<script lang="ts">
  import { onMount } from 'svelte';
  import { api } from '../api';
  import { app } from '../state.svelte';
  import { LANGUAGES, speakerColor, speakerName } from '../format';
  import { speakersInTranscript } from '../transcript';
  import type { Accuracy, FontFamily, Settings, Theme } from '../types';
  import Icon, { type IconName } from './Icon.svelte';
  import ModelList from './ModelList.svelte';

  type Tab = typeof app.settingsTab;
  const TABS: { id: Tab; label: string; icon: IconName }[] = [
    { id: 'captions', label: 'Captions', icon: 'type' },
    { id: 'audio', label: 'Audio', icon: 'mic' },
    { id: 'speech', label: 'Speech', icon: 'globe' },
    { id: 'speakers', label: 'Speakers', icon: 'users' },
    { id: 'models', label: 'Models', icon: 'download' },
    { id: 'about', label: 'About', icon: 'info' },
  ];

  const s = $derived(app.settings!);
  const info = $derived(app.info);
  let panel: HTMLElement | undefined = $state();

  const speechModel = $derived(
    app.models?.catalog.find((m) => m.id === s.speechModel) ??
      app.models?.installed.find((m) => m.id === s.speechModel),
  );
  const englishOnly = $derived(
    !!speechModel && 'multilingual' in speechModel && !speechModel.multilingual,
  );
  const installedSpeech = $derived(
    (app.models?.installed ?? []).filter((m) => m.kind === 'speech'),
  );
  const installedSpeaker = $derived(
    (app.models?.installed ?? []).filter((m) => m.kind === 'speaker'),
  );
  const speakers = $derived(speakersInTranscript(app.transcript));

  function set(change: (s: Settings) => void, affectsEngine = false) {
    void app.update(change, affectsEngine);
  }

  function close() {
    app.settingsOpen = false;
  }

  function onKey(e: KeyboardEvent) {
    if (e.key === 'Escape') {
      e.stopPropagation();
      close();
    }
  }

  function onTabKey(e: KeyboardEvent) {
    const i = TABS.findIndex((t) => t.id === app.settingsTab);
    const next =
      e.key === 'ArrowDown' || e.key === 'ArrowRight'
        ? 1
        : e.key === 'ArrowUp' || e.key === 'ArrowLeft'
          ? -1
          : 0;
    if (!next) return;
    e.preventDefault();
    const t = TABS[(i + next + TABS.length) % TABS.length]!;
    app.settingsTab = t.id;
    panel?.querySelector<HTMLButtonElement>(`#tab-${t.id}`)?.focus();
  }

  onMount(() => {
    panel?.querySelector<HTMLButtonElement>(`#tab-${app.settingsTab}`)?.focus();
    void app.refreshDevices();
  });

  const THEMES: { id: Theme; label: string }[] = [
    { id: 'system', label: 'Match system' },
    { id: 'light', label: 'Light' },
    { id: 'dark', label: 'Dark' },
    { id: 'contrast', label: 'High contrast (yellow on black)' },
  ];
  const FONTS: { id: FontFamily; label: string }[] = [
    { id: 'hyperlegible', label: 'Atkinson Hyperlegible (designed for low vision)' },
    { id: 'system', label: 'System font' },
    { id: 'dyslexic', label: 'OpenDyslexic' },
    { id: 'mono', label: 'Monospace' },
  ];
  const ACCURACY: { id: Accuracy; label: string; hint: string }[] = [
    { id: 'fast', label: 'Fast', hint: 'Lowest delay. Best for slower computers.' },
    { id: 'balanced', label: 'Balanced', hint: 'Retries uncertain phrases. A good default.' },
    { id: 'accurate', label: 'Accurate', hint: 'Beam search. More CPU, slightly more delay.' },
  ];
</script>

<div class="backdrop" onclick={close} aria-hidden="true"></div>
<div
  class="panel"
  role="dialog"
  aria-modal="true"
  aria-labelledby="settings-title"
  tabindex="-1"
  bind:this={panel}
  onkeydown={onKey}
>
  <div class="top">
    <h2 id="settings-title">Settings</h2>
    <button class="icon-btn" onclick={close}><Icon name="x" label="Close settings" /></button>
  </div>

  <div class="body">
    <div class="tabs" role="tablist" aria-orientation="vertical" tabindex="-1" onkeydown={onTabKey}>
      {#each TABS as t (t.id)}
        <button
          id="tab-{t.id}"
          role="tab"
          aria-selected={app.settingsTab === t.id}
          aria-controls="tabpanel"
          tabindex={app.settingsTab === t.id ? 0 : -1}
          onclick={() => (app.settingsTab = t.id)}
        >
          <Icon name={t.icon} size={18} />
          {t.label}
        </button>
      {/each}
    </div>

    <div class="content" id="tabpanel" role="tabpanel" aria-labelledby="tab-{app.settingsTab}">
      {#if app.settingsTab === 'captions'}
        <fieldset class="field">
          <legend class="label">Theme</legend>
          {#each THEMES as t (t.id)}
            <label class="choice">
              <input
                type="radio"
                name="theme"
                checked={s.appearance.theme === t.id}
                onchange={() => set((x) => (x.appearance.theme = t.id))}
              />
              {t.label}
            </label>
          {/each}
        </fieldset>

        <label class="field">
          <span>Caption font</span>
          <select
            value={s.appearance.fontFamily}
            onchange={(e) =>
              set((x) => (x.appearance.fontFamily = e.currentTarget.value as FontFamily))}
          >
            {#each FONTS as f (f.id)}<option value={f.id}>{f.label}</option>{/each}
          </select>
        </label>

        <label class="field">
          <span>Text size · {Math.round(s.appearance.fontScale * 100)}%</span>
          <input
            type="range"
            min="0.6"
            max="4"
            step="0.05"
            value={s.appearance.fontScale}
            oninput={(e) => set((x) => (x.appearance.fontScale = Number(e.currentTarget.value)))}
          />
          <span class="hint">Shortcut: Ctrl + and Ctrl − (Ctrl 0 resets).</span>
        </label>

        <label class="field">
          <span>Line spacing · {s.appearance.lineSpacing.toFixed(2)}</span>
          <input
            type="range"
            min="1"
            max="2.5"
            step="0.05"
            value={s.appearance.lineSpacing}
            oninput={(e) => set((x) => (x.appearance.lineSpacing = Number(e.currentTarget.value)))}
          />
        </label>

        <div class="preview" aria-hidden="true">
          <div class="who" style:color={speakerColor(1, app.dark)}>SPEAKER 1</div>
          <p>The appointment moved to Thursday at three o'clock.</p>
        </div>

        <label class="switch-row">
          <input
            type="checkbox"
            checked={s.appearance.showTimestamps}
            onchange={(e) => set((x) => (x.appearance.showTimestamps = e.currentTarget.checked))}
          />
          <span><span class="label">Show timestamps</span></span>
        </label>
        <label class="switch-row">
          <input
            type="checkbox"
            checked={s.appearance.alwaysOnTop}
            onchange={(e) => set((x) => (x.appearance.alwaysOnTop = e.currentTarget.checked))}
          />
          <span>
            <span class="label">Keep window on top</span>
            <span class="hint">Float captions over a video call or film.</span>
          </span>
        </label>
        <label class="switch-row">
          <input
            type="checkbox"
            checked={s.processing.partialResults}
            onchange={(e) =>
              set((x) => (x.processing.partialResults = e.currentTarget.checked), true)}
          />
          <span>
            <span class="label">Show words as they are spoken</span>
            <span class="hint"
              >Grey, provisional text that is corrected when the phrase ends. Uses more CPU.</span
            >
          </span>
        </label>
      {:else if app.settingsTab === 'audio'}
        {@const devices = app.devices}
        <label class="field">
          <span>Microphone</span>
          <select
            value={s.microphoneDevice ?? ''}
            onchange={(e) => set((x) => (x.microphoneDevice = e.currentTarget.value || null), true)}
          >
            <option value="">Default device</option>
            {#each devices?.microphones ?? [] as d (d.id)}
              <option value={d.id}>{d.name}{d.isDefault ? ' (default)' : ''}</option>
            {/each}
          </select>
        </label>

        <label class="field">
          <span>System audio</span>
          <select
            value={s.systemDevice ?? ''}
            onchange={(e) => set((x) => (x.systemDevice = e.currentTarget.value || null), true)}
          >
            <option value="">Default output</option>
            {#each devices?.system ?? [] as d (d.id)}
              <option value={d.id}>{d.name}{d.isDefault ? ' (default)' : ''}</option>
            {/each}
          </select>
          {#if devices && !devices.systemCaptureNative}
            <span class="hint">
              This system can't capture other apps' audio directly. Install a virtual loopback
              device (for example BlackHole on macOS) and choose it here.
            </span>
          {:else}
            <span class="hint">
              Captions what this computer is playing: calls, videos, podcasts. Audio is read locally{devices
                ? ` through ${devices.host}`
                : ''}.
            </span>
          {/if}
        </label>
        <button class="btn" onclick={() => app.refreshDevices()}>
          <Icon name="refresh" size={16} /> Refresh devices
        </button>

        <label class="field">
          <span>Voice sensitivity · {Math.round(s.processing.vadSensitivity * 100)}%</span>
          <input
            type="range"
            min="0"
            max="1"
            step="0.05"
            value={s.processing.vadSensitivity}
            onchange={(e) =>
              set((x) => (x.processing.vadSensitivity = Number(e.currentTarget.value)), true)}
          />
          <span class="hint">Raise it for quiet or distant voices; lower it in noisy places.</span>
        </label>

        <label class="field">
          <span
            >Pause before a caption ends · {(s.processing.endSilenceMs / 1000).toFixed(2)} s</span
          >
          <input
            type="range"
            min="300"
            max="1500"
            step="50"
            value={s.processing.endSilenceMs}
            onchange={(e) =>
              set((x) => (x.processing.endSilenceMs = Number(e.currentTarget.value)), true)}
          />
        </label>

        <label class="switch-row">
          <input
            type="checkbox"
            checked={s.processing.suppressMicEcho}
            onchange={(e) =>
              set((x) => (x.processing.suppressMicEcho = e.currentTarget.checked), true)}
          />
          <span>
            <span class="label">Ignore the microphone while system audio is talking</span>
            <span class="hint"
              >Prevents double captions when a call plays through speakers instead of headphones.</span
            >
          </span>
        </label>
      {:else if app.settingsTab === 'speech'}
        <label class="field">
          <span>Speech model</span>
          <select
            value={s.speechModel ?? ''}
            onchange={(e) => set((x) => (x.speechModel = e.currentTarget.value || null), true)}
          >
            {#if !s.speechModel}<option value="">Choose a model…</option>{/if}
            {#each installedSpeech as m (m.id)}<option value={m.id}>{m.name}</option>{/each}
          </select>
          {#if installedSpeech.length === 0}
            <span class="hint">No speech model yet — get one in the Models tab.</span>
          {/if}
        </label>

        <label class="field">
          <span>Spoken language</span>
          <select
            value={englishOnly ? 'en' : s.language}
            disabled={englishOnly}
            onchange={(e) => set((x) => (x.language = e.currentTarget.value), true)}
          >
            {#each LANGUAGES as [code, name] (code)}<option value={code}>{name}</option>{/each}
          </select>
          {#if englishOnly}
            <span class="hint"
              >This model understands English only. Choose a multilingual model for other languages.</span
            >
          {:else}
            <span class="hint"
              >Choosing the language is faster and more accurate than detection.</span
            >
          {/if}
        </label>

        <label class="switch-row">
          <input
            type="checkbox"
            checked={s.translate}
            disabled={englishOnly}
            onchange={(e) => set((x) => (x.translate = e.currentTarget.checked), true)}
          />
          <span>
            <span class="label">Translate into English</span>
            <span class="hint">Show English captions for speech in other languages.</span>
          </span>
        </label>

        <fieldset class="field">
          <legend class="label">Speed and accuracy</legend>
          {#each ACCURACY as a (a.id)}
            <label class="choice">
              <input
                type="radio"
                name="accuracy"
                checked={s.accuracy === a.id}
                onchange={() => set((x) => (x.accuracy = a.id), true)}
              />
              <span><strong>{a.label}</strong> — <span class="hint">{a.hint}</span></span>
            </label>
          {/each}
        </fieldset>

        <label class="field">
          <span>CPU threads</span>
          <input
            type="number"
            min="0"
            max="32"
            value={s.threads}
            onchange={(e) =>
              set((x) => (x.threads = Math.max(0, Number(e.currentTarget.value) || 0)), true)}
          />
          <span class="hint">0 chooses automatically.</span>
        </label>
      {:else if app.settingsTab === 'speakers'}
        <label class="switch-row">
          <input
            type="checkbox"
            checked={s.speakerLabels}
            onchange={(e) => set((x) => (x.speakerLabels = e.currentTarget.checked), true)}
          />
          <span>
            <span class="label">Label who is speaking</span>
            <span class="hint"
              >Tells voices apart on this computer. No limit on the number of speakers.</span
            >
          </span>
        </label>

        {#if s.speakerLabels}
          <label class="field">
            <span>Speaker model</span>
            <select
              value={s.speakerModel ?? ''}
              onchange={(e) => set((x) => (x.speakerModel = e.currentTarget.value || null), true)}
            >
              <option value="">Automatic</option>
              {#each installedSpeaker as m (m.id)}<option value={m.id}>{m.name}</option>{/each}
            </select>
            {#if installedSpeaker.length === 0}
              <span class="hint">Download a speaker model in the Models tab to turn labels on.</span
              >
            {/if}
          </label>

          <label class="field">
            <span>Speaker separation · {Math.round(s.processing.speakerSeparation * 100)}%</span>
            <input
              type="range"
              min="0"
              max="1"
              step="0.05"
              value={s.processing.speakerSeparation}
              onchange={(e) =>
                set((x) => (x.processing.speakerSeparation = Number(e.currentTarget.value)), true)}
            />
            <span class="hint">
              Lower if one person keeps getting split into two speakers; higher if different people
              share a label.
            </span>
          </label>
        {/if}

        <h3>People in this transcript</h3>
        {#if speakers.length === 0}
          <p class="hint">
            Speakers appear here once they talk. You can also click a name in the captions to rename
            it.
          </p>
        {:else}
          <ul class="people">
            {#each speakers as sp (sp)}
              <li>
                <span class="swatch" style:background={speakerColor(sp, app.dark)}></span>
                <label class="visually-hidden" for="name-{sp}">Name for speaker {sp}</label>
                <input
                  id="name-{sp}"
                  type="text"
                  placeholder="Speaker {sp}"
                  value={app.transcript.names[sp] ?? ''}
                  onchange={(e) => app.rename(sp, e.currentTarget.value)}
                />
                <span class="hint">{speakerName(sp, {})}</span>
              </li>
            {/each}
          </ul>
        {/if}
      {:else if app.settingsTab === 'models'}
        <ModelList />
      {:else if app.settingsTab === 'about'}
        <div class="about">
          <h3>Hushcap {info?.build.version}</h3>
          <p class="hint">
            Build {info?.build.buildNumber} · {info?.build.gitSha} · {info?.build.buildDate} ·
            {info?.build.os}/{info?.build.arch}
            {#if info?.build.whisperCpp}· whisper.cpp {info.build.whisperCpp}{/if}
          </p>
          <p>
            <strong>Private by design.</strong> Speech recognition and speaker labelling run on this computer.
            Audio and transcripts are never uploaded. There are no accounts, analytics, ads or update
            checks. The only network request Hushcap ever makes is a model download you start yourself.
          </p>
          <p>Transcripts stay in memory until you save them, clear them or close the app.</p>
          <h4>Keyboard shortcuts</h4>
          <dl class="keys">
            <dt>Ctrl + Enter</dt>
            <dd>Start or stop captions</dd>
            <dt>Ctrl + ,</dt>
            <dd>Settings</dd>
            <dt>Ctrl + S</dt>
            <dd>Save transcript</dd>
            <dt>Ctrl + Shift + C</dt>
            <dd>Copy transcript</dd>
            <dt>Ctrl + L</dt>
            <dd>Clear transcript</dd>
            <dt>Ctrl + = / − / 0</dt>
            <dd>Bigger, smaller, reset text</dd>
            <dt>Ctrl + T</dt>
            <dd>Keep window on top</dd>
            <dt>Ctrl + M</dt>
            <dd>Captions-only view (Esc to leave)</dd>
          </dl>
          <p class="hint">
            Free software under the GNU GPL v3 or later. Built with whisper.cpp (MIT), tract
            (MIT/Apache 2.0), cpal (Apache 2.0), Tauri (MIT/Apache 2.0) and Svelte (MIT). Fonts:
            Atkinson Hyperlegible Next and OpenDyslexic (SIL OFL 1.1).
          </p>
          <button class="btn" onclick={() => api.openProjectPage()}>
            <Icon name="external" size={16} /> Project page, help and source code
          </button>
        </div>
      {/if}
    </div>
  </div>
</div>

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    background: rgb(0 0 0 / 0.35);
    z-index: 30;
  }

  .panel {
    position: fixed;
    top: 0;
    right: 0;
    bottom: 0;
    z-index: 31;
    width: min(760px, 100vw);
    display: flex;
    flex-direction: column;
    background: var(--bg);
    border-left: 1px solid var(--border);
    box-shadow: var(--shadow);
  }

  .top {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 10px 12px 10px 20px;
    border-bottom: 1px solid var(--border);
  }

  h2 {
    margin: 0;
    font-size: 18px;
  }

  .body {
    flex: 1;
    min-height: 0;
    display: grid;
    grid-template-columns: 180px 1fr;
  }

  .tabs {
    display: flex;
    flex-direction: column;
    gap: 2px;
    padding: 12px 8px;
    border-right: 1px solid var(--border);
  }

  .tabs button {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 9px 12px;
    border: 0;
    border-radius: var(--radius-sm);
    background: none;
    text-align: left;
    font-weight: 600;
    color: var(--muted);
  }

  .tabs button[aria-selected='true'] {
    background: var(--accent-soft);
    color: var(--text);
  }

  .content {
    overflow-y: auto;
    padding: 8px 24px 32px;
  }

  fieldset {
    border: 0;
    padding: 0;
  }

  legend {
    margin-bottom: 4px;
  }

  .choice {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 5px 0;
  }

  .preview {
    padding: 14px 16px;
    margin: 6px 0 10px;
    border: 1px dashed var(--border);
    border-radius: var(--radius);
    background: var(--surface);
  }

  .preview .who {
    font-weight: 800;
    font-size: 13px;
    letter-spacing: 0.03em;
  }

  .preview p {
    margin: 2px 0 0;
    font-family: var(--caption-font);
    font-size: calc(28px * var(--font-scale));
    line-height: var(--line-spacing);
    color: var(--caption);
  }

  h3 {
    margin: 22px 0 8px;
    font-size: 15px;
  }

  .people {
    list-style: none;
    padding: 0;
    margin: 0;
    display: grid;
    gap: 8px;
  }

  .people li {
    display: flex;
    align-items: center;
    gap: 10px;
  }

  .swatch {
    width: 14px;
    height: 14px;
    border-radius: 4px;
    flex: none;
  }

  .about p {
    line-height: 1.55;
  }

  .keys {
    display: grid;
    grid-template-columns: max-content 1fr;
    gap: 6px 16px;
  }

  .keys dt {
    font-family: ui-monospace, monospace;
    font-size: 13px;
    color: var(--muted);
  }

  .keys dd {
    margin: 0;
  }

  @media (max-width: 640px) {
    .body {
      grid-template-columns: 1fr;
      grid-template-rows: auto 1fr;
    }
    .tabs {
      flex-direction: row;
      overflow-x: auto;
      border-right: 0;
      border-bottom: 1px solid var(--border);
    }
  }
</style>
