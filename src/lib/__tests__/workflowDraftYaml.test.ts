import { describe, expect, it } from 'vitest';
import { draftToYamlObject, yamlObjectToDraft } from '../workflowDraftYaml';

describe('workflowDraftYaml', () => {
  it('round-trips a minimal draft through yaml object', () => {
    const draft = {
      id: 'wf-1',
      name: 'Demo',
      description: 'desc',
      variables: [],
      steps: [{ type: 'query' as const, id: 'q1', sql: 'SELECT 1' }],
      scheduleEnabled: false,
      scheduleIntervalSecs: 3600,
    };
    const obj = draftToYamlObject(draft);
    const back = yamlObjectToDraft(obj);
    expect(back.id).toBe('wf-1');
    expect(back.steps[0]?.sql).toBe('SELECT 1');
  });
});
