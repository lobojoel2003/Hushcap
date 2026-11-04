import { describe, expect, it } from 'vitest';
import {
  applyEvent,
  emptyTranscript,
  pendingLines,
  renameSpeaker,
  speakersInTranscript,
  type TranscriptState,
} from './transcript';
import type { Caption, EngineEvent } from './types';

const caption = (id: number, startMs: number, speaker: number | null = 1): Caption => ({
  id,
  source: 'microphone',
  speaker,
  text: `line ${id}`,
  startMs,
  endMs: startMs + 1000,
});

function run(events: EngineEvent[], state: TranscriptState = emptyTranscript()) {
  return events.reduce(applyEvent, state);
}

describe('transcript reducer', () => {
  it('shows a pending line, updates it, then replaces it with the final', () => {
    let s = run([{ type: 'speechStarted', id: 1, source: 'microphone', startMs: 100 }]);
    expect(pendingLines(s)).toEqual([{ id: 1, source: 'microphone', text: '', startMs: 100 }]);
    s = run([{ type: 'partial', id: 1, source: 'microphone', text: 'hel', startMs: 100 }], s);
    expect(pendingLines(s)[0]?.text).toBe('hel');
    s = run([{ type: 'final', caption: caption(1, 100) }], s);
    expect(pendingLines(s)).toEqual([]);
    expect(s.captions.map((c) => c.id)).toEqual([1]);
  });

  it('ignores late partials for finished captions', () => {
    const s = run([
      { type: 'final', caption: caption(1, 0) },
      { type: 'partial', id: 1, source: 'microphone', text: 'stale', startMs: 0 },
    ]);
    expect(pendingLines(s)).toEqual([]);
  });

  it('drops noise', () => {
    const s = run([
      { type: 'speechStarted', id: 4, source: 'system', startMs: 0 },
      { type: 'dropped', id: 4 },
    ]);
    expect(pendingLines(s)).toEqual([]);
    expect(s.captions).toEqual([]);
  });

  it('keeps captions in time order across sources', () => {
    const s = run([
      { type: 'final', caption: caption(2, 5000) },
      { type: 'final', caption: { ...caption(1, 3000), source: 'system' } },
      { type: 'final', caption: caption(3, 9000) },
    ]);
    expect(s.captions.map((c) => c.id)).toEqual([1, 2, 3]);
  });

  it('relabels merged speakers and carries the custom name over', () => {
    let s = run([
      { type: 'final', caption: caption(1, 0, 1) },
      { type: 'final', caption: caption(2, 2000, 2) },
    ]);
    s = renameSpeaker(s, 2, 'Grandma');
    s = run([{ type: 'speakersMerged', from: 2, into: 1 }], s);
    expect(s.captions.map((c) => c.speaker)).toEqual([1, 1]);
    expect(s.names).toEqual({ 1: 'Grandma' });
    expect(speakersInTranscript(s)).toEqual([1]);
  });

  it('clears pending lines when the engine stops', () => {
    const s = run([
      { type: 'speechStarted', id: 1, source: 'microphone', startMs: 0 },
      { type: 'state', state: 'stopped' },
    ]);
    expect(pendingLines(s)).toEqual([]);
  });

  it('renames and un-names speakers', () => {
    let s = renameSpeaker(emptyTranscript(), 3, '  Dr. Rivera ');
    expect(s.names[3]).toBe('Dr. Rivera');
    s = renameSpeaker(s, 3, '   ');
    expect(s.names[3]).toBeUndefined();
  });
});
