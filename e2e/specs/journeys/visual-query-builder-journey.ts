/**
 * QB-JOURNEY-A — Visual Query Builder, normal journey.
 *
 * Full closed loop: open from the toolbar → add a table from the navigator →
 * pick columns on the card → swap to the Preview tab → collapse/resize →
 * add a WHERE condition → DISTINCT → **OK** writes the SQL back → execute and
 * see the filtered rows.
 *
 * Covers PRD §13.3.1 (A1–A16). See `visualQueryBuilderHelpers.ts` for the
 * driving conventions (panel chrome = real DOM; canvas = `__qbStore`).
 */
import { browser, $, expect } from '@wdio/globals';
import { captureJourneyStep } from '../../helpers.js';
import {
  addTableCard,
  addWhereCondition,
  clickQbClose,
  clickQbOk,
  confirmDiscardIfAsked,
  configureWhereRow,
  dragCard,
  dragNavigatorTableToCanvas,
  dragSplitter,
  existsInDom,
  heightOf,
  isQbOpen,
  openQbViaMenu,
  previewText,
  qbCall,
  qbRead,
  readEditorSql,
  removeTableCard,
  selectCardColumn,
  toggleAllColumns,
  setEditorSql,
  setupQbJourney,
  switchQbTab,
  teardownQbJourney,
  waitQbClosed,
  waitQbOpen,
} from './visualQueryBuilderHelpers.js';
import type { QbJourneySetup } from './visualQueryBuilderHelpers.js';

const TABLE = `e2e_qb_a_${Date.now().toString(36)}`;
const COLUMNS = ['id', 'name', 'category', 'score'];

/** Tall enough (16 rows) that its card must scroll internally — see A17. */
const WIDE = `e2e_qb_a_wide_${Date.now().toString(36)}`;
const WIDE_COLUMNS = Array.from({ length: 16 }, (_, i) => `col_${String(i + 1).padStart(2, '0')}`);

