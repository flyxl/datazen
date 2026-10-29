import { describe, expect, it } from 'vitest';
import { workflowDraftToDefinition } from '../workflowDraftConvert';
import { draftToYamlObject, yamlObjectToDraft } from '../../../lib/workflowDraftYaml';
import { emptyDraft } from '../WorkflowForm';

/** `draftToYamlObject` hands back a loose record; these tests read its `steps` list. */
function yamlSteps(yamlObj: Record<string, unknown>): Array<Record<string, unknown>> {
  return yamlObj.steps as Array<Record<string, unknown>>;
}

/**
 * Regression: a command step's database was silently dropped on the way to the
 * backend. The form offers a step-level database dropdown for command steps,
 * but `WorkflowStep::Command` has no `database` field in Rust — its target
 * lives in `input.database`. Selecting a database therefore had no effect and
 * the step fell through to the driver's built-in default.
 */
describe('command step database survives conversion', () => {
  const base = () => {
    const d = emptyDraft();
    return {
      ...d,
      id: 'wf-1',
      name: 'Demo',
      connection: 'pg',
      database: 'wf_db',
      steps: [
        { type: 'command' as const, id: 's1', command: 'query', input: {}, database: 'step_db' },
      ],
    };
  };

  it('moves a command step database into input.database', () => {
    const def = workflowDraftToDefinition(base());
    expect(def.steps[0]).toMatchObject({ type: 'command', input: { database: 'step_db' } });
  });

  it('keeps existing input fields when injecting the database', () => {
    const def = workflowDraftToDefinition({
      ...base(),
      steps: [
        {
          type: 'command' as const,
          id: 's1',
          command: 'query',
          input: { limit: 10 },
          database: 'step_db',
        },
      ],
    });
    expect(def.steps[0]).toMatchObject({ input: { limit: 10, database: 'step_db' } });
  });

  it('does not let the form overwrite an explicit input.database', () => {
    const def = workflowDraftToDefinition({
      ...base(),
      steps: [
        {
          type: 'command' as const,
          id: 's1',
          command: 'query',
          input: { database: 'input_db' },
          database: 'step_db',
        },
      ],
    });
    expect(def.steps[0]).toMatchObject({ input: { database: 'input_db' } });
  });

  it('leaves a command step untouched when no database is chosen', () => {
    const def = workflowDraftToDefinition({
      ...base(),
      steps: [{ type: 'command' as const, id: 's1', command: 'query', input: {} }],
    });
    expect(def.steps[0]).toMatchObject({ input: {} });
  });

  it('carries the workflow-level database onto the definition', () => {
    expect(workflowDraftToDefinition(base()).database).toBe('wf_db');
  });

  it('writes a command step database into input when serializing to YAML', () => {
    const yamlObj = draftToYamlObject(base());
    expect(yamlSteps(yamlObj)[0]).toMatchObject({
      type: 'command',
      input: { database: 'step_db' },
    });
    // Must not be a bare `database` key — the Rust deserializer drops it.
    expect(yamlSteps(yamlObj)[0].database).toBeUndefined();
  });

  it('keeps workflow-level connection and database in the YAML form', () => {
    const yamlObj = draftToYamlObject(base());
    expect(yamlObj.connection).toBe('pg');
    expect(yamlObj.database).toBe('wf_db');
  });

  it('round-trips a command step database through YAML', () => {
    const draft = yamlObjectToDraft(draftToYamlObject(base()));
    expect(draft.steps[0].database).toBe('step_db');
    expect(draft.database).toBe('wf_db');
    expect(draft.connection).toBe('pg');
  });

  it('reads a command step database from input when parsing YAML', () => {
    const draft = yamlObjectToDraft({
      id: 'wf-1',
      name: 'Demo',
      connection: 'pg',
      database: 'wf_db',
      steps: [{ type: 'command', id: 's1', command: 'query', input: { database: 'from_input' } }],
    });
    expect(draft.steps[0].database).toBe('from_input');
  });

  it('still writes a query step database as a plain database key', () => {
    const yamlObj = draftToYamlObject({
      ...base(),
      steps: [{ type: 'query' as const, id: 's1', sql: 'SELECT 1', database: 'step_db' }],
    });
    expect(yamlSteps(yamlObj)[0]).toMatchObject({ type: 'query', database: 'step_db' });
  });
});
