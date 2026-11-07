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
    steps: d.steps.map((s) => {
      if (s.type === 'command') {
        const input: Record<string, unknown> = { ...(s.input ?? {}) };
        if (s.database) input.database = s.database;
        return {
          type: s.type,
          id: s.id,
          command: s.command ?? '',
          connection: s.connection,
          input,
        };
      }
      return {
        type: s.type,
        id: s.id,
        sql: s.sql,
        prompt: s.prompt,
        connection: s.connection,
        database: s.database,
      };
    }),
    connection: d.connection,
    database: d.database,
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
    ? (obj.steps as Array<Record<string, unknown>>).map((s) => {
        // Command-step databases are persisted inside input.database.
        const input = (s.input ?? {}) as Record<string, unknown>;
        const database =
          s.database != null
            ? String(s.database)
            : input.database != null
              ? String(input.database)
              : undefined;
        return {
          type: String(s.type ?? 'query') as WorkflowStepType,
          id: String(s.id ?? ''),
          sql: String(s.sql ?? ''),
          prompt: String(s.prompt ?? ''),
          command: s.command != null ? String(s.command) : '',
          input,
          connection: s.connection != null ? String(s.connection) : undefined,
          database,
        };
      })
    : [];
  return {
    id: String(obj.id ?? ''),
    name: String(obj.name ?? ''),
    description: String(obj.description ?? ''),
    connection: obj.connection != null ? String(obj.connection) : undefined,
    database: obj.database != null ? String(obj.database) : undefined,
    variables,
    steps,
    scheduleEnabled: schedule?.enabled ?? false,
    scheduleIntervalSecs: schedule?.interval_secs ?? 3600,
  };
}
