/**
 * Data Sync Diff Workspace — full user journeys for PostgreSQL and MySQL.
 *
 * Covers validation branches, compare/review/preview/execute paths, option
 * toggles, mapping controls, table filters, and post-execute verification.
 * Each step captures a screenshot with `--screenshot` / `pnpm e2e:data-sync`.
 *
 * Requires `e2e/setup-sync-dbs.sh` (PG: datazen_sync_src/tgt; MySQL: datazen_sync_mysql_src/tgt).
 */
import { expect, browser, $ } from '@wdio/globals';
import { t } from '../../i18n.js';
import { closeExtraWindows, invokeBackend, selectDzOptionInWrap } from '../../helpers.js';
import {
  captureStep,
  cleanupFixture,
  createFixture,
  mysqlConfig,
  openDataSyncWindow,
  pgConfig,
  runCompare,
  runDeleteEnableAcceptBranch,
  runEndpointSwapBranch,
  runExecuteAndVerify,
  runExecuteDeleteConfirmBranch,
  runPostCompareReviewBranches,
  runPreCompareValidationBranches,
  runUnsupportedPairHintBranch,
  saveFixtureConnections,
  seedFixtureTable,
  selectFixtureEndpoints,
  type SyncJourneyFixture,
} from './dataSyncJourneyHelpers.js';

function defineDriverJourney(label: string, driver: 'postgresql' | 'mysql') {
  describe(label, () => {
    let mainWindow: string;
    let f: SyncJourneyFixture;
    let sameEndpointId: string;
    let sameEndpointName: string;
    let foreignTargetId: string | undefined;
    let foreignTargetName: string | undefined;

    before(async () => {
      mainWindow = await browser.getWindowHandle();
      await browser.switchToWindow(mainWindow);
      await browser.url('tauri://localhost/');
      await $('[data-testid="workspace-nav-connections"]').waitForDisplayed({ timeout: 15000 });

      const stamp = `${driver === 'postgresql' ? 'pg' : 'my'}_${Date.now().toString(36)}`;
      f = createFixture(driver, stamp);

      sameEndpointId = `e2e_ds_same_${stamp}`;
      sameEndpointName = `DS-Same-${stamp}`;
      const sameCfg =
        driver === 'postgresql'
          ? pgConfig(sameEndpointId, sameEndpointName, f.srcDatabase)
          : mysqlConfig(sameEndpointId, sameEndpointName, f.srcDatabase);
      await invokeBackend('save_connection', { config: sameCfg });

      await saveFixtureConnections(f);
      await seedFixtureTable(f);

      if (driver === 'postgresql') {
        foreignTargetId = `e2e_ds_j_foreign_my_${stamp}`;
        foreignTargetName = `DS-J-Foreign-My-${stamp}`;
        await invokeBackend('save_connection', {
          config: mysqlConfig(foreignTargetId, foreignTargetName, f.tgtDatabase),
        });
      }
    });

    after(async () => {
      await cleanupFixture(f);
      try {
        await invokeBackend('delete_connection', { id: sameEndpointId });
      } catch {
        /* ok */
      }
      if (foreignTargetId) {
        try {
          await invokeBackend('delete_connection', { id: foreignTargetId });
        } catch {
          /* ok */
        }
      }
      await closeExtraWindows(mainWindow);
      await browser.switchToWindow(mainWindow);
      await browser.url('tauri://localhost/');
      await browser.pause(500);
    });

    if (driver === 'postgresql') {
      it('PG 源下 MySQL 目标应标记 unsupportedPair', async () => {
        await openDataSyncWindow();
        await selectDzOptionInWrap('data-sync-source', f.srcName);
        await browser.pause(500);
        await runUnsupportedPairHintBranch(f, foreignTargetName!);
        await closeExtraWindows(mainWindow);
        await browser.switchToWindow(mainWindow);
      });
    }

    it('完整旅程：校验 → 比较 → Review → 执行', async () => {
      await runPreCompareValidationBranches(f, sameEndpointId, sameEndpointName);

      await runEndpointSwapBranch(f);
      await runDeleteEnableAcceptBranch(f);

      await selectFixtureEndpoints(f);
      await browser.pause(300);
      if (driver === 'postgresql') {
        const schema = await $('[data-testid="data-sync-source-schema"]');
        if (await schema.isDisplayed().catch(() => false)) {
          await expect(schema).toBeDisplayed();
          await captureStep(`${f.screenshotPrefix}-14-pg-schema-picker`);
        }
      }

      await runCompare(f);
      await captureStep(`${f.screenshotPrefix}-12-mapping-rows`);

      await runPostCompareReviewBranches(f);
      await captureStep(`${f.screenshotPrefix}-13-review-branches`);

      await runExecuteAndVerify(f);
      await captureStep(`${f.screenshotPrefix}-15-execute-verified`);

      await runExecuteDeleteConfirmBranch(f);
      await captureStep(`${f.screenshotPrefix}-18-delete-execute-verified`);
    });
  });
}

defineDriverJourney('数据同步完整用户旅程 — PostgreSQL (DS-JOURNEY-PG)', 'postgresql');
defineDriverJourney('数据同步完整用户旅程 — MySQL (DS-JOURNEY-MYSQL)', 'mysql');
