<script lang="ts">
  import { app } from '../state.svelte';
  import Icon from './Icon.svelte';
  import ModelList from './ModelList.svelte';

  const catalog = $derived(app.models?.catalog ?? []);
  const speech = $derived(catalog.find((m) => m.kind === 'speech' && m.recommended));
  const speaker = $derived(catalog.find((m) => m.kind === 'speaker' && m.recommended));
  let withSpeakers = $state(true);
  let showAll = $state(false);
  const downloading = $derived(Object.values(app.downloads).some((d) => !d.done));
  const total = $derived((speech?.sizeMb ?? 0) + (withSpeakers ? (speaker?.sizeMb ?? 0) : 0));

  async function getStarted() {
    if (speech && !speech.installed) await app.download(speech.id);
    if (withSpeakers && speaker && !speaker.installed) await app.download(speaker.id);
  }

  async function finish() {
    await app.update((s) => (s.onboardingDone = true));
  }
</script>

<div class="backdrop"></div>
<div class="dialog" role="dialog" aria-modal="true" aria-labelledby="welcome-title">
  <div class="hero">
    <h2 id="welcome-title">Live captions that stay on your computer</h2>
    <ul class="promises">
      <li>
        <Icon name="lock" size={18} /> Speech is recognised on this device. Nothing is uploaded.
      </li>
      <li><Icon name="wave" size={18} /> Works offline: on a bus, in a clinic, anywhere.</li>
      <li>
        <Icon name="volume" size={18} /> Captions your microphone and what your computer plays.
      </li>
      <li>
        <Icon name="users" size={18} /> Shows who is speaking, for as many people as are talking.
      </li>
    </ul>
  </div>

  <div class="step">
    {#if !showAll}
      <p>
        Hushcap needs a speech model once (about {total} MB). Downloading it is the only time Hushcap
        uses the internet.
      </p>
      {#if speech}
        <div class="choice">
          <strong>{speech.name}</strong>
          <span class="hint">{speech.description}</span>
        </div>
      {/if}
      {#if speaker}
        <label class="switch-row">
          <input type="checkbox" bind:checked={withSpeakers} />
          <span>
            <span class="label">Also label who is speaking (+{speaker.sizeMb} MB)</span>
            <span class="hint">{speaker.description}</span>
          </span>
        </label>
      {/if}
      {#each Object.values(app.downloads) as d (d.id)}
        {@const m = catalog.find((x) => x.id === d.id)}
        {#if m && !d.done}
          {@const total = d.total ?? m.sizeMb * 1_000_000}
          <div class="progress-row">
            <span>{m.name}</span>
            <div
              class="progress"
              role="progressbar"
              aria-label="Downloading {m.name}"
              aria-valuemin={0}
              aria-valuemax={100}
              aria-valuenow={Math.round(((d.downloaded ?? 0) / total) * 100)}
            >
              <div
                class="fill"
                style:width="{Math.min(100, ((d.downloaded ?? 0) / total) * 100)}%"
              ></div>
            </div>
          </div>
        {/if}
      {/each}
    {:else}
      <ModelList />
    {/if}
  </div>

  <div class="actions">
    <button class="btn ghost" onclick={() => (showAll = !showAll)}>
      {showAll ? 'Back' : 'Other languages, or import a file…'}
    </button>
    {#if app.hasSpeechModel}
      <button class="btn primary" onclick={finish}
        ><Icon name="check" size={18} /> Start using Hushcap</button
      >
    {:else if !showAll}
      <button class="btn primary" onclick={getStarted} disabled={downloading}>
        <Icon name="download" size={18} />
        {downloading ? 'Downloading…' : `Download ${total} MB and continue`}
      </button>
    {/if}
  </div>
</div>

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    background: rgb(0 0 0 / 0.45);
    z-index: 40;
  }

  .dialog {
    position: fixed;
    z-index: 41;
    inset: 0;
    margin: auto;
    width: min(680px, calc(100vw - 32px));
    height: fit-content;
    max-height: calc(100vh - 32px);
    overflow-y: auto;
    border-radius: 18px;
    background: var(--bg);
    border: 1px solid var(--border);
    box-shadow: var(--shadow);
  }

  .hero {
    padding: 28px 28px 18px;
    background: var(--surface);
    border-bottom: 1px solid var(--border);
  }

  h2 {
    margin: 0 0 14px;
    font-size: 24px;
    line-height: 1.25;
  }

  .promises {
    list-style: none;
    margin: 0;
    padding: 0;
    display: grid;
    gap: 9px;
  }

  .promises li {
    display: flex;
    gap: 10px;
    align-items: center;
    line-height: 1.35;
  }

  .promises :global(svg) {
    color: var(--accent);
    flex: none;
  }

  .step {
    padding: 8px 28px 4px;
    line-height: 1.5;
  }

  .choice {
    display: grid;
    gap: 2px;
    padding: 12px 14px;
    border: 1px solid var(--accent);
    background: var(--accent-soft);
    border-radius: var(--radius);
  }

  .progress-row {
    display: grid;
    gap: 6px;
    margin: 10px 0;
  }

  .progress {
    height: 8px;
    border-radius: 4px;
    background: var(--surface-2);
    overflow: hidden;
  }

  .fill {
    height: 100%;
    background: var(--accent);
    transition: width 0.2s;
  }

  .actions {
    display: flex;
    justify-content: space-between;
    gap: 10px;
    padding: 16px 28px 24px;
  }
</style>
