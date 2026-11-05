import { defaultAiSkillsSettings, type AiLoadedSkill, type AiSkillsSettings } from '../types';

/** Fill fields omitted by configs written before T17. */
export function normalizeAiSkillsSettings(
  skills: Partial<AiSkillsSettings> | null | undefined,
): AiSkillsSettings {
  return {
    ...defaultAiSkillsSettings,
    ...skills,
    enabled: skills?.enabled === true,
    autoUse: skills?.autoUse === true,
    customDir: typeof skills?.customDir === 'string' ? skills.customDir : null,
  };
}

/** Read session metadata written by this version or an older one. */
export function normalizeLoadedSkills(value: unknown): AiLoadedSkill[] {
  if (!Array.isArray(value)) return [];
  return value.flatMap((entry) => {
    if (!entry || typeof entry !== 'object') return [];
    const record = entry as Record<string, unknown>;
    if (typeof record.name !== 'string' || !record.name.trim()) return [];
    return [
      {
        name: record.name,
        source: typeof record.source === 'string' ? record.source : undefined,
        loadedAt: typeof record.loadedAt === 'string' ? record.loadedAt : new Date().toISOString(),
      },
    ];
  });
}
