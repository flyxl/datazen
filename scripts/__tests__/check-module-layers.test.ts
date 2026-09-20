/** @vitest-environment node */
import { describe, expect, it } from 'vitest';
import { checkModuleLayers, LAYER_RULES } from '../check-module-layers.mjs';

/** Collect the messages a run logged, and its exit code. */
function run() {
  const logs: string[] = [];
  const code = checkModuleLayers({ log: (msg: unknown) => logs.push(String(msg)) });
  return { code, logs, output: logs.join('\n') };
}

describe('checkModuleLayers', () => {
  it('passes on the current tree', () => {
    const { code, output } = run();
    expect(output).not.toMatch(/violation/);
    expect(code).toBe(0);
  });

  it('guards the editor and the query builder against importing each other', () => {
    // The rule that matters: they are peers, so neither may reach into the other.
    const pairs = LAYER_RULES.map((r) => [r.from, r.forbidden.join(',')]);
    expect(pairs).toContainEqual(['src/components/query-builder', 'src/components/sql-editor']);
    expect(pairs).toContainEqual(['src/components/sql-editor', 'src/components/query-builder']);
  });

  it('guards the shared relation-metadata layer against importing its consumers', () => {
    const rule = LAYER_RULES.find((r) => r.from === 'src/lib/relationMetadata');
    expect(rule).toBeDefined();
    expect(rule!.forbidden).toEqual(
      expect.arrayContaining(['src/components', 'src/stores', 'src/windows', 'src/hooks']),
    );
  });

  it('reports the offending file and target when a rule is violated', () => {
    // A synthetic rule proves the detector actually inspects imports rather than
    // trusting the rule table. QueryBuilderPanel really does import the store,
    // so forbidding that edge must be reported.
    const probe = {
      name: 'probe',
      from: 'src/components/query-builder',
      forbidden: ['src/stores/queryBuilderStore'],
    };
    LAYER_RULES.push(probe);
    try {
      const { code, output } = run();
      expect(code).toBe(1);
      expect(output).toContain('src/stores/queryBuilderStore');
      expect(output).toContain('probe');
    } finally {
      LAYER_RULES.pop();
    }
  });
});
