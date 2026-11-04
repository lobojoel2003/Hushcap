// Pure state machine for the live transcript. Every engine event goes
// through `applyEvent`; the UI only renders the resulting state.

import type { AudioSource, Caption, EngineEvent } from './types';

export interface PendingLine {
  id: number;
  source: AudioSource;
  text: string;
  startMs: number;
}

export interface TranscriptState {
  captions: Caption[];
  /** Utterances in progress, by caption id. */
  pending: Record<number, PendingLine>;
  /** Custom speaker names. */
  names: Record<number, string>;
}

export function emptyTranscript(): TranscriptState {
  return { captions: [], pending: {}, names: {} };
}

function withoutPending(pending: Record<number, PendingLine>, id: number) {
  if (!(id in pending)) return pending;
  const next = { ...pending };
  delete next[id];
  return next;
}

export function applyEvent(state: TranscriptState, event: EngineEvent): TranscriptState {
  switch (event.type) {
    case 'speechStarted':
      // Show an empty "someone is talking" line until words arrive.
      return {
        ...state,
        pending: {
          ...state.pending,
          [event.id]: { id: event.id, source: event.source, text: '', startMs: event.startMs },
        },
      };
    case 'partial':
      if (state.captions.some((c) => c.id === event.id)) return state;
      return {
        ...state,
        pending: {
          ...state.pending,
          [event.id]: {
            id: event.id,
            source: event.source,
            text: event.text,
            startMs: event.startMs,
          },
        },
      };
    case 'final': {
      const caption = event.caption;
      const captions = [...state.captions];
      // Keep chronological order even if two sources finish out of order.
      let i = captions.length;
      while (i > 0 && (captions[i - 1]?.startMs ?? 0) > caption.startMs) i--;
      captions.splice(i, 0, caption);
      return { ...state, captions, pending: withoutPending(state.pending, caption.id) };
    }
    case 'dropped':
      return { ...state, pending: withoutPending(state.pending, event.id) };
    case 'speakersMerged': {
      const names = { ...state.names };
      const fromName = names[event.from];
      if (fromName && !names[event.into]) names[event.into] = fromName;
      delete names[event.from];
      return {
        ...state,
        names,
        captions: state.captions.map((c) =>
          c.speaker === event.from ? { ...c, speaker: event.into } : c,
        ),
      };
    }
    case 'state':
      if (event.state === 'stopped') return { ...state, pending: {} };
      return state;
    default:
      return state;
  }
}

export function renameSpeaker(
  state: TranscriptState,
  speaker: number,
  name: string,
): TranscriptState {
  const names = { ...state.names };
  const trimmed = name.trim();
  if (trimmed) names[speaker] = trimmed;
  else delete names[speaker];
  return { ...state, names };
}

export function pendingLines(state: TranscriptState): PendingLine[] {
  return Object.values(state.pending).sort((a, b) => a.startMs - b.startMs);
}

/** Speakers that appear in the transcript, in order of first appearance. */
export function speakersInTranscript(state: TranscriptState): number[] {
  const seen: number[] = [];
  for (const c of state.captions) {
    if (c.speaker != null && !seen.includes(c.speaker)) seen.push(c.speaker);
  }
  return seen;
}
