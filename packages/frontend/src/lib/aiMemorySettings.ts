import type { AiMemoryCloudPolicy, AiMemorySettings, AiSettingsConfig } from '../types';
import { defaultAiMemorySettings } from '../types';

const CLOUD_POLICIES: AiMemoryCloudPolicy[] = ['normal_only', 'allow_internal', 'disable_cloud'];

/** Fill fields omitted by configs written before Memory settings existed. */
export function normalizeMemorySettings(
  memory: Partial<AiMemorySettings> | null | undefined,
): AiMemorySettings {
  const cloudPolicy = CLOUD_POLICIES.includes(memory?.cloudPolicy as AiMemoryCloudPolicy)
    ? (memory?.cloudPolicy as AiMemoryCloudPolicy)
    : defaultAiMemorySettings.cloudPolicy;
  return {
    ...defaultAiMemorySettings,
    ...memory,
    enabled: memory?.enabled === true,
    autoLearn: memory?.autoLearn === true,
    cloudPolicy,
    privacyAcknowledged: memory?.privacyAcknowledged === true,
  };
}

/** Normalize a settings config returned by an older host. */
export function normalizeAiSettingsConfig(config: AiSettingsConfig): AiSettingsConfig {
  return { ...config, memory: normalizeMemorySettings(config.memory) };
}

/** Memory is opt-in; an unset/disabled config must send `off`. */
export function memoryModeFor(settings: AiMemorySettings | null | undefined): 'off' | 'auto' {
  return normalizeMemorySettings(settings).enabled ? 'auto' : 'off';
}
