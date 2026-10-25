/**
 * UIX Host E2E — user-visible behaviour of the `@datazen/ui` extraction track
 * (commit e4c46a59a: six components moved from `src/components/ui/` into
 * `packages/ui/src/`, old paths became single-line re-export shims, and
 * `src/lib/tid.ts` became a re-export of `packages/ui/src/tid.ts`).
 *
 * These cases deliberately target the behaviours that a pure "move the file"
 * diff can silently break and that unit tests with a mocked `useI18n` cannot see:
 *
 *   UIX-001/002  ConfirmDialog cancel and confirm paths (data cleanup purge).
 *                The dialog is driven through the *real* shared i18n registry, so
 *                its labels are whatever the running app's locale says.
 *   UIX-003      The Dialog header close button's aria-label must be the
 *                LOCALIZED `common.close`, not the library default `'Close'`.
 *                This is the single most easily regressed implicit contract of
 *                the move: in English `common.close` happens to equal the
 *                `Dialog` default, so the defect is invisible under `en` and
 *                only shows as a hardcoded English "Close" in every other
 *                locale. The E2E default language is zh-CN (asserted by
 *                `i18n-menu.ts` I2N-001), which is what makes the check
 *                meaningful; UIX-003 therefore asserts it explicitly.
 *   UIX-004      CopyableError's copy button flips to the localized "copied"
 *                label after a click.
 *   UIX-005/006  ResultMessageDialog (success kind): no copy affordance, a
 *                localized close aria-label, and a second instance reached by
 *                confirming the follow-up backup-key ConfirmDialog.
 *
 * DB-free by construction: every journey is settings / extensions / app-data
 * only, so this spec belongs to the `core` suite.
 *
 * Locators are all `data-*` attributes — no viewport geometry, per the
 * "数据属性解耦" rule.
 */
import { browser, $, expect } from '@wdio/globals';
import { t } from '../i18n.js';
import {
  backFromSettingsInMainWindow,
  captureJourneyStep,
  dismissAnyOpenDialog,
  emitCrossWindowEvent,
  injectDialogPath,
  openSettingsInMainWindow,
  resetDialogQueue,
} from '../helpers.js';

/** The Dialog header close button: the only one carrying an aria-label. */
const HEADER_CLOSE = '[role="dialog"] button[aria-label]';

/**
 * Sentinel the app cannot have produced if the host dictionary registered.
 * `@datazen/ui`'s `t()` falls back to the raw key when a locale is not
 * registered, so a leaked key is trivially detectable in zh-CN.
 */
const KEY_LEAK = /^(common|appData|settings|extensions)\.[A-Za-z]/;

/** Assert the running UI really is zh-CN before trusting any localized label. */
async function expectZhCnRunning(): Promise<void> {
  expect(t('common.close')).toBe('关闭');
  // If the app were running in English, the close label would be the library
  // default "Close" and every "关闭" assertion below would silently pass on the
  // wrong premise, so assert the app's own visible copy first.
  const body = await $('body').getText();
  expect(body).not.toMatch(KEY_LEAK);
}

async function headerCloseLabel(): Promise<string> {
  const el = await $(HEADER_CLOSE);
  await el.waitForDisplayed({ timeout: 8000 });
  const label = await el.getAttribute('aria-label');
  expect(label).not.toBeNull();
  return label as string;
}

async function openDataCleanupConfirm(): Promise<void> {
  await openSettingsInMainWindow('behavior');
  const runBtn = await $('[data-testid="data-cleanup-run"]');
  await runBtn.waitForDisplayed({ timeout: 15000 });
  await runBtn.click();

  const okBtn = await $('[data-testid="confirm-dialog-ok"]');
  await okBtn.waitForDisplayed({ timeout: 10000 });
}

/** Text of the data-cleanup inline result line (empty when nothing ran yet). */
async function dataCleanupMessage(): Promise<string> {
  const section = await $('[data-testid="data-cleanup-section"]');
  await section.waitForDisplayed({ timeout: 10000 });
  return section.getText();
}

