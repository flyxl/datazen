import { describe, expect, it } from 'vitest';
import { mergeDriverSettings, readBooleanField } from '../driverSettings';

describe('wapp settings helpers', () => {
  it('merges one wapp bucket without clobbering others', () => {
    const next = mergeDriverSettings({ kiwi: { x: 1 } }, 'redis', { allowFlush: true });
    expect(next).toEqual({ kiwi: { x: 1 }, redis: { allowFlush: true } });
  });

  it('readBooleanField defaults', () => {
    expect(readBooleanField({}, 'allowFlush', false)).toBe(false);
    expect(readBooleanField({ allowFlush: true }, 'allowFlush', false)).toBe(true);
  });
});
