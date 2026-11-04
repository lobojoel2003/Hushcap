import { mount } from 'svelte';
import '@fontsource/atkinson-hyperlegible-next/400.css';
import '@fontsource/atkinson-hyperlegible-next/700.css';
import '@fontsource/opendyslexic/400.css';
import './app.css';
import App from './App.svelte';

// In a plain browser during development, stand in for the Rust backend.
if (import.meta.env.DEV && !('__TAURI_INTERNALS__' in window)) {
  const { installMockBackend } = await import('./lib/mock');
  installMockBackend();
}

const app = mount(App, { target: document.getElementById('app')! });

export default app;
