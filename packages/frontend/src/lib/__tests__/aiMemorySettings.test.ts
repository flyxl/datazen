import { describe, expect, it } from 'vitest';
import {
  memoryModeFor,
  normalizeAiSettingsConfig,
  normalizeMemorySettings,
} from '../aiMemorySettings';
import type { AiSettingsConfig } from '../../types';

const base = {
  activeProfileId: 'p1',
  profiles: [],
  globalInstructions: '',
  promptTemplates: [],
};

describe('aiMemorySettings', () => {
  it('defaults old settings to disabled, normal-only, unacknowledged memory', () => {
    const normalized = normalizeAiSettingsConfig(base as unknown as AiSettingsConfig);
    expect(normalized.memory).toEqual({
      enabled: false,
      autoLearn: false,
      cloudPolicy: 'normal_only',
      privacyAcknowledged: false,
    });
    expect(memoryModeFor(normalized.memory)).toBe('off');
  });

  it('selects auto only when memory is enabled and tolerates bad stored values', () => {
    const normalized = normalizeAiSettingsConfig({
      ...base,
      memory: { enabled: true, autoLearn: true, cloudPolicy: 'bogus' },
    } as unknown as AiSettingsConfig);
    expect(normalized.memory).toEqual({
      enabled: true,
      autoLearn: true,
      cloudPolicy: 'normal_only',
      privacyAcknowledged: false,
    });
    expect(memoryModeFor(normalized.memory)).toBe('auto');

    expect(
      normalizeMemorySettings({ enabled: false, autoLearn: true, cloudPolicy: 'disable_cloud' }),
    ).toEqual({
      enabled: false,
      autoLearn: true,
      cloudPolicy: 'disable_cloud',
      privacyAcknowledged: false,
    });
    expect(
      memoryModeFor({ enabled: false, autoLearn: true, cloudPolicy: 'disable_cloud' } as never),
    ).toBe('off');
  });
});
