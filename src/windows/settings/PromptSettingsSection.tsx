import { useCallback, useEffect, useState } from 'react';
import { Button } from '../../components/ui/Button';
import { Select } from '../../components/ui/Select';
import { useI18n } from '../../hooks/useI18n';
import { useConfirmDialog } from '../../hooks/useConfirmDialog';
import { cn } from '../../lib/cn';
import {
  aiCommands,
  type PromptInfo,
  type PromptOverrideEntry,
  type PromptScenario,
  type PromptSource,
} from '../../commands/ai';
import { DB_REGISTRY } from '../../lib/databaseTypes';
import type { DatabaseType } from '../../types';
import { SectionTitle, SettingRow } from './settingsUi';

const SCENARIO_VARIABLES: Record<PromptScenario, string[]> = {
  nl2sql: ['db_type', 'version', 'schema', 'recent'],
  diagnose: ['db_type', 'schema'],
  nl_filter: ['db_type', 'columns'],
  schema_doc_select_tables: ['db_type', 'table_names'],
  schema_doc: ['db_type', 'schema'],
  connection_diagnose: [],
  query_summary: [],
  explain_analysis: ['db_type'],
  chat: [],
  workflow_generate: ['db_type', 'schema', 'connections'],
};

const SOURCE_BADGE_CLASSES: Record<PromptSource, string> = {
  default: 'bg-edge text-fg-muted',
  driver: 'bg-blue-500/15 text-blue-500',
  user: 'bg-green-500/15 text-green-500',
};

export function PromptSettingsSection() {
  const { t } = useI18n();
  const [confirmReset, confirmResetDialog] = useConfirmDialog();
  const [driverType, setDriverType] = useState<string>('*');
  const [prompts, setPrompts] = useState<PromptInfo[]>([]);
  const [editingIdx, setEditingIdx] = useState<number | null>(null);
  const [editZh, setEditZh] = useState('');
  const [editEn, setEditEn] = useState('');
  const [feedback, setFeedback] = useState('');
  const driverOptions = [
    { value: '*', label: t('settings.prompts.allDrivers') },
    ...(Object.entries(DB_REGISTRY) as [DatabaseType, { label: string }][]).map(([key, meta]) => ({
      value: key,
      label: meta.label,
    })),
  ];

  const loadPrompts = useCallback(async () => {
    const dt = driverType === '*' ? undefined : driverType;
    const list = await aiCommands.promptList(dt);
    setPrompts(list);
    setEditingIdx(null);
  }, [driverType]);

  useEffect(() => {
    void loadPrompts();
  }, [loadPrompts]);

  const handleEdit = (idx: number) => {
    setEditingIdx(idx);
    setEditZh(prompts[idx].systemZh);
    setEditEn(prompts[idx].systemEn);
  };

  const handleSave = async () => {
    if (editingIdx === null) return;
    const p = prompts[editingIdx];
    const entry: PromptOverrideEntry = {
      driverType,
      scenario: p.scenario,
      systemZh: editZh,
      systemEn: editEn,
    };
    await aiCommands.promptSetOverride(entry);
    setFeedback(t('settings.prompts.saved'));
    setTimeout(() => setFeedback(''), 2000);
    setEditingIdx(null);
    void loadPrompts();
  };

  const handleReset = async (p: PromptInfo) => {
    const ok = await confirmReset({
      title: t('settings.prompts.reset'),
      message: t('settings.prompts.resetConfirm'),
      kind: 'warning',
    });
    if (!ok) return;
    await aiCommands.promptRemoveOverride(driverType, p.scenario);
    setFeedback(t('settings.prompts.resetDone'));
    setTimeout(() => setFeedback(''), 2000);
    void loadPrompts();
  };

  const sourceLabel = (source: PromptSource) => {
    return t(
      source === 'default'
        ? 'settings.prompts.source.default'
        : source === 'driver'
          ? 'settings.prompts.source.driver'
          : 'settings.prompts.source.user',
    );
  };

  const textareaClass =
    'w-full rounded-md border border-edge bg-surface px-3 py-2 text-xs font-mono text-fg outline-none focus:border-blue-500 resize-y min-h-[80px]';

  return (
    <>
      <SectionTitle>{t('settings.prompts')}</SectionTitle>
      <p className="text-xs text-fg-muted">{t('settings.prompts.description')}</p>

      <SettingRow label={t('settings.prompts.driver')}>
        <Select value={driverType} options={driverOptions} onChange={setDriverType} />
      </SettingRow>

      {feedback && <p className="text-xs text-green-500">{feedback}</p>}

      <div className="space-y-2">
        {prompts.map((p, idx) => {
          const isEditing = editingIdx === idx;
          const vars = SCENARIO_VARIABLES[p.scenario] ?? [];
          return (
            <div
              key={p.scenario}
              className="rounded-md border border-edge bg-surface p-3 space-y-2"
            >
              <div className="flex items-center justify-between">
                <div className="flex items-center gap-2">
                  <span className="text-sm font-medium text-fg">{p.label}</span>
                  <span
                    className={cn(
                      'rounded-full px-2 py-0.5 text-[10px] font-medium',
                      SOURCE_BADGE_CLASSES[p.source],
                    )}
                  >
                    {sourceLabel(p.source)}
                  </span>
                </div>
                <div className="flex items-center gap-1">
                  {!isEditing && (
                    <Button variant="secondary" onClick={() => handleEdit(idx)}>
                      {t('settings.prompts.edit')}
                    </Button>
                  )}
                  {p.source === 'user' && !isEditing && (
                    <Button variant="secondary" onClick={() => void handleReset(p)}>
                      {t('settings.prompts.reset')}
                    </Button>
                  )}
                </div>
              </div>

              {vars.length > 0 && (
                <p className="text-[10px] text-fg-muted">
                  {t('settings.prompts.variables')}: {vars.map((v) => `{{${v}}}`).join(', ')}
                </p>
              )}

              {isEditing ? (
                <div className="space-y-2">
                  <div>
                    <label className="block text-xs text-fg-secondary mb-1">
                      {t('settings.prompts.zh')}
                    </label>
                    <textarea
                      value={editZh}
                      onChange={(e) => setEditZh(e.target.value)}
                      rows={6}
                      className={textareaClass}
                    />
                  </div>
                  <div>
                    <label className="block text-xs text-fg-secondary mb-1">
                      {t('settings.prompts.en')}
                    </label>
                    <textarea
                      value={editEn}
                      onChange={(e) => setEditEn(e.target.value)}
                      rows={6}
                      className={textareaClass}
                    />
                  </div>
                  <div className="flex gap-2">
                    <Button variant="primary" onClick={() => void handleSave()}>
                      {t('common.save')}
                    </Button>
                    <Button variant="secondary" onClick={() => setEditingIdx(null)}>
                      {t('common.cancel')}
                    </Button>
                  </div>
                </div>
              ) : (
                <pre className="max-h-20 overflow-y-auto whitespace-pre-wrap text-[11px] text-fg-muted bg-surface-alt rounded p-2">
                  {p.systemZh.slice(0, 200)}
                  {p.systemZh.length > 200 ? '…' : ''}
                </pre>
              )}
            </div>
          );
        })}
      </div>
      {confirmResetDialog}
    </>
  );
}
