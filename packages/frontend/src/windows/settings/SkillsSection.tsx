import { useCallback, useEffect, useState } from 'react';
import { FolderOpen, RefreshCw } from 'lucide-react';
import { Button } from '../../components/ui/Button';
import { PathInput } from '@datazen/ui';
import { useI18n } from '../../hooks/useI18n';
import { useAiStore } from '../../stores/aiStore';
import { aiCommands } from '../../commands/ai';
import { normalizeAiSkillsSettings } from '../../lib/aiSkills';
import { pickPath } from '../../lib/pathPicker';
import { tid } from '../../lib/tid';
import type { AiSkillSummary, AiSkillsSettings, AiSettingsConfig } from '../../types';
import { SectionTitle, ToggleRow } from './settingsUi';

/**
 * Settings → AI → Skills. The host owns discovery and path policy; this panel
 * only toggles opt-in switches and renders the name/description listing.
 */
export function SkillsSection({ config }: { config: AiSettingsConfig }) {
  const { t } = useI18n();
  const saveSettingsConfig = useAiStore((s) => s.saveSettingsConfig);
  const [skills, setSkills] = useState<AiSkillsSettings>(() =>
    normalizeAiSkillsSettings(config.skills),
  );
  const [customDir, setCustomDir] = useState(() => skills.customDir ?? '');
  const [listing, setListing] = useState<AiSkillSummary[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);

  useEffect(() => {
    const normalized = normalizeAiSkillsSettings(config.skills);
    setSkills(normalized);
    setCustomDir(normalized.customDir ?? '');
  }, [config.skills]);

  const refresh = useCallback(async () => {
    if (!skills.enabled) {
      setListing([]);
      return;
    }
    setLoading(true);
    setError(null);
    try {
      setListing(await aiCommands.listSkills());
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : String(cause));
    } finally {
      setLoading(false);
    }
  }, [skills.enabled]);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  const persist = async (next: AiSkillsSettings) => {
    setSkills(next);
    const ok = await saveSettingsConfig({ ...config, skills: next });
    if (!ok) setError(t('settings.ai.skills.loadError'));
  };

  const saveDirectory = async () => {
    await persist({ ...skills, customDir: customDir.trim() || null });
    await refresh();
  };

  return (
    <div className="space-y-3 border-t border-edge/60 pt-4" {...tid('ai-skills-section')}>
      <SectionTitle hint={t('settings.ai.skills.description')}>
        {t('settings.ai.skills.title')}
      </SectionTitle>

      <ToggleRow
        label={t('settings.ai.skills.enabled')}
        hint={t('settings.ai.skills.enabledHint')}
        checked={skills.enabled}
        onChange={(enabled) => void persist({ ...skills, enabled })}
        testId="ai-skills-enabled"
      />
      <ToggleRow
        label={t('settings.ai.skills.autoUse')}
        hint={t('settings.ai.skills.autoUseHint')}
        checked={skills.autoUse}
        onChange={(autoUse) => void persist({ ...skills, autoUse })}
        testId="ai-skills-auto-use"
      />

      <div>
        <p className="mb-1.5 text-xs font-medium text-fg">{t('settings.ai.skills.customDir')}</p>
        <div className="flex items-center gap-2">
          <PathInput
            value={customDir}
            onChange={setCustomDir}
            onBrowse={pickPath}
            placeholder="~/.agents/skills"
            dialogOptions={{ directory: true }}
            className="flex-1"
            inputTestId="ai-skills-dir"
          />
          <Button variant="secondary" className="h-9 shrink-0" onClick={() => void saveDirectory()}>
            {t('settings.ai.skills.saveDir')}
          </Button>
          <Button
            variant="ghost"
            className="h-9 w-9 shrink-0 px-0"
            onClick={() => void refresh()}
            title={t('settings.ai.skills.refresh')}
            aria-label={t('settings.ai.skills.refresh')}
            data-testid="ai-skills-refresh"
          >
            <RefreshCw className="h-4 w-4" />
          </Button>
        </div>
        <p className="mt-1 text-[11px] text-fg-muted">{t('settings.ai.skills.customDirHint')}</p>
      </div>

      {error && <p className="select-text text-xs text-red-500">{error}</p>}

      {!skills.enabled ? (
        <p className="text-xs text-fg-muted" data-testid="ai-skills-disabled">
          {t('settings.ai.skills.disabledHint')}
        </p>
      ) : loading ? (
        <p className="text-xs text-fg-muted">{t('settings.ai.skills.loading')}</p>
      ) : listing.length === 0 ? (
        <p className="text-xs text-fg-muted" data-testid="ai-skills-empty">
          {t('settings.ai.skills.empty')}
        </p>
      ) : (
        <div
          className="flex flex-col divide-y divide-edge rounded-xl border border-edge bg-surface"
          data-testid="ai-skills-list"
        >
          {listing.map((skill) => (
            <div
              key={skill.name}
              className="flex flex-col gap-1 px-3 py-2"
              data-testid={`ai-skills-row-${skill.name}`}
            >
              <div className="flex items-center justify-between gap-2">
                <span className="text-sm font-medium text-fg">{skill.name}</span>
                <span className="rounded bg-surface-alt px-1.5 py-0.5 text-[10px] text-fg-muted">
                  {skill.source === 'custom'
                    ? t('settings.ai.skills.sourceCustom')
                    : t('settings.ai.skills.sourceUser')}
                </span>
              </div>
              {skill.description && <p className="text-xs text-fg-muted">{skill.description}</p>}
            </div>
          ))}
        </div>
      )}

      <p className="flex items-center gap-1 text-[11px] text-fg-muted">
        <FolderOpen className="h-3 w-3" />
        {t('settings.ai.skills.defaultDir')}
      </p>
    </div>
  );
}
