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
    expect(
      validateWorkflowFields({ id: 'wf-1', name: 'Test', steps: [{ id: 's1' }] }),
    ).toBeNull();
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
