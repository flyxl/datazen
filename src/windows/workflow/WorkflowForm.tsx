import { Trash2 } from 'lucide-react';
import { Button } from '../../components/ui/Button';
import { Select } from '../../components/ui/Select';
import { useI18n } from '../../hooks/useI18n';
import type { WorkflowStepType } from '../../types';

export interface WorkflowStepDraft {
  type: WorkflowStepType;
  id: string;
  sql?: string;
  prompt?: string;
  connection?: string;
  database?: string;
}

export interface WorkflowDraft {
  id: string;
  name: string;
  description: string;
  variables: { name: string; varType: string; description: string; required: boolean }[];
  steps: WorkflowStepDraft[];
}

export function emptyDraft(): WorkflowDraft {
  return {
    id: '',
    name: '',
    description: '',
    variables: [],
    steps: [{ type: 'query' as WorkflowStepType, id: 'step1', sql: '' }],
  };
}

interface WorkflowFormProps {
  draft: WorkflowDraft;
  editingId: string | null;
  connections: { id: string; name: string; databaseType: string }[];
  onDraftChange: (d: WorkflowDraft) => void;
  onSave: () => void;
  onCancel: () => void;
}

export function WorkflowForm({
  draft, editingId, connections, onDraftChange, onSave, onCancel,
}: WorkflowFormProps) {
  const { t } = useI18n();
  const inputClass = 'w-full h-8 rounded border border-edge bg-surface-alt px-2.5 text-xs text-fg outline-none focus:border-accent';
  const textareaClass = 'w-full rounded border border-edge bg-surface-alt px-2.5 py-1.5 text-xs font-mono text-fg outline-none focus:border-accent resize-y min-h-[80px]';

  return (
    <div className="w-full max-w-3xl mx-auto p-6 space-y-4">
      <div className="grid grid-cols-2 gap-4">
        <div>
          <label className="text-xs text-fg-muted block mb-1">ID</label>
          <input className={inputClass} value={draft.id} onChange={(e) => onDraftChange({ ...draft, id: e.target.value })} disabled={!!editingId} />
        </div>
        <div>
          <label className="text-xs text-fg-muted block mb-1">{t('workflows.name')}</label>
          <input className={inputClass} value={draft.name} onChange={(e) => onDraftChange({ ...draft, name: e.target.value })} />
        </div>
      </div>
      <div>
        <label className="text-xs text-fg-muted block mb-1">{t('workflows.description')}</label>
        <input className={inputClass} value={draft.description} onChange={(e) => onDraftChange({ ...draft, description: e.target.value })} />
      </div>

      <div>
        <div className="flex items-center justify-between mb-2">
          <label className="text-xs text-fg-muted font-medium">{t('workflows.steps')}</label>
          <button type="button" onClick={() => onDraftChange({ ...draft, steps: [...draft.steps, { type: 'query', id: `step${draft.steps.length + 1}`, sql: '' }] })} className="text-accent text-xs hover:underline">
            + {t('workflows.addStep')}
          </button>
        </div>
        {draft.steps.map((step, i) => (
          <div key={i} className="mb-3 rounded-lg border border-edge p-3 space-y-2">
            <div className="flex items-center gap-2">
              <input className="h-7 w-28 rounded border border-edge bg-surface-alt px-2 text-xs text-fg outline-none focus:border-accent" value={step.id}
                onChange={(e) => { const s = [...draft.steps]; s[i] = { ...s[i], id: e.target.value }; onDraftChange({ ...draft, steps: s }); }} placeholder="step_id" />
              <Select value={step.type} options={[{ value: 'query', label: 'Query' }, { value: 'ai', label: 'AI' }]}
                onChange={(v) => { const s = [...draft.steps]; s[i] = { ...s[i], type: v as WorkflowStepType }; onDraftChange({ ...draft, steps: s }); }} className="!h-7 !text-xs w-24" />
              {connections.length > 0 && (
                <Select value={step.connection ?? ''} options={[{ value: '', label: t('workflows.defaultConn') }, ...connections.map((c) => ({ value: c.id, label: c.name }))]}
                  onChange={(v) => { const s = [...draft.steps]; s[i] = { ...s[i], connection: v || undefined }; onDraftChange({ ...draft, steps: s }); }} className="!h-7 !text-xs flex-1" />
              )}
              {draft.steps.length > 1 && (
                <button type="button" onClick={() => onDraftChange({ ...draft, steps: draft.steps.filter((_, j) => j !== i) })} className="text-red-400 hover:text-red-300 p-1 rounded hover:bg-red-500/10">
                  <Trash2 className="h-3.5 w-3.5" />
                </button>
              )}
            </div>
            {step.type === 'query' && (
              <textarea className={textareaClass} value={step.sql ?? ''} onChange={(e) => { const s = [...draft.steps]; s[i] = { ...s[i], sql: e.target.value }; onDraftChange({ ...draft, steps: s }); }} placeholder="SELECT ..." rows={4} />
            )}
            {step.type === 'ai' && (
              <textarea className={textareaClass} value={step.prompt ?? ''} onChange={(e) => { const s = [...draft.steps]; s[i] = { ...s[i], prompt: e.target.value }; onDraftChange({ ...draft, steps: s }); }} placeholder="AI prompt..." rows={4} />
            )}
          </div>
        ))}
      </div>

      <div className="flex gap-3 pt-2">
        <Button onClick={onSave} className="px-6">{t('common.save')}</Button>
        <Button variant="secondary" onClick={onCancel} className="px-6">{t('common.cancel')}</Button>
      </div>
    </div>
  );
}
