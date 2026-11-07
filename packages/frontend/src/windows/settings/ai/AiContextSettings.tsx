import { useEffect, useMemo, useState } from 'react';
import { Pencil, Plus, Trash2 } from 'lucide-react';
import { Button } from '../../../components/ui/Button';
import { useI18n } from '../../../hooks/useI18n';
import { useAiStore } from '../../../stores/aiStore';
import type { AiSettingsConfig, PromptTemplate } from '../../../types';
import { SectionTitle } from '../settingsUi';

const MAX_GLOBAL_BYTES = 4 * 1024;
const MAX_TEMPLATE_BYTES = 8 * 1024;
const MAX_TOTAL_TEMPLATE_BYTES = 16 * 1024;

export function byteLength(value: string): number {
  return new TextEncoder().encode(value).length;
}

function newTemplate(): PromptTemplate {
  return {
    id: crypto.randomUUID(),
    name: '',
    content: '',
    databaseTypes: [],
    isDefault: false,
  };
}

function parseTypes(value: string): string[] {
  return value
    .split(',')
    .map((item) => item.trim())
    .filter(Boolean);
}

export function AiContextSettings({ config }: { config: AiSettingsConfig }) {
  const { t } = useI18n();
  const saveSettingsConfig = useAiStore((s) => s.saveSettingsConfig);
  const saving = useAiStore((s) => s.saving);

  const [instructions, setInstructions] = useState(config.globalInstructions ?? '');
  const [templates, setTemplates] = useState<PromptTemplate[]>(config.promptTemplates ?? []);
  const [editing, setEditing] = useState<PromptTemplate | null>(null);
  const [editingIsNew, setEditingIsNew] = useState(false);
  const [editorError, setEditorError] = useState('');
  const [saved, setSaved] = useState(false);

  useEffect(() => {
    setInstructions(config.globalInstructions ?? '');
    setTemplates(config.promptTemplates ?? []);
  }, [config.globalInstructions, config.promptTemplates]);

  const instructionBytes = byteLength(instructions);
  const totalTemplateBytes = useMemo(
    () => templates.reduce((sum, template) => sum + byteLength(template.content), 0),
    [templates],
  );
  const overLimit =
    instructionBytes > MAX_GLOBAL_BYTES ||
    totalTemplateBytes > MAX_TOTAL_TEMPLATE_BYTES ||
    templates.some((template) => byteLength(template.content) > MAX_TEMPLATE_BYTES);

  const beginCreate = () => {
    setEditing(newTemplate());
    setEditingIsNew(true);
    setEditorError('');
  };

  const beginEdit = (template: PromptTemplate) => {
    setEditing({ ...template, databaseTypes: [...template.databaseTypes] });
    setEditingIsNew(false);
    setEditorError('');
  };

  const applyEditor = () => {
    if (!editing) return;
    if (!editing.name.trim()) {
      setEditorError(t('settings.ai.templateNameRequired'));
      return;
    }
    if (!editing.content.trim()) {
      setEditorError(t('settings.ai.templateContentRequired'));
      return;
    }
    const contentBytes = byteLength(editing.content);
    if (contentBytes > MAX_TEMPLATE_BYTES) {
      setEditorError(t('settings.ai.templateTooLarge', { max: MAX_TEMPLATE_BYTES }));
      return;
    }
    setTemplates((current) =>
      editingIsNew
        ? [...current, { ...editing, name: editing.name.trim() }]
        : current.map((template) =>
            template.id === editing.id ? { ...editing, name: editing.name.trim() } : template,
          ),
    );
    setEditing(null);
    setEditorError('');
  };

  const removeTemplate = (id: string) => {
    setTemplates((current) => current.filter((template) => template.id !== id));
  };

  const handleSave = async () => {
    if (overLimit) return;
    const ok = await saveSettingsConfig({
      ...config,
      globalInstructions: instructions,
      promptTemplates: templates,
    });
    if (ok) {
      setSaved(true);
      setTimeout(() => setSaved(false), 2000);
    }
  };

  return (
    <div className="space-y-5" data-testid="ai-context-settings">
      <div className="space-y-2">
        <SectionTitle hint={t('settings.ai.instructionsHint')}>
          {t('settings.ai.instructionsTitle')}
        </SectionTitle>
        <textarea
          data-testid="ai-global-instructions"
          className="min-h-[88px] w-full resize-y rounded border border-edge bg-surface px-2 py-1.5 text-sm text-fg outline-none focus:border-accent"
          value={instructions}
          onChange={(event) => setInstructions(event.target.value)}
          placeholder={t('settings.ai.instructionsPlaceholder')}
        />
        <div
          className={`text-[11px] ${instructionBytes > MAX_GLOBAL_BYTES ? 'text-danger' : 'text-fg-muted'}`}
          data-testid="ai-global-instructions-count"
        >
          {instructionBytes} / {MAX_GLOBAL_BYTES} bytes
          {instructionBytes > MAX_GLOBAL_BYTES && ` · ${t('settings.ai.overLimitHint')}`}
        </div>
      </div>

      <div className="space-y-2">
        <div className="flex items-center justify-between">
          <SectionTitle hint={t('settings.ai.templatesHint')}>
            {t('settings.ai.templatesTitle')}
          </SectionTitle>
          <Button variant="secondary" className="h-7 px-2 text-xs" onClick={beginCreate}>
            <Plus className="mr-1 h-3 w-3" />
            {t('settings.ai.addTemplate')}
          </Button>
        </div>

        {templates.length === 0 && (
          <p className="text-xs text-fg-muted" data-testid="ai-templates-empty">
            {t('settings.ai.templatesEmpty')}
          </p>
        )}

        <div className="space-y-1.5">
          {templates.map((template) => {
            const bytes = byteLength(template.content);
            return (
              <div
                key={template.id}
                className="flex items-center gap-2 rounded border border-edge/70 bg-surface px-2 py-1.5"
                data-testid={`ai-template-row-${template.id}`}
              >
                <div className="min-w-0 flex-1">
                  <div className="flex items-center gap-1.5">
                    <span className="truncate text-xs font-medium text-fg">{template.name}</span>
                    {template.isDefault && (
                      <span className="rounded bg-accent/15 px-1 text-[9px] text-accent">
                        {t('settings.ai.defaultBadge')}
                      </span>
                    )}
                  </div>
                  <div className="truncate text-[10px] text-fg-muted">
                    {template.databaseTypes.length > 0
                      ? template.databaseTypes.join(', ')
                      : t('settings.ai.allDatabaseTypes')}
                    {' · '}
                    <span className={bytes > MAX_TEMPLATE_BYTES ? 'text-danger' : undefined}>
                      {bytes} / {MAX_TEMPLATE_BYTES} bytes
                    </span>
                  </div>
                </div>
                <Button
                  variant="ghost"
                  className="h-6 w-6 px-0"
                  aria-label={t('common.edit')}
                  onClick={() => beginEdit(template)}
                >
                  <Pencil className="h-3 w-3" />
                </Button>
                <Button
                  variant="ghost"
                  className="h-6 w-6 px-0 text-danger"
                  aria-label={t('common.delete')}
                  onClick={() => removeTemplate(template.id)}
                >
                  <Trash2 className="h-3 w-3" />
                </Button>
              </div>
            );
          })}
        </div>
        <div
          className={`text-[11px] ${totalTemplateBytes > MAX_TOTAL_TEMPLATE_BYTES ? 'text-danger' : 'text-fg-muted'}`}
          data-testid="ai-templates-total-count"
        >
          {totalTemplateBytes} / {MAX_TOTAL_TEMPLATE_BYTES} bytes
          {totalTemplateBytes > MAX_TOTAL_TEMPLATE_BYTES && ` · ${t('settings.ai.overLimitHint')}`}
        </div>
      </div>

      {editing && (
        <div
          className="space-y-2 rounded border border-edge bg-surface-alt/40 p-2"
          data-testid="ai-template-editor"
        >
          <input
            data-testid="ai-template-name"
            className="w-full rounded border border-edge bg-surface px-2 py-1 text-xs"
            value={editing.name}
            onChange={(event) => setEditing({ ...editing, name: event.target.value })}
            placeholder={t('settings.ai.templateNamePlaceholder')}
          />
          <input
            data-testid="ai-template-database-types"
            className="w-full rounded border border-edge bg-surface px-2 py-1 text-xs"
            value={editing.databaseTypes.join(', ')}
            onChange={(event) =>
              setEditing({ ...editing, databaseTypes: parseTypes(event.target.value) })
            }
            placeholder={t('settings.ai.templateDatabaseTypesPlaceholder')}
          />
          <textarea
            data-testid="ai-template-content"
            className="min-h-[100px] w-full resize-y rounded border border-edge bg-surface px-2 py-1 text-xs"
            value={editing.content}
            onChange={(event) => setEditing({ ...editing, content: event.target.value })}
            placeholder={t('settings.ai.templateContentPlaceholder')}
          />
          <div
            className={`text-[10px] ${byteLength(editing.content) > MAX_TEMPLATE_BYTES ? 'text-danger' : 'text-fg-muted'}`}
            data-testid="ai-template-content-count"
          >
            {byteLength(editing.content)} / {MAX_TEMPLATE_BYTES} bytes
          </div>
          <label className="flex items-center gap-1.5 text-xs text-fg">
            <input
              type="checkbox"
              checked={editing.isDefault}
              onChange={(event) => setEditing({ ...editing, isDefault: event.target.checked })}
            />
            {t('settings.ai.setDefault')}
          </label>
          {editorError && <p className="text-[11px] text-danger">{editorError}</p>}
          <div className="flex justify-end gap-1.5">
            <Button variant="ghost" className="h-7 px-2 text-xs" onClick={() => setEditing(null)}>
              {t('common.cancel')}
            </Button>
            <Button variant="secondary" className="h-7 px-2 text-xs" onClick={applyEditor}>
              {t('common.confirm')}
            </Button>
          </div>
        </div>
      )}

      <div className="flex items-center gap-2">
        <Button
          variant="primary"
          className="h-8 px-3 text-xs"
          disabled={saving || overLimit}
          onClick={() => void handleSave()}
          data-testid="ai-context-save"
        >
          {saving
            ? t('settings.ai.saving')
            : saved
              ? t('common.success')
              : t('settings.ai.saveContext')}
        </Button>
        {overLimit && (
          <span className="text-[11px] text-danger">{t('settings.ai.overLimitHint')}</span>
        )}
      </div>
    </div>
  );
}
