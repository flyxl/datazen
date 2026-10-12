import { browser, expect } from '@wdio/globals';
import { invokeBackend } from '../../../../e2e/helpers.js';

type Run = { success: boolean; steps: Array<{ stepId: string; status: string; error: string | null; result: { rows?: Array<Record<string, unknown>> } | null }> };
type View = { state: string; effectOutcome: string };

describe('postgres workflow timeout journey', () => {
  it('confirms SQL terminal state before skip, fallback and the next step', async function () {
    this.timeout(120_000);
    await browser.setTimeout({ script: 120_000 });
    const profiles = await browser.executeAsync((done: (rows: Array<{ id: string; databaseType: string; database?: string }>) => void) => {
      const api = window as unknown as { __TAURI_INTERNALS__: { invoke: (command: string) => Promise<Array<{ id: string; databaseType: string; database?: string }>> } };
      void api.__TAURI_INTERNALS__.invoke('get_connections').then(rows => done(rows.map(({ id, databaseType, database }) => ({ id, databaseType, database }))));
    });
    const profile = profiles.find(row => row.databaseType === 'postgresql' && row.database);
    if (!profile?.database) throw new Error('Required seeded postgres profile unavailable');
    const cases = [
      { entry: 'legacy-query', strategy: 'skip' },
      { entry: 'legacy-query', strategy: 'fallback' },
      { entry: 'command-query', strategy: 'skip' },
      { entry: 'command-execute', strategy: 'fallback' },
    ] as const;
    for (const { entry, strategy } of cases) {
      for (const globalTimeout of [false, true]) {
        const id = `workflow-timeout-postgres-${entry}-${strategy}-${Date.now()}`;
        const observer = { type: 'query', id: 'observe', sql: "SELECT count(*) AS active FROM pg_stat_activity WHERE state = 'active' AND query LIKE '%wf_timeout_postgres%' AND pid <> pg_backend_pid()" };
        const sql = 'SELECT pg_sleep(10) /* wf_timeout_postgres */';
        const slowStep = {
          id: 'slow', timeout_secs: globalTimeout ? 30 : 1,
          on_error: { strategy, ...(strategy === 'fallback' ? { fallback_steps: [observer] } : {}) },
          ...(entry === 'legacy-query'
            ? { type: 'query', sql }
            : { type: 'command', command: entry === 'command-query' ? 'query' : 'execute', input: { sql } }),
        };
        await invokeBackend('workflow_save', { workflow: {
          id, name: id, description: 'SQL timeout terminal ordering', variables: [],
          connection: profile.id, database: profile.database,
          ...(globalTimeout ? { timeout_secs: 1 } : {}),
          steps: [slowStep,
            ...(strategy === 'skip' ? [observer] : []), { type: 'query', id: 'next', sql: 'SELECT 73' }],
        } });
        try {
          const result = await invokeBackend<Run>('workflow_execute', { workflowId: id, variables: {}, connectionId: profile.id });
          if (globalTimeout) {
            expect(result.success).toBe(false);
            const slow = result.steps.find(step => step.stepId === 'slow');
            expect(slow?.status).toBe('failed');
            expect(slow?.error).toContain('GlobalTimeout');
            expect(slow?.error).toContain('effects=');
            expect(result.steps.map(step => step.stepId)).toEqual(['slow']);
            const executionId = slow?.error?.match(/execution ([^: ]+):/)?.[1];
            if (!executionId) throw new Error('Global timeout lost execution identity');
            const execution = await invokeBackend<View>('get_execution', { request: { executionId } });
            expect(['succeeded', 'failed', 'cancelled']).toContain(execution.state);
            continue;
          }
          expect(result.success).toBe(true);
          const slow = result.steps.find(step => step.stepId === 'slow');
          expect(slow?.status).toBe(strategy === 'skip' ? 'skipped' : 'failed');
          expect(slow?.error).toContain('StepTimeout');
          const executionId = slow?.error?.match(/execution ([^: ]+):/)?.[1];
          if (!executionId) throw new Error('Timeout lost execution identity');
          const execution = await invokeBackend<View>('get_execution', { request: { executionId } });
          expect(['succeeded', 'failed', 'cancelled']).toContain(execution.state);
          expect(slow?.error).toContain('effects=');
          const observed = result.steps.find(step => step.stepId === 'observe');
          expect(observed?.status).toBe('success');
          expect(Number(observed?.result?.rows?.[0]?.active)).toBe(0);
          expect(result.steps.map(step => step.stepId)).toEqual(['slow', 'observe', 'next']);
          expect(result.steps.find(step => step.stepId === 'next')?.status).toBe('success');
        } finally {
          await invokeBackend('workflow_delete', { workflowId: id });
        }
      }
    }
  });
});
