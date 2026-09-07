import { describe, expect, it } from 'vitest';
import {
  AI_SAFETY_PRESETS,
  detectSafetyPreset,
  getDefaultSafetyGate,
  isLocalEndpoint,
} from '../aiSafetyPresets';

describe('aiSafetyPresets', () => {
  it('provides valid defaults for each preset', () => {
    const cloudStrict = getDefaultSafetyGate('cloud_strict');
    expect(cloudStrict.dataEgressLevel).toBe('strict');
    expect(cloudStrict.redactCredentials).toBe(true);
    expect(cloudStrict.dbToolPolicy).toBe('read_only');
    expect(cloudStrict.mcpToolPolicy).toBe('disabled');
    expect(cloudStrict.requireSqlConfirm).toBe(true);

    const enterprise = getDefaultSafetyGate('enterprise_balanced');
    expect(enterprise.dataEgressLevel).toBe('sample_masked');
    expect(enterprise.maxSampleRows).toBe(3);
    expect(enterprise.dbToolPolicy).toBe('require_confirm');
    expect(enterprise.mcpToolPolicy).toBe('require_confirm');

    const local = getDefaultSafetyGate('local_trust');
    expect(local.dataEgressLevel).toBe('unrestricted');
    expect(local.dbToolPolicy).toBe('unrestricted');
    expect(local.mcpToolPolicy).toBe('unrestricted');
    expect(local.requireSqlConfirm).toBe(false);
  });

  it('detects presets correctly', () => {
    expect(detectSafetyPreset(AI_SAFETY_PRESETS.cloud_strict)).toBe('cloud_strict');
    expect(detectSafetyPreset(AI_SAFETY_PRESETS.enterprise_balanced)).toBe('enterprise_balanced');
    expect(detectSafetyPreset(AI_SAFETY_PRESETS.local_trust)).toBe('local_trust');

    expect(
      detectSafetyPreset({
        ...AI_SAFETY_PRESETS.cloud_strict,
        dbToolPolicy: 'unrestricted',
      }),
    ).toBe('custom');
  });

  it('identifies local vs remote endpoints', () => {
    expect(isLocalEndpoint('http://localhost:11434')).toBe(true);
    expect(isLocalEndpoint('http://127.0.0.1:8000/v1')).toBe(true);
    expect(isLocalEndpoint('http://192.168.1.100:11434')).toBe(true);
    expect(isLocalEndpoint('https://api.openai.com/v1')).toBe(false);
    expect(isLocalEndpoint('https://api.deepseek.com')).toBe(false);
    expect(isLocalEndpoint('')).toBe(false);
    expect(isLocalEndpoint(undefined)).toBe(false);
  });
});
