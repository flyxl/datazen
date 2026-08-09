import { describe, expect, it } from 'vitest';
import { mergePluginSettings, readBooleanField } from '../../plugin-sdk/settings';

describe('plugin settings helpers', () => {
  it('merges one plugin bucket without clobbering others', () => {
    const next = mergePluginSettings(
      { kiwi: { x: 1 } },
      'redis',
      { allowFlush: true },
    );
    expect(next).toEqual({ kiwi: { x: 1 }, redis: { allowFlush: true } });
  });

  it('readBooleanField defaults', () => {
    expect(readBooleanField({}, 'allowFlush', false)).toBe(false);
    expect(readBooleanField({ allowFlush: true }, 'allowFlush', false)).toBe(true);
  });
});
