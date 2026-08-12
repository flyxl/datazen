/**
 * Host Connection Contract matrix (F2): for each DriverFixture, open its
 * connection window and run HC-DATA / HC-FILTER / HC-QUERY.
 */
import { browser, $ } from '@wdio/globals';
import { t } from '../i18n.js';
import { closeExtraWindows } from '../helpers.js';
import {
  DEFAULT_MATRIX_DRIVERS,
  getFixture,
  type DriverFixtureId,
} from '../contract/fixtures';
import { openFixtureConnection, type ContractConnCtx } from '../contract/open-fixture';
import { describeMatrixTitle, planJourneys } from '../contract/journeys/plan';
import { runHcData, runHcFilter, runHcQuery } from '../contract/journeys/run-core';

const DRIVERS: DriverFixtureId[] = [...DEFAULT_MATRIX_DRIVERS];

describe('Host Connection Contract matrix (F2)', () => {
  let mainWindow: string;

  before(async () => {
    mainWindow = await browser.getWindowHandle();
    await $(`button*=${t('action.newConnection')}`).waitForDisplayed({ timeout: 15000 });
  });

  after(async () => {
    await closeExtraWindows(mainWindow);
  });

  for (const driverId of DRIVERS) {
    const fixture = getFixture(driverId);

    describe(describeMatrixTitle(fixture), () => {
      const plan = planJourneys(fixture);
      let ctx: ContractConnCtx;

      before(async () => {
        ctx = await openFixtureConnection(fixture, mainWindow);
      });

      after(async () => {
        await closeExtraWindows(mainWindow);
        await browser.switchToWindow(mainWindow);
        await browser.pause(400);
      });

      for (const step of plan) {
        const title = `${step.id}${step.status === 'skip' ? ' (skipped)' : ''}`;
        it(title, async function () {
          if (step.status === 'skip') {
            this.skip();
            return;
          }
          if (step.id === 'HC-QUERY') await runHcQuery(ctx);
          else if (step.id === 'HC-DATA') await runHcData(ctx);
          else if (step.id === 'HC-FILTER') await runHcFilter(ctx);
        });
      }
    });
  }
});
