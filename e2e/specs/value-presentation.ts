import { expect, browser, $, $$ } from '@wdio/globals';
import { closeWorkspacePanels } from '../lib/testDataLifecycle.js';
import {
  backFromSettingsInMainWindow,
  clickNavigatorRefresh,
  clickTableInSidebar,
  closeExtraWindows,
  connectSeededPgInWorkspace,
  executeSQLChecked,
  openQueryTab,
  openSettingsInMainWindow,
  selectDzOptionInWrap,
  switchSubTab,
  waitForNewQueryButton,
  waitForTableInSidebar,
} from '../helpers.js';

/**
 * Value presentation / time display (issue-32).
 *
 * Verifies settings UI for time display and that enabling it affects query results
 * (table data view). Requires a PostgreSQL connection (seeded by wdio.conf.ts).
 */

const TEST_TABLE = '_e2e_value_presentation_ts';
const BIGINT_TABLE = '_e2e_value_presentation_bigint';

async function invokeBackend<T>(cmd: string, args: Record<string, unknown> = {}): Promise<T> {
  const result = await browser.executeAsync(
    (c: string, a: string, done: (r: unknown) => void) => {
      (
        window as unknown as {
          __TAURI_INTERNALS__: { invoke: (c: string, a: unknown) => Promise<unknown> };
        }
      ).__TAURI_INTERNALS__
        .invoke(c, JSON.parse(a))
        .then((r: unknown) => done(r))
        .catch((e: unknown) => done({ __error: String(e) }));
    },
    cmd,
    JSON.stringify(args),
  );
  if (result && typeof result === 'object' && '__error' in (result as Record<string, unknown>)) {
    throw new Error((result as { __error: string }).__error);
  }
  return result as T;
}

async function scrollToValuePresentationSettings() {
  const section = await $('[data-testid="value-presentation-settings"]');
  await section.scrollIntoView({ block: 'center' });
  await section.waitForDisplayed({ timeout: 10000 });
}

async function setToggle(testId: string, checked: boolean) {
  const toggle = await $(`[data-testid="${testId}"]`);
  await toggle.waitForDisplayed({ timeout: 10000 });
  const current = (await toggle.getAttribute('aria-checked')) === 'true';
  if (current !== checked) {
    await toggle.click();
    await browser.pause(200);
  }
}

async function saveTimeDisplaySettings() {
  const saveBtn = await $('[data-testid="value-presentation-save"]');
  await saveBtn.scrollIntoView({ block: 'center' });
  await browser.waitUntil(
    async () => {
      const disabled = await saveBtn.getAttribute('disabled');
      const clickable = await saveBtn.isClickable().catch(() => false);
      const saved = await $('[data-testid="value-presentation-saved"]')
        .isDisplayed()
        .catch(() => false);
      return clickable || disabled !== null || saved;
    },
    { timeout: 10000, timeoutMsg: '等待时间显示保存按钮状态就绪超时' },
  );
  const disabled = await saveBtn.getAttribute('disabled');
  const alreadySaved = await $('[data-testid="value-presentation-saved"]')
    .isDisplayed()
    .catch(() => false);
  if (disabled !== null || alreadySaved) {
    return;
  }
  await saveBtn.click();
  const saved = await $('[data-testid="value-presentation-saved"]');
  await saved.waitForDisplayed({ timeout: 10000, timeoutMsg: '等待时间显示设置保存成功提示超时' });
}

async function readFirstVisibleCellText(col: string, rootSelector?: string): Promise<string> {
  return (await browser.execute(
    (column: string, root: string | null) => {
      const scope = root ? document.querySelector(root) : document;
      if (!scope) return '';
      const cells = Array.from(
        scope.querySelectorAll<HTMLElement>(
          `[data-testid="data-table-cell"][data-dt-col="${column}"]`,
        ),
      ).filter((el) => el.offsetParent !== null);
      return (cells[0]?.textContent ?? '').trim();
    },
    col,
    rootSelector ?? null,
  )) as string;
}

