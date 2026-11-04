/** Direct PG↔MySQL confirmed-rollback journeys with no shared reset hooks. */
import { config as base } from './wdio.conf.js';
import { browser } from '@wdio/globals';

export const config: WebdriverIO.Config = {
  ...base,
  specs: [
    '../crates/drivers/postgres/e2e/data-transfer-rollback-continue-pg-mysql.ts',
    '../crates/drivers/mysql/e2e/data-transfer-rollback-continue-mysql-pg.ts',
  ],
  maxInstances: 1,
  maxInstancesPerCapability: 1,
  // The specs create and own unique databases. Skip shared-worker database and
  // app-data hooks so these journeys cannot modify another suite's fixtures.
  before: async () => {
    await browser.url('tauri://localhost');
  },
  after: async () => {},
};
