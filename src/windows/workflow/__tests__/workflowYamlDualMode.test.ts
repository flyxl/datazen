import { describe, it, expect } from 'vitest';
import { parseWorkflowYaml, validateWorkflowFields } from '../../../lib/workflowYaml';

describe('workflow yaml dual-mode helpers', () => {
  it('accepts a minimal workflow yaml object', () => {
    const yaml = `
id: demo
name: Demo
steps:
  - type: query
    id: q1
    sql: SELECT 1
`;
    const obj = parseWorkflowYaml(yaml);
    expect(validateWorkflowFields(obj)).toBeNull();
  });

  it('rejects missing steps', () => {
    const obj = parseWorkflowYaml('id: a\nname: A\nsteps: []\n');
    expect(validateWorkflowFields(obj)).toBe('steps');
  });
});
