import { browser } from '@wdio/globals';
import { executeSQL, expandConnectedConnectionInNavigator, openQueryTab } from '../../helpers.js';
import type { ContractConnCtx } from '../open-fixture';
import { focusContractCtx } from '../open-fixture';

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
  await expandConnectedConnectionInNavigator();
  const table = CONTRACT_TABLES[ctx.fixture.id][suffix];
  await openQueryTab();

  // Use tables present in the initial fixture snapshot. Post-connect DDL
  // would require a driver-specific schema-cache refresh and is unnecessary
  // for these contract journeys. Safe Mode permits DELETE only when it has a
  // WHERE clause.
  if (ctx.fixture.id === 'sqlite') {
    await executeSQL(`DELETE FROM ${table} WHERE 1 = 1`);
    const rows = suffix === 'data'
      ? Array.from({ length: 60 }, (_, i) => `('user_${i + 1}', 'user_${i + 1}@e2e.test', ${i + 1})`)
      : ["('alpha', 'alpha@e2e.test', 10)", "('beta', 'beta@e2e.test', 20)", "('gamma', 'gamma@e2e.test', 30)"];
    await executeSQL(`INSERT INTO ${table} (name, email, age) VALUES ${rows.join(', ')}`);
  } else {
    await executeSQL(`DELETE FROM ${table} WHERE 1 = 1`);
    const rows = suffix === 'data'
      ? Array.from({ length: 60 }, (_, i) => `('user_${i + 1}', 'active')`)
      : ["('alpha', 'active')", "('beta', 'active')", "('gamma', 'active')"];
    await executeSQL(`INSERT INTO ${table} (name, status) VALUES ${rows.join(', ')}`);
  }

  return table;
}
