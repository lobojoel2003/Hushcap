<script lang="ts">
  import { onMount } from 'svelte';
  import { errorMessage } from './lib/api';
  import { app } from './lib/state.svelte';
  import CaptionView from './lib/components/CaptionView.svelte';
  import ControlBar from './lib/components/ControlBar.svelte';
  import Header from './lib/components/Header.svelte';
  import Icon from './lib/components/Icon.svelte';
  import Onboarding from './lib/components/Onboarding.svelte';
  import SettingsPanel from './lib/components/SettingsPanel.svelte';
  import Toasts from './lib/components/Toasts.svelte';

  let header: Header | undefined = $state();
  let failed = $state<string | null>(null);

  const appearance = $derived(app.settings?.appearance);
  const compact = $derived(appearance?.compact ?? false);
  const showOnboarding = $derived(
    app.ready && !!app.settings && (!app.settings.onboardingDone || !app.hasSpeechModel),
  );

  onMount(() => {
    app.init().catch((e) => (failed = errorMessage(e)));
    const media = window.matchMedia('(prefers-color-scheme: dark)');
    const sync = () => {
      const theme = app.settings?.appearance.theme ?? 'system';
      app.dark = theme === 'dark' || theme === 'contrast' || (theme === 'system' && media.matches);
    };
    media.addEventListener('change', sync);
    $effect.root(() => {
      $effect(() => {
        void app.settings?.appearance.theme;
        sync();
      });
    });
    return () => media.removeEventListener('change', sync);
  });

  $effect(() => {
    const root = document.documentElement;
    const a = appearance;
    root.dataset.theme = a?.theme ?? 'system';
    root.dataset.font = a?.fontFamily ?? 'hyperlegible';
    root.style.setProperty('--font-scale', String(a?.fontScale ?? 1));
    root.style.setProperty('--line-spacing', String(a?.lineSpacing ?? 1.45));
  });

  function scaleText(delta: number | null) {
    void app.update((s) => {
      const next = delta === null ? 1 : s.appearance.fontScale + delta;
      s.appearance.fontScale = Math.round(Math.min(4, Math.max(0.6, next)) * 100) / 100;
    });
  }

  function onKey(e: KeyboardEvent) {
    if (!app.ready || showOnboarding) return;
    const mod = e.ctrlKey || e.metaKey;
    if (e.key === 'Escape') {
      if (app.settingsOpen) app.settingsOpen = false;
      else if (compact) void app.update((s) => (s.appearance.compact = false));
      return;
    }
    if (!mod) return;
    const key = e.key.toLowerCase();
    const handled = (() => {
      switch (key) {
        case 'enter':
          void app.toggle();
          return true;
        case ',':
          app.settingsOpen = !app.settingsOpen;
          return true;
        case 's':
          void header?.saveAs('txt');
          return true;
        case 'c':
          if (!e.shiftKey) return false;
          void header?.copyAll();
          return true;
        case 'l':
          void header?.clear();
          return true;
        case '=':
        case '+':
          scaleText(0.1);
          return true;
        case '-':
          scaleText(-0.1);
          return true;
        case '0':
          scaleText(null);
          return true;
        case 't':
          void app.update((s) => (s.appearance.alwaysOnTop = !s.appearance.alwaysOnTop));
          return true;
        case 'm':
          void app.update((s) => (s.appearance.compact = !s.appearance.compact));
          return true;
        default:
          return false;
      }
    })();
    if (handled) e.preventDefault();
  }
</script>

<svelte:window onkeydown={onKey} />

{#if failed}
  <div class="splash">
    <p><strong>Hushcap could not start.</strong></p>
    <p class="hint">{failed}</p>
  </div>
{:else if !app.ready}
  <div class="splash" aria-busy="true"><p>Loading…</p></div>
{:else}
  <div class="shell" class:compact>
    {#if !compact}
      <Header bind:this={header} />
    {/if}
    <main>
      <CaptionView />
    </main>
    {#if compact}
      <div class="mini">
        <button
          class="icon-btn"
          onclick={() => app.toggle()}
          title={app.running ? 'Stop (Ctrl+Enter)' : 'Start (Ctrl+Enter)'}
        >
          <Icon
            name={app.running ? 'stop' : 'play'}
            label={app.running ? 'Stop captions' : 'Start captions'}
          />
        </button>
        <button
          class="icon-btn"
          onclick={() => app.update((s) => (s.appearance.compact = false))}
          title="Show controls (Esc)"
        >
          <Icon name="expand" label="Show controls" />
        </button>
      </div>
    {:else}
      <ControlBar />
    {/if}
  </div>
  {#if app.settingsOpen}
    <SettingsPanel />
  {/if}
  {#if showOnboarding}
    <Onboarding />
  {/if}
{/if}
<Toasts />

<style>
  .shell {
    display: grid;
    grid-template-rows: auto minmax(0, 1fr) auto;
    height: 100vh;
  }

  .shell.compact {
    grid-template-rows: minmax(0, 1fr);
  }

  main {
    min-height: 0;
  }

  .mini {
    position: fixed;
    top: 8px;
    right: 8px;
    display: flex;
    gap: 4px;
    padding: 4px;
    border-radius: 12px;
    background: var(--surface);
    border: 1px solid var(--border);
    opacity: 0.35;
    transition: opacity 0.2s;
  }

  .mini:hover,
  .mini:focus-within {
    opacity: 1;
  }

  .splash {
    display: grid;
    place-content: center;
    height: 100vh;
    text-align: center;
    padding: 24px;
  }
</style>
