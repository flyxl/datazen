/** Direct PG↔MySQL Transfer commit-ack-loss journeys with no shared reset hooks. */
import { config as base } from './wdio.conf.js';
import { browser } from '@wdio/globals';

export const config: WebdriverIO.Config = {
  ...base,
  specs: [
    '../crates/drivers/postgres/e2e/data-transfer-commit-ack-loss-pg-mysql.ts',
    '../crates/drivers/mysql/e2e/data-transfer-commit-ack-loss-mysql-pg.ts',
  ],
  maxInstances: 1,
  maxInstancesPerCapability: 1,
  // These suites own unique table/connection fixtures. Do not invoke the base
  // worker database/app-data setup or teardown that touches shared fixtures.
  before: async () => {
    await browser.url('tauri://localhost');
  },
  after: async () => {},
};