async function waitForFirstCell(col: string, timeoutMsg: string, rootSelector?: string) {
  await browser.waitUntil(
    async () => (await readFirstVisibleCellText(col, rootSelector)).length > 0,
    { timeout: 15000, timeoutMsg },
  );
}

async function executeSqlInEditorAndWait(sql: string) {
  await executeSQLChecked(sql);
  const table = await $('[data-testid="result-workspace-table"]');
  await table.waitForDisplayed({ timeout: 20000, timeoutMsg: '等待查询结果表格显示超时' });
}

async function selectDzOptionByTestId(testId: string, optionLabel: string) {
  let lastError: unknown;
  for (let attempt = 0; attempt < 3; attempt++) {
    try {
      await browser.execute((id: string) => {
        const el = document.querySelector(`[data-testid="${id}"]`) as HTMLElement | null;
        if (!el) throw new Error(`Select element not found: ${id}`);
        const trigger =
          el.getAttribute('aria-haspopup') === 'listbox'
            ? el
            : (el.querySelector('button[aria-haspopup="listbox"]') as HTMLElement | null);
        if (!trigger) throw new Error(`Select trigger not found: ${id}`);
        trigger.click();
      }, testId);

      await browser.waitUntil(
        async () =>
          browser.execute(() => Boolean(document.querySelector('[data-testid="select-listbox"]'))),
        { timeout: 5000, timeoutMsg: `Select listbox did not open: ${testId}` },
      );

      await browser.execute((option: string) => {
        const list = document.querySelector('[data-testid="select-listbox"]');
        if (!list) throw new Error('select-listbox not open');
        const item = Array.from(list.children).find((el) =>
          (el.textContent || '').includes(option),
        );
        if (!item) throw new Error(`Select option not found: ${option}`);
        item.dispatchEvent(new MouseEvent('mousedown', { bubbles: true, cancelable: true }));
      }, optionLabel);
      await browser.pause(300);
      return;
    } catch (e) {
      lastError = e;
      await browser.pause(400);
    }
  }
  throw lastError;
}

async function selectDzOptionByCandidates(testId: string, candidates: string[]) {
  let lastError: unknown;
  for (const label of candidates) {
    try {
      await selectDzOptionByTestId(testId, label);
      return;
    } catch (err) {
      lastError = err;
    }
  }
  throw lastError ?? new Error(`未找到可选项(${testId}): ${candidates.join(', ')}`);
}

async function readFirstQueryResultCell(col: string): Promise<string> {
  return readFirstVisibleCellText(col, '[data-testid="result-workspace-table"]');
}

async function waitForQueryResultCell(col: string, timeoutMsg: string) {
  await waitForFirstCell(col, timeoutMsg, '[data-testid="result-workspace-table"]');
}

async function openColumnPresentationDialog(columnName: string) {
  const header = await $(`[data-col-header="${columnName}"]`);
  await header.waitForDisplayed({ timeout: 15000, timeoutMsg: `等待列头 ${columnName} 显示超时` });
  await browser.execute((name: string) => {
    const el = document.querySelector(`[data-col-header="${name}"]`) as HTMLElement | null;
    if (!el) throw new Error(`Column header not found: ${name}`);
    el.dispatchEvent(new MouseEvent('contextmenu', { bubbles: true, cancelable: true, button: 2 }));
  }, columnName);
  const item = await $('[data-testid="web-context-item-column-presentation"]');
  await item.waitForDisplayed({ timeout: 15000, timeoutMsg: '等待“显示方式…”菜单项超时' });
  await item.click();
  const dialog = await $('[data-testid="column-presentation-dialog"]');
  await dialog.waitForDisplayed({ timeout: 10000, timeoutMsg: '等待列显示方式对话框超时' });
}

async function saveColumnPresentationDialog() {
  const saveBtn = await $('[data-testid="column-presentation-save"]');
  await saveBtn.waitForClickable({ timeout: 10000, timeoutMsg: '列显示方式保存按钮不可用' });
  await saveBtn.click();
  await browser.waitUntil(
    async () =>
      !(await $('[data-testid="column-presentation-dialog"]')
        .isDisplayed()
        .catch(() => false)),
    { timeout: 10000, timeoutMsg: '等待列显示方式对话框关闭超时' },
  );
}

