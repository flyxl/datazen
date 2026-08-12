import type { WorkflowStepType } from '../types';
import type { WorkflowDraft } from '../windows/workflow/WorkflowForm';

export function draftToYamlObject(d: WorkflowDraft): Record<string, unknown> {
  return {
    id: d.id.trim() || 'new-workflow',
    name: d.name.trim() || 'New Workflow',
    description: d.description.trim(),
    variables: d.variables.map((v) => ({
      name: v.name,
      type: v.varType,
      description: v.description,
      required: v.required,
    })),
    steps: d.steps.map((s) => ({
      type: s.type,
      id: s.id,
      sql: s.sql,
      prompt: s.prompt,
      connection: s.connection,
      database: s.database,
    })),
    schedule: d.scheduleEnabled
      ? { enabled: true, interval_secs: Math.max(30, d.scheduleIntervalSecs ?? 3600) }
      : { enabled: false },
  };
}

export function yamlObjectToDraft(obj: Record<string, unknown>): WorkflowDraft {
  const schedule = obj.schedule as { enabled?: boolean; interval_secs?: number } | undefined;
  const variables = Array.isArray(obj.variables)
    ? (obj.variables as Array<Record<string, unknown>>).map((v) => ({
        name: String(v.name ?? ''),
        varType: String(v.type ?? 'string'),
        description: String(v.description ?? ''),
        required: Boolean(v.required),
      }))
    : [];
  const steps = Array.isArray(obj.steps)
    ? (obj.steps as Array<Record<string, unknown>>).map((s) => ({
        type: String(s.type ?? 'query') as WorkflowStepType,
        id: String(s.id ?? ''),
        sql: String(s.sql ?? ''),
        prompt: String(s.prompt ?? ''),
        connection: s.connection != null ? String(s.connection) : undefined,
        database: s.database != null ? String(s.database) : undefined,
      }))
    : [];
  return {
    id: String(obj.id ?? ''),
    name: String(obj.name ?? ''),
    description: String(obj.description ?? ''),
    variables,
    steps,
    scheduleEnabled: schedule?.enabled ?? false,
    scheduleIntervalSecs: schedule?.interval_secs ?? 3600,
  };
}
