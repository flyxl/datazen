import { browser, expect } from '@wdio/globals';
import { withSafeModeOff } from '../../../../e2e/helpers.js';
import { withFixtureCleanup } from '../../../../e2e/helpers/fixture-cleanup.js';
import { invokeMonitorBackend as call, deleteMonitorDashboard } from '../../../../e2e/helpers/monitor-journey.js';
import { withMysqlProfile } from './mysql-profile-fixture.js';

type Receipt = { jobId: string; state: string; scheduledAt: number; results: Array<{ items: Array<{ itemId: string }> }> };
type Evidence = { tickAccepted: boolean; receipts: Receipt[]; physicalDispatches: Array<{ connectionId: string; count: number }> };

describe('mysql Monitor scheduled desktop journey', () => {
  it('waits for the timer, reads a physical fixture and rejects duplicate/revoked ticks', async function () {
    this.timeout(240_000);
    // Standard driver fixture provisioning fails closed with a safe fixed code
    // when neither a saved profile nor explicit E2E MySQL configuration exists.
    await withMysqlProfile(async (sourceProfile) => {
      const id = `monitor_mysql_${Date.now()}`;
      const table = `${id}_fixture`;
      const fixtureId = `monitor-webdriver-fixture-${crypto.randomUUID()}`;
      // Clone inside the webview: credentials never enter WebDriver responses.
      const saved = await browser.executeAsync(async (sourceId: string, fixtureId: string, done: (ok: boolean) => void) => {
        const api = window as unknown as { __TAURI_INTERNALS__: { invoke: <T>(command: string, args?: Record<string, unknown>) => Promise<T> } };
        try {
          const rows = await api.__TAURI_INTERNALS__.invoke<Array<Record<string, unknown>>>('get_connections');
          const source = rows.find(row => row.id === sourceId);
          if (!source) { done(false); return; }
          await api.__TAURI_INTERNALS__.invoke('save_connection', { config: { ...source, id: fixtureId, name: fixtureId } });
          done(true);
        } catch { done(false); }
      }, sourceProfile.id, fixtureId);
      if (!saved) throw new Error('Monitor fixture profile creation failed');
      const profile = { id: fixtureId, database: sourceProfile.database };
      const args = { dashboardId: id, widgetId: `${id}_widget` };
      async function sql(command: string, text: string) {
        const workflowId = `${id}_setup`;
        await withFixtureCleanup(async () => {
          await call('workflow_save', { workflow: { id: workflowId, name: workflowId,
            description: 'Monitor fixture SQL', variables: [], connection: profile.id, database: profile.database,
            steps: [{ id: 'fixture', type: 'command', command, input: { database: profile.database, sql: text } }] } });
          const result = await call<{ success: boolean }>('workflow_execute', { workflowId, variables: {}, connectionId: profile.id });
          if (!result.success) throw new Error('Monitor fixture workflow failed [SQL_EXECUTION_FAILED]');
        }, [{ label: 'fixture-workflow', run: () => call('workflow_delete', { workflowId }) }]);
      }
      const evidence = (tick = false) => call<Evidence>('webdriver_monitor_evidence', { ...args, tick });
      await withSafeModeOff(async () => {
        await withFixtureCleanup(async () => {
          await sql('execute', `CREATE TABLE ${table} (marker INTEGER)`);
          await sql('execute', `INSERT INTO ${table} VALUES (73129)`);
          await call('workflow_save', { workflow: { id, name: id, description: 'Monitor scheduled read', variables: [], connection: profile.id,
            database: profile.database, steps: [{ id: 'read', type: 'command', command: 'query', connection: profile.id, input: { database: profile.database, sql: `SELECT marker FROM ${table}` } }] } });
          // Start near a slot boundary so timer receipt and duplicate tick share
          // the same production 30-second idempotency slot with ample margin.
          await browser.waitUntil(async () => Date.now() % 30_000 < 4_000,
            { timeout: 32_000, interval: 100 });
          const now = new Date().toISOString();
          await call('save_dashboard', { dashboard: { id, name: id, createdAt: now, updatedAt: now,
            layout: { cols: 12, rowHeight: 80 }, enabled: true, refreshPaused: false,
            widgets: [{ id: args.widgetId, title: id, workflowId: id, viewMode: 'table', chartConfig: null,
              layout: { x: 0, y: 0, w: 4, h: 3 }, enabled: true,
              refresh: { mode: 'interval', refreshSec: 30 }, alert: null }] } });
          await call('authorize_monitor_widget_schedule', { ...args, ttlSecs: 180 });
          // No manual widget run or controlled tick before this assertion: production timer only.
          let first: Receipt | undefined;
          await browser.waitUntil(async () => {
            first = (await evidence()).receipts.find(row => row.state === 'succeeded');
            return Boolean(first?.results.length);
          }, { timeout: 65_000, interval: 100 });
          if (!first) throw new Error('Production Monitor timer did not produce a durable receipt');
          const runId = first.results[0]?.items[0]?.itemId;
          const run = await call<{ status: string; rows: unknown[][] }>('get_widget_run', { ...args, runId });
          expect(run.status).toBe('ok');
          expect(Number(run.rows[0]?.[0])).toBe(73129);
          // Stay in the accepted slot: a new wall-clock slot legitimately admits a new occurrence.
          if (Math.floor(Date.now() / 30_000) !== first.scheduledAt / 30) {
            throw new Error('Timer receipt crossed slot boundary; retry journey');
          }
          const before = await evidence();
          const duplicate = await evidence(true);
          expect(duplicate.tickAccepted).toBe(false);
          expect(duplicate.physicalDispatches).toEqual(before.physicalDispatches);
          expect((await evidence()).receipts.map(row => row.jobId)).toEqual(before.receipts.map(row => row.jobId));
          const runs = await call<Array<{ id: string }>>('list_widget_runs', { ...args, limit: 100 });
          expect(runs.map(row => row.id)).toEqual([runId]);
          expect(before.physicalDispatches[0]?.count).toBeGreaterThan(0);
          await call('revoke_monitor_widget_schedule', args);
          const revokedTick = await evidence(true);
          expect(revokedTick.tickAccepted).toBe(false);
          expect(revokedTick.physicalDispatches).toEqual(before.physicalDispatches);
          // Let a complete production timer interval pass after revoke.
          await browser.waitUntil(async () => {
            const revoked = await evidence();
            expect(revoked.receipts.map(row => row.jobId)).toEqual(before.receipts.map(row => row.jobId));
            expect(revoked.physicalDispatches).toEqual(before.physicalDispatches);
            return Date.now() >= first!.scheduledAt * 1000 + 62_000;
          }, { timeout: 65_000, interval: 1000 });
          expect((await call<Array<{ id: string }>>('list_widget_runs', { ...args, limit: 100 })).map(row => row.id)).toEqual([runId]);
        }, [
          { label: 'dashboard', run: () => deleteMonitorDashboard(id) },
          { label: 'monitor-workflow', run: () => call('workflow_delete', { workflowId: id }) },
          { label: 'fixture-table', run: () => sql('execute', `DROP TABLE IF EXISTS ${table}`) },
          { label: 'monitor-profile', run: () => call('delete_connection', { id: fixtureId }) },
        ]);
      });
    });
  });
});
