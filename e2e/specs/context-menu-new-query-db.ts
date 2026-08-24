/**
 * E2E: context menu "New Query" opens with the right-clicked database.
 *
 * Regression test for: right-clicking a database node under a multi-db
 * connection and selecting "New Query" should open a query panel bound
 * to that specific database — not whatever was previously active.
 *
 * Requires: PostgreSQL with at least 2 databases (default: postgres +
 * a secondary). Uses E2E_PG_* env vars; skips when PG is unreachable.
 */
import { expect, browser, $, $$ } from '@wdio/globals';
import { createConnection } from 'node:net';
import { createAndConnectPostgreSQL, closeExtraWindows } from '../helpers.js';

const CONN_NAME = 'E2E-CtxNewQuery';
const PG_HOST = process.env.E2E_PG_HOST || '127.0.0.1';
const PG_PORT = Number(process.env.E2E_PG_PORT) || 5432;
const PRIMARY_DB = process.env.E2E_PG_DB || 'goecoride';
const SECONDARY_DB = process.env.E2E_PG_SECONDARY_DB || 'datazen';

function skipRequested(): boolean {
  return process.env.E2E_SKIP_PG === '1';
}

async function postgresReachable(timeoutMs = 2000): Promise<boolean> {
  return new Promise((resolve) => {
    const sock = createConnection({ host: PG_HOST, port: PG_PORT });
    const timer = setTimeout(() => {
      sock.destroy();
      resolve(false);
    }, timeoutMs);
    sock.on('connect', () => {
      clearTimeout(timer);
      sock.destroy();
      resolve(true);
    });
    sock.on('error', () => {
      clearTimeout(timer);
      resolve(false);
    });
  });
}

/** Right-click a DOM element matched by data-tree-node attribute + text. */
async function rightClickDb(dbName: string) {
  const opened = await browser.execute((name: string) => {
    const nodes = document.querySelectorAll('[data-tree-node="db"]');
    for (const n of nodes) {
      if ((n.textContent || '').trim() === name || n.getAttribute('data-db-name') === name) {
        const rect = (n as HTMLElement).getBoundingClientRect();
        n.dispatchEvent(
          new MouseEvent('contextmenu', {
            bubbles: true,
            cancelable: true,
            clientX: rect.left + rect.width / 2,
            clientY: rect.top + rect.height / 2,
          }),
        );
        return true;
      }
    }
    return false;
  }, dbName);
  if (!opened) throw new Error(`db node "${dbName}" not found for context menu`);
  await browser.pause(600);
}

/** Click a web context menu item by label text. */
async function clickMenuItem(label: string) {
  const hit = await browser.execute((lbl: string) => {
    const items = document.querySelectorAll(
      '[data-testid="web-context-menu"] button, [data-testid="web-context-menu"] [role="menuitem"]',
    );
    for (const item of items) {
      if ((item.textContent || '').includes(lbl)) {
        (item as HTMLElement).click();
        return true;
      }
    }
    return false;
  }, label);
  if (!hit) throw new Error(`context menu item "${label}" not found`);
  await browser.pause(800);
}

/** Expand the connection node to reveal database nodes. */
async function expandConnection() {
  await browser.execute((name: string) => {
    const items = document.querySelectorAll('[data-conn-item]');
    for (const item of items) {
      if ((item.getAttribute('data-conn-name') || '').includes(name)) {
        const chev = item.querySelector('svg.lucide-chevron-right');
        if (chev) (chev.closest('button') || chev.parentElement)?.click();
        break;
      }
    }
  }, CONN_NAME);
  await browser.pause(2000);
}

/** Expand a database node by clicking it. */
async function expandDb(dbName: string) {
  await browser.execute((name: string) => {
    const nodes = document.querySelectorAll('[data-tree-node="db"]');
    for (const n of nodes) {
      if (n.getAttribute('data-db-name') === name || (n.textContent || '').trim() === name) {
        (n as HTMLElement).click();
        break;
      }
    }
  }, dbName);
  await browser.pause(1500);
}

/** Get the tab title text of the currently active query panel. */
async function getActiveQueryTabTitle(): Promise<string> {
  return browser.execute(() => {
    // Query panel tabs show connectionName@database in their title
    const activeTab = document.querySelector(
      '[data-testid="panel-tab"][data-active="true"], [data-testid="panel-tab"].active',
    );
    if (activeTab) return (activeTab.textContent || '').trim();
    // Fallback: find any visible query panel title
    const panelTitles = document.querySelectorAll('[data-testid="panel-tab"]');
    for (const t of panelTitles) {
      if ((t.textContent || '').includes('@')) return (t.textContent || '').trim();
    }
    return '';
  });
}

