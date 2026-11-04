<script lang="ts">
  import { app } from '../state.svelte';
  import { levelFraction } from '../format';
  import type { AudioDevice, AudioSource } from '../types';
  import Icon from './Icon.svelte';

  const s = $derived(app.settings);
  const devices = $derived(app.devices);
  const locked = $derived(app.running || app.busy);

  const status = $derived.by(() => {
    if (app.engine === 'starting') return { text: 'Starting…', tone: 'muted' };
    if (app.engine === 'stopping') return { text: 'Finishing the last phrase…', tone: 'muted' };
    if (!app.running) return { text: 'Not listening', tone: 'muted' };
    const st = app.stats;
    if (st && (st.queued > 2 || st.realtimeFactor > 1.2)) {
      return { text: 'Falling behind — try a faster model', tone: 'warning' };
    }
    return { text: 'Listening · on this device', tone: 'ok' };
  });

  function deviceList(source: AudioSource): AudioDevice[] {
    if (!devices) return [];
    return source === 'microphone' ? devices.microphones : devices.system;
  }

  async function toggleSource(source: AudioSource) {
    await app.update((st) => {
      if (source === 'microphone') st.microphoneEnabled = !st.microphoneEnabled;
      else st.systemEnabled = !st.systemEnabled;
    });
  }

  async function chooseDevice(source: AudioSource, value: string) {
    await app.update((st) => {
      const id = value === '' ? null : value;
      if (source === 'microphone') st.microphoneDevice = id;
      else st.systemDevice = id;
    });
  }
</script>

{#snippet sourceControl(source: AudioSource, label: string, icon: 'mic' | 'volume')}
  {@const enabled = source === 'microphone' ? s?.microphoneEnabled : s?.systemEnabled}
  {@const selected = (source === 'microphone' ? s?.microphoneDevice : s?.systemDevice) ?? ''}
  {@const level = app.levels[source]}
  <div class="source" class:on={enabled}>
    <button
      class="toggle"
      aria-pressed={enabled ? 'true' : 'false'}
      disabled={locked}
      onclick={() => toggleSource(source)}
      title={locked ? 'Stop captions to change sources' : `Caption ${label.toLowerCase()}`}
    >
      <Icon name={icon} size={18} />
      <span>{label}</span>
      <span class="check" aria-hidden="true">{enabled ? 'On' : 'Off'}</span>
    </button>
    <label class="visually-hidden" for="device-{source}">{label} device</label>
    <select
      id="device-{source}"
      value={selected}
      disabled={locked || !enabled}
      onchange={(e) => chooseDevice(source, e.currentTarget.value)}
    >
      <option value="">Default device</option>
      {#each deviceList(source) as d (d.id)}
        <option value={d.id}>{d.name}{d.isDefault ? ' (default)' : ''}</option>
      {/each}
    </select>
    <div
      class="meter"
      class:speaking={level.speaking && app.running}
      role="meter"
      aria-label="{label} level"
      aria-valuemin={0}
      aria-valuemax={100}
      aria-valuenow={Math.round(levelFraction(app.running && enabled ? level.db : -100) * 100)}
    >
      <div
        class="fill"
        style:width="{levelFraction(app.running && enabled ? level.db : -100) * 100}%"
      ></div>
    </div>
  </div>
{/snippet}

<footer class="bar">
  <div class="sources">
    {@render sourceControl('microphone', 'Microphone', 'mic')}
    {@render sourceControl('system', 'System audio', 'volume')}
  </div>

  <div class="right">
    {#if app.pendingRestart}
      <button class="btn" onclick={() => app.restartIfRunning()} title="Apply new settings">
        <Icon name="refresh" size={16} /> Apply changes
      </button>
    {/if}
    <span class="status {status.tone}" role="status">
      <span class="dot"></span>{status.text}
    </span>
    <button
      class="btn big {app.running ? 'danger' : 'primary'}"
      onclick={() => app.toggle()}
      disabled={app.busy || (!app.running && !app.hasSpeechModel)}
      aria-keyshortcuts="Control+Enter"
    >
      <Icon name={app.running ? 'stop' : 'play'} size={18} />
      {app.running ? 'Stop' : 'Start captions'}
    </button>
  </div>
</footer>

<style>
  .bar {
    display: flex;
    align-items: center;
    justify-content: space-between;
    flex-wrap: wrap;
    gap: 12px 20px;
    padding: 12px 18px;
    border-top: 1px solid var(--border);
    background: var(--surface);
  }

  .sources {
    display: flex;
    flex-wrap: wrap;
    gap: 12px;
  }

  .source {
    display: grid;
    grid-template-columns: auto minmax(0, 170px);
    grid-template-rows: auto 6px;
    gap: 6px 8px;
    align-items: center;
  }

  .toggle {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    min-height: 38px;
    padding: 0 10px;
    border-radius: var(--radius-sm);
    border: 1px solid var(--border);
    background: var(--surface);
    font-weight: 600;
  }

  /* Locked while captions run, but the state must stay readable. */
  .toggle:disabled {
    opacity: 0.9;
    cursor: default;
  }

  .toggle[aria-pressed='true'] {
    background: var(--accent-soft);
    border-color: var(--accent);
    color: var(--text);
  }

  .toggle .check {
    font-size: 12px;
    font-weight: 700;
    color: var(--muted);
    padding-left: 6px;
    border-left: 1px solid var(--border);
  }

  .toggle[aria-pressed='true'] .check {
    color: var(--accent);
  }

  select {
    width: 100%;
    min-width: 0;
  }

  .meter {
    grid-column: 1 / -1;
    height: 6px;
    border-radius: 3px;
    background: var(--surface-2);
    overflow: hidden;
  }

  .meter .fill {
    height: 100%;
    background: var(--muted);
    transition: width 90ms linear;
  }

  .meter.speaking .fill {
    background: var(--ok);
  }

  .right {
    display: flex;
    align-items: center;
    gap: 14px;
    margin-left: auto;
  }

  .status {
    white-space: nowrap;
    display: inline-flex;
    align-items: center;
    gap: 8px;
    color: var(--muted);
    font-size: 14px;
  }

  .status .dot {
    width: 9px;
    height: 9px;
    border-radius: 50%;
    background: currentColor;
  }

  .status.ok {
    color: var(--ok);
  }

  .status.warning {
    color: var(--warning);
  }

  .big {
    min-height: 46px;
    padding: 0 22px;
    font-size: 16px;
    border-radius: 12px;
  }

  @media (max-width: 760px) {
    .source {
      grid-template-columns: auto minmax(0, 1fr);
    }
    .right {
      width: 100%;
      justify-content: space-between;
    }
  }
</style>
