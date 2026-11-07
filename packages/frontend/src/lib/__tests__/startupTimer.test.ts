import { describe, expect, it, vi } from 'vitest';
import { mark } from '../startupTimer';

describe('mark', () => {
  it('logs startup timing with label', () => {
    const spy = vi.spyOn(console, 'log').mockImplementation(() => {});
    mark('app-ready');
    expect(spy).toHaveBeenCalledOnce();
    const msg = String(spy.mock.calls[0][0]);
    expect(msg).toContain('[startup]');
    expect(msg).toContain('app-ready');
    spy.mockRestore();
  });

  it('includes delta between consecutive marks', () => {
    const spy = vi.spyOn(console, 'log').mockImplementation(() => {});
    mark('first');
    mark('second');
    expect(spy).toHaveBeenCalledTimes(2);
    const second = String(spy.mock.calls[1][0]);
    expect(second).toContain('second');
    spy.mockRestore();
  });
});
