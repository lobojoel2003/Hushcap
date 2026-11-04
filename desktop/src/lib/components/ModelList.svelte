<script lang="ts">
  import { open } from '@tauri-apps/plugin-dialog';
  import { api, errorMessage } from '../api';
  import { app } from '../state.svelte';
  import { megabytes } from '../format';
  import type { ModelKind, ModelStatus } from '../types';
  import Icon from './Icon.svelte';

  let { kinds = ['speech', 'speaker'] }: { kinds?: ModelKind[] } = $props();

  const catalog = $derived(app.models?.catalog ?? []);
  const imported = $derived((app.models?.installed ?? []).filter((m) => !m.fromCatalog));

  function inUse(m: { id: string; kind: ModelKind }) {
    const s = app.settings;
    if (!s) return false;
    return m.kind === 'speech' ? s.speechModel === m.id : s.speakerModel === m.id;
  }

  async function use(m: { id: string; kind: ModelKind }) {
    await app.update((s) => {
      if (m.kind === 'speech') s.speechModel = m.id;
      else s.speakerModel = m.id;
    }, true);
  }

  async function importFile() {
    const path = await open({
      multiple: false,
      directory: false,
      filters: [{ name: 'Models', extensions: ['bin', 'onnx'] }],
    });
    if (typeof path === 'string') await app.importModel(path);
  }

  async function openFolder() {
    try {
      await api.openModelsFolder();
    } catch (e) {
      app.toast('error', errorMessage(e));
    }
  }

  function dots(n: number) {
    return '●'.repeat(n) + '○'.repeat(5 - n);
  }

  function progressOf(m: ModelStatus) {
    const d = app.downloads[m.id];
    if (!d || d.done) return null;
    const total = d.total ?? m.sizeBytes ?? m.sizeMb * 1_000_000;
    return { fraction: total ? (d.downloaded ?? 0) / total : 0, d };
  }
</script>

{#each kinds as kind (kind)}
  <section class="group">
    <h3>{kind === 'speech' ? 'Speech models' : 'Speaker label models'}</h3>
    <ul>
      {#each catalog.filter((m) => m.kind === kind) as m (m.id)}
        {@const p = progressOf(m)}
        <li class:active={inUse(m)}>
          <div class="info">
            <div class="title">
              <strong>{m.name}</strong>
              {#if m.recommended}<span class="tag">Recommended</span>{/if}
              {#if inUse(m)}<span class="tag on"><Icon name="check" size={12} /> In use</span>{/if}
            </div>
            <div class="desc">{m.description}</div>
            <div class="facts">
              <span>{m.sizeMb} MB</span>
              <span>{m.languages}</span>
              {#if kind === 'speech'}
                <span title="Speed">Speed <span class="dots">{dots(m.speed)}</span></span>
                <span title="Accuracy">Accuracy <span class="dots">{dots(m.accuracy)}</span></span>
              {/if}
            </div>
            {#if p}
              <div
                class="progress"
                role="progressbar"
                aria-label="Downloading {m.name}"
                aria-valuemin={0}
                aria-valuemax={100}
                aria-valuenow={Math.round(p.fraction * 100)}
              >
                <div class="fill" style:width="{Math.min(100, p.fraction * 100)}%"></div>
              </div>
              <div class="hint">
                {megabytes(p.d.downloaded ?? 0)}
                {#if p.d.bytesPerSecond}· {megabytes(p.d.bytesPerSecond)}/s{/if}
              </div>
            {/if}
          </div>
          <div class="buttons">
            {#if p}
              <button class="btn" onclick={() => app.cancelDownload(m.id)}>Cancel</button>
            {:else if m.installed}
              {#if !inUse(m)}
                <button class="btn" onclick={() => use(m)}>Use</button>
              {/if}
              <button
                class="icon-btn"
                title="Delete {m.name}"
                onclick={() => app.deleteModel(m.id)}
                disabled={app.running && inUse(m)}
              >
                <Icon name="trash" label="Delete {m.name}" />
              </button>
            {:else}
              <button class="btn primary" onclick={() => app.download(m.id)}>
                <Icon name="download" size={16} /> Download
              </button>
            {/if}
          </div>
        </li>
      {/each}
      {#each imported.filter((m) => m.kind === kind) as m (m.id)}
        <li class:active={inUse(m)}>
          <div class="info">
            <div class="title">
              <strong>{m.name}</strong>
              {#if inUse(m)}<span class="tag on"><Icon name="check" size={12} /> In use</span>{/if}
            </div>
            <div class="facts"><span>{megabytes(m.sizeBytes)}</span><span>Imported</span></div>
          </div>
          <div class="buttons">
            {#if !inUse(m)}<button class="btn" onclick={() => use(m)}>Use</button>{/if}
            <button class="icon-btn" title="Delete" onclick={() => app.deleteModel(m.id)}>
              <Icon name="trash" label="Delete {m.name}" />
            </button>
          </div>
        </li>
      {/each}
    </ul>
  </section>
{/each}

<div class="footer">
  <button class="btn" onclick={importFile}
    ><Icon name="upload" size={16} /> Import a model file…</button
  >
  <button class="btn ghost" onclick={openFolder}
    ><Icon name="folder" size={16} /> Open models folder</button
  >
</div>
<p class="hint">
  Downloading a model is the only time Hushcap uses the internet. No internet here? Download a
  <code>.bin</code> (speech) or <code>.onnx</code> (speaker) file on another computer and import it.
</p>

<style>
  .group h3 {
    margin: 18px 0 8px;
    font-size: 14px;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    color: var(--muted);
  }

  ul {
    list-style: none;
    margin: 0;
    padding: 0;
    display: grid;
    gap: 8px;
  }

  li {
    display: flex;
    gap: 12px;
    align-items: flex-start;
    justify-content: space-between;
    padding: 12px 14px;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: var(--surface);
  }

  li.active {
    border-color: var(--accent);
    background: var(--accent-soft);
  }

  .info {
    min-width: 0;
    flex: 1;
  }

  .title {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 6px 8px;
  }

  .tag {
    display: inline-flex;
    align-items: center;
    gap: 3px;
    padding: 1px 8px;
    border-radius: 999px;
    font-size: 11.5px;
    font-weight: 700;
    background: var(--surface-2);
    color: var(--muted);
  }

  .tag.on {
    color: var(--accent);
  }

  .desc {
    margin-top: 3px;
    color: var(--muted);
    line-height: 1.4;
  }

  .facts {
    display: flex;
    flex-wrap: wrap;
    gap: 4px 14px;
    margin-top: 6px;
    font-size: 12.5px;
    color: var(--muted);
  }

  .dots {
    letter-spacing: 1px;
    color: var(--accent);
  }

  .progress {
    margin-top: 8px;
    height: 6px;
    border-radius: 3px;
    background: var(--surface-2);
    overflow: hidden;
  }

  .progress .fill {
    height: 100%;
    background: var(--accent);
    transition: width 0.2s;
  }

  .buttons {
    display: flex;
    gap: 6px;
    align-items: center;
    flex: none;
  }

  .footer {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
    margin-top: 16px;
  }

  code {
    font-size: 0.9em;
  }
</style>
