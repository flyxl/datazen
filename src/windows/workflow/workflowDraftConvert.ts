import type { WorkflowDefinition, WorkflowStepType } from '../../types';
import type { WorkflowDraft } from './WorkflowForm';

export function workflowDefinitionToDraft(workflow: WorkflowDefinition): WorkflowDraft {
  return {
    id: workflow.id,
    name: workflow.name,
    description: workflow.description,
    connection: workflow.connection,
    variables: workflow.variables.map((v) => ({
      name: v.name,
      varType: v.type || 'string',
      description: v.description,
      required: v.required ?? false,
    })),
    steps: workflow.steps.map((s) => ({
      type: s.type as WorkflowStepType,
      id: s.id,
      sql: s.sql,
      prompt: s.prompt,
      connection: s.connection,
      database: s.database,
      command: s.type === 'command' ? s.command : undefined,
      input: s.type === 'command' ? (s.input ?? {}) : undefined,
    })),
    scheduleEnabled: workflow.schedule?.enabled ?? false,
    scheduleIntervalSecs:
      workflow.schedule?.interval_secs ?? workflow.schedule?.intervalSecs ?? 3600,
  };
}

export function workflowDraftToDefinition(draft: WorkflowDraft): WorkflowDefinition {
  return {
    id: draft.id.trim(),
    name: draft.name.trim(),
    description: draft.description.trim(),
    connection: draft.connection,
    variables: draft.variables.map((v) => ({
      name: v.name,
      type: v.varType,
      description: v.description,
      required: v.required,
    })),
    steps: draft.steps.map((s) => {
      if (s.type === 'command') {
        return {
          type: 'command' as const,
          id: s.id,
          command: s.command ?? '',
          input: s.input ?? {},
          connection: s.connection,
        };
      }
      if (s.type === 'ai') {
        return { type: 'ai' as const, id: s.id, prompt: s.prompt ?? '' };
      }
      return {
        type: 'query' as const,
        id: s.id,
        sql: s.sql ?? '',
        connection: s.connection,
        database: s.database,
      };
    }),
    schedule: draft.scheduleEnabled
      ? { enabled: true, interval_secs: Math.max(30, draft.scheduleIntervalSecs ?? 3600) }
      : { enabled: false },
  };
}
