import { describe, expect, it } from 'vitest';
import {
  extractWorkflowYaml,
  parseValidatedWorkflowDefinition,
  parseWorkflowYaml,
  validateWorkflowFields,
} from '../workflowYaml';

describe('extractWorkflowYaml', () => {
  it('extracts yaml/yml blocks that contain workflow fields', () => {
    const text = `
Here is a workflow:
\`\`\`yaml
id: wf-1
name: Test
steps:
  - id: s1
    type: query
\`\`\`
\`\`\`yml
id: wf-2
name: Other
steps: []
\`\`\`
\`\`\`yaml
just: data
no steps here
\`\`\`
`;
    const blocks = extractWorkflowYaml(text);
    expect(blocks).toHaveLength(2);
    expect(blocks[0]).toContain('id: wf-1');
    expect(blocks[1]).toContain('id: wf-2');
  });

  it('returns empty array when no matching blocks', () => {
    expect(extractWorkflowYaml('no code blocks')).toEqual([]);
    expect(extractWorkflowYaml('```yaml\nfoo: bar\n```')).toEqual([]);
  });
});

describe('parseWorkflowYaml', () => {
  it('parses valid YAML object', () => {
    const obj = parseWorkflowYaml('id: wf-1\nname: Test\nsteps: []');
    expect(obj).toEqual({ id: 'wf-1', name: 'Test', steps: [] });
  });

  it('throws when YAML is not an object', () => {
    expect(() => parseWorkflowYaml('[1, 2]')).toThrow(/must be an object/);
    expect(() => parseWorkflowYaml('null')).toThrow(/must be an object/);
    expect(() => parseWorkflowYaml('just a string')).toThrow(/must be an object/);
  });
});

describe('validateWorkflowFields', () => {
  it('returns null for valid workflow', () => {
    expect(validateWorkflowFields({ id: 'wf-1', name: 'Test', steps: [{ id: 's1' }] })).toBeNull();
  });

  it('returns first missing field name', () => {
    expect(validateWorkflowFields({ name: 'Test', steps: [{}] })).toBe('id');
    expect(validateWorkflowFields({ id: 'wf-1', steps: [{}] })).toBe('name');
    expect(validateWorkflowFields({ id: 'wf-1', name: 'Test' })).toBe('steps');
    expect(validateWorkflowFields({ id: 'wf-1', name: 'Test', steps: [] })).toBe('steps');
  });
});

describe('[tester] parseValidatedWorkflowDefinition', () => {
  it('narrows a valid workflow object to WorkflowDefinition', () => {
    const workflow = parseValidatedWorkflowDefinition({
      id: 'wf-1',
      name: 'Demo',
      description: 'desc',
      variables: [{ name: 'q', varType: 'string' }],
      steps: [{ id: 's1', type: 'query' }],
      version: '1',
      visibility: 'user',
    });
    expect(workflow.id).toBe('wf-1');
    expect(workflow.variables).toHaveLength(1);
    expect(workflow.steps[0]?.type).toBe('query');
  });

  it('throws on invalid optional fields and malformed steps', () => {
    expect(() =>
      parseValidatedWorkflowDefinition({
        id: 'wf-1',
        name: 'Demo',
        description: 123,
        steps: [{ id: 's1', type: 'query' }],
      }),
    ).toThrow(/description must be a string/);

    expect(() =>
      parseValidatedWorkflowDefinition({
        id: 'wf-1',
        name: 'Demo',
        variables: [{ bad: true }],
        steps: [{ id: 's1', type: 'query' }],
      }),
    ).toThrow(/variables must be an array/);

    expect(() =>
      parseValidatedWorkflowDefinition({
        id: 'wf-1',
        name: 'Demo',
        steps: [{ id: 's1' }],
      }),
    ).toThrow(/steps must be a non-empty array/);
  });
});

