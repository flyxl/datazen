/**
 * The two measurement formatters every Redis readout shares.
 *
 * Both are copy-free by design: they format a number the server sent, never a
 * unit a translator could rephrase, so these tests assert the exact strings a
 * user reads rather than an i18n key.
 *
 * `formatCompactCount` used to live in the now-deleted `contextBarModel`
 * alongside the context bar's own pure helpers; it moved here with its only
 * caller (the status bar) when that band was removed.
 */
import { describe, expect, it } from 'vitest';

import { formatCompactCount, formatSize } from '../shared/formatSize';

describe('formatSize', () => {
  it('switches units at 1024 and never drops the unit', () => {
    expect(formatSize(0)).toBe('—');
    expect(formatSize(512)).toBe('512 B');
    expect(formatSize(1024)).toBe('1.0 KB');
    expect(formatSize(1536)).toBe('1.5 KB');
    expect(formatSize(1024 * 1024)).toBe('1.0 MB');
    expect(formatSize(4 * 1024 * 1024)).toBe('4.0 MB');
  });

  it('answers an impossible size with the em dash rather than a fake number', () => {
    // A missing `memory_bytes` must not render `0 B` — that is a claim the
    // server never made.
    expect(formatSize(-1)).toBe('—');
    expect(formatSize(Number.NaN)).toBe('—');
  });
});

describe('formatCompactCount', () => {
  it('keeps a narrow readout readable', () => {
    expect(formatCompactCount(0)).toBe('0');
    expect(formatCompactCount(52)).toBe('52');
    expect(formatCompactCount(999)).toBe('999');
    expect(formatCompactCount(1000)).toBe('1k');
    expect(formatCompactCount(1200)).toBe('1.2k');
    expect(formatCompactCount(12_000)).toBe('12k');
    expect(formatCompactCount(999_999)).toBe('1000k');
    expect(formatCompactCount(1_200_000)).toBe('1M');
  });

  it('collapses a count the server never reported to zero', () => {
    expect(formatCompactCount(-5)).toBe('0');
    expect(formatCompactCount(Number.NaN)).toBe('0');
    expect(formatCompactCount(12.7)).toBe('12');
  });
});
