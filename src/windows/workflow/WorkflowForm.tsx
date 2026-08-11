import { useEffect, useMemo, useRef, useState } from 'react';
import { Trash2 } from 'lucide-react';
import { Button } from '../../components/ui/Button';
import { Select } from '../../components/ui/Select';
import { useI18n } from '../../hooks/useI18n';
import { driverCommands } from '../../commands/driver';
import { aiCommands } from '../../commands/ai';
import { rememberWorkflowDraft } from './draftBridge';
import type { DriverCommandDefinition, WorkflowStepType } from '../../types';

export interface WorkflowStepDraft {
  type: WorkflowStepType;
  id: string;
  sql?: string;
  prompt?: string;
  connection?: string;
  database?: string;
  command?: string;
  input?: Record<string, unknown>;
}

export interface WorkflowDraft {
  id: string;
  name: string;
  description: string;
  connection?: string;
  variables: { name: string; varType: string; description: string; required: boolean }[];
  steps: WorkflowStepDraft[];
}

export function emptyDraft(): WorkflowDraft {
  return {
    id: '',
    name: '',
    description: '',
    connection: undefined,
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

function commandOptionLabel(definition: DriverCommandDefinition) {
  const category = definition.metadata?.category ? ` · ${definition.metadata.category}` : '';
  const deprecated = definition.metadata?.deprecated ? ' (deprecated)' : '';
  return `${definition.name}${category}${deprecated}`;
}

function schemaProperties(definition?: DriverCommandDefinition) {
  const schema = definition?.inputSchema;
  const properties = schema?.properties;
  if (!properties || typeof properties !== 'object' || Array.isArray(properties)) return [];
  return Object.entries(properties as Record<string, unknown>).map(([name, raw]) => ({
    name,
    schema: (raw && typeof raw === 'object' ? raw : {}) as Record<string, unknown>,
    required: Array.isArray(schema?.required) && schema.required.includes(name),
  }));
}

function defaultInput(definition?: DriverCommandDefinition): Record<string, unknown> {
  const input: Record<string, unknown> = {};
  for (const field of schemaProperties(definition)) {
    const type = field.schema.type;
    if (type === 'boolean') input[field.name] = false;
    else if (type === 'array') input[field.name] = [];
    else if (type === 'object') input[field.name] = {};
    else input[field.name] = '';
  }
  return input;
}

function CommandInputEditor({ definition, value, onChange }: { definition?: DriverCommandDefinition; value: Record<string, unknown>; onChange: (value: Record<string, unknown>) => void }) {
  const fields = schemaProperties(definition);
  const [rawJson, setRawJson] = useState(() => JSON.stringify(value, null, 2));
  const inputClass = 'w-full h-8 rounded border border-edge bg-surface-alt px-2.5 text-xs text-fg outline-none focus:border-accent';
  const textareaClass = 'w-full rounded border border-edge bg-surface-alt px-2.5 py-1.5 text-xs font-mono text-fg outline-none focus:border-accent resize-y min-h-[80px]';

  useEffect(() => { setRawJson(JSON.stringify(value, null, 2)); }, [definition?.id]);
  if (!definition) return <div className="text-xs text-fg-muted">Select a command to configure its input.</div>;

  const update = (name: string, next: unknown) => onChange({ ...value, [name]: next });
  return (
    <div className="space-y-2">
      {fields.map((field) => {
        const type = field.schema.type;
        const current = value[field.name];
        const label = typeof field.schema.title === 'string' && field.schema.title ? field.schema.title : field.name;
        if (type === 'boolean') return <label key={field.name} className="flex items-center gap-2 text-xs text-fg"><input type="checkbox" checked={Boolean(current)} onChange={(e) => update(field.name, e.target.checked)} />{label}{field.required ? ' *' : ''}</label>;
        if (type === 'array' || type === 'object') return (
          <div key={field.name}>
            <label className="text-xs text-fg-muted block mb-1">{label}{field.required ? ' *' : ''}</label>
            <textarea className={textareaClass} value={typeof current === 'string' ? current : JSON.stringify(current ?? (type === 'array' ? [] : {}), null, 2)} onChange={(e) => { const text = e.target.value; setRawJson(text); try { update(field.name, JSON.parse(text)); } catch { /* keep editing */ } }} />
          </div>
        );
        const isNumber = type === 'number' || type === 'integer';
        return (
          <div key={field.name}>
            <label className="text-xs text-fg-muted block mb-1">{label}{field.required ? ' *' : ''}</label>
            <input className={inputClass} type={isNumber ? 'number' : 'text'} value={current == null ? '' : String(current)} onChange={(e) => update(field.name, isNumber && e.target.value !== '' ? Number(e.target.value) : e.target.value)} placeholder={typeof field.schema.description === 'string' ? field.schema.description : undefined} />
          </div>
        );
      })}
      {fields.length === 0 && <textarea className={textareaClass} value={rawJson} onChange={(e) => { setRawJson(e.target.value); try { const parsed = JSON.parse(e.target.value); if (parsed && typeof parsed === 'object' && !Array.isArray(parsed)) onChange(parsed); } catch { /* keep editing */ } }} rows={5} aria-label="Command input JSON" />}
    </div>
  );
}

export function WorkflowForm({ draft, editingId, connections, onDraftChange, onSave, onCancel }: WorkflowFormProps) {
  const { t } = useI18n();
  const inputClass = 'w-full h-8 rounded border border-edge bg-surface-alt px-2.5 text-xs text-fg outline-none focus:border-accent';
  const textareaClass = 'w-full rounded border border-edge bg-surface-alt px-2.5 py-1.5 text-xs font-mono text-fg outline-none focus:border-accent resize-y min-h-[80px]';
  const [commandsByConnection, setCommandsByConnection] = useState<Record<string, DriverCommandDefinition[]>>({});
  const [loadingCommands, setLoadingCommands] = useState<Record<string, boolean>>({});
  const hydrated = useRef<string | null>(null);

  useEffect(() => {
    if (!editingId || !draft.id || hydrated.current === draft.id) return;
    hydrated.current = draft.id;
    let cancelled = false;
    void aiCommands.workflowGet(draft.id).then((workflow) => {
      if (cancelled) return;
      const nextSteps = draft.steps.map((step, index) => {
        const source = workflow.steps[index];
        if (!source || source.type !== 'command') return step;
        return { ...step, command: source.command, input: source.input ?? {}, connection: source.connection ?? step.connection };
      });
      onDraftChange({ ...draft, connection: draft.connection ?? workflow.connection, steps: nextSteps });
    }).catch(() => { /* legacy or unavailable backend; keep current draft */ });
    return () => { cancelled = true; };
  }, [editingId, draft.id]);

  useEffect(() => { rememberWorkflowDraft(draft); }, [draft]);

  const effectiveConnection = (step: WorkflowStepDraft) => step.connection || draft.connection || '';
  const commandConnections = useMemo(() => Array.from(new Set(draft.steps.filter((s) => s.type === 'command').map(effectiveConnection).filter(Boolean))), [draft.steps, draft.connection]);

  useEffect(() => {
    let cancelled = false;
    for (const connectionId of commandConnections) {
      if (commandsByConnection[connectionId]) continue;
      setLoadingCommands((prev) => ({ ...prev, [connectionId]: true }));
      void driverCommands.getConnectionCommands(connectionId).then((definitions) => {
        if (!cancelled) setCommandsByConnection((prev) => ({ ...prev, [connectionId]: definitions }));
      }).catch(() => {
        if (!cancelled) setCommandsByConnection((prev) => ({ ...prev, [connectionId]: [] }));
      }).finally(() => {
        if (!cancelled) setLoadingCommands((prev) => ({ ...prev, [connectionId]: false }));
      });
    }
    return () => { cancelled = true; };
  }, [commandConnections.join('|')]);

  const getDefinitions = (step: WorkflowStepDraft) =>
    (commandsByConnection[effectiveConnection(step)] ?? []).filter(
      (d) => d.metadata?.workflow !== false && (!d.metadata?.deprecated || d.id === step.command),
    );
  const getDefinition = (step: WorkflowStepDraft) => getDefinitions(step).find((d) => d.id === step.command);
  const setStep = (index: number, patch: Partial<WorkflowStepDraft>) => {
    const steps = [...draft.steps];
    steps[index] = { ...steps[index], ...patch };
    onDraftChange({ ...draft, steps });
  };

  return (
    <div className="w-full max-w-3xl mx-auto p-6 space-y-4">
      <div className="grid grid-cols-2 gap-4">
        <div><label className="text-xs text-fg-muted block mb-1">ID</label><input className={inputClass} value={draft.id} onChange={(e) => onDraftChange({ ...draft, id: e.target.value })} disabled={!!editingId} /></div>
        <div><label className="text-xs text-fg-muted block mb-1">{t('workflows.name')}</label><input className={inputClass} value={draft.name} onChange={(e) => onDraftChange({ ...draft, name: e.target.value })} /></div>
      </div>
      <div><label className="text-xs text-fg-muted block mb-1">{t('workflows.description')}</label><input className={inputClass} value={draft.description} onChange={(e) => onDraftChange({ ...draft, description: e.target.value })} /></div>

      <div>
        <label className="text-xs text-fg-muted block mb-1">Workflow Connection</label>
        <Select value={draft.connection ?? ''} options={[{ value: '', label: 'No default connection' }, ...connections.map((c) => ({ value: c.id, label: c.name }))]} onChange={(value) => onDraftChange({ ...draft, connection: value || undefined })} className="!h-8 !text-xs w-full" />
        <div className="text-[11px] text-fg-muted mt-1">Data-operation steps inherit this connection unless they override it.</div>
      </div>

      <div>
        <div className="flex items-center justify-between mb-2"><label className="text-xs text-fg-muted font-medium">{t('workflows.form.variables')}</label><button type="button" onMouseDown={(e) => e.preventDefault()} onClick={() => onDraftChange({ ...draft, variables: [...draft.variables, { name: '', varType: 'string', description: '', required: false }] })} className="text-accent text-xs hover:underline">+ {t('workflows.form.addVariable')}</button></div>
        {draft.variables.map((v, i) => (
          <div key={i} className="mb-2 flex items-center gap-2">
            <input className={inputClass} value={v.name} placeholder={t('workflows.form.varName')} style={{ width: '25%' }} onChange={(e) => { const vars = [...draft.variables]; vars[i] = { ...vars[i], name: e.target.value }; onDraftChange({ ...draft, variables: vars }); }} />
            <Select value={v.varType} options={[{ value: 'string', label: 'string' }, { value: 'number', label: 'number' }, { value: 'connection', label: t('workflows.form.varTypeConnection') }]} onChange={(val) => { const vars = [...draft.variables]; vars[i] = { ...vars[i], varType: val }; onDraftChange({ ...draft, variables: vars }); }} className="!h-8 !text-xs w-[20%]" />
            <input className={inputClass} value={v.description} placeholder={t('workflows.form.varDesc')} style={{ width: '35%' }} onChange={(e) => { const vars = [...draft.variables]; vars[i] = { ...vars[i], description: e.target.value }; onDraftChange({ ...draft, variables: vars }); }} />
            <label className="flex items-center gap-1 text-xs text-fg-muted whitespace-nowrap"><input type="checkbox" checked={v.required} onChange={(e) => { const vars = [...draft.variables]; vars[i] = { ...vars[i], required: e.target.checked }; onDraftChange({ ...draft, variables: vars }); }} />{t('workflows.form.varRequired')}</label>
            <button type="button" onMouseDown={(e) => e.preventDefault()} onClick={() => onDraftChange({ ...draft, variables: draft.variables.filter((_, j) => j !== i) })} className="p-1 text-fg-muted hover:text-red-400"><Trash2 className="h-3.5 w-3.5" /></button>
          </div>
        ))}
      </div>

      <div>
        <div className="flex items-center justify-between mb-2"><label className="text-xs text-fg-muted font-medium">{t('workflows.steps')}</label><button type="button" onMouseDown={(e) => e.preventDefault()} onClick={() => onDraftChange({ ...draft, steps: [...draft.steps, { type: 'command', id: `step${draft.steps.length + 1}`, command: '', input: {} }] })} className="text-accent text-xs hover:underline">+ {t('workflows.addStep')}</button></div>
        {draft.steps.map((step, i) => {
          const definitions = getDefinitions(step);
          const definition = getDefinition(step);
          const connId = effectiveConnection(step);
          const isLoading = Boolean(loadingCommands[connId]);
          return (
            <div key={i} className="mb-3 rounded-lg border border-edge p-3 space-y-2">
              <div className="flex items-center gap-2">
                <input className="h-7 w-28 rounded border border-edge bg-surface-alt px-2 text-xs text-fg outline-none focus:border-accent" value={step.id} onChange={(e) => setStep(i, { id: e.target.value })} placeholder="step_id" />
                <Select value={step.type} options={[{ value: 'query', label: 'Query (legacy)' }, { value: 'command', label: 'Command' }, { value: 'ai', label: 'AI' }]} onChange={(v) => setStep(i, { type: v as WorkflowStepType, command: v === 'command' ? step.command : undefined, input: v === 'command' ? step.input ?? {} : undefined })} className="!h-7 !text-xs w-32" />
                {(step.type === 'query' || step.type === 'command') && connections.length > 0 && <Select value={step.connection ?? ''} options={[{ value: '', label: draft.connection ? 'Inherited' : 'Select connection' }, ...connections.map((c) => ({ value: c.id, label: c.name }))]} onChange={(v) => setStep(i, { connection: v || undefined, command: v ? undefined : step.command })} className="!h-7 !text-xs flex-1" />}
                {draft.steps.length > 1 && <button type="button" onMouseDown={(e) => e.preventDefault()} onClick={() => onDraftChange({ ...draft, steps: draft.steps.filter((_, j) => j !== i) })} className="text-red-400 hover:text-red-300 p-1 rounded hover:bg-red-500/10"><Trash2 className="h-3.5 w-3.5" /></button>}
              </div>

              {step.type === 'command' && <>
                <div>
                  <label className="text-xs text-fg-muted block mb-1">Command</label>
                  <Select value={step.command ?? ''} options={[{ value: '', label: isLoading ? 'Loading commands…' : connId ? 'Select command' : 'Select a connection first' }, ...definitions.map((d) => ({ value: d.id, label: commandOptionLabel(d) }))]} onChange={(command) => { const next = definitions.find((d) => d.id === command); setStep(i, { command: command || undefined, input: next ? defaultInput(next) : {} }); }} className="!h-8 !text-xs w-full" />
                  {definition?.description && <div className="text-[11px] text-fg-muted mt-1">{definition.description}{definition.metadata?.risk ? ` · ${definition.metadata.risk}` : ''}</div>}
                </div>
                <CommandInputEditor definition={definition} value={step.input ?? {}} onChange={(input) => setStep(i, { input })} />
              </>}
              {step.type === 'query' && <textarea className={textareaClass} value={step.sql ?? ''} onChange={(e) => setStep(i, { sql: e.target.value })} placeholder="SELECT ..." rows={4} />}
              {step.type === 'ai' && <textarea className={textareaClass} value={step.prompt ?? ''} onChange={(e) => setStep(i, { prompt: e.target.value })} placeholder="AI prompt..." rows={4} />}
            </div>
          );
        })}
      </div>

      <div className="flex gap-3 pt-2"><Button onClick={onSave} className="px-6">{t('common.save')}</Button><Button variant="secondary" onClick={onCancel} className="px-6">{t('common.cancel')}</Button></div>
    </div>
  );
}