describe('UIX — @datazen/ui 抽取后的对话框与复制行为', () => {
  before(async () => {
    await browser.url('tauri://localhost');
    const nav = await $('[data-testid="workspace-nav-databases"]');
    await nav.waitForDisplayed({ timeout: 20000 });
  });

  beforeEach(async () => {
    await dismissAnyOpenDialog();
  });

  after(async () => {
    await dismissAnyOpenDialog();
    const settingsPage = await $('[data-testid="settings-page"]');
    if (await settingsPage.isDisplayed().catch(() => false)) {
      await backFromSettingsInMainWindow();
    }
  });

  it('UIX-001: ConfirmDialog 取消路径 —— 关闭弹窗且不执行清理', async () => {
    await openDataCleanupConfirm();

    // The dialog's own labels come from the shared registry, so under zh-CN they
    // must be the localized strings, never a raw key.
    await expect($('[data-testid="confirm-dialog-ok"]')).toHaveText(
      expect.stringContaining(t('common.confirm')),
    );
    await expect($('[data-testid="confirm-dialog-cancel"]')).toHaveText(
      expect.stringContaining(t('common.cancel')),
    );

    const before = await dataCleanupMessage();
    await $('[data-testid="confirm-dialog-cancel"]').click();

    // Cancelling closes the modal and must not run the purge.
    await browser.waitUntil(
      async () =>
        !(await $('[data-testid="confirm-dialog-ok"]')
          .isExisting()
          .catch(() => false)),
      { timeout: 8000, timeoutMsg: 'ConfirmDialog 未在取消后关闭' },
    );
    const after = await dataCleanupMessage();
    expect(after).not.toContain(t('settings.dataCleanup.success', { count: '0' }));
    expect(after.replace(/\s+/g, '')).toBe(before.replace(/\s+/g, ''));
    await captureJourneyStep('uix-001-confirm-cancel', 0, true);
  });

  it('UIX-002: ConfirmDialog 确认路径 —— 确认后执行清理并回显结果', async () => {
    await openDataCleanupConfirm();

    await $('[data-testid="confirm-dialog-ok"]').click();

    await browser.waitUntil(
      async () =>
        !(await $('[data-testid="confirm-dialog-ok"]')
          .isExisting()
          .catch(() => false)),
      { timeout: 8000, timeoutMsg: 'ConfirmDialog 未在确认后关闭' },
    );
    // Confirming actually performs the work: the success/error line appears.
    await browser.waitUntil(
      async () => {
        const text = await dataCleanupMessage();
        return (
          text.includes('已删除') || // t('settings.dataCleanup.success')
          text.includes(t('settings.dataCleanup.error'))
        );
      },
      { timeout: 15000, timeoutMsg: '确认后未出现清理结果回显' },
    );
    const text = await dataCleanupMessage();
    expect(text).not.toMatch(KEY_LEAK);
    await captureJourneyStep('uix-002-confirm-accept', 0, true);

    await backFromSettingsInMainWindow();
  });

  it('UIX-003: Dialog 关闭按钮的 aria-label 必须是本地化文案，不是硬编码的 "Close"', async () => {
    await expectZhCnRunning();
    await openDataCleanupConfirm();

    const label = await headerCloseLabel();
    expect(label).toBe(t('common.close'));
    // The regression this guards: `Dialog` defaults `closeLabel = 'Close'`, and
    // the English `common.close` is also 'Close', so a dropped `closeLabel`
    // prop is indistinguishable under `en` and shows up here as English.
    expect(label).not.toBe('Close');
    expect(label).not.toMatch(KEY_LEAK);

    // Closing via the header button must run the same path as Cancel.
    await $(HEADER_CLOSE).click();
    await browser.waitUntil(
      async () =>
        !(await $('[data-testid="confirm-dialog-ok"]')
          .isExisting()
          .catch(() => false)),
      { timeout: 8000, timeoutMsg: '点击关闭按钮后 ConfirmDialog 未关闭' },
    );

    await backFromSettingsInMainWindow();
  });

  it('UIX-004: CopyableError 复制按钮点击后变为"已复制"', async () => {
    await browser.url('tauri://localhost');
    const nav = await $('[data-testid="workspace-nav-extensions"]');
    await nav.waitForDisplayed({ timeout: 15000 });
    await nav.click();
    await $('[data-testid="extension-management-page"]').waitForDisplayed({ timeout: 15000 });

    await $('[data-testid="extension-install-button"]').click();
    await $('[role="dialog"]').waitForDisplayed({ timeout: 10000 });

    // A bogus path makes the validate-only inspect fail, which is the
    // CopyableError(copyButton) surface in InstallWappDialog.
    await resetDialogQueue();
    await injectDialogPath('/datazen-e2e-no-such-wapp-package');
    await $('[data-testid="extension-install-browse-folder"]').click();

    const err = await $('[data-testid="extension-install-error"]');
    await err.waitForDisplayed({ timeout: 15000 });
    expect(await err.getText()).not.toMatch(KEY_LEAK);

    const copyBtn = await $('[data-testid="copyable-error-copy"]');
    await copyBtn.waitForDisplayed({ timeout: 8000 });
    expect(await copyBtn.getText()).toBe(t('common.copy'));

    await copyBtn.click();

    await browser.waitUntil(async () => (await copyBtn.getText()) === t('common.copied'), {
      timeout: 8000,
      timeoutMsg: 'CopyableError 复制后未切换为"已复制"状态',
    });
    await captureJourneyStep('uix-004-copyable-error-copied', 0, true);

    // The copied state is a 1.5s affordance and must fall back to "复制".
    await browser.waitUntil(async () => (await copyBtn.getText()) === t('common.copy'), {
      timeout: 8000,
      interval: 200,
      timeoutMsg: 'CopyableError 的已复制状态未在超时后复位',
    });

    await $('[role="dialog"] button[aria-label]').click();
    await browser.waitUntil(
      async () =>
        !(await $('[data-testid="extension-install-error"]')
          .isExisting()
          .catch(() => false)),
      { timeout: 8000, timeoutMsg: '安装弹窗未关闭' },
    );
  });

  it('UIX-005: ResultMessageDialog（成功）—— 无复制按钮且关闭按钮文案已本地化', async () => {
    // Queue the save-dialog answer *before* triggering the export: emitting
    // first lets the native dialog open (and wait) before any answer exists,
    // and resetting the queue afterwards then threw that pending request away —
    // the export never finished, so the success dialog never appeared.
    await resetDialogQueue();
    await injectDialogPath(`${process.env.DATAZEN_DATA_DIR ?? '.'}/uix-export.zip`);
    await emitCrossWindowEvent('menu:export-config');

    const okBtn = await $('[data-testid="result-message-ok"]');
    await okBtn.waitForDisplayed({ timeout: 20000 });
    // OK is the localized `common.ok` ("确定" under zh-CN), NOT a raw key. The
    // unit-test swap from `getByRole(...'common.ok')` to the data-testid was
    // made against a mocked `useI18n`; this case pins the real rendered copy.
    expect(await okBtn.getText()).toBe(t('common.ok'));
    expect(await okBtn.getText()).not.toBe('OK');
    // Success kind has no copy affordance (error-only in the component).
    expect(
      await $('[data-testid="result-message-copy"]')
        .isExisting()
        .catch(() => false),
    ).toBe(false);

    const label = await headerCloseLabel();
    expect(label).toBe(t('common.close'));
    expect(label).not.toBe('Close');
    expect(label).not.toMatch(KEY_LEAK);
    await captureJourneyStep('uix-005-result-message-success', 0, true);

    await okBtn.click();
    await browser.waitUntil(async () => !(await okBtn.isExisting().catch(() => false)), {
      timeout: 8000,
      timeoutMsg: 'ResultMessageDialog 未在点击确定后关闭',
    });
  });

  it('UIX-006: 确认备份密钥的 ConfirmDialog → 第二个 ResultMessageDialog（成功）', async () => {
    // Drive the export again here instead of relying on whatever UIX-005 left
    // behind: the key prompt only appears *after* the export-success dialog is
    // dismissed, and a case that starts mid-flow cannot tell "not reached yet"
    // from "never shown".
    await resetDialogQueue();
    await injectDialogPath(`${process.env.DATAZEN_DATA_DIR ?? '.'}/uix-export-key.zip`);
    await emitCrossWindowEvent('menu:export-config');

    const successOk = await $('[data-testid="result-message-ok"]');
    await successOk.waitForDisplayed({ timeout: 20000 });
    await successOk.click();
    await browser.waitUntil(async () => !(await successOk.isExisting().catch(() => false)), {
      timeout: 8000,
      timeoutMsg: '导出成功弹窗未关闭',
    });

    const okBtn = await $('[data-testid="confirm-dialog-ok"]');
    await okBtn.waitForDisplayed({ timeout: 15000 });
    await expect(okBtn).toHaveText(expect.stringContaining(t('common.save')));
    expect(await headerCloseLabel()).toBe(t('common.close'));

    // Same ordering rule as UIX-005: answer the key-file save dialog before the
    // click that opens it.
    await resetDialogQueue();
    await injectDialogPath(`${process.env.DATAZEN_DATA_DIR ?? '.'}/uix-datazen.key`);
    await okBtn.click();

    const savedOk = await $('[data-testid="result-message-ok"]');
    await savedOk.waitForDisplayed({ timeout: 20000 });
    expect(await headerCloseLabel()).toBe(t('common.close'));
    expect(await headerCloseLabel()).not.toBe('Close');
    expect(
      await $('[data-testid="result-message-copy"]')
        .isExisting()
        .catch(() => false),
    ).toBe(false);

    await savedOk.click();
    await browser.waitUntil(async () => !(await savedOk.isExisting().catch(() => false)), {
      timeout: 8000,
      timeoutMsg: '备份密钥保存成功弹窗未关闭',
    });
  });
});
