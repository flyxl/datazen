import { describe, expect, it } from 'vitest';
import { applySchemaDefaults } from '@datazen/driver-sdk';
import { redisSettingsSchema } from '../connection/settings';

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
