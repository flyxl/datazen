import type { AiSafetyGateConfig } from '../types';

export type AiSafetyPresetType = 'cloud_strict' | 'enterprise_balanced' | 'local_trust';

export const AI_SAFETY_PRESETS: Record<AiSafetyPresetType, AiSafetyGateConfig> = {
  cloud_strict: {
    redactCredentials: true,
    dataEgressLevel: 'strict',
    maxSampleRows: 0,
    dbToolPolicy: 'read_only',
    mcpToolPolicy: 'disabled',
    requireSqlConfirm: true,
    maxContextBytes: 32000,
  },
  enterprise_balanced: {
    redactCredentials: true,
    dataEgressLevel: 'sample_masked',
    maxSampleRows: 3,
    dbToolPolicy: 'require_confirm',
    mcpToolPolicy: 'require_confirm',
    requireSqlConfirm: true,
    maxContextBytes: 32000,
  },
  local_trust: {
    redactCredentials: true,
    dataEgressLevel: 'unrestricted',
    maxSampleRows: 10,
    dbToolPolicy: 'unrestricted',
    mcpToolPolicy: 'unrestricted',
    requireSqlConfirm: false,
    maxContextBytes: 64000,
  },
};

export function getDefaultSafetyGate(
  preset: AiSafetyPresetType = 'cloud_strict',
): AiSafetyGateConfig {
  return { ...AI_SAFETY_PRESETS[preset] };
}

export function detectSafetyPreset(gate: AiSafetyGateConfig): AiSafetyPresetType | 'custom' {
  if (
    gate.dataEgressLevel === 'strict' &&
    gate.dbToolPolicy === 'read_only' &&
    gate.mcpToolPolicy === 'disabled' &&
    gate.requireSqlConfirm
  ) {
    return 'cloud_strict';
  }
  if (
    gate.dataEgressLevel === 'sample_masked' &&
    gate.dbToolPolicy === 'require_confirm' &&
    gate.mcpToolPolicy === 'require_confirm' &&
    gate.requireSqlConfirm
  ) {
    return 'enterprise_balanced';
  }
  if (
    gate.dataEgressLevel === 'unrestricted' &&
    gate.dbToolPolicy === 'unrestricted' &&
    gate.mcpToolPolicy === 'unrestricted' &&
    !gate.requireSqlConfirm
  ) {
    return 'local_trust';
  }
  return 'custom';
}

export function isLocalEndpoint(endpoint?: string): boolean {
  if (!endpoint) return false;
  const lower = endpoint.toLowerCase();
  return (
    lower.includes('localhost') ||
    lower.includes('127.0.0.1') ||
    lower.includes('0.0.0.0') ||
    lower.includes('192.168.') ||
    lower.includes('10.') ||
    lower.includes(':11434')
  );
}
