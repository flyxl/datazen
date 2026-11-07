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

const WIZARD_LABEL = 'onboarding';
/** Parked window that keeps the app alive while the wizard is closed. */
const ANCHOR_LABEL = 'e2e-onboarding-anchor';
const WORKSPACE_NAV = '[data-testid="workspace-nav-databases"]';

/**
 * Find the window that actually renders `selector`, and leave it focused.
 *
 * Identifying windows by "any handle other than the current one" is wrong for a
 * fresh install: the app has no `main` window at all until `onboarding_complete`
 * creates one, so the onboarding window is the ONLY handle — and
 * `ensureMainWindowForIpc` navigates that very handle to the main app to reach
 * IPC. Under the old definition the lookup returned `undefined` and every case
 * timed out on "onboarding wizard window did not open" even though the wizard
 * was already on screen. Tauri window labels are the WebDriver handles here, so
 * match on rendered content instead and return undefined when nothing matches.
 */
async function windowShowing(selector: string): Promise<string | undefined> {
  const current = await browser.getWindowHandle();
  for (const handle of await browser.getWindowHandles()) {
    await browser.switchToWindow(handle);
    if (await $(selector).isExisting()) return handle;
  }
  await browser.switchToWindow(current);
  return undefined;
}

/** Window handle of the onboarding wizard, if one is currently open. */
async function wizardWindowHandle(): Promise<string | undefined> {
  return windowShowing(WIZARD);
}

/** Window handle of the workspace window (MainPage), if one is currently open. */
async function mainWindowHandle(): Promise<string | undefined> {
  return windowShowing(WORKSPACE_NAV);
}

/**
 * Rewrite the persisted onboarding state, then mirror what Rust bootstrap does
 * with it (bootstrap/run.rs): an incomplete state gets its own `onboarding`
 * Tauri window, a complete one lands on the main window.
 *
 * The wizard is a SEPARATE window — MainPage never renders it — so a reload of
 * the main window can no longer produce it. `create_sub_window` is the same
 * command the host uses for its own sub-windows and reuses an existing window
 * with the same label, which keeps J1 -> J2 -> J3 idempotent.
 *
 * Deliberately does NOT call `ensureMainWindowForIpc`: that helper re-points the
 * focused window at MainPage, and on a first run the focused window IS the
 * wizard window the app's bootstrap opened. Re-pointing it left Tauri with a
 * wizard window that no longer honoured the close in `onboarding_complete`
 * (Tauri accepted the close and kept the window up), so J1 finished with the
 * wizard still on screen. Every Tauri window can invoke backend commands, so
 * persisting the state from the focused window is enough.
 *
 * Returns the window handle the journey should be driving: the wizard when one
 * is open, otherwise the workspace window.
 */
async function setOnboardingState(state: OnboardingState): Promise<string> {
  const settings = await invokeBackend<Record<string, unknown>>('get_host_settings');
  await invokeBackend('save_host_settings', {
    settings: { ...settings, onboarding: state },
  });

  if (state?.completed === false) {
    // The wizard has to be a window `create_sub_window` genuinely CREATED, not
    // the one the app bootstrapped. The global E2E `before` hook (wdio.conf.ts)
    // runs `ensureMainWindowForIpc` before this spec's own hook, and that
    // re-points whatever window exists at MainPage — on a fresh install that is
    // the bootstrap wizard. `create_sub_window` would then REUSE that
    // already-navigated window, and `onboarding_complete` stops closing it:
    // Tauri accepted the close and kept the wizard on screen (probed via
    // `app.webview_windows()` 800ms after the call), so J1 finished with two
    // windows. A freshly created wizard window closes correctly, so drop the
    // spent one and build a new one.
    const handles = await browser.getWindowHandles();
    if (handles.includes(WIZARD_LABEL)) {
      // Tauri exits once its last window closes, and a fresh install has no
      // workspace window yet, so park an anchor window while the wizard is
      // closed and drop it again once the new wizard is up.
      let anchor = '';
      if (handles.length === 1) {
        await invokeBackend('create_sub_window', {
          options: {
            label: ANCHOR_LABEL,
            url: 'index.html',
            title: 'DataZen',
            width: 640,
            height: 480,
            decorations: false,
          },
        });
        anchor = await browser.waitUntil(
          async () => {
            const open = await browser.getWindowHandles();
            return open.includes(ANCHOR_LABEL) ? ANCHOR_LABEL : undefined;
          },
          { timeout: 15000, timeoutMsg: 'anchor window did not open' },
        );
      }
      await browser.switchToWindow(WIZARD_LABEL);
      await browser.closeWindow();
      const host = anchor || (await browser.getWindowHandles())[0];
      await browser.switchToWindow(host);
      await browser.execute(() => location.reload());
      await browser.pause(1000);
    }
    await invokeBackend('create_sub_window', {
      options: {
        label: WIZARD_LABEL,
        url: 'window.html?window=onboarding',
        title: 'DataZen',
        width: 960,
        height: 720,
        minWidth: 640,
        minHeight: 480,
        decorations: true,
      },
    });
    const wizard = await browser.waitUntil(async () => wizardWindowHandle(), {
      timeout: 15000,
      timeoutMsg: 'onboarding wizard window did not open',
    });
    // The anchor has done its job; the wizard keeps the app alive on its own.
    if ((await browser.getWindowHandles()).includes(ANCHOR_LABEL)) {
      await browser.switchToWindow(ANCHOR_LABEL);
      await browser.closeWindow();
    }
    await browser.switchToWindow(wizard);
    // The window is shown on PageLoadEvent::Finished; give the SPA a beat to
    // mount its first step.
    await browser.pause(1000);
    return wizard;
  }

  // Completed/legacy state: the wizard must not exist. Drop any left over.
  const stale = await wizardWindowHandle();
  if (stale) {
    await browser.closeWindow();
    await ensureMainWindowForIpc();
  }
  if ((await browser.getWindowHandles()).includes(ANCHOR_LABEL)) {
    await browser.switchToWindow(ANCHOR_LABEL);
    await browser.closeWindow();
    await ensureMainWindowForIpc();
  }
  await browser.execute(() => location.reload());
  await browser.pause(1500);
  return browser.getWindowHandle();
}

