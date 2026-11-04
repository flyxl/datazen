import { config as base } from './wdio.conf.js';
import { browser } from '@wdio/globals';
export const config: WebdriverIO.Config = {
  ...base,
  specs: [
    '../crates/drivers/postgres/e2e/sync-wave-one.ts',
    '../crates/drivers/mysql/e2e/sync-wave-one.ts',
  ],
  before: async () => {
    await browser.url('tauri://localhost');
  },
  after: async () => {},
};
