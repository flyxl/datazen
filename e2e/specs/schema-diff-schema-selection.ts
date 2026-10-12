import { expect, browser, $ } from '@wdio/globals';
import {
  clickSchemaDiffNext,
  closeExtraWindows,
  disconnectBackend,
  invokeBackend,
  openSchemaDiffWindow,
  queryScalar,
  withSafeModeOff,
  type QueryResultPayload,
} from '../helpers.js';
import { PG_SYNC_DB, pgConnectionConfig } from '../lib/schemaDiffFixtures.js';

// Wait for the actual asynchronously-loaded option, and never toggle an
// already-open listbox during retry. Generic helpers search only once per open.
async function selectEndpointOption(testId: string, label: string): Promise<void> {
  await browser.waitUntil(async () => browser.execute((id: string, wanted: string) => {
    const trigger = document.querySelector(`[data-testid="${id}"] button[aria-haspopup="listbox"]`) as HTMLElement | null;
    if (!trigger) return false;
    if (trigger.textContent?.trim() === wanted) return true;
    if (trigger.getAttribute('aria-expanded') !== 'true') trigger.click();
    const option = Array.from(document.querySelectorAll('[data-testid="select-listbox"] [role="option"]'))
      .find((node) => node.textContent?.trim() === wanted);
    if (!option) return false;
    option.dispatchEvent(new MouseEvent('mousedown', { bubbles: true, cancelable: true }));
    return true;
  }, testId, label), { timeout: 20000, timeoutMsg: `Endpoint option unavailable: ${testId}/${label}` });
  await browser.waitUntil(async () => (await $(`[data-testid="${testId}"]`).getText()).includes(label), { timeout: 5000 });
}

describe('Schema Diff PostgreSQL selected schema', function () {
  this.timeout(120000);

  it('shows only the selected schema objects on each side of step two', async () => {
    const stamp = `${Date.now().toString(36)}_${Math.random().toString(36).slice(2, 8)}`;
    const connectionId = `e2e_sd_schema_${stamp}`;
    const connectionName = `Schema selection ${stamp}`;
    const sourceSchema = `sd_source_${stamp}`;
    const targetSchema = `sd_target_${stamp}`;
    const emptySchema = `sd_empty_${stamp}`;
    const viewName = `selected_view_${stamp}`;
    const mainWindow = await browser.getWindowHandle();
    const mainUrl = await browser.getUrl();
    const ownedSchemas: string[] = [];
    let saved = false;
    let session: string | undefined;
    try {
      await invokeBackend('save_connection', {
        config: pgConnectionConfig(connectionId, connectionName, PG_SYNC_DB),
      });
      saved = true;
      session = await invokeBackend<string>('connect', { connectionId });
      await withSafeModeOff(async () => {
        for (const schema of [sourceSchema, targetSchema, emptySchema]) {
          // CREATE without IF NOT EXISTS proves ownership before cleanup can drop it.
          await invokeBackend('execute_query', { dbSessionId: session, sql: `CREATE SCHEMA "${schema}"` });
          ownedSchemas.push(schema);
          if (schema === emptySchema) continue;
          await invokeBackend('execute_query', {
            dbSessionId: session,
            sql: `CREATE VIEW "${schema}"."${viewName}" AS SELECT 1 AS id`,
          });
        }
      });
      await openSchemaDiffWindow();
      await selectEndpointOption('schema-diff-source', connectionName);
      await selectEndpointOption('schema-diff-target', connectionName);
      await selectEndpointOption('schema-diff-source-database', PG_SYNC_DB);
      await selectEndpointOption('schema-diff-target-database', PG_SYNC_DB);
      // An empty schema must be selectable from catalog.schemas, independent of relations.
      await selectEndpointOption('schema-diff-source-schema', emptySchema);
      expect(await $('[data-testid="schema-diff-source-schema"]').getText()).toContain(emptySchema);
      await selectEndpointOption('schema-diff-source-schema', sourceSchema);
      await selectEndpointOption('schema-diff-target-schema', targetSchema);
      // This fixture is connected only through backend/dedicated sessions;
      // no workspace connection action registers it in activeConnectionStore.
      expect(await $('[data-testid="schema-diff-source-database"]').getText()).toContain(PG_SYNC_DB);
      expect(await $('[data-testid="schema-diff-target-database"]').getText()).toContain(PG_SYNC_DB);
      expect(await $('[data-testid="schema-diff-source-schema"]').getText()).toContain(sourceSchema);
      expect(await $('[data-testid="schema-diff-target-schema"]').getText()).toContain(targetSchema);

      await clickSchemaDiffNext();
      await browser.waitUntil(async () => {
        const identities = await browser.execute(() => Array.from(
          document.querySelectorAll('[data-object-kind="view"][data-object-identity]'),
          (row) => row.getAttribute('data-object-identity'),
        ));
        return [sourceSchema, targetSchema].every((schema) => identities.some((identity) =>
          identity && JSON.parse(identity)[1] === schema && JSON.parse(identity)[2] === viewName,
        ));
      }, { timeout: 30000, timeoutMsg: 'Selected-schema views were not discovered on both sides' });
      const observed = await browser.execute(() => ({
        source: Array.from(document.querySelectorAll('[data-testid^="schema-diff-object-row-source-"]'),
          (row) => row.getAttribute('data-object-identity')),
        target: Array.from(document.querySelectorAll('[data-testid^="schema-diff-object-row-target-"]'),
          (row) => row.getAttribute('data-object-identity')),
        errors: document.querySelectorAll('[data-testid^="schema-diff-object-error-"]').length,
      }));
      expect(observed.errors).toBe(0);
      for (const [schema, identities] of [[sourceSchema, observed.source], [targetSchema, observed.target]] as const) {
        expect(identities).toHaveLength(1);
        expect(JSON.parse(identities[0] ?? 'null')).toEqual(['view', schema, viewName, null, null, null]);
        expect(identities.every((identity) => identity && JSON.parse(identity)[1] === schema)).toBe(true);
      }
      await expect(await $('[data-testid="schema-diff-objects-panel"]')).toBeDisplayed();
    } finally {
      // Restore navigation even when setup or discovery fails.
      try {
        await closeExtraWindows(mainWindow);
        await browser.switchToWindow(mainWindow);
        await browser.url(mainUrl);
      } finally {
        try {
          if (session) {
            let cleanupFailed = false;
            await withSafeModeOff(async () => {
              for (const schema of ownedSchemas) {
                try {
                  await invokeBackend('execute_query', { dbSessionId: session, sql: `DROP SCHEMA "${schema}" CASCADE` });
                } catch {
                  cleanupFailed = true;
                }
              }
            });
            const remaining = await invokeBackend<QueryResultPayload>('execute_query', {
              dbSessionId: session,
                sql: `SELECT count(*) AS c FROM pg_namespace WHERE nspname IN ('${sourceSchema}', '${targetSchema}', '${emptySchema}')`,
            });
            if (cleanupFailed || queryScalar(remaining, 'c') !== 0) {
              throw new Error('Schema selection fixture cleanup failed');
            }
          }
        } finally {
          try {
            if (session) await disconnectBackend(session);
          } finally {
            if (saved) await invokeBackend('delete_connection', { id: connectionId });
          }
        }
      }
    }
  });
});
