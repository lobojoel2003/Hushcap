<script lang="ts">
  import { tick } from 'svelte';
  import { app } from '../state.svelte';
  import { pendingLines } from '../transcript';
  import { clock, speakerColor, speakerName } from '../format';
  import type { Caption } from '../types';
  import Icon from './Icon.svelte';

  let scroller: HTMLDivElement | undefined = $state();
  let following = $state(true);
  let editing = $state<number | null>(null);
  let draft = $state('');

  const captions = $derived(app.transcript.captions);
  const pending = $derived(pendingLines(app.transcript));
  const names = $derived(app.transcript.names);
  const showTimes = $derived(app.settings?.appearance.showTimestamps ?? false);
  const labelsOn = $derived(app.settings?.speakerLabels ?? true);

  /** Start a new visual group when the speaker or source changes, or after a long pause. */
  function startsGroup(i: number): boolean {
    const c = captions[i];
    const prev = captions[i - 1];
    if (!c || !prev) return true;
    return (
      c.speaker !== prev.speaker || c.source !== prev.source || c.startMs - prev.endMs > 30_000
    );
  }

  function onScroll() {
    if (!scroller) return;
    following = scroller.scrollHeight - scroller.scrollTop - scroller.clientHeight < 96;
  }

  async function jumpToLatest() {
    following = true;
    await tick();
    scroller?.scrollTo({ top: scroller.scrollHeight, behavior: 'smooth' });
  }

  $effect(() => {
    // Re-run whenever lines are added or interim text changes.
    void captions.length;
    void pending.map((p) => p.text).join('\u0000');
    if (!following) return;
    void tick().then(() => scroller?.scrollTo({ top: scroller.scrollHeight }));
  });

  function beginRename(speaker: number) {
    editing = speaker;
    draft = names[speaker] ?? '';
  }

  function commitRename() {
    if (editing != null) app.rename(editing, draft);
    editing = null;
  }

  function onRenameKey(e: KeyboardEvent) {
    if (e.key === 'Enter') commitRename();
    if (e.key === 'Escape') editing = null;
  }

  function focusOnMount(node: HTMLInputElement) {
    node.focus();
    node.select();
  }

  const latestId = $derived(captions.at(-1)?.id);
</script>

