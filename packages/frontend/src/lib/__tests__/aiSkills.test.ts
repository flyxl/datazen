import { describe, expect, it } from 'vitest';
import { normalizeAiSkillsSettings, normalizeLoadedSkills } from '../aiSkills';

describe('aiSkills settings', () => {
  it('defaults and validates selected stable ids', () => {
    expect(normalizeAiSkillsSettings(undefined).selected).toEqual([]);
    expect(
      normalizeAiSkillsSettings({
        enabled: true,
        customDir: null,
        autoUse: false,
        selected: ['user:one', 'user:one', 3],
      } as never).selected,
    ).toEqual(['user:one']);
  });

  it('keeps skill ids when restoring a persisted session', () => {
    expect(
      normalizeLoadedSkills([
        { id: 'user:one', name: 'one', source: 'user', loadedAt: '2026-01-01T00:00:00Z' },
        { name: 'legacy', loadedAt: '2026-01-01T00:00:00Z' },
      ]),
    ).toEqual([
      expect.objectContaining({ id: 'user:one', name: 'one' }),
      expect.objectContaining({ name: 'legacy' }),
    ]);
  });
});
