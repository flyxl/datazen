/**
 * First-run journey (onboarding wizard) — cross-module continuous journey.
 *
 * Covers the contract the wizard must hold for a brand-new installation:
 * - S0 shows the three entry cards and nothing else is clickable;
 * - the sidebar version label and the footer step indicator share one bottom line;
 * - "Import connections" renders the import form INLINE (never a modal dialog)
 *   and a successful import unlocks Continue;
 * - step 2 is the AI provider step for every entry (import / sample / manual);
 * - "Open DataZen" persists `onboarding.completed` and lands in the workspace;
 * - upgrading users (settings without an onboarding state) never see the journey.
 *
 * The upgrade case is proven end-to-end by removing the `onboarding` state: that
 * is exactly what the backend hands the UI for a legacy settings.json.
 */
import { expect, browser, $, $$ } from '@wdio/globals';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { t } from '../../i18n.js';
import {
  captureJourneyStep,
  closeExtraWindows,
  ensureMainWindowForIpc,
  injectDialogPath,
  invokeBackend,
  resetDialogQueue,
} from '../../helpers.js';
import {
  deleteJourneyConnectionsByName,
  PG_FORM_DEFAULTS,
  restoreDefaultJourneyConnection,
} from './connectionJourneyHelpers.js';
import { seedDefaultPgConnection } from '../../lib/testDataLifecycle.js';

const WIZARD = '[data-testid="onboarding-wizard"]';
const STEP_LABEL = '[data-testid="onboarding-step-label"]';
const CONTINUE = '[data-testid="onboarding-continue"]';
const SAMPLE_CONN_NAME = 'Sample Playground';
const MANUAL_CONN_NAME = `E2E 引导手动连接 ${Date.now().toString(36)}`;
const SHARE_PASSWORD = 'e2e-onboarding-pass';

type OnboardingState = { completed: boolean; version: number } | null;

/** Rewrite the persisted onboarding state and reload so MainPage re-evaluates the gate. */
async function setOnboardingState(state: OnboardingState): Promise<void> {
  await ensureMainWindowForIpc();
  const settings = await invokeBackend<Record<string, unknown>>('get_settings');
  await invokeBackend('save_settings', {
    settings: { ...settings, onboarding: state },
  });
  await browser.execute(() => location.reload());
  await browser.pause(1500);
}

/** Enter the journey as a fresh install would. */
async function enterFreshInstall(): Promise<void> {
  await setOnboardingState({ completed: false, version: 1 });
  await $(WIZARD).waitForDisplayed({ timeout: 15000 });
}

/** Assert the visible step number in the footer. */
async function expectStepLabel(key: string): Promise<void> {
  await browser.waitUntil(async () => (await $(STEP_LABEL).getText()) === t(key), {
    timeout: 10000,
    timeoutMsg: `等待步骤标签 ${key} 超时`,
  });
}

/** Bottom edge (viewport px) of an element — used for the footer alignment check. */
async function bottomOf(selector: string): Promise<number> {
  const el = await $(selector);
  await el.waitForDisplayed({ timeout: 10000 });
  const { y } = await el.getLocation();
  const { height } = await el.getSize();
  return y + height;
}

/** Create a real encrypted DataZen export of the seeded connection. */
async function createImportFixture(): Promise<string> {
  await ensureMainWindowForIpc();
  const target = path.join(os.tmpdir(), `datazen-onboarding-${Date.now()}.datazenconnection`);
  const count = await invokeBackend<number | null>('export_connections', {
    password: SHARE_PASSWORD,
    defaultFileName: 'onboarding-e2e.datazenconnection',
    overridePath: target,
  });
  expect(count ?? 0).toBeGreaterThan(0);
  expect(fs.existsSync(target)).toBe(true);
  return target;
}

async function clickContinue(): Promise<void> {
  const button = await $(CONTINUE);
  await browser.waitUntil(async () => await button.isEnabled(), {
    timeout: 10000,
    timeoutMsg: 'Continue 未解锁：第一步尚未完成',
  });
  await button.click();
}

/** Wait until the sample dataset finished seeding (or report the failure loudly). */
async function waitForSampleSeeded(): Promise<void> {
  await browser.waitUntil(
    async () =>
      (await $('[data-testid="onboarding-sample-path"]').isExisting()) ||
      (await $('[data-testid="onboarding-sample-error"]').isExisting()),
    { timeout: 30000, timeoutMsg: '示例数据既未就绪也未报错' },
  );
  const error = await $('[data-testid="onboarding-sample-error"]');
  if (await error.isExisting()) {
    throw new Error(`示例数据种子失败: ${await error.getText()}`);
  }
}

