// Small display helpers shared by components.

/** `M:SS` or `H:MM:SS`. */
export function clock(ms: number): string {
  const total = Math.max(0, Math.floor(ms / 1000));
  const h = Math.floor(total / 3600);
  const m = Math.floor((total % 3600) / 60);
  const s = total % 60;
  const ss = String(s).padStart(2, '0');
  return h > 0 ? `${h}:${String(m).padStart(2, '0')}:${ss}` : `${m}:${ss}`;
}

export function speakerName(speaker: number, names: Record<number, string>): string {
  return names[speaker]?.trim() || `Speaker ${speaker}`;
}

// Okabe–Ito palette: distinguishable with common colour-vision deficiencies.
const LIGHT = [
  '#0072B2',
  '#C2410C',
  '#047857',
  '#B4467E',
  '#7A5A00',
  '#2563EB',
  '#6A3D9A',
  '#334155',
];
const DARK = [
  '#56B4E9',
  '#FF9F5A',
  '#34D399',
  '#F0A3CF',
  '#F0E442',
  '#93C5FD',
  '#C4A7FF',
  '#E2E8F0',
];

export function speakerColor(speaker: number, dark: boolean): string {
  const palette = dark ? DARK : LIGHT;
  const i = (((speaker - 1) % palette.length) + palette.length) % palette.length;
  return palette[i] ?? palette[0]!;
}

export function megabytes(bytes: number): string {
  return `${(bytes / 1_000_000).toFixed(bytes < 10_000_000 ? 1 : 0)} MB`;
}

/** Map a dBFS reading to 0–1 for level meters. */
export function levelFraction(db: number): number {
  return Math.min(1, Math.max(0, (db + 60) / 60));
}

/** Whisper's language list, most common first. `auto` detects per phrase. */
export const LANGUAGES: [code: string, name: string][] = [
  ['auto', 'Detect automatically'],
  ['en', 'English'],
  ['es', 'Spanish'],
  ['fr', 'French'],
  ['de', 'German'],
  ['it', 'Italian'],
  ['pt', 'Portuguese'],
  ['nl', 'Dutch'],
  ['hi', 'Hindi'],
  ['zh', 'Chinese'],
  ['ja', 'Japanese'],
  ['ko', 'Korean'],
  ['ar', 'Arabic'],
  ['ru', 'Russian'],
  ['uk', 'Ukrainian'],
  ['pl', 'Polish'],
  ['tr', 'Turkish'],
  ['vi', 'Vietnamese'],
  ['id', 'Indonesian'],
  ['sv', 'Swedish'],
  ['no', 'Norwegian'],
  ['da', 'Danish'],
  ['fi', 'Finnish'],
  ['el', 'Greek'],
  ['he', 'Hebrew'],
  ['cs', 'Czech'],
  ['ro', 'Romanian'],
  ['hu', 'Hungarian'],
  ['th', 'Thai'],
  ['ta', 'Tamil'],
  ['bn', 'Bengali'],
  ['ur', 'Urdu'],
  ['fa', 'Persian'],
  ['ms', 'Malay'],
  ['tl', 'Tagalog'],
  ['sw', 'Swahili'],
];