{#snippet header(c: Caption, showSpeaker: boolean)}
  <div class="meta">
    {#if showSpeaker && labelsOn && c.speaker != null}
      {#if editing === c.speaker}
        <input
          class="rename"
          type="text"
          bind:value={draft}
          onkeydown={onRenameKey}
          onblur={commitRename}
          aria-label="Name for speaker {c.speaker}"
          placeholder="Speaker {c.speaker}"
          use:focusOnMount
        />
      {:else}
        <button
          class="speaker"
          style:color={speakerColor(c.speaker, app.dark)}
          onclick={() => beginRename(c.speaker!)}
          title="Rename this speaker"
        >
          {speakerName(c.speaker, names)}
          <span class="edit"><Icon name="edit" size={13} /></span>
        </button>
      {/if}
    {/if}
    {#if c.source === 'system'}
      <span class="source" title="From system audio"><Icon name="volume" size={14} /> System</span>
    {/if}
    {#if showTimes}
      <time class="time">{clock(c.startMs)}</time>
    {/if}
  </div>
{/snippet}

<div class="wrap">
  <div
    class="scroller"
    bind:this={scroller}
    onscroll={onScroll}
    role="log"
    aria-label="Live captions"
    aria-live="polite"
    aria-relevant="additions"
  >
    <div class="column">
      {#if captions.length === 0 && pending.length === 0}
        <div class="empty">
          {#if app.running}
            <Icon name="wave" size={40} />
            <p>Listening… captions will appear here as people speak.</p>
          {:else if app.engine === 'starting'}
            <p>Loading the speech model…</p>
          {:else}
            <Icon name="lock" size={40} />
            <p>
              Press <strong>Start captions</strong> to caption your microphone
              {#if app.settings?.systemEnabled}and system audio{/if}. Everything stays on this
              computer.
            </p>
          {/if}
        </div>
      {/if}

      {#each captions as c, i (c.id)}
        {@const first = startsGroup(i)}
        <article
          class="line"
          class:grouped={!first}
          aria-current={c.id === latestId ? 'true' : undefined}
        >
          {#if first || showTimes}
            {@render header(c, first)}
          {/if}
          <p class="text">{c.text}</p>
        </article>
      {/each}

      {#each pending as p (p.id)}
        <article class="line pending" aria-hidden="true">
          {#if p.source === 'system'}
            <div class="meta">
              <span class="source"><Icon name="volume" size={14} /> System</span>
            </div>
          {/if}
          <p class="text">
            {#if p.text}{p.text}{:else}<span class="dots"
                ><span></span><span></span><span></span></span
              >{/if}
          </p>
        </article>
      {/each}
    </div>
  </div>

  {#if !following}
    <button class="btn primary jump" onclick={jumpToLatest}>
      <Icon name="arrowDown" size={18} /> Jump to latest
    </button>
  {/if}
</div>

<style>
  .wrap {
    position: relative;
    height: 100%;
    min-height: 0;
  }

  .scroller {
    height: 100%;
    overflow-y: auto;
    scroll-behavior: smooth;
  }

  .column {
    max-width: 980px;
    margin: 0 auto;
    padding: 28px 28px 40px;
  }

  .line {
    margin-top: calc(18px * var(--font-scale));
  }

  .line.grouped {
    margin-top: calc(6px * var(--font-scale));
  }

  .meta {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 10px;
    margin-bottom: 2px;
    min-height: 24px;
  }

  .speaker {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    border: 0;
    background: none;
    padding: 2px 4px;
    margin-left: -4px;
    font-weight: 800;
    font-size: calc(14px * min(var(--font-scale), 1.6));
    letter-spacing: 0.02em;
    text-transform: uppercase;
    border-radius: 6px;
  }

  .speaker .edit {
    opacity: 0;
    transition: opacity 0.15s;
  }

  .speaker:hover .edit,
  .speaker:focus-visible .edit {
    opacity: 0.8;
  }

  .rename {
    font-weight: 700;
    min-height: 28px;
    width: 14em;
  }

  .source,
  .time {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    color: var(--muted);
    font-size: 13px;
    font-variant-numeric: tabular-nums;
  }

  .text {
    margin: 0;
    font-family: var(--caption-font);
    font-size: calc(28px * var(--font-scale));
    line-height: var(--line-spacing);
    color: var(--caption);
    overflow-wrap: anywhere;
  }

  .pending .text {
    opacity: 0.62;
    font-style: italic;
  }

  .empty {
    display: grid;
    justify-items: center;
    gap: 12px;
    margin-top: 12vh;
    color: var(--muted);
    text-align: center;
    font-size: 18px;
    line-height: 1.5;
  }

  .empty p {
    max-width: 30em;
    margin: 0;
  }

  .dots {
    display: inline-flex;
    gap: 6px;
    vertical-align: middle;
  }

  .dots span {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: currentColor;
    animation: pulse 1.2s infinite ease-in-out;
  }

  .dots span:nth-child(2) {
    animation-delay: 0.2s;
  }

  .dots span:nth-child(3) {
    animation-delay: 0.4s;
  }

  @keyframes pulse {
    0%,
    80%,
    100% {
      opacity: 0.25;
    }
    40% {
      opacity: 1;
    }
  }

  .jump {
    position: absolute;
    right: 24px;
    bottom: 18px;
    box-shadow: var(--shadow);
    border-radius: 999px;
  }

  @media (max-width: 560px) {
    .column {
      padding: 16px 16px 28px;
    }
  }
</style>