/** The wizard's connection form has no dialog wrapper, so scope inputs to the step. */
async function setWizardPort(value: string): Promise<void> {
  const step = await $('[data-testid="onboarding-step-s1-manual"]');
  const inputs = await step.$$('input');
  for (const input of inputs) {
    const type = (await input.getAttribute('type')) || 'text';
    const placeholder = (await input.getAttribute('placeholder')) || '';
    if (type !== 'password' && placeholder === '') {
      await input.clearValue();
      await input.setValue(value);
      return;
    }
  }
  throw new Error('未找到引导页连接端口输入框');
}

describe('首次安装引导旅程 (ONBOARDING-JOURNEY)', () => {
  let mainWindow: string;
  let fixturePath = '';

  before(async () => {
    mainWindow = await browser.getWindowHandle();
    // Both helpers navigate/reload when the workspace is not visible, so they
    // must run BEFORE the journey replaces MainPage.
    await ensureMainWindowForIpc();
    await seedDefaultPgConnection(browser);
    fixturePath = await createImportFixture();
  });

  afterEach(async () => {
    await closeExtraWindows(mainWindow);
    await browser.switchToWindow(mainWindow);
  });

  after(async () => {
    // Never leak the journey (or its fixtures) into the following suites.
    if (fixturePath) {
      try {
        fs.unlinkSync(fixturePath);
      } catch {
        /* fixture already gone */
      }
    }
    await deleteJourneyConnectionsByName(SAMPLE_CONN_NAME, MANUAL_CONN_NAME);
    await restoreDefaultJourneyConnection();
    await resetDialogQueue();
    await setOnboardingState({ completed: true, version: 1 });
    await browser.pause(500);
  });

  it('J1 全新安装：S0 三入口 + 底部对齐 → 内联导入表单（非弹窗）→ 导入成功 → 第二步 AI → Done → 工作区', async () => {
    await enterFreshInstall();

    // S0: three entry cards, no dialog, wizard replaces the workspace.
    await $('[data-testid="onboarding-entry-import"]').waitForDisplayed({ timeout: 10000 });
    await $('[data-testid="onboarding-entry-manual"]').waitForDisplayed();
    await $('[data-testid="onboarding-entry-sample"]').waitForDisplayed();
    await expect(await $('[data-testid="welcome-page"]')).not.toBeExisting();
    expect(await $$('[role="dialog"]')).toHaveLength(0);
    await captureJourneyStep('onboarding-s0');

    // Requirement 1: the sidebar version label and the footer step label end on
    // the same bottom line (same text metrics, same fixed footer height).
    const versionBottom = await bottomOf('[data-testid="onboarding-version-label"]');
    await $('[data-testid="onboarding-entry-import"]').click();
    await $('[data-testid="onboarding-step-s1-import"]').waitForDisplayed({ timeout: 10000 });
    const stepBottom = await bottomOf(`${STEP_LABEL}`);
    expect(Math.abs(versionBottom - stepBottom)).toBeLessThanOrEqual(1);

    // Requirement 2: the import form is rendered inline inside the wizard.
    const form = await $('[data-testid="onboarding-import-form"]');
    await form.waitForDisplayed({ timeout: 10000 });
    expect(await $(WIZARD).isDisplayed()).toBe(true);
    expect(await $$('[role="dialog"]')).toHaveLength(0);
    await expectStepLabel('onboarding.s1.stepLabel');
    // Nothing imported yet → Continue stays locked.
    expect(await $(CONTINUE).isEnabled()).toBe(false);
    await captureJourneyStep('onboarding-import-inline-form');

    // Pin the source to a file: a detected client config would pre-select an
    // external app instead of the fixture.
    await $('[data-testid="onboarding-import-source-file"]').click();

    // Pick the export through the real native picker (injected) + import it.
    await resetDialogQueue();
    await injectDialogPath(fixturePath);
    const submit = await $('[data-testid="onboarding-import-submit"]');
    await submit.click();
    await $('[data-testid="import-selected-file"]').waitForDisplayed({ timeout: 15000 });
    await $('input[placeholder="' + t('connShare.passwordImportPlaceholder') + '"]').setValue(
      SHARE_PASSWORD,
    );
    await submit.click();
    await $('[data-testid="onboarding-import-success"]').waitForDisplayed({ timeout: 30000 });
    // Still no modal: the result is inline.
    expect(await $$('[role="dialog"]')).toHaveLength(0);
    await captureJourneyStep('onboarding-import-success');

    // Continue → step 2 of 2, which is ALWAYS the AI provider step.
    await clickContinue();
    await $('[data-testid="onboarding-step-s2-ai"]').waitForDisplayed({ timeout: 15000 });
    await expectStepLabel('onboarding.s2.stepLabel');
    await captureJourneyStep('onboarding-step2-ai');

    // Skip AI (soft gate) and finish into the workspace.
    await $('[data-testid="onboarding-finish"]').click();
    await $('[data-testid="onboarding-step-s3"]').waitForDisplayed({ timeout: 15000 });
    expect(await $('[data-testid="onboarding-summary-ai"]').getText()).toContain(
      t('onboarding.s3.aiNotConfigured'),
    );
    await captureJourneyStep('onboarding-done');

    await $('[data-testid="onboard-open-datazen"]').click();
    await browser.waitUntil(async () => !(await $(WIZARD).isExisting()), {
      timeout: 15000,
      timeoutMsg: '完成向导后仍停留在引导页',
    });
    await $('[data-testid="workspace-nav-databases"]').waitForDisplayed({ timeout: 15000 });

    // The completion flag is persisted → a reload never shows the journey again.
    await browser.execute(() => location.reload());
    await browser.pause(1500);
    expect(await $(WIZARD).isExisting()).toBe(false);
  });

  it('J2 sample 入口：种子示例库并建连接 → 第二步仍是 AI → 工作区可见 Sample Playground', async () => {
    await enterFreshInstall();

    await $('[data-testid="onboarding-entry-sample"]').click();
    await $('[data-testid="onboarding-step-s1-sample"]').waitForDisplayed({ timeout: 10000 });
    await waitForSampleSeeded();
    await expectStepLabel('onboarding.s1.stepLabel');

    await clickContinue();
    await $('[data-testid="onboarding-step-s2-ai"]').waitForDisplayed({ timeout: 15000 });
    await expectStepLabel('onboarding.s2.stepLabel');
    await captureJourneyStep('onboarding-sample-step2-ai');

    await $('[data-testid="onboarding-skip"]').click();
    await $('[data-testid="onboarding-step-s3"]').waitForDisplayed({ timeout: 15000 });
    expect(await $('[data-testid="onboarding-summary-connection"]').getText()).toContain(
      SAMPLE_CONN_NAME,
    );

    await $('[data-testid="onboard-open-datazen"]').click();
    await browser.waitUntil(async () => !(await $(WIZARD).isExisting()), {
      timeout: 15000,
      timeoutMsg: '完成向导后仍停留在引导页',
    });
    const names = await invokeBackend<Array<{ name: string }>>('get_connections');
    expect(names.some((c) => c.name === SAMPLE_CONN_NAME)).toBe(true);
  });

  it('J3 manual 入口：第一步是连接表单（无全局 Continue）→ 测试并保存 → 第二步 AI', async () => {
    await enterFreshInstall();

    await $('[data-testid="onboarding-entry-manual"]').click();
    await $('[data-testid="onboarding-step-s1-manual"]').waitForDisplayed({ timeout: 10000 });
    await expectStepLabel('onboarding.s1.stepLabel');
    // The connection form owns step 1: no footer Continue until it is saved.
    expect(await $(CONTINUE).isExisting()).toBe(false);

    const nameInput = await $('input[placeholder="例如：主数据库"]');
    await nameInput.waitForDisplayed({ timeout: 10000 });
    await nameInput.setValue(MANUAL_CONN_NAME);
    const hostInput = await $('input[placeholder="prod-db.example.com"]');
    await hostInput.clearValue();
    await hostInput.setValue(PG_FORM_DEFAULTS.host);
    await setWizardPort(PG_FORM_DEFAULTS.port);
    const databaseInput = await $('input[placeholder="myapp_production"]');
    await databaseInput.clearValue();
    await databaseInput.setValue(PG_FORM_DEFAULTS.database);
    const usernameInput = await $('input[placeholder="postgres"]');
    await usernameInput.clearValue();
    await usernameInput.setValue(PG_FORM_DEFAULTS.username);
    if (PG_FORM_DEFAULTS.password) {
      await $('input[type="password"]').setValue(PG_FORM_DEFAULTS.password);
    }
    await captureJourneyStep('onboarding-manual-form');

    // Save only unlocks after a successful connection test (same gate as the
    // real New Connection dialog).
    const saveButton = await $('[data-testid="onboard-s1-save"]');
    expect(await saveButton.isEnabled()).toBe(false);
    await $('[data-testid="onboard-test-connection"]').click();
    await browser.waitUntil(async () => await saveButton.isEnabled(), {
      timeout: 45000,
      timeoutMsg: '连接测试成功后 Save 仍未解锁',
    });
    await saveButton.click();

    // Save → step 2 of 2 is the AI provider step.
    await $('[data-testid="onboarding-step-s2-ai"]').waitForDisplayed({ timeout: 15000 });
    await expectStepLabel('onboarding.s2.stepLabel');
    await captureJourneyStep('onboarding-manual-step2-ai');

    const connections = await invokeBackend<Array<{ name: string }>>('get_connections');
    expect(connections.some((c) => c.name === MANUAL_CONN_NAME)).toBe(true);
  });

  it('J4 升级用户：settings 无 onboarding 状态时不显示引导，直接进入工作区', async () => {
    // Exactly what an installation predating the journey looks like.
    await setOnboardingState(null);

    expect(await $(WIZARD).isExisting()).toBe(false);
    await $('[data-testid="workspace-nav-databases"]').waitForDisplayed({ timeout: 15000 });
    await captureJourneyStep('onboarding-upgrade-bypass');
  });
});