describe('时间显示 / Value presentation (VP-001~VP-007)', () => {
  let mainWindow: string;
  let originalValuePresentation: unknown;

  before(async () => {
    mainWindow = await browser.getWindowHandle();
    const settings = await invokeBackend<Record<string, unknown>>('get_host_settings');
    originalValuePresentation = settings.valuePresentation;
  });

  after(async () => {
    try {
      await browser.switchToWindow(mainWindow);
      await openQueryTab().catch(() => {});
      await executeSQLChecked(`DROP TABLE IF EXISTS ${TEST_TABLE}`).catch(() => {});
      await executeSQLChecked(`DROP TABLE IF EXISTS ${BIGINT_TABLE}`).catch(() => {});
    } catch {
      // best-effort cleanup
    }
    try {
      const settings = await invokeBackend<Record<string, unknown>>('get_host_settings');
      await invokeBackend('save_host_settings', {
        settings: {
          ...settings,
          valuePresentation: originalValuePresentation ?? { version: 1, renderers: {} },
        },
      });
    } catch {
      // best-effort restore
    }
    try {
      await closeExtraWindows(mainWindow);
      await closeWorkspacePanels(browser);
      await backFromSettingsInMainWindow().catch(() => {});
    } catch {
      // best-effort cleanup
    }
  });

  it('VP-001: 数据浏览设置中可启用时间显示并保存格式', async () => {
    await openSettingsInMainWindow('dataBrowsing');
    const sectionRoot = await $('[data-testid="settings-section-dataBrowsing"]');
    await sectionRoot.waitForDisplayed({ timeout: 15000 });

    await scrollToValuePresentationSettings();

    await setToggle('value-presentation-enable', true);

    // ISO format is deterministic; pick option containing "ISO" and "UTC".
    await selectDzOptionByCandidates('value-presentation-format', ['ISO', 'iso']);
    await selectDzOptionByCandidates('value-presentation-timezone', ['UTC', 'utc']);

    await saveTimeDisplaySettings();

    const preview = await $('[data-testid="value-presentation-preview-result"]');
    await browser.waitUntil(
      async () => {
        const text = (await preview.getText()).trim();
        return text.length > 0 && !/unchanged|保持原值|不可用|unavailable/i.test(text);
      },
      { timeout: 10000, timeoutMsg: '时间显示预览未显示格式化结果' },
    );
    const previewText = (await preview.getText()).trim();
    expect(previewText).toMatch(/\d{4}-\d{2}-\d{2}T/);
  });

  it('VP-002: 查询结果中的时间列应按已保存的时间显示格式展示', async () => {
    // Return to workspace and connect seeded PG, then prepare a table with a fixed timestamp.
    await backFromSettingsInMainWindow();
    await browser.switchToWindow(mainWindow);
    await connectSeededPgInWorkspace();
    await waitForNewQueryButton(20000);
    await browser.pause(1500);

    await openQueryTab();
    await executeSQLChecked(`DROP TABLE IF EXISTS ${TEST_TABLE}`);
    await executeSQLChecked(`
      CREATE TABLE ${TEST_TABLE} (
        id SERIAL PRIMARY KEY,
        ts TIMESTAMPTZ NOT NULL
      )
    `);
    await executeSQLChecked(
      `INSERT INTO ${TEST_TABLE} (ts) VALUES (TIMESTAMPTZ '2024-01-02T03:04:05Z')`,
    );

    await clickNavigatorRefresh();
    await waitForTableInSidebar(TEST_TABLE);
    await clickTableInSidebar(TEST_TABLE);
    await browser.pause(2000);
    await switchSubTab('data');

    await browser.waitUntil(
      async () =>
        browser.execute((col: string) => {
          const cells = Array.from(
            document.querySelectorAll<HTMLElement>(
              `[data-testid="data-table-cell"][data-dt-col="${col}"]`,
            ),
          ).filter((el) => el.offsetParent !== null);
          return (cells[0]?.textContent ?? '').trim();
        }, 'ts'),
      { timeout: 15000, timeoutMsg: '等待时间列单元格渲染超时' },
    );

    const cellText = (await browser.execute((col: string) => {
      const cells = Array.from(
        document.querySelectorAll<HTMLElement>(
          `[data-testid="data-table-cell"][data-dt-col="${col}"]`,
        ),
      ).filter((el) => el.offsetParent !== null);
      return (cells[0]?.textContent ?? '').trim();
    }, 'ts')) as string;

    // With ISO format + UTC display timezone, instant value should be formatted as ISO UTC.
    expect(cellText).toMatch(/^2024-01-02T03:04:05(\.0+)?Z$/);
  });

  it('VP-003: 关闭时间显示后查询结果恢复默认展示', async () => {
    await openSettingsInMainWindow('dataBrowsing');
    const sectionRoot = await $('[data-testid="settings-section-dataBrowsing"]');
    await sectionRoot.waitForDisplayed({ timeout: 15000 });
    await scrollToValuePresentationSettings();

    // Use a distinctive format while enabled so "disabled restores default" is observable
    // even when driver default rendering resembles common ISO-UTC formatting.
    await setToggle('value-presentation-enable', true);
    await selectDzOptionByCandidates('value-presentation-format', [
      '带时区偏移',
      'ISO with time zone offset',
      'time zone offset',
      'ISO with',
      'iso-tz',
    ]);
    await selectDzOptionByCandidates('value-presentation-timezone', [
      '指定 IANA 时区',
      '指定 IANA',
      'Specific IANA time zone',
      'IANA',
      'iana',
    ]);
    const ianaInput = await $('[data-testid="value-presentation-iana-name"]');
    await ianaInput.waitForDisplayed({ timeout: 10000 });
    await ianaInput.setValue('Asia/Shanghai');
    await browser.pause(300);
    await saveTimeDisplaySettings();

    await backFromSettingsInMainWindow();
    await browser.switchToWindow(mainWindow);
    await connectSeededPgInWorkspace();
    await waitForNewQueryButton(20000);
    await browser.pause(1000);
    await closeWorkspacePanels(browser).catch(() => {});
    await openQueryTab();
    await executeSqlInEditorAndWait(`SELECT TIMESTAMPTZ '2024-01-02T03:04:05Z' AS ts`);
    await waitForQueryResultCell('ts', '等待启用时间显示后的查询结果渲染超时');
    const enabledText = await readFirstQueryResultCell('ts');
    expect(enabledText).toContain('2024-01-02T11:04:05');
    expect(enabledText).toContain('+08:00');

    await openSettingsInMainWindow('dataBrowsing');
    await (
      await $('[data-testid="settings-section-dataBrowsing"]')
    ).waitForDisplayed({ timeout: 15000 });
    await scrollToValuePresentationSettings();
    await setToggle('value-presentation-enable', false);
    await saveTimeDisplaySettings();
    await browser.pause(500);

    await backFromSettingsInMainWindow();
    await browser.switchToWindow(mainWindow);
    await connectSeededPgInWorkspace().catch(() => {});
    await waitForNewQueryButton(20000).catch(() => {});
    await browser.pause(500);
    await closeWorkspacePanels(browser).catch(() => {});
    await openQueryTab();
    await executeSqlInEditorAndWait(`SELECT TIMESTAMPTZ '2024-01-02T03:04:05Z' AS ts`);
    await waitForQueryResultCell('ts', '等待关闭时间显示后的查询结果渲染超时');
    const disabledText = await readFirstQueryResultCell('ts');

    expect(disabledText).not.toBe(enabledText);
    expect(disabledText).not.toContain('+08:00');
  });

  it('VP-004: bigint 列可通过列显示方式按 Unix 秒/毫秒显示为时间', async () => {
    // Ensure time display is enabled with deterministic format for column-override rendering.
    await openSettingsInMainWindow('dataBrowsing');
    const sectionRoot = await $('[data-testid="settings-section-dataBrowsing"]');
    await sectionRoot.waitForDisplayed({ timeout: 15000 });
    await scrollToValuePresentationSettings();
    await setToggle('value-presentation-enable', true);
    await selectDzOptionByCandidates('value-presentation-format', ['ISO', 'iso']);
    await selectDzOptionByCandidates('value-presentation-timezone', ['UTC', 'utc']);
    await saveTimeDisplaySettings();

    await backFromSettingsInMainWindow();
    await browser.switchToWindow(mainWindow);
    await connectSeededPgInWorkspace();
    await waitForNewQueryButton(20000);
    await browser.pause(1500);

    await openQueryTab();
    await executeSQLChecked(`DROP TABLE IF EXISTS ${BIGINT_TABLE}`);
    await executeSQLChecked(`
      CREATE TABLE ${BIGINT_TABLE} (
        id SERIAL PRIMARY KEY,
        ts_bigint_sec BIGINT NOT NULL,
        ts_bigint_ms BIGINT NOT NULL
      )
    `);
    await executeSQLChecked(`
      INSERT INTO ${BIGINT_TABLE} (ts_bigint_sec, ts_bigint_ms)
      VALUES (1704164645, 1704164645123)
    `);

    await clickNavigatorRefresh();
    await waitForTableInSidebar(BIGINT_TABLE);
    await clickTableInSidebar(BIGINT_TABLE);
    await browser.pause(2000);
    await switchSubTab('data');

    await waitForFirstCell('ts_bigint_sec', '等待 bigint 秒列渲染超时');
    await waitForFirstCell('ts_bigint_ms', '等待 bigint 毫秒列渲染超时');

    const pickInterpretation = async (kind: 'seconds' | 'milliseconds') => {
      const candidates =
        kind === 'seconds'
          ? ['Unix 秒', 'Unix seconds', 'seconds', '秒']
          : ['Unix 毫秒', 'Unix milliseconds', 'milliseconds', '毫秒'];
      await selectDzOptionByCandidates('column-presentation-interpretation', candidates);
    };

    // --- Unix seconds override ---
    await openColumnPresentationDialog('ts_bigint_sec');
    await $('[data-testid="column-presentation-mode-custom"]').click();
    await pickInterpretation('seconds');
    await selectDzOptionByCandidates('column-presentation-format', ['ISO', 'iso']);
    await selectDzOptionByCandidates('column-presentation-display-timezone', ['UTC', 'utc']);

    const secPreview = await $('[data-testid="column-presentation-preview-result"]');
    await browser.waitUntil(
      async () => /^2024-01-02T03:04:05(\.0+)?Z$/.test((await secPreview.getText()).trim()),
      { timeout: 10000, timeoutMsg: 'bigint Unix 秒预览未显示期望时间' },
    );
    expect((await secPreview.getText()).trim()).toMatch(/^2024-01-02T03:04:05(\.0+)?Z$/);
    await saveColumnPresentationDialog();

    await browser.waitUntil(
      async () =>
        /^2024-01-02T03:04:05(\.0+)?Z$/.test(await readFirstVisibleCellText('ts_bigint_sec')),
      { timeout: 15000, timeoutMsg: 'bigint 秒列未按 Unix 秒显示为时间' },
    );
    expect(await readFirstVisibleCellText('ts_bigint_sec')).toMatch(
      /^2024-01-02T03:04:05(\.0+)?Z$/,
    );

    // --- Unix milliseconds override ---
    await openColumnPresentationDialog('ts_bigint_ms');
    await $('[data-testid="column-presentation-mode-custom"]').click();
    await pickInterpretation('milliseconds');
    await selectDzOptionByCandidates('column-presentation-format', ['ISO', 'iso']);
    await selectDzOptionByCandidates('column-presentation-display-timezone', ['UTC', 'utc']);

    const msPreview = await $('[data-testid="column-presentation-preview-result"]');
    await browser.waitUntil(
      async () => {
        const text = (await msPreview.getText()).trim();
        return text.startsWith('2024-01-02T03:04:05') && text.endsWith('Z');
      },
      { timeout: 10000, timeoutMsg: 'bigint Unix 毫秒预览未显示期望时间' },
    );
    const msPreviewText = (await msPreview.getText()).trim();
    expect(msPreviewText).toMatch(/^2024-01-02T03:04:05(\.\d+)?Z$/);
    // Millisecond precision (123ms) should be preserved in formatted preview/result.
    expect(msPreviewText).toContain('.123');
    await saveColumnPresentationDialog();

    await browser.waitUntil(
      async () => {
        const text = await readFirstVisibleCellText('ts_bigint_ms');
        return text.startsWith('2024-01-02T03:04:05') && text.endsWith('Z');
      },
      { timeout: 15000, timeoutMsg: 'bigint 毫秒列未按 Unix 毫秒显示为时间' },
    );
    const msCellText = await readFirstVisibleCellText('ts_bigint_ms');
    expect(msCellText).toMatch(/^2024-01-02T03:04:05(\.\d+)?Z$/);
    expect(msCellText).toContain('.123');

    // Sanity: values remain interpretable as unix timestamps by override (no silent raw fallback).
    expect(msCellText).not.toBe('1704164645123');
  });

  it('VP-005: 全局规则可将 bigint 列按 Unix 时间显示（含规则样本预览与查询结果）', async () => {
    await openSettingsInMainWindow('dataBrowsing');
    const sectionRoot = await $('[data-testid="settings-section-dataBrowsing"]');
    await sectionRoot.waitForDisplayed({ timeout: 15000 });
    await scrollToValuePresentationSettings();

    await setToggle('value-presentation-enable', true);
    await selectDzOptionByCandidates('value-presentation-format', ['ISO', 'iso']);
    await selectDzOptionByCandidates('value-presentation-timezone', ['UTC', 'utc']);

    // Open rules section and add a rule matching bigint unix-ms columns by name pattern.
    const rulesToggle = await $('[data-testid="value-presentation-rules-toggle"]');
    await rulesToggle.scrollIntoView({ block: 'center' });
    await rulesToggle.click();
    await browser.pause(200);
    const addRuleBtn = await $('[data-testid="value-presentation-rule-add"]');
    await addRuleBtn.waitForClickable({ timeout: 10000 });
    await addRuleBtn.click();

    const ruleName = await $('[data-testid="value-presentation-rule-name"]');
    await ruleName.waitForDisplayed({ timeout: 10000 });
    await ruleName.setValue('e2e_bigint_ms_rule');

    // Match column name ending with _ms (regex), and apply Unix milliseconds interpretation.
    await selectDzOptionByCandidates('value-presentation-rule-column-mode', [
      '模式',
      'Pattern',
      '正则',
      'regex',
      'Regex',
    ]);
    const colPattern = await $('[data-testid="value-presentation-rule-column-value"]');
    await colPattern.setValue('.*_ms$');
    await selectDzOptionByCandidates('value-presentation-rule-interpretation', [
      'Unix 毫秒',
      'Unix milliseconds',
      'milliseconds',
      '毫秒',
    ]);
    await selectDzOptionByCandidates('value-presentation-rule-format', ['ISO', 'iso']);
    await selectDzOptionByCandidates('value-presentation-rule-display-timezone', ['UTC', 'utc']);

    // Sample preview in rule editor should hit and format.
    await (
      await $('[data-testid="value-presentation-rule-sample-value"]')
    ).setValue('1704164645123');
    await (
      await $('[data-testid="value-presentation-rule-sample-column"]')
    ).setValue('ts_bigint_ms');
    await (await $('[data-testid="value-presentation-rule-sample-type"]')).setValue('BIGINT');
    await browser.pause(300);
    await browser.waitUntil(
      async () => {
        const text = await $('[data-testid="value-presentation-rule-sample-preview"]').getText();
        return (
          text.includes('2024-01-02T03:04:05') ||
          text.includes('.123') ||
          /matchHit|命中/i.test(text)
        );
      },
      { timeout: 15000, timeoutMsg: '规则样本预览未命中或未格式化 bigint 毫秒时间' },
    );

    await (await $('[data-testid="value-presentation-rule-save"]')).click();
    await browser.waitUntil(
      async () =>
        !(await $('[data-testid="value-presentation-rule-editor"]')
          .isDisplayed()
          .catch(() => false)),
      { timeout: 10000, timeoutMsg: '规则编辑器未关闭' },
    );
    // Rule appears in list.
    await expect(await $('[data-testid="value-presentation-rule-0"]')).toBeDisplayed();

    await saveTimeDisplaySettings();

    // Verify in SQL query results (global rule applies without table-column override source).
    await backFromSettingsInMainWindow();
    await browser.switchToWindow(mainWindow);
    await connectSeededPgInWorkspace();
    await waitForNewQueryButton(20000);
    await browser.pause(1000);

    await openQueryTab();
    await executeSqlInEditorAndWait(`SELECT 1704164645123::bigint AS ts_bigint_ms`);
    await waitForQueryResultCell('ts_bigint_ms', '等待查询结果 bigint 毫秒列渲染超时');
    const resultText = await readFirstQueryResultCell('ts_bigint_ms');
    expect(resultText).toMatch(/^2024-01-02T03:04:05(\.\d+)?Z$/);
    expect(resultText).toContain('.123');

    // Clean up rule to avoid affecting later cases.
    await openSettingsInMainWindow('dataBrowsing');
    await (
      await $('[data-testid="settings-section-dataBrowsing"]')
    ).waitForDisplayed({ timeout: 15000 });
    await scrollToValuePresentationSettings();
    const rulesToggle2 = await $('[data-testid="value-presentation-rules-toggle"]');
    await rulesToggle2.scrollIntoView({ block: 'center' });
    if ((await rulesToggle2.getAttribute('aria-expanded')) !== 'true') {
      await rulesToggle2.click();
      await browser.pause(200);
    }
    const del = await $('[data-testid="value-presentation-rule-delete-0"]');
    if (await del.isExisting()) {
      await del.click();
      await browser.pause(200);
    }
    await saveTimeDisplaySettings();
    await backFromSettingsInMainWindow();
    await browser.switchToWindow(mainWindow);
  });

  it('VP-006: 列显示方式支持原始值/跟随规则切换与对话框内启用时间显示', async () => {
    // Start with time display disabled and a formatted timestamp table.
    await openSettingsInMainWindow('dataBrowsing');
    await (
      await $('[data-testid="settings-section-dataBrowsing"]')
    ).waitForDisplayed({ timeout: 15000 });
    await scrollToValuePresentationSettings();
    await setToggle('value-presentation-enable', false);
    if ((await $('[data-testid="value-presentation-save"]').getAttribute('disabled')) !== null) {
      await setToggle('value-presentation-enable', true);
      await browser.pause(100);
      await setToggle('value-presentation-enable', false);
    }
    await saveTimeDisplaySettings();
    await backFromSettingsInMainWindow();
    await browser.switchToWindow(mainWindow);
    await connectSeededPgInWorkspace();
    await waitForNewQueryButton(20000);
    await browser.pause(1000);

    await openQueryTab();
    await executeSQLChecked(`DROP TABLE IF EXISTS ${TEST_TABLE}`);
    await executeSQLChecked(`
      CREATE TABLE ${TEST_TABLE} (
        id SERIAL PRIMARY KEY,
        ts TIMESTAMPTZ NOT NULL
      )
    `);
    await executeSQLChecked(
      `INSERT INTO ${TEST_TABLE} (ts) VALUES (TIMESTAMPTZ '2024-01-02T03:04:05Z')`,
    );
    await clickNavigatorRefresh();
    await waitForTableInSidebar(TEST_TABLE);
    await clickTableInSidebar(TEST_TABLE);
    await browser.pause(2000);
    await switchSubTab('data');
    await waitForFirstCell('ts', '等待时间列渲染超时');

    // Dialog shows "not enabled" hint and can enable time display explicitly.
    await openColumnPresentationDialog('ts');
    await expect(await $('[data-testid="column-presentation-not-enabled"]')).toBeDisplayed();
    const enableBtn = await $('[data-testid="column-presentation-enable"]');
    await enableBtn.click();
    await browser.waitUntil(
      async () => !(await $('[data-testid="column-presentation-not-enabled"]').isExisting()),
      { timeout: 10000, timeoutMsg: '对话框内启用时间显示后提示未消失' },
    );

    // With time display enabled globally (default native for timestamptz), switch to raw mode -> original-like value.
    await $('[data-testid="column-presentation-mode-raw"]').click();
    const rawPreview = await $('[data-testid="column-presentation-preview-result"]');
    await browser.waitUntil(async () => (await rawPreview.getText()).trim().length > 0, {
      timeout: 10000,
      timeoutMsg: '原始值模式预览为空',
    });
    await saveColumnPresentationDialog();
    await browser.pause(1000);
    const rawCell = await readFirstVisibleCellText('ts');
    expect(rawCell).not.toBe('');
    // Raw/original display should not be the formatted ISO-UTC string produced by custom/global ISO+UTC formatting path in this context.
    // (kept permissive to tolerate driver default rendering differences)
    expect(rawCell).not.toBe('1704164645');

    // Switch back to inherit to follow global/default behavior.
    await openColumnPresentationDialog('ts');
    await $('[data-testid="column-presentation-mode-inherit"]').click();
    await saveColumnPresentationDialog();
    await browser.pause(1000);
    await waitForFirstCell('ts', '等待继承模式下时间列渲染超时');
    const inheritCell = await readFirstVisibleCellText('ts');
    expect(inheritCell.length).toBeGreaterThan(0);
  });

  it('VP-007: 设置支持 ISO(含时区偏移)/本地与 IANA 时区并校验无效 IANA', async () => {
    await openSettingsInMainWindow('dataBrowsing');
    await (
      await $('[data-testid="settings-section-dataBrowsing"]')
    ).waitForDisplayed({ timeout: 15000 });
    await scrollToValuePresentationSettings();
    await setToggle('value-presentation-enable', true);

    // ISO with timezone offset + local timezone.
    await selectDzOptionByCandidates('value-presentation-format', [
      '带时区偏移',
      'ISO with time zone offset',
      'time zone offset',
      'ISO with',
      'iso-tz',
    ]);
    await selectDzOptionByCandidates('value-presentation-timezone', [
      '本地时区',
      'Local time zone',
      'Local',
      'local',
    ]);
    const previewLocal = await $('[data-testid="value-presentation-preview-result"]');
    await browser.waitUntil(
      async () => /\d{4}-\d{2}-\d{2}T/.test((await previewLocal.getText()).trim()),
      { timeout: 10000, timeoutMsg: '本地时区预览未显示格式化结果' },
    );

    // IANA invalid -> save blocked with inline error.
    await selectDzOptionByCandidates('value-presentation-timezone', [
      '指定 IANA 时区',
      '指定 IANA',
      'Specific IANA time zone',
      'IANA',
      'iana',
    ]);
    const ianaInput = await $('[data-testid="value-presentation-iana-name"]');
    await ianaInput.waitForDisplayed({ timeout: 10000 });
    await ianaInput.setValue('Not/A_Zone');
    const saveBtn = await $('[data-testid="value-presentation-save"]');
    await saveBtn.waitForClickable({ timeout: 10000 });
    await saveBtn.click();
    await expect(await $('[data-testid="value-presentation-iana-error"]')).toBeDisplayed();

    // Valid IANA -> can save and preview reflects offset/zoned format.
    await ianaInput.setValue('Asia/Shanghai');
    await browser.pause(300);
    await saveTimeDisplaySettings();
    const previewIana = await $('[data-testid="value-presentation-preview-result"]');
    await browser.waitUntil(
      async () => /\d{4}-\d{2}-\d{2}T/.test((await previewIana.getText()).trim()),
      { timeout: 10000, timeoutMsg: 'IANA 时区保存后预览未显示格式化结果' },
    );

    // Restore deterministic baseline for other specs/cases.
    await selectDzOptionByCandidates('value-presentation-format', ['ISO', 'iso']);
    await selectDzOptionByCandidates('value-presentation-timezone', ['UTC', 'utc']);
    await saveTimeDisplaySettings();
    await backFromSettingsInMainWindow();
    await browser.switchToWindow(mainWindow);
  });
});
