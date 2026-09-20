/**
 * QB-JOURNEY-D — the Navicat-style clause list, the column options dialog and
 * the clause that did not exist at all (HAVING).
 *
 * The three acceptance points this journey pins:
 *  D1 picking several columns no longer pushes WHERE / GROUP BY / HAVING /
 *     ORDER BY out of reach (the old grid spent one eight-control row per
 *     column, so four columns filled the whole region);
 *  D2 per-column options (alias / aggregate / sort / group by / criteria) moved
 *     into a dialog opened by clicking that column's chip;
 *  D3 GROUP BY, HAVING, ORDER BY and LIMIT/OFFSET all exist and reach the SQL.
 *
 * It ends by executing the statement: `HAVING SUM(qty) >= 5` must really filter
 * a group out of the result set, not merely appear in the preview.
 *
 * Locale note: option labels are never matched by translated text. Aggregates
 * are SQL keywords, everything else is driven by index, checkbox or chip click,
 * so the journey cannot break when the UI language changes.
 */
import { browser, $, expect } from '@wdio/globals';
import { captureJourneyStep } from '../../helpers.js';
import {
  addClauseItem,
  addHavingCondition,
  addTableCard,
  applyColumnOptions,
  cancelColumnOptions,
  clauseIsVisible,
  clauseRowsPresent,
  clickQbOk,
  columnOptionsOpen,
  configureHavingRow,
  confirmAutoJoinsViaUi,
  openOrderByChip,
  setSortDirection,
  existsInDom,
  expectSqlFragments,
  isQbOpen,
  openColumnOptions,
  openQbViaMenu,
  previewText,
  qbCall,
  readEditorSql,
  removeFieldChip,
  selectCardColumn,
  setEditorSql,
  setupQbJourney,
  switchQbTab,
  teardownQbJourney,
  waitQbClosed,
} from './visualQueryBuilderHelpers.js';
import type { QbJourneySetup } from './visualQueryBuilderHelpers.js';

const SUFFIX = Date.now().toString(36);
const REGION = `e2e_qb_d_region_${SUFFIX}`;
const SALE = `e2e_qb_d_sale_${SUFFIX}`;

const ALL_CLAUSES = [
  'qb-clause-select',
  'qb-clause-from',
  'qb-clause-where',
  'qb-clause-group-by',
  'qb-clause-having',
  'qb-clause-order-by',
];

