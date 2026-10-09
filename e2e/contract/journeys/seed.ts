import { browser } from '@wdio/globals';
import { connectConfig, executeQuery, invokeBackend } from '../../helpers.js';
import { contractTableDdl } from '../fixtures.js';
import type { ContractConnCtx } from '../open-fixture';
import { focusContractConnection, focusContractCtx } from '../open-fixture';

const CONTRACT_TABLES = {
  postgres: {
    conn: 'e2e_contract_conn',
    data: 'e2e_contract_data',
    filter: 'e2e_contract_filter',
    edit: 'e2e_contract_edit',
    struct: 'e2e_contract_struct',
    idx: 'e2e_contract_index',
    export: 'e2e_contract_export',
  },
  mysql: {
    conn: 'e2e_contract_conn',
    data: 'e2e_contract_data',
    filter: 'e2e_contract_filter',
    edit: 'e2e_contract_edit',
    struct: 'e2e_contract_struct',
    idx: 'e2e_contract_index',
    export: 'e2e_contract_export',
  },
  sqlite: {
    conn: 'e2e_contract_conn',
    data: 'e2e_contract_data',
    filter: 'e2e_contract_filter',
    edit: 'e2e_contract_edit',
    struct: 'e2e_contract_struct',
    idx: 'e2e_contract_index',
    export: 'e2e_contract_export',
  },
} as const;

/**
 * Empty one contract table, recreating it first if it is gone.
 *
 * The table is part of the schema snapshot the app loaded when it connected, so
 * a table that another process dropped mid-run only needs to exist again — no
 * driver-specific schema refresh. Any other SQL error is rethrown untouched.
 */
async function deleteRows(sessionId: string, table: string, ctx: ContractConnCtx): Promise<void> {
  const sql = `DELETE FROM ${table} WHERE 1 = 1`;
  try {
    await executeQuery(sessionId, sql);
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    if (!/42S02|doesn't exist|no such table|does not exist|不存在/i.test(message)) throw err;
    // `teardown-e2e-env.sh` drops every `e2e%` object, so a second run.mjs
    // sharing this MySQL/PostgreSQL server can delete the contract tables while
    // this matrix is still seeding them. Put the one table back and retry.
    await executeQuery(sessionId, contractTableDdl(ctx.fixture, table));
    await executeQuery(sessionId, sql);
  }
}

/**
 * Resolve THIS fixture's live dbSessionId through IPC instead of guessing at
 * the UI's "currently active query tab". The matrix runs postgres, then mysql,
 * then sqlite inside one Tauri process and every fixture session stays live:
 * the editor-path seed (`executeSQLChecked` on the ambient active tab) was
 * observed running its DELETE/INSERT through a leftover PostgreSQL session
 * (log `db_session_id=63fc3d29-…`, PG-style error
 * `column "email" of relation "e2e_contract_data" does not exist`) because the
 * active panel/tab derivation had flipped back. Binding the seed to an
 * explicit session removes that whole class of misrouting.
 */
async function resolveFixtureSessionId(ctx: ContractConnCtx): Promise<string> {
  const conns = await invokeBackend<Array<{ id: string; name?: string }>>('get_connections');
  const cfg = conns.find((c) => c.name === ctx.fixture.displayName);
  if (!cfg) {
    throw new Error(`seed: no persisted connection named ${ctx.fixture.displayName}`);
  }
  return connectConfig(cfg.id);
}

const sessionCache = new WeakMap<ContractConnCtx, Promise<string>>();

async function cachedFixtureSessionId(ctx: ContractConnCtx): Promise<string> {
  const cached = sessionCache.get(ctx);
  if (cached) return cached;
  const fresh = resolveFixtureSessionId(ctx);
  sessionCache.set(ctx, fresh);
  return fresh;
}

type ContractJourneySuffix = keyof (typeof CONTRACT_TABLES)['postgres'];

/** Seed a table for a contract journey and return the table name to open. */
export async function seedContractTable(
  ctx: ContractConnCtx,
  suffix: ContractJourneySuffix,
): Promise<string> {
  await focusContractCtx(ctx);
  // Each contract journey reuses a single connection window. Close every
  // existing panel, not only the active one: the TableWorkspace open guard
  // intentionally reuses an already-open table panel, which would otherwise
  // make the next journey assert the previous journey's rows.
  for (let attempt = 0; attempt < 12; attempt++) {
    const closed = await browser.execute(() => {
      const close = document.querySelector<HTMLElement>('[data-testid="panel-tab-close"]');
      if (!close) return false;
      // The close button is intentionally opacity-0 until hover; dispatching
      // through the DOM keeps fixture cleanup independent of hover timing.
      close.click();
      return true;
    });
    if (!closed) break;
    await browser.pause(250);
  }
  await browser.pause(300);
  // Target THIS fixture's connection by name. The matrix runs postgres, then
  // mysql, then sqlite in one Tauri process, and `after` only closes extra OS
  // windows — the previous fixture's session stays live in the navigator. The
  // no-argument form expands the *first* connected item, which by then is the
  // leftover MySQL connection, so the seeded SQL ran through the MySQL driver
  // (`INSERT INTO \`datazen_test\`...`) and the table view loaded 0 rows.
  await focusContractConnection(ctx.fixture.displayName);
  const table = CONTRACT_TABLES[ctx.fixture.id][suffix];
  const sessionId = await cachedFixtureSessionId(ctx);

  // Use tables present in the initial fixture snapshot. Post-connect DDL
  // would require a driver-specific schema-cache refresh and is unnecessary
  // for these contract journeys. Safe Mode permits DELETE only when it has a
  // WHERE clause. Seed goes over IPC with an explicit dbSessionId (see
  // resolveFixtureSessionId), so driver errors surface at the seed itself.
  if (ctx.fixture.id === 'sqlite') {
    await deleteRows(sessionId, table, ctx);
    const rows =
      suffix === 'data'
        ? Array.from(
            { length: 60 },
            (_, i) => `('user_${i + 1}', 'user_${i + 1}@e2e.test', ${i + 1})`,
          )
        : [
            "('alpha', 'alpha@e2e.test', 10)",
            "('beta', 'beta@e2e.test', 20)",
            "('gamma', 'gamma@e2e.test', 30)",
          ];
    await executeQuery(
      sessionId,
      `INSERT INTO ${table} (name, email, age) VALUES ${rows.join(', ')}`,
    );
  } else {
    await deleteRows(sessionId, table, ctx);
    const rows =
      suffix === 'data'
        ? Array.from({ length: 60 }, (_, i) => `('user_${i + 1}', 'active')`)
        : ["('alpha', 'active')", "('beta', 'active')", "('gamma', 'active')"];
    await executeQuery(sessionId, `INSERT INTO ${table} (name, status) VALUES ${rows.join(', ')}`);
  }

  return table;
}