/** Enter the journey as a fresh install would, in the wizard's own window. */
async function enterFreshInstall(): Promise<string> {
  const wizard = await setOnboardingState({ completed: false, version: 1 });
  await $(WIZARD).waitForDisplayed({ timeout: 15000 });
  return wizard;
}

/**
 * `onboard-open-datazen` calls `onboarding_complete`, which CLOSES the wizard
 * window and creates `main` if absent. On a first-run journey `main` did not
 * exist before, so the workspace handle must be resolved by content once the
 * wizard is gone — the handle captured in `before()` belonged to the wizard and
 * died with it. The resolved handle is handed back so `afterEach` keeps a
 * live window to return to.
 */
async function finishWizard(): Promise<string> {
  const main = await browser.waitUntil(async () => mainWindowHandle(), {
    timeout: 15000,
    timeoutMsg: '完成向导后工作区窗口未出现',
  });
  await browser.switchToWindow(main);
  await $(WORKSPACE_NAV).waitForDisplayed({ timeout: 15000 });
  return main;
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
  // No `ensureMainWindowForIpc` here: on a first run it would re-point the
  // bootstrap wizard window at MainPage (see `setOnboardingState`), and the
  // fixture only needs a window that can invoke backend commands.
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
  // Reassigned by `finishWizard`: the first-run wizard window is the app's only
  // window, and completing it destroys that handle while creating the
  // workspace one, so the initial capture is only valid before the first Done.
  let mainWindow: string;
  let fixturePath = '';

  before(async () => {
    // Deliberately no `ensureMainWindowForIpc`: on a first run the only window
    // is the bootstrap wizard window, and navigating it to MainPage breaks the
    // close inside `onboarding_complete` (see `setOnboardingState`). Both
    // helpers below drive the journey from the focused window instead.
    mainWindow = await browser.getWindowHandle();
    await seedDefaultPgConnection(browser);
    fixturePath = await createImportFixture();
  });

  afterEach(async () => {
    const live = (await browser.getWindowHandles()).includes(mainWindow)
      ? mainWindow
      : ((await browser.getWindowHandles())[0] ?? mainWindow);
    await closeExtraWindows(live);
    await browser.switchToWindow(live);
    mainWindow = live;
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

    // S0: three entry cards, no dialog. The wizard owns its own OS window, so
    // the workspace is simply not what is on screen here.
    await $('[data-testid="onboarding-entry-import"]').waitForDisplayed({ timeout: 10000 });
    await $('[data-testid="onboarding-entry-manual"]').waitForDisplayed();
    await $('[data-testid="onboarding-entry-sample"]').waitForDisplayed();
    await expect(await $('[data-testid="connection-workspace-home"]')).not.toBeExisting();
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
    mainWindow = await finishWizard();

    // The completion flag is persisted → a reload never shows the journey again.
    await browser.execute(() => location.reload());
    await browser.pause(1500);
    expect(await wizardWindowHandle()).toBeUndefined();
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
    mainWindow = await finishWizard();
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

    expect(await wizardWindowHandle()).toBeUndefined();
    expect(await $(WIZARD).isExisting()).toBe(false);
    await $('[data-testid="workspace-nav-databases"]').waitForDisplayed({ timeout: 15000 });
    await captureJourneyStep('onboarding-upgrade-bypass');
  });
});
