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
  let observed = { optionCount: 0, matched: false, triggerExpanded: false, selected: false };
  let discoveryCode = '';
  try {
    await browser.waitUntil(async () => {
      const result = await browser.execute((id: string, wanted: string) => {
        const diagnostic = document.querySelector('[data-testid="schema-diff-window"] [data-testid="copyable-error-message"]')?.textContent?.trim()
          .match(/^databaseDiscovery\.(source|target):([A-Za-z]+)$/);
        // The product emits only its allowlisted API code, never a raw error message.
        const discoveryCode = diagnostic ? `databaseDiscovery.${diagnostic[1]}:${diagnostic[2]}` : '';
        const trigger = document.querySelector(`[data-testid="${id}"] > button[aria-haspopup="listbox"]`) as HTMLButtonElement | null;
        const selected = trigger?.querySelector('span')?.textContent?.trim() === wanted;
        const triggerExpanded = trigger?.getAttribute('aria-expanded') === 'true';
        const listId = trigger?.getAttribute('aria-controls');
        const list = listId ? document.getElementById(listId) : null;
        const options = Array.from(list?.querySelectorAll('[role="option"]') ?? []);
        const option = options.find((node) => node.querySelector('span')?.textContent?.trim() === wanted);
        const state = { optionCount: options.length, matched: Boolean(option), triggerExpanded, selected, discoveryCode };
        if (discoveryCode || selected) return state;
        if (trigger && !triggerExpanded) trigger.click();
        if (option) option.dispatchEvent(new MouseEvent('mousedown', { bubbles: true, cancelable: true }));
        // Selection is confirmed on the next observation after React commits.
        return state;
      }, testId, label);
      observed = result;
      discoveryCode = result.discoveryCode;
      if (discoveryCode) throw new Error('Endpoint discovery rejected');
      return result.selected;
    }, { timeout: 20000, timeoutMsg: 'Endpoint option wait expired' });
  } catch {
    // Labels, option contents and WebDriver exception messages can contain
    // connection/database identities. Only structural fields leave this helper.
    throw new Error(`Endpoint option unavailable: ${testId}; optionCount=${observed.optionCount}; matched=${observed.matched}; triggerExpanded=${observed.triggerExpanded}; selected=${observed.selected}${discoveryCode ? `; ${discoveryCode}` : ''}`);
  }
}

// Keep timeout evidence available even when an awaited WebDriver IPC never
// resolves. Every call site supplies a fixed stage, never fixture identities.
function breadcrumb(stage:
  | 'fixture-window-handle' | 'fixture-window-url'
  | 'fixture-save' | 'fixture-connect' | 'fixture-ddl-start'
  | 'fixture-create-schema' | 'fixture-create-view' | 'fixture-ddl-complete'
  | 'window-open' | 'window-open-complete'
  | 'select-source' | 'select-target'
  | 'select-source-database' | 'select-target-database'
  | 'select-empty-schema' | 'assert-empty-schema'
  | 'select-source-schema' | 'select-target-schema' | 'assert-endpoints'
  | 'next' | 'objects-wait' | 'objects-observe' | 'objects-assert'
  | 'panel-assert' | 'journey-complete'
  | 'cleanup-window' | 'cleanup-ddl' | 'cleanup-verification'
  | 'cleanup-disconnect' | 'cleanup-delete' | 'cleanup-complete'
): void {
  console.info(`P4 schema selection stage=${stage}`);
}

