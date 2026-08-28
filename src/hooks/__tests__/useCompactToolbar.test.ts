import { describe, expect, it } from 'vitest';
import { estimateExpandedToolbarWidth } from '../useCompactToolbar';

describe('estimateExpandedToolbarWidth', () => {
  it('returns padding-only width for zero buttons', () => {
    expect(estimateExpandedToolbarWidth({ expandedButtonCount: 0 })).toBe(32);
  });

  it('scales with button count and fixed extras', () => {
    expect(
      estimateExpandedToolbarWidth({
        expandedButtonCount: 6,
        fixedExtraWidth: 120,
      }),
    ).toBe(32 + 6 * 96 + 5 * 8 + 120);
  });
});
