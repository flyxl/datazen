import { describe, expect, it } from 'vitest';
import {
  extractWorkflowYaml,
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