describe('Schema Diff PostgreSQL selected schema', function () {
  this.timeout(120000);

  it('shows only the selected schema objects on each side of step two', async () => {
    const stamp = `${Date.now().toString(36)}_${Math.random().toString(36).slice(2, 8)}`;
    const connectionId = `e2e_sd_schema_${stamp}`;
    const connectionName = `Schema selection ${stamp}`;
    const connectionLabel = `${connectionName} (postgresql)`;
    const sourceSchema = `sd_source_${stamp}`;
    const targetSchema = `sd_target_${stamp}`;
    const emptySchema = `sd_empty_${stamp}`;
    const viewName = `selected_view_${stamp}`;
    breadcrumb('fixture-window-handle');
    const mainWindow = await browser.getWindowHandle();
    breadcrumb('fixture-window-url');
    const mainUrl = await browser.getUrl();
    const ownedSchemas: string[] = [];
    let saved = false;
    let session: string | undefined;
    try {
      breadcrumb('fixture-save');
      await invokeBackend('save_connection', {
        config: pgConnectionConfig(connectionId, connectionName, PG_SYNC_DB),
      });
      saved = true;
      breadcrumb('fixture-connect');
      session = await invokeBackend<string>('connect', { connectionId });
      breadcrumb('fixture-ddl-start');
      await withSafeModeOff(async () => {
        for (const schema of [sourceSchema, targetSchema, emptySchema]) {
          // CREATE without IF NOT EXISTS proves ownership before cleanup can drop it.
          breadcrumb('fixture-create-schema');
          await invokeBackend('execute_query', { dbSessionId: session, sql: `CREATE SCHEMA "${schema}"` });
          ownedSchemas.push(schema);
          if (schema === emptySchema) continue;
          breadcrumb('fixture-create-view');
          await invokeBackend('execute_query', {
            dbSessionId: session,
            sql: `CREATE VIEW "${schema}"."${viewName}" AS SELECT 1 AS id`,
          });
        }
      });
      breadcrumb('fixture-ddl-complete');
      breadcrumb('window-open');
      await openSchemaDiffWindow();
      breadcrumb('window-open-complete');
      // Endpoint options include the driver type, unlike navigator card names.
      breadcrumb('select-source');
      await selectEndpointOption('schema-diff-source', connectionLabel);
      breadcrumb('select-target');
      await selectEndpointOption('schema-diff-target', connectionLabel);
      breadcrumb('select-source-database');
      await selectEndpointOption('schema-diff-source-database', PG_SYNC_DB);
      breadcrumb('select-target-database');
      await selectEndpointOption('schema-diff-target-database', PG_SYNC_DB);
      // An empty schema must be selectable from catalog.schemas, independent of relations.
      breadcrumb('select-empty-schema');
      await selectEndpointOption('schema-diff-source-schema', emptySchema);
      breadcrumb('assert-empty-schema');
      expect(await $('[data-testid="schema-diff-source-schema"]').getText()).toContain(emptySchema);
      breadcrumb('select-source-schema');
      await selectEndpointOption('schema-diff-source-schema', sourceSchema);
      breadcrumb('select-target-schema');
      await selectEndpointOption('schema-diff-target-schema', targetSchema);
      // This fixture is connected only through backend/dedicated sessions;
      // no workspace connection action registers it in activeConnectionStore.
      breadcrumb('assert-endpoints');
      expect(await $('[data-testid="schema-diff-source-database"]').getText()).toContain(PG_SYNC_DB);
      expect(await $('[data-testid="schema-diff-target-database"]').getText()).toContain(PG_SYNC_DB);
      expect(await $('[data-testid="schema-diff-source-schema"]').getText()).toContain(sourceSchema);
      expect(await $('[data-testid="schema-diff-target-schema"]').getText()).toContain(targetSchema);

      breadcrumb('next');
      await clickSchemaDiffNext();
      breadcrumb('objects-wait');
      await browser.waitUntil(async () => {
        const identities = await browser.execute(() => Array.from(
          document.querySelectorAll('[data-object-kind="view"][data-object-identity]'),
          (row) => row.getAttribute('data-object-identity'),
        ));
        return [sourceSchema, targetSchema].every((schema) => identities.some((identity) =>
          identity && JSON.parse(identity)[1] === schema && JSON.parse(identity)[2] === viewName,
        ));
      }, { timeout: 30000, timeoutMsg: 'Selected-schema views were not discovered on both sides' });
      breadcrumb('objects-observe');
      const observed = await browser.execute(() => ({
        source: Array.from(document.querySelectorAll('[data-testid^="schema-diff-object-row-source-"]'),
          (row) => row.getAttribute('data-object-identity')),
        target: Array.from(document.querySelectorAll('[data-testid^="schema-diff-object-row-target-"]'),
          (row) => row.getAttribute('data-object-identity')),
        errors: document.querySelectorAll('[data-testid^="schema-diff-object-error-"]').length,
      }));
      breadcrumb('objects-assert');
      expect(observed.errors).toBe(0);
      for (const [schema, identities] of [[sourceSchema, observed.source], [targetSchema, observed.target]] as const) {
        expect(identities).toHaveLength(1);
        expect(JSON.parse(identities[0] ?? 'null')).toEqual(['view', schema, viewName, null, null, null]);
        expect(identities.every((identity) => identity && JSON.parse(identity)[1] === schema)).toBe(true);
      }
      breadcrumb('panel-assert');
      await expect(await $('[data-testid="schema-diff-objects-panel"]')).toBeDisplayed();
      breadcrumb('journey-complete');
    } finally {
      // Restore navigation even when setup or discovery fails.
      try {
        breadcrumb('cleanup-window');
        await closeExtraWindows(mainWindow);
        await browser.switchToWindow(mainWindow);
        await browser.url(mainUrl);
      } finally {
        try {
          if (session) {
            breadcrumb('cleanup-ddl');
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
            breadcrumb('cleanup-verification');
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
            breadcrumb('cleanup-disconnect');
            if (session) await disconnectBackend(session);
          } finally {
            breadcrumb('cleanup-delete');
            if (saved) await invokeBackend('delete_connection', { id: connectionId });
            breadcrumb('cleanup-complete');
          }
        }
      }
    }
  });
});