describe('validateWorkflowFields — multi-db database rule', () => {
  // A PostgreSQL connection with no `database` of its own: every data-operation
  // step must resolve one. This is the exact shape that produced
  // `relation "public.test_orders" does not exist` at run time.
  const PG_UNPINNED = [{ id: 'pg', requiresExplicitDatabase: true }];
  // A connection locked to one database never needs the step to name one.
  const PG_PINNED = [{ id: 'pg', requiresExplicitDatabase: false }];

  const wf = (extra: Record<string, unknown>) => ({
    id: 'wf-1',
    name: 'Demo',
    steps: [{ type: 'query', id: 's1', sql: 'SELECT 1' }],
    ...extra,
  });

  it('rejects a query step with no database on an unpinned multi-db connection', () => {
    expect(validateWorkflowFields(wf({ connection: 'pg' }), PG_UNPINNED)).toBe('steps[0].database');
  });

  it('accepts a step-level database', () => {
    expect(
      validateWorkflowFields(
        wf({ connection: 'pg', steps: [{ type: 'query', id: 's1', database: 'datazen_demo' }] }),
        PG_UNPINNED,
      ),
    ).toBeNull();
  });

  it('accepts a workflow-level database inherited by the step', () => {
    expect(
      validateWorkflowFields(wf({ connection: 'pg', database: 'datazen_demo' }), PG_UNPINNED),
    ).toBeNull();
  });

  it('accepts a step that overrides the workflow default', () => {
    expect(
      validateWorkflowFields(
        wf({ database: 'wf_db', steps: [{ type: 'query', id: 's1', database: 'step_db' }] }),
        PG_UNPINNED,
      ),
    ).toBeNull();
  });

  it('reads a command step target from input.database', () => {
    expect(
      validateWorkflowFields(
        wf({
          connection: 'pg',
          steps: [{ type: 'command', id: 's1', input: { database: 'datazen_demo' } }],
        }),
        PG_UNPINNED,
      ),
    ).toBeNull();
    expect(
      validateWorkflowFields(
        wf({ connection: 'pg', steps: [{ type: 'command', id: 's1', input: {} }] }),
        PG_UNPINNED,
      ),
    ).toBe('steps[0].database');
  });

  it('lets a step-level connection override the workflow connection', () => {
    // Workflow points at the unpinned connection, but the step uses a pinned
    // one, so the rule must not fire.
    expect(
      validateWorkflowFields(
        wf({ connection: 'pg', steps: [{ type: 'query', id: 's1', connection: 'other' }] }),
        [...PG_UNPINNED, { id: 'other', requiresExplicitDatabase: false }],
      ),
    ).toBeNull();
  });

  it('reports the offending step index', () => {
    expect(
      validateWorkflowFields(
        wf({
          connection: 'pg',
          steps: [
            { type: 'query', id: 'ok', database: 'd' },
            { type: 'query', id: 'bad' },
          ],
        }),
        PG_UNPINNED,
      ),
    ).toBe('steps[1].database');
  });

  it('ignores non-data steps and unknown connections', () => {
    expect(
      validateWorkflowFields(
        wf({ connection: 'nope', steps: [{ type: 'ai', id: 's1' }] }),
        PG_UNPINNED,
      ),
    ).toBeNull();
    expect(validateWorkflowFields(wf({ connection: 'ghost' }), PG_UNPINNED)).toBeNull();
  });

  it('treats a blank database as unset', () => {
    expect(validateWorkflowFields(wf({ connection: 'pg', database: '   ' }), PG_UNPINNED)).toBe(
      'steps[0].database',
    );
  });

  it('keeps the pure shape check when no connection context is supplied', () => {
    expect(validateWorkflowFields(wf({ connection: 'pg' }))).toBeNull();
    expect(validateWorkflowFields({ id: 'x' })).toBe('name');
  });

  it('does not require a database for a pinned connection', () => {
    expect(validateWorkflowFields(wf({ connection: 'pg' }), PG_PINNED)).toBeNull();
  });
});
