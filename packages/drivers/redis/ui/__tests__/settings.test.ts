import { describe, expect, it } from 'vitest';
import { applySchemaDefaults } from '@datazen/plugin-sdk';
import { redisSettingsSchema } from '../settings';

describe('redisSettingsSchema', () => {
  it('defaults clusterRouting to auto', () => {
    const defaults = applySchemaDefaults(redisSettingsSchema, {});
    expect(defaults.allowFlush).toBe(false);
    expect(defaults.clusterRouting).toBe('auto');
  });

  it('preserves pinnedNode when set', () => {
    const value = applySchemaDefaults(redisSettingsSchema, {
      clusterRouting: 'pinnedNode',
    });
    expect(value.clusterRouting).toBe('pinnedNode');
  });
});
