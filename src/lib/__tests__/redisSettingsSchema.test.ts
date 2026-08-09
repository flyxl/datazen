import { describe, expect, it } from 'vitest';
import { redisSettingsSchema } from '../../../packages/drivers/redis/ui/settings';

describe('redisSettingsSchema', () => {
  it('defines allowFlush boolean property', () => {
    expect(redisSettingsSchema.properties.allowFlush).toMatchObject({
      type: 'boolean',
      default: false,
    });
  });
});