describe('Visual Query Builder 正常旅程 (QB-JOURNEY-A)', () => {
  let setup: QbJourneySetup;

  before(async () => {
    setup = await setupQbJourney({
      connectionId: 'e2e_qb_a_conn',
      connectionName: 'E2E-QB-A',
      captureLabel: 'qba-open-suite',
      tables: { [TABLE]: COLUMNS, [WIDE]: WIDE_COLUMNS },
      seedStatements: [
        `DROP TABLE IF EXISTS ${TABLE} CASCADE`,
        `CREATE TABLE ${TABLE} (id INTEGER, name TEXT, category TEXT, score INTEGER)`,
        `INSERT INTO ${TABLE} (id, name, category, score) VALUES ` +
          `(1, 'Alice', 'A', 90), (2, 'Bob', 'B', 80), ` +
          `(3, 'Charlie', 'A', 70), (4, 'Diana', 'B', 95)`,
        `DROP TABLE IF EXISTS ${WIDE} CASCADE`,
        `CREATE TABLE ${WIDE} (${WIDE_COLUMNS.map((c) => `${c} INTEGER`).join(', ')})`,
      ],
    });
    await setEditorSql('');
  });

  after(async () => {
    await teardownQbJourney(setup);
  });

  it('A1–A16: 完整正常旅程', async () => {
    // ── A1: open the builder from the toolbar ──
    await openQbViaMenu('qba-open');

    const panelId = await browser.execute(
      () =>
        document.querySelector('[data-testid="qb-panel"]')?.getAttribute('data-qb-panel-id') ?? '',
    );
    expect(panelId.length).toBeGreaterThan(0);

    // The host navigator is reused — the builder brings no tree of its own.
    await expect(await $('[data-testid="connection-navigator-aside"]')).toBeDisplayed();
    expect(await existsInDom('[data-testid="object-tree-panel"]')).toBe(false);

    // ── A2: the builder replaces the editor area ──
    const [panelHeight, queryHeight] = await Promise.all([
      heightOf('qb-panel'),
      heightOf('query-panel'),
    ]);
    expect(panelHeight).toBeGreaterThan(queryHeight * 0.6);

    // The editor stays mounted (hidden) so its undo stack survives.
    expect(await existsInDom('[data-testid="query-editor-host"]')).toBe(true);
    expect(
      await $('[data-testid="query-editor-host"]')
        .isDisplayed()
        .catch(() => false),
    ).toBe(false);

    // ── A3: defaults to the Build tab, preview not mounted ──
    expect(await existsInDom('[data-testid="qb-statement"]')).toBe(true);
    expect(await existsInDom('[data-testid="qb-sql-preview"]')).toBe(false);

    // ── A4: drag the table in from the navigator ──
    expect(await dragNavigatorTableToCanvas(TABLE)).toBe(true);
    await browser.waitUntil(() => existsInDom(`[data-testid="qb-drag-${TABLE}"]`), {
      timeout: 5000,
      timeoutMsg: '拖动表到画布后未生成卡片',
    });
    // The table arrives with a default alias, so the generated SQL is
    // qualified from the very first column.
    const alias = await browser.execute(
      (t: string) => (window as any).__qbStore.getState().tableAliases[t] ?? '',
      TABLE,
    );
    expect(alias.length).toBeGreaterThan(0);
    await captureJourneyStep('qba-table-from-navigator');

    // ── A4b: the card can be repositioned by dragging ──
    const posBefore = (await qbRead(['tablePositions'])).tablePositions as Record<
      string,
      { x: number; y: number }
    >;
    await dragCard(TABLE, { x: 96, y: 48 });
    const posAfter = (await qbRead(['tablePositions'])).tablePositions as Record<
      string,
      { x: number; y: number }
    >;
    expect(posAfter[TABLE]).not.toEqual(posBefore[TABLE]);
    // Positions stay on the card grid so hand-placed cards line up.
    expect(posAfter[TABLE]!.x % 24).toBe(0);
    expect(posAfter[TABLE]!.y % 24).toBe(0);

    // ── A5: pick two columns on the card ──
    await selectCardColumn(TABLE, 'name');
    await selectCardColumn(TABLE, 'score');
    await browser.pause(300);

    // ── A5b: select every column at once, then narrow back to two ──
    await toggleAllColumns(TABLE);
    let selectedCount = await browser.execute(
      () => ((window as any).__qbStore.getState().selectedColumns ?? []).length,
    );
    expect(selectedCount).toBe(COLUMNS.length);
    await toggleAllColumns(TABLE);
    selectedCount = await browser.execute(
      () => ((window as any).__qbStore.getState().selectedColumns ?? []).length,
    );
    expect(selectedCount).toBe(0);
    await selectCardColumn(TABLE, 'name');
    await selectCardColumn(TABLE, 'score');
    await browser.pause(300);

    // ── A6: switch to Preview ──
    await switchQbTab('preview');
    let sql = await previewText();
    expect(sql).toContain('SELECT');
    expect(sql).toContain(TABLE);
    expect(sql).toContain('name');
    expect(sql).toContain('score');
    // The default alias is what the column references are qualified with.
    expect(sql).toContain(`"${alias}"`);
    // Mutually exclusive tabs: the clause list is unmounted while Preview is active.
    expect(await existsInDom('[data-testid="qb-statement"]')).toBe(false);
    await captureJourneyStep('qba-preview');

    // ── A7: drag the splitter up → the bottom region grows ──
    const bottomBefore = await heightOf('qb-bottom-region');
    await dragSplitter(-70);
    const bottomAfter = await heightOf('qb-bottom-region');
    expect(bottomAfter).toBeGreaterThan(bottomBefore);
    expect(bottomAfter).toBeGreaterThanOrEqual(139);

    // ── A8: Ctrl/Cmd+B collapses the canvas, then restores it ──
    const mod = process.platform === 'darwin' ? '\uE03D' : '\uE009'; // Cmd / Ctrl
    await browser.keys([mod, 'b']);
    await browser.pause(400);
    expect(await existsInDom('[data-testid="qb-diagram-canvas"]')).toBe(false);
    const collapsedHeight = await heightOf('qb-bottom-region');
    expect(collapsedHeight).toBeGreaterThan(bottomAfter);

    await browser.keys([mod, 'b']);
    await browser.pause(400);
    expect(await existsInDom('[data-testid="qb-diagram-canvas"]')).toBe(true);

    // ── A9/A10: build tab → add a condition row, set score >= 80 ──
    await switchQbTab('build');
    await addWhereCondition();
    await configureWhereRow(0, { field: `${TABLE}.score`, operator: '>=', value: '80' });

    await switchQbTab('preview');
    sql = await previewText();
    expect(sql).toContain('WHERE');
    expect(sql).toContain('>= 80');
    await captureJourneyStep('qba-where');

    // ── A11: DISTINCT ──
    // The toggle belongs to the SELECT clause row, so it is only mounted on the
    // Build tab — the preview we assert on afterwards is the same state.
    await switchQbTab('build');
    await $('[data-testid="qb-distinct-checkbox"]').click();
    await browser.pause(300);
    await switchQbTab('preview');
    sql = await previewText();
    expect(sql.toUpperCase()).toContain('SELECT DISTINCT');

    // ── A12: OK writes the SQL back and focuses the editor ──
    // The editor is already empty here, and it is CSS-hidden while the builder
    // is up — so it must not be touched through the DOM at this point.
    await clickQbOk();
    await waitQbClosed();

    const toast = await $('[data-testid="qb-toast"]');
    await toast.waitForDisplayed({ timeout: 5000 });

    const editorSql = (await readEditorSql()).replace(/\s+/g, ' ').trim();
    expect(editorSql).toContain('SELECT');
    expect(editorSql).toContain(TABLE);
    expect(editorSql.toUpperCase()).toContain('DISTINCT');
    expect(editorSql).toContain('>= 80');
    expect(editorSql.toUpperCase()).toContain('WHERE');
    await captureJourneyStep('qba-committed');

    // ── A13: execute and verify the filter really applied ──
    const exec = await $('[data-testid="editor-execute-button"]');
    await exec.waitForClickable({ timeout: 10000 });
    await exec.click();

    const resultTable = await $('[data-testid="result-workspace-table"]');
    await resultTable.waitForDisplayed({ timeout: 20000 });
    const resultText = await resultTable.getText();
    expect(resultText).toContain('Alice');
    expect(resultText).toContain('Diana');
    expect(resultText).not.toContain('Charlie');
    await captureJourneyStep('qba-result');

    // ── A14: reopening keeps the canvas (OK does not clear it) ──
    await openQbViaMenu('qba-reopen');
    expect(await existsInDom(`[data-testid="qb-drag-${TABLE}"]`)).toBe(true);
    // Tab memory (PRD F-07.1): we committed from Preview, so it reopens there
    // with the same SQL still generated.
    expect(await existsInDom('[data-testid="qb-sql-preview"]')).toBe(true);
    expect(await previewText()).toContain(TABLE);

    // ── A15: Reset clears the canvas but keeps the panel open ──
    await $('[data-testid="qb-reset"]').click();
    await browser.pause(400);
    expect(await existsInDom(`[data-testid="qb-drag-${TABLE}"]`)).toBe(false);
    await waitQbOpen(3000);

    // ── A16: closing with × leaves the editor content untouched ──
    // A15's Reset is itself a canvas change, so × must ask before discarding.
    const beforeClose = (await readEditorSql()).replace(/\s+/g, ' ').trim();
    await clickQbClose();
    expect(await confirmDiscardIfAsked()).toBe(true);
    await waitQbClosed();
    const afterClose = (await readEditorSql()).replace(/\s+/g, ' ').trim();
    expect(afterClose).toBe(beforeClose);
    await captureJourneyStep('qba-closed');
  });

  /**
   * A17–A20 — the three shape regressions that a "it renders something" test
   * would miss, each one measured on the real rendered geometry:
   *   A17 a card is a FIXED height object that scrolls its column list
   *       internally (Navicat behaviour) instead of growing with the table;
   *   A18 that height does not change when the list is scrolled;
   *   A19 the preview is pretty-printed, not the generator's one long line;
   *   A20 the preview is syntax-highlighted and fills its tab region.
   */
  it('A17–A20: 卡片固定高度内滚 + Preview 高亮/格式化/占满高度', async () => {
    if (!(await isQbOpen())) await openQbViaMenu('qba-shape');
    await qbCall('reset');
    await addTableCard(WIDE, { x: 20, y: 20 });
    await addTableCard(TABLE, { x: 500, y: 20 });
    await browser.pause(500);

    // ── A17: the 16-column card is capped and scrolls internally ──
    const metrics = await browser.execute(
      (wide: string, small: string) => {
        const card = (t: string) =>
          document.querySelector<HTMLElement>(`[data-testid="qb-drag-${t}"]`);
        const list = (t: string) =>
          document.querySelector<HTMLElement>(`[data-testid="qb-col-list-${t}"]`);
        const wl = list(wide);
        const sl = list(small);
        return {
          wideHeight: card(wide)?.getBoundingClientRect().height ?? 0,
          smallHeight: card(small)?.getBoundingClientRect().height ?? 0,
          wideListClientHeight: wl?.clientHeight ?? 0,
          wideListScrollHeight: wl?.scrollHeight ?? 0,
          smallScrolls: sl ? sl.scrollHeight > sl.clientHeight + 1 : true,
        };
      },
      WIDE,
      TABLE,
    );
    // 16 rows × 24px + padding ≈ 392px of content …
    expect(metrics.wideListScrollHeight).toBeGreaterThan(metrics.wideListClientHeight + 100);
    // … inside a card capped at header + 200px (≈237px), not 430px.
    expect(metrics.wideHeight).toBeLessThan(260);
    expect(metrics.smallHeight).toBeGreaterThan(0);
    expect(metrics.smallHeight).toBeLessThan(metrics.wideHeight);
    // A short table has nothing to scroll — its list must not fake a scrollbar.
    expect(metrics.smallScrolls).toBe(false);
    await captureJourneyStep('qba-fixed-height-card');

    // ── A18: scrolling the list does not resize the card ──
    const scrolled = await browser.execute((t: string) => {
      const list = document.querySelector<HTMLElement>(`[data-testid="qb-col-list-${t}"]`);
      if (!list) return null;
      list.scrollTop = 120;
      const card = document.querySelector<HTMLElement>(`[data-testid="qb-drag-${t}"]`);
      return { scrollTop: list.scrollTop, height: card?.getBoundingClientRect().height ?? 0 };
    }, WIDE);
    expect(scrolled).not.toBeNull();
    expect(scrolled!.scrollTop).toBeGreaterThan(0);
    expect(scrolled!.height).toBe(metrics.wideHeight);

    // ── A19–A20: preview formatting, highlighting and height ──
    await removeTableCard(TABLE);
    await toggleAllColumns(WIDE);
    await switchQbTab('preview');
    await browser.pause(300);

    const sql = await previewText();
    // The generator's output is a single line; the preview must be formatted.
    expect(sql).toContain('\n');
    expect(sql.toUpperCase()).toContain('SELECT');

    const preview = await browser.execute(() => {
      const pre = document.querySelector<HTMLElement>('[data-testid="qb-sql-preview"]');
      const tab = document.querySelector<HTMLElement>('[data-testid="qb-tab-content"]');
      if (!pre || !tab) return null;
      return {
        spans: pre.querySelectorAll('span').length,
        keywords: pre.querySelectorAll('span[class*="text-accent"]').length,
        height: pre.getBoundingClientRect().height,
        tabHeight: tab.getBoundingClientRect().height,
      };
    });
    expect(preview).not.toBeNull();
    // Highlighting is real token spans, and keywords take the accent colour.
    expect(preview!.spans).toBeGreaterThan(5);
    expect(preview!.keywords).toBeGreaterThan(0);
    // The preview fills the region — leaving room only for its own header row.
    expect(preview!.height).toBeGreaterThan(120);
    expect(preview!.height).toBeGreaterThan(preview!.tabHeight - 70);
    await captureJourneyStep('qba-preview-shape');

    // Leave the journey clean for the runner's teardown.
    await qbCall('reset');
  });
});
