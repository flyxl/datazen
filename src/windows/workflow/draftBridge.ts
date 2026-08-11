import type { WorkflowDefinition } from '../../types';
import type { WorkflowDraft } from './WorkflowForm';

const drafts = new Map<string, WorkflowDraft>();

export function rememberWorkflowDraft(draft: WorkflowDraft): void {
  if (draft.id) drafts.set(draft.id, draft);
}

export function forgetWorkflowDraft(id: string): void {
  drafts.delete(id);
}

/**
 * The legacy WorkflowWindow callback currently reconstructs WorkflowStep and
 * therefore only forwards the historical fields. Keep the richer editor draft
 * here until that callback is migrated, so Command fields are not silently lost.
 */
export function mergeDraftCommandFields(workflow: WorkflowDefinition): WorkflowDefinition {
  const draft = drafts.get(workflow.id);
  if (!draft) return workflow;

  return {
    ...workflow,
    connection: workflow.connection ?? draft.connection,
    steps: workflow.steps.map((step, index) => {
      const source = draft.steps[index];
      if (!source || source.type !== 'command' || step.type !== 'command') return step;
      return {
        ...step,
        command: source.command,
        input: source.input ?? {},
      };
    }),
  };
}
