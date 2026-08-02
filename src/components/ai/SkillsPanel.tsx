import { useCallback, useEffect, useState } from 'react';
import {
  FolderOpen,
  Loader2,
  Pencil,
  Play,
  Plus,
  RefreshCw,
  Trash2,
  Wand2,
} from 'lucide-react';
import { useI18n } from '../../hooks/useI18n';
import { useAiStore } from '../../stores/aiStore';
import { aiCommands } from '../../commands/ai';
import type { SkillDefinition, SkillListItem } from '../../types';

interface SkillsPanelProps {
  connectionId?: string;
}

interface SkillStepDraft {
  type: 'query' | 'ai';
  id: string;
  sql?: string;
  prompt?: string;
}

interface SkillVariableDraft {
  name: string;
  varType: string;
  description: string;
  required: boolean;
}

function emptyDraft(): {
  id: string;
  name: string;
  description: string;
  variables: SkillVariableDraft[];
  steps: SkillStepDraft[];
} {
  return {
    id: '',
    name: '',
    description: '',
    variables: [],
    steps: [{ type: 'query', id: 'step1', sql: '' }],
  };
}

export function SkillsPanel({ connectionId }: SkillsPanelProps) {
  const { t } = useI18n();
  const skills = useAiStore((s) => s.skills);
  const skillsLoading = useAiStore((s) => s.skillsLoading);
  const loadSkills = useAiStore((s) => s.loadSkills);
  const executeSkill = useAiStore((s) => s.executeSkill);
  const result = useAiStore((s) => s.skillExecutionResult);
  const isExecuting = useAiStore((s) => s.isExecutingSkill);
  const skillError = useAiStore((s) => s.skillError);
  const clearSkillResult = useAiStore((s) => s.clearSkillResult);

  const [selectedSkill, setSelectedSkill] = useState<SkillListItem | null>(null);
  const [variables, setVariables] = useState<Record<string, string>>({});
  const [skillsDir, setSkillsDir] = useState('');
  const [showForm, setShowForm] = useState(false);
  const [editingId, setEditingId] = useState<string | null>(null);
  const [draft, setDraft] = useState(emptyDraft());
  const [feedback, setFeedback] = useState('');

  useEffect(() => {
    void loadSkills();
    void aiCommands.skillGetDir().then(setSkillsDir);
  }, [loadSkills]);

  const handleSelect = (skill: SkillListItem) => {
    setSelectedSkill(skill);
    clearSkillResult();
    const defaults: Record<string, string> = {};
    for (const v of skill.variables) {
      defaults[v.name] = v.default != null ? String(v.default) : '';
    }
    setVariables(defaults);
  };

  const handleExecute = async () => {
    if (!selectedSkill) return;
    await executeSkill({ skillId: selectedSkill.id, variables, connectionId });
  };

  const handleCreate = () => {
    setDraft(emptyDraft());
    setEditingId(null);
    setShowForm(true);
  };

  const handleEdit = async (skillId: string) => {
    try {
      const skill: SkillDefinition = await aiCommands.skillGet(skillId);
      setDraft({
        id: skill.id,
        name: skill.name,
        description: skill.description,
        variables: skill.variables.map((v) => ({
          name: v.name,
          varType: v.type || 'string',
          description: v.description,
          required: v.required ?? false,
        })),
        steps: skill.steps.map((s) => {
          if ('sql' in s) return { type: 'query' as const, id: s.id, sql: s.sql };
          return { type: 'ai' as const, id: s.id, prompt: s.prompt };
        }),
      });
      setEditingId(skillId);
      setShowForm(true);
    } catch (e) {
      setFeedback(String(e));
    }
  };

  const handleDelete = async (skillId: string) => {
    if (!confirm(t('skills.deleteConfirm'))) return;
    await aiCommands.skillDelete(skillId);
    if (selectedSkill?.id === skillId) {
      setSelectedSkill(null);
      clearSkillResult();
    }
    void loadSkills();
  };

  const handleSave = async () => {
    const skill: SkillDefinition = {
      id: draft.id.trim(),
      name: draft.name.trim(),
      description: draft.description.trim(),
      variables: draft.variables.map((v) => ({
        name: v.name,
        type: v.varType,
        description: v.description,
        required: v.required,
      })),
      steps: draft.steps.map((s) => {
        if (s.type === 'query') return { type: 'query' as const, id: s.id, sql: s.sql ?? '' };
        return { type: 'ai' as const, id: s.id, prompt: s.prompt ?? '' };
      }),
    };
    try {
      await aiCommands.skillSave(skill);
      setShowForm(false);
      setFeedback(t('skills.saved'));
      setTimeout(() => setFeedback(''), 2000);
      void loadSkills();
    } catch (e) {
      setFeedback(String(e));
    }
  };

  const handleReload = useCallback(async () => {
    await aiCommands.skillReload();
    void loadSkills();
  }, [loadSkills]);

  const addStep = (type: 'query' | 'ai') => {
    const idx = draft.steps.length + 1;
    setDraft((prev) => ({
      ...prev,
      steps: [
        ...prev.steps,
        { type, id: `step${idx}`, sql: type === 'query' ? '' : undefined, prompt: type === 'ai' ? '' : undefined },
      ],
    }));
  };

  const removeStep = (idx: number) => {
    setDraft((prev) => ({ ...prev, steps: prev.steps.filter((_, i) => i !== idx) }));
  };

  const updateStep = (idx: number, field: string, value: string) => {
    setDraft((prev) => ({
      ...prev,
      steps: prev.steps.map((s, i) => (i === idx ? { ...s, [field]: value } : s)),
    }));
  };

  const addVariable = () => {
    setDraft((prev) => ({
      ...prev,
      variables: [...prev.variables, { name: '', varType: 'string', description: '', required: false }],
    }));
  };

  const removeVariable = (idx: number) => {
    setDraft((prev) => ({ ...prev, variables: prev.variables.filter((_, i) => i !== idx) }));
  };

  const inputClass =
    'w-full h-7 rounded border border-edge bg-surface px-2 text-xs text-fg outline-none focus:border-accent';
  const textareaClass =
    'w-full rounded border border-edge bg-surface px-2 py-1 text-xs font-mono text-fg outline-none focus:border-accent resize-y min-h-[60px]';

  if (skillsLoading) {
    return (
      <div className="flex items-center justify-center py-4 text-fg-muted text-xs">
        <Loader2 className="h-4 w-4 animate-spin mr-1" />
        {t('skills.loading')}
      </div>
    );
  }

  return (
    <div className="space-y-3">
      {/* Header */}
      <div className="flex items-center justify-between">
        <h3 className="text-sm font-medium flex items-center gap-2">
          <Wand2 className="w-4 h-4" />
          {t('skills.title')}
        </h3>
        <div className="flex items-center gap-1">
          <button
            type="button"
            onClick={handleCreate}
            className="flex items-center gap-1 px-2 py-1 text-[11px] text-accent hover:bg-accent/10 rounded transition-colors"
            title={t('skills.create')}
          >
            <Plus className="h-3.5 w-3.5" />
            {t('skills.create')}
          </button>
          <button
            type="button"
            onClick={() => void handleReload()}
            className="p-1 text-fg-muted hover:text-fg rounded transition-colors"
            title={t('skills.reload')}
          >
            <RefreshCw className="h-3.5 w-3.5" />
          </button>
        </div>
      </div>

      {/* Storage path */}
      {skillsDir && (
        <div className="flex items-start gap-2 rounded-md border border-edge bg-surface-alt/50 p-2">
          <FolderOpen className="h-3.5 w-3.5 shrink-0 mt-0.5 text-fg-muted" />
          <div>
            <div className="text-[10px] text-fg-muted">{t('skills.storageDir')}</div>
            <code className="text-[11px] text-fg-secondary break-all select-all">{skillsDir}</code>
            <div className="text-[10px] text-fg-muted mt-0.5">{t('skills.storageDirHint')}</div>
          </div>
        </div>
      )}

      {feedback && (
        <p className={`text-xs ${feedback.startsWith('Error') || feedback.startsWith('error') ? 'text-red-400' : 'text-green-500'}`}>{feedback}</p>
      )}

      {/* Create/Edit form */}
      {showForm && (
        <div className="space-y-2 border border-edge rounded-md p-3 bg-surface">
          <h4 className="text-xs font-medium text-fg">
            {editingId ? t('skills.edit') : t('skills.create')}
          </h4>

          <div>
            <label className="text-[10px] text-fg-muted block mb-0.5">{t('skills.form.id')}</label>
            <input
              type="text"
              value={draft.id}
              onChange={(e) => setDraft((p) => ({ ...p, id: e.target.value }))}
              className={inputClass}
              placeholder={t('skills.form.idPlaceholder')}
              disabled={!!editingId}
            />
          </div>

          <div>
            <label className="text-[10px] text-fg-muted block mb-0.5">{t('skills.form.name')}</label>
            <input
              type="text"
              value={draft.name}
              onChange={(e) => setDraft((p) => ({ ...p, name: e.target.value }))}
              className={inputClass}
              placeholder={t('skills.form.namePlaceholder')}
            />
          </div>

          <div>
            <label className="text-[10px] text-fg-muted block mb-0.5">{t('skills.form.description')}</label>
            <input
              type="text"
              value={draft.description}
              onChange={(e) => setDraft((p) => ({ ...p, description: e.target.value }))}
              className={inputClass}
              placeholder={t('skills.form.descriptionPlaceholder')}
            />
          </div>

          {/* Variables */}
          <div>
            <div className="flex items-center justify-between mb-1">
              <label className="text-[10px] text-fg-muted">{t('skills.form.variables')}</label>
              <button
                type="button"
                onClick={addVariable}
                className="text-[10px] text-accent hover:underline"
              >
                {t('skills.form.addVariable')}
              </button>
            </div>
            {draft.variables.map((v, i) => (
              <div key={i} className="flex items-center gap-1 mb-1">
                <input
                  type="text"
                  value={v.name}
                  onChange={(e) => {
                    const vars = [...draft.variables];
                    vars[i] = { ...vars[i], name: e.target.value };
                    setDraft((p) => ({ ...p, variables: vars }));
                  }}
                  className={inputClass}
                  placeholder={t('skills.form.varName')}
                  style={{ width: '25%' }}
                />
                <input
                  type="text"
                  value={v.description}
                  onChange={(e) => {
                    const vars = [...draft.variables];
                    vars[i] = { ...vars[i], description: e.target.value };
                    setDraft((p) => ({ ...p, variables: vars }));
                  }}
                  className={inputClass}
                  placeholder={t('skills.form.varDesc')}
                  style={{ width: '45%' }}
                />
                <label className="flex items-center gap-0.5 text-[10px] text-fg-muted whitespace-nowrap">
                  <input
                    type="checkbox"
                    checked={v.required}
                    onChange={(e) => {
                      const vars = [...draft.variables];
                      vars[i] = { ...vars[i], required: e.target.checked };
                      setDraft((p) => ({ ...p, variables: vars }));
                    }}
                  />
                  {t('skills.form.varRequired')}
                </label>
                <button
                  type="button"
                  onClick={() => removeVariable(i)}
                  className="p-0.5 text-fg-muted hover:text-red-400"
                >
                  <Trash2 className="h-3 w-3" />
                </button>
              </div>
            ))}
          </div>

          {/* Steps */}
          <div>
            <label className="text-[10px] text-fg-muted block mb-1">{t('skills.form.steps')}</label>
            {draft.steps.map((step, i) => (
              <div key={i} className="mb-2 rounded border border-edge p-2 space-y-1 bg-surface-alt/30">
                <div className="flex items-center justify-between">
                  <span className="text-[10px] font-medium text-fg-secondary">
                    #{i + 1} {step.type === 'query' ? t('skills.form.sql') : t('skills.form.prompt')}
                  </span>
                  <button
                    type="button"
                    onClick={() => removeStep(i)}
                    className="text-[10px] text-red-400 hover:underline"
                  >
                    {t('skills.form.removeStep')}
                  </button>
                </div>
                <div className="flex items-center gap-1">
                  <label className="text-[10px] text-fg-muted w-12 shrink-0">{t('skills.form.stepId')}</label>
                  <input
                    type="text"
                    value={step.id}
                    onChange={(e) => updateStep(i, 'id', e.target.value)}
                    className={inputClass}
                  />
                </div>
                {step.type === 'query' ? (
                  <textarea
                    value={step.sql ?? ''}
                    onChange={(e) => updateStep(i, 'sql', e.target.value)}
                    className={textareaClass}
                    rows={3}
                    placeholder={t('skills.form.sql')}
                  />
                ) : (
                  <textarea
                    value={step.prompt ?? ''}
                    onChange={(e) => updateStep(i, 'prompt', e.target.value)}
                    className={textareaClass}
                    rows={3}
                    placeholder={t('skills.form.prompt')}
                  />
                )}
              </div>
            ))}
            <div className="flex gap-2">
              <button
                type="button"
                onClick={() => addStep('query')}
                className="text-[10px] text-accent hover:underline"
              >
                {t('skills.form.addQueryStep')}
              </button>
              <button
                type="button"
                onClick={() => addStep('ai')}
                className="text-[10px] text-accent hover:underline"
              >
                {t('skills.form.addAiStep')}
              </button>
            </div>
          </div>

          {/* Actions */}
          <div className="flex gap-2 pt-1">
            <button
              type="button"
              className="px-3 py-1 text-xs bg-accent text-white rounded hover:bg-accent/90 transition-colors disabled:opacity-50"
              onClick={() => void handleSave()}
              disabled={!draft.id.trim() || !draft.name.trim() || draft.steps.length === 0}
            >
              {t('common.save')}
            </button>
            <button
              type="button"
              className="px-3 py-1 text-xs text-fg-secondary border border-edge rounded hover:bg-surface-raised transition-colors"
              onClick={() => setShowForm(false)}
            >
              {t('common.cancel')}
            </button>
          </div>
        </div>
      )}

      {/* Skills list */}
      {skills.length === 0 && !showForm ? (
        <div className="py-4 text-center text-xs text-fg-muted">
          {t('skills.empty')}
        </div>
      ) : (
        <div className="space-y-1">
          {skills.map((skill) => (
            <div
              key={skill.id}
              className={`flex items-center justify-between px-3 py-2 rounded-md text-sm transition-colors cursor-pointer ${
                selectedSkill?.id === skill.id
                  ? 'bg-accent/10 text-accent'
                  : 'hover:bg-surface-raised text-fg-secondary'
              }`}
              onClick={() => handleSelect(skill)}
            >
              <div className="min-w-0 flex-1">
                <div className="font-medium">{skill.name}</div>
                <div className="text-[11px] text-fg-muted truncate">{skill.description}</div>
              </div>
              <div className="flex items-center gap-0.5 shrink-0 ml-2">
                <button
                  type="button"
                  onClick={(e) => { e.stopPropagation(); void handleEdit(skill.id); }}
                  className="p-1 text-fg-muted hover:text-fg rounded transition-colors"
                  title={t('skills.edit')}
                >
                  <Pencil className="h-3 w-3" />
                </button>
                <button
                  type="button"
                  onClick={(e) => { e.stopPropagation(); void handleDelete(skill.id); }}
                  className="p-1 text-fg-muted hover:text-red-400 rounded transition-colors"
                  title={t('skills.delete')}
                >
                  <Trash2 className="h-3 w-3" />
                </button>
              </div>
            </div>
          ))}
        </div>
      )}

      {/* Execute selected skill */}
      {selectedSkill && !showForm && (
        <div className="space-y-2 border-t border-edge pt-3">
          {selectedSkill.variables.map((v) => (
            <div key={v.name}>
              <label className="text-[11px] text-fg-muted block mb-0.5">
                {v.description || v.name}
                {v.required && <span className="text-red-400 ml-0.5">*</span>}
              </label>
              <input
                type="text"
                value={variables[v.name] ?? ''}
                onChange={(e) =>
                  setVariables((prev) => ({ ...prev, [v.name]: e.target.value }))
                }
                className={inputClass}
                placeholder={v.name}
                disabled={isExecuting}
              />
            </div>
          ))}

          <button
            type="button"
            className="flex items-center gap-1.5 px-3 py-1.5 text-xs bg-accent text-white rounded hover:bg-accent/90 disabled:opacity-50 transition-colors"
            onClick={() => void handleExecute()}
            disabled={isExecuting}
          >
            {isExecuting ? (
              <Loader2 className="h-3.5 w-3.5 animate-spin" />
            ) : (
              <Play className="h-3.5 w-3.5" />
            )}
            {isExecuting ? t('skills.running') : t('skills.run')}
          </button>
        </div>
      )}

      {skillError && (
        <div className="text-xs text-red-400 rounded bg-red-500/10 p-2">
          {skillError}
        </div>
      )}

      {result && (
        <div className="border border-edge rounded-md p-3 bg-surface">
          <div className="text-[11px] text-fg-muted mb-1">{t('skills.result')}</div>
          <pre className="text-xs text-fg-secondary whitespace-pre-wrap break-words max-h-60 overflow-auto">
            {result}
          </pre>
        </div>
      )}
    </div>
  );
}
