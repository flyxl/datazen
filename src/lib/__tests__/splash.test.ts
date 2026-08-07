import { describe, it, expect, vi } from 'vitest';
import { hideSplash } from '../splash';

describe('hideSplash', () => {
  it('adds hide class and schedules remove', () => {
    vi.useFakeTimers();
    const el = document.createElement('div');
    document.body.appendChild(el);
    hideSplash(el);
    expect(el.classList.contains('hide')).toBe(true);
    vi.advanceTimersByTime(350);
    expect(document.body.contains(el)).toBe(false);
    vi.useRealTimers();
  });
});