/** Get the database shown in the query panel's context selectors. */
async function getQueryPanelDatabase(): Promise<string> {
  return browser.execute(() => {
    const selectors = document.querySelector('[data-testid="query-context-selectors"]');
    if (!selectors) return '';
    const btn = selectors.querySelector('button');
    if (!btn) return '';
    // The button text may contain a checkmark; strip it
    return (btn.textContent || '').replace(/✓/g, '').trim();
  });
}

describe('Context menu New Query opens correct database', () => {
  let mainWindow: string;
  let shouldSkip = false;

  before(async () => {
    if (skipRequested()) {
      shouldSkip = true;
      return;
    }
    if (!(await postgresReachable())) {
      console.warn(`⏩ Skipping: PostgreSQL ${PG_HOST}:${PG_PORT} unreachable`);
      shouldSkip = true;
      return;
    }

    mainWindow = await browser.getWindowHandle();
    // Connect without specifying database (multi-db mode)
    await createAndConnectPostgreSQL({
      name: CONN_NAME,
      database: '',
    });
    await browser.pause(1500);
  });

  after(async () => {
    if (shouldSkip) return;
    try {
      await closeExtraWindows(mainWindow);
    } catch {
      /* best effort */
    }
  });

  it('right-click database B → New Query → panel shows database B', async () => {
    if (shouldSkip) return;

    await expandConnection();
    await browser.pause(1000);

    // Verify both databases are visible
    const dbNames = await browser.execute(() => {
      return Array.from(document.querySelectorAll('[data-tree-node="db"]')).map((n) =>
        (n.textContent || '').trim(),
      );
    });
    console.log(`[ctx-new-query] visible databases: ${JSON.stringify(dbNames)}`);

    // Left-click PRIMARY_DB to set it as currentDatabase
    await expandDb(PRIMARY_DB);
    await browser.pause(1000);

    // Right-click SECONDARY_DB → New Query
    await rightClickDb(SECONDARY_DB);
    await clickMenuItem('新建查询');

    // Verify the query panel's database selector shows SECONDARY_DB
    await browser.waitUntil(
      async () => {
        const db = await getQueryPanelDatabase();
        return db === SECONDARY_DB;
      },
      {
        timeout: 10000,
        timeoutMsg: `query panel database should be "${SECONDARY_DB}" but got "${await getQueryPanelDatabase()}"`,
      },
    );

    const finalDb = await getQueryPanelDatabase();
    expect(finalDb).toBe(SECONDARY_DB);
  });

  it('right-click schema node → New Query → panel shows parent database', async () => {
    if (shouldSkip) return;

    // Ensure SECONDARY_DB is expanded to show schemas
    await expandDb(SECONDARY_DB);
    await browser.pause(1500);

    // Find a schema node under SECONDARY_DB
    const schemaName = await browser.execute((dbName: string) => {
      const all = Array.from(document.querySelectorAll('[data-tree-node="db"]'));
      const dbIdx = all.findIndex((n) => n.getAttribute('data-db-name') === dbName);
      if (dbIdx < 0) return '';
      const rest = all.slice(dbIdx + 1);
      const nextDb = rest.findIndex((n) => n.getAttribute('data-tree-node') === 'db');
      const subtree = nextDb >= 0 ? rest.slice(0, nextDb) : rest;
      const schema = subtree.find((n) => n.getAttribute('data-tree-node') === 'schema');
      return schema
        ? (schema.getAttribute('data-schema-name') || schema.textContent || '').trim()
        : '';
    }, SECONDARY_DB);

    if (!schemaName) {
      console.log('[ctx-new-query] no schema node found under secondary db, skipping');
      return;
    }
    console.log(`[ctx-new-query] found schema "${schemaName}" under ${SECONDARY_DB}`);

    // Switch to a different database first to make the test meaningful
    await expandDb(PRIMARY_DB);
    await browser.pause(1000);

    // Right-click the schema node → New Query
    await browser.execute((sName: string) => {
      const nodes = document.querySelectorAll('[data-tree-node="schema"]');
      for (const n of nodes) {
        if (
          n.getAttribute('data-schema-name') === sName ||
          (n.textContent || '').trim() === sName
        ) {
          const rect = (n as HTMLElement).getBoundingClientRect();
          n.dispatchEvent(
            new MouseEvent('contextmenu', {
              bubbles: true,
              cancelable: true,
              clientX: rect.left + rect.width / 2,
              clientY: rect.top + rect.height / 2,
            }),
          );
          return;
        }
      }
    }, schemaName);
    await browser.pause(600);
    await clickMenuItem('新建查询');

    // The query panel should show SECONDARY_DB (the parent of the schema)
    await browser.waitUntil(
      async () => {
        const db = await getQueryPanelDatabase();
        return db === SECONDARY_DB;
      },
      {
        timeout: 10000,
        timeoutMsg: `query panel database should be "${SECONDARY_DB}" (schema parent) but got "${await getQueryPanelDatabase()}"`,
      },
    );

    const finalDb = await getQueryPanelDatabase();
    expect(finalDb).toBe(SECONDARY_DB);
  });
});