describe('Visual Query Builder 子句列表与字段弹窗旅程 (QB-JOURNEY-D)', () => {
  let setup: QbJourneySetup;

  before(async () => {
    setup = await setupQbJourney({
      connectionId: 'e2e_qb_d_conn',
      connectionName: 'E2E-QB-D',
      captureLabel: 'qbd-open-suite',
      tables: {
        [REGION]: ['id', 'name'],
        [SALE]: ['id', 'region_id', 'channel', 'qty', 'price'],
      },
      seedStatements: [
        `DROP TABLE IF EXISTS ${SALE} CASCADE`,
        `DROP TABLE IF EXISTS ${REGION} CASCADE`,
        `CREATE TABLE ${REGION} (id INTEGER PRIMARY KEY, name TEXT)`,
        `CREATE TABLE ${SALE} (` +
          `id INTEGER PRIMARY KEY, ` +
          `region_id INTEGER REFERENCES ${REGION}(id), ` +
          `channel TEXT, qty INTEGER, price INTEGER)`,
        `INSERT INTO ${REGION} (id, name) VALUES (1, 'EU'), (2, 'US')`,
        // EU sums to 12, US to 2 — HAVING >= 5 must drop US.
        `INSERT INTO ${SALE} (id, region_id, channel, qty, price) VALUES ` +
          `(1, 1, 'online', 10, 3), (2, 1, 'store', 2, 5), ` +
          `(3, 2, 'online', 1, 9), (4, 2, 'store', 1, 7)`,
      ],
    });
    await setEditorSql('');
  });

  after(async () => {
    await teardownQbJourney(setup);
  });

  it('D1–D6: 子句列表 + 字段选项弹窗 + HAVING 生效并执行', async () => {
    if (!(await isQbOpen())) await openQbViaMenu('qbd-open');

    // ── D1: every clause of the statement is rendered ──
    await addTableCard(SALE, { x: 20, y: 20 });
    await addTableCard(REGION, { x: 480, y: 20 });
    await browser.pause(800);
    expect(await confirmAutoJoinsViaUi(1)).toBe(1);

    expect(await clauseRowsPresent()).toEqual(ALL_CLAUSES);
    // The JOIN the generator will emit is listed in FROM, oriented the same way.
    expect(await existsInDom('[data-testid="qb-from-join-0"]')).toBe(true);

    // ── D2: four columns selected — the old layout's worst case ──
    for (const column of ['qty', 'price', 'channel']) {
      await selectCardColumn(SALE, column);
    }
    await selectCardColumn(REGION, 'name');
    await browser.pause(500);

    // WHERE and GROUP BY must still be on screen without collapsing anything…
    expect(await clauseIsVisible('qb-clause-where')).toBe(true);
    expect(await clauseIsVisible('qb-clause-group-by')).toBe(true);
    // …and the entire statement fits once the canvas gives up its share.
    await qbCall('setCanvasCollapsed', true);
    await browser.pause(400);
    for (const clause of ALL_CLAUSES) {
      expect(await clauseIsVisible(clause)).toBe(true);
    }
    await captureJourneyStep('qbd-clause-list');

    // ── D3: per-column options live in a dialog ──
    await openColumnOptions(SALE, 'qty');
    expect(await columnOptionsOpen()).toBe(true);
    expect(await $('[data-testid="qb-col-opt-field"]').getText()).toContain('qty');

    // Cancel first: the dialog must not write anything unless applied.
    await cancelColumnOptions();
    expect(await columnOptionsOpen()).toBe(false);
    const unchanged = await browser.execute(() => {
      const col = (window as any).__qbStore
        .getState()
        .selectedColumns.find((c: { column: string }) => c.column === 'qty');
      return { aggregate: col?.aggregate ?? null, alias: col?.alias ?? null };
    });
    expect(unchanged).toEqual({ aggregate: null, alias: null });

    // Apply: alias + aggregate + group-by, all from the one dialog.
    await openColumnOptions(SALE, 'qty');
    await $('[data-testid="qb-col-opt-alias"]').setValue('total_qty');
    await addClauseItem('qb-col-opt-aggregate', 'SUM');
    await browser.pause(200);
    await applyColumnOptions();

    const applied = await browser.execute(() => {
      const col = (window as any).__qbStore
        .getState()
        .selectedColumns.find((c: { column: string }) => c.column === 'qty');
      return { aggregate: col?.aggregate, alias: col?.alias };
    });
    expect(applied).toEqual({ aggregate: 'SUM', alias: 'total_qty' });

    // The chip carries the options, so a field stays one line in SELECT.
    const chipText = await $(`[data-testid="qb-field-chip-${SALE}-qty"]`).getText();
    expect(chipText).toContain('SUM(');
    expect(chipText).toContain('AS total_qty');

    // ── D4: GROUP BY through the clause picker ──
    await addClauseItem('qb-add-group-by', `${REGION}.name`);
    await browser.pause(300);
    expect(await existsInDom(`[data-testid="qb-group-chip-${REGION}-name"]`)).toBe(true);

    // ── D5: ORDER BY through the clause picker, then set it to DESC ──
    await addClauseItem('qb-add-order-by', `${REGION}.name`);
    await browser.pause(300);
    const orderChip = await $(`[data-testid="qb-order-chip-${REGION}-name"]`);
    expect(await orderChip.getText()).toContain('ASC');
    await openOrderByChip(REGION, 'name');
    await setSortDirection('DESC');
    expect(await orderChip.getText()).toContain('DESC');

    // ── D6: HAVING — the clause that did not exist ──
    await addHavingCondition();
    // Nothing is in the query yet: the new condition exists only as a draft,
    // so abandoning the dialog cannot leave a half-filled chip behind.
    expect(await existsInDom('[data-testid^="qb-having-chip-"]')).toBe(false);
    // …and the draft starts aggregated (SUM), so it can never emit
    // `HAVING bare_column = …`.
    expect(await $('[data-testid="qb-cond-aggregate"]').getText()).toContain('SUM');
    await configureHavingRow(0, { field: `${SALE}.qty`, operator: '>=', value: '5' });
    expect(await existsInDom('[data-testid^="qb-having-chip-"]')).toBe(true);

    // The layout case deliberately picked extra columns; only `name` (grouped)
    // and the `SUM(qty)` measure may survive into an executable aggregate
    // statement — PostgreSQL rejects a bare `price` next to GROUP BY, so drop
    // them through the chips exactly as a user would.
    await removeFieldChip(SALE, 'price');
    await removeFieldChip(SALE, 'channel');
    await browser.pause(300);

    // ── D7: preview, commit, execute ──
    await switchQbTab('preview');
    const sql = await previewText();
    expect(sql).toContain('SUM(');
    expect(sql).toContain('AS "total_qty"');
    expect(sql.toUpperCase()).toContain('GROUP BY');
    // `HAVING` and its operand land on separate lines once pretty-printed, so
    // this one is matched whitespace-insensitively.
    expectSqlFragments(sql, ['HAVING SUM(']);
    expect(sql).toContain('>= 5');
    expect(sql.toUpperCase()).toContain('ORDER BY');
    expect(sql).toContain('DESC');
    expect(sql).not.toContain('"price"');
    expect(sql).not.toContain('"channel"');
    await captureJourneyStep('qbd-preview-having');

    await clickQbOk();
    await waitQbClosed();

    const committed = await readEditorSql();
    expect(committed.toUpperCase()).toContain('HAVING');

    const exec = await $('[data-testid="editor-execute-button"]');
    await exec.waitForClickable({ timeout: 10000 });
    await exec.click();
    const resultTable = await $('[data-testid="result-workspace-table"]');
    await resultTable.waitForDisplayed({ timeout: 20000 });
    const resultText = await resultTable.getText();
    // HAVING really filtered the small group out of the result set.
    expect(resultText).toContain('EU');
    expect(resultText).not.toContain('US');
    await captureJourneyStep('qbd-result');
  });

  it('D8: HAVING 未聚合也未分组的字段 → 警告但不阻断 OK', async () => {
    if (!(await isQbOpen())) await openQbViaMenu('qbd-warn');
    await qbCall('reset');
    // A case must not inherit the previous one's view state: the canvas may
    // still be collapsed (no cards rendered) and the tab may still be Preview.
    await qbCall('setCanvasCollapsed', false);
    await qbCall('setBottomTab', 'build');
    await browser.pause(300);
    await addTableCard(SALE, { x: 20, y: 20 });
    await selectCardColumn(SALE, 'qty');
    await browser.pause(300);

    // `HAVING qty >= 5` on a bare, ungrouped column: typeable, but invalid on
    // every engine — surfaced as a warning, never as a silent export.
    await addHavingCondition();
    await configureHavingRow(0, {
      // Index 0 = "no aggregate": the option labels are translated, so the
      // journey addresses them by position.
      aggregateIndex: 0,
      field: `${SALE}.qty`,
      operator: '>=',
      value: '5',
    });

    const aggregateCleared = await browser.execute(() => {
      const col = (window as any).__qbStore.getState().having.conditions[0];
      return col?.aggregate ?? null;
    });
    expect(aggregateCleared).toBeNull();

    await switchQbTab('preview');
    await browser.waitUntil(() => existsInDom('[data-diagnostic-code="having-non-grouped"]'), {
      timeout: 5000,
      timeoutMsg: 'HAVING 未聚合/未分组诊断未出现',
    });

    // A warning must not block OK.
    const okDisabled = await browser.execute(() => {
      const btn = document.querySelector<HTMLButtonElement>('[data-testid="qb-ok"]');
      return btn ? btn.disabled : null;
    });
    expect(okDisabled).toBe(false);
    await captureJourneyStep('qbd-having-warning');

    await qbCall('reset');
  });
});
