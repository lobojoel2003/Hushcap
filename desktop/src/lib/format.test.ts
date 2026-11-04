import { describe, expect, it } from 'vitest';
import { clock, levelFraction, megabytes, speakerColor, speakerName, LANGUAGES } from './format';

describe('format helpers', () => {
  it('formats clocks', () => {
    expect(clock(0)).toBe('0:00');
    expect(clock(65_000)).toBe('1:05');
    expect(clock(3_725_000)).toBe('1:02:05');
    expect(clock(-5)).toBe('0:00');
  });

  it('names speakers', () => {
    expect(speakerName(2, {})).toBe('Speaker 2');
    expect(speakerName(2, { 2: 'Ana' })).toBe('Ana');
    expect(speakerName(2, { 2: '  ' })).toBe('Speaker 2');
  });

  it('cycles speaker colours', () => {
    expect(speakerColor(1, false)).toBe(speakerColor(9, false));
    expect(speakerColor(1, true)).not.toBe(speakerColor(2, true));
    expect(speakerColor(0, true)).toMatch(/^#[0-9A-F]{6}$/i);
  });

  it('maps levels and sizes', () => {
    expect(levelFraction(-100)).toBe(0);
    expect(levelFraction(0)).toBe(1);
    expect(levelFraction(-30)).toBeCloseTo(0.5);
    expect(megabytes(57_000_000)).toBe('57 MB');
    expect(megabytes(1_500_000)).toBe('1.5 MB');
  });

  it('lists auto-detection first with unique codes', () => {
    expect(LANGUAGES[0]?.[0]).toBe('auto');
    expect(new Set(LANGUAGES.map(([c]) => c)).size).toBe(LANGUAGES.length);
  });
});
