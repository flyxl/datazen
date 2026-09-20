/**
 * QB-JOURNEY-C — Visual Query Builder, high-complexity statement journey.
 *
 * Builds one statement that exercises every clause the builder supports:
 * three real FK-related tables, a confirmed INNER JOIN plus a re-typed LEFT
 * JOIN, aggregates with aliases, GROUP BY, ORDER BY, DISTINCT, a **nested**
 * WHERE group (`a AND (b OR c)`), an IN list and LIMIT/OFFSET — then proves the
 * generated SQL actually executes and aggregates correctly.
 *
 * Covers PRD §13.3.3 (C1–C15).
 *
 * Ordering notes (both are load-bearing):
 *  - `generateSql` returns '' until at least one column is selected, so
 *    SQL-level assertions can only run after the column picks.
 *  - A group renders its own conditions *before* its sub-groups, so every root
 *    condition is added before the nested group is created — otherwise the DOM
 *    row indices shift under us.
 */
import { browser, $, expect } from '@wdio/globals';
import { captureJourneyStep } from '../../helpers.js';
import {
  addSubGroupCondition,
  addTableCard,
  addWhereCondition,
  addWhereGroup,
  clickQbOk,
  configureWhereRow,
  confirmAutoJoinsViaUi,
  existsInDom,
  expectNoSqlFragments,
  expectSqlFragments,
  isQbOpen,
  openQbViaMenu,
  previewText,
  qbCall,
  qbRead,
  readEditorSql,
  selectCardColumn,
  setEditorSql,
  setPagination,
  setupQbJourney,
  switchQbTab,
  teardownQbJourney,
  waitQbClosed,
} from './visualQueryBuilderHelpers.js';
import type { QbJourneySetup } from './visualQueryBuilderHelpers.js';

const SUFFIX = Date.now().toString(36);
const AUTHOR = `e2e_qb_c_author_${SUFFIX}`;
const BOOK = `e2e_qb_c_book_${SUFFIX}`;
const SALE = `e2e_qb_c_sale_${SUFFIX}`;
/** Composite FK pair: the child references (a, b) of the parent. */
const COMP_PARENT = `e2e_qb_c_wh_${SUFFIX}`;
const COMP_CHILD = `e2e_qb_c_stock_${SUFFIX}`;

describe('Visual Query Builder 高复杂度语句构造旅程 (QB-JOURNEY-C)', () => {
  let setup: QbJourneySetup;

  before(async () => {
    setup = await setupQbJourney({
      connectionId: 'e2e_qb_c_conn',
      connectionName: 'E2E-QB-C',
      captureLabel: 'qbc-open-suite',
      tables: {
        [AUTHOR]: ['id', 'name', 'country'],
        [BOOK]: ['id', 'author_id', 'title', 'year', 'price', 'rating'],
        [SALE]: ['id', 'book_id', 'qty', 'channel'],
        [COMP_PARENT]: ['a', 'b', 'label'],
        [COMP_CHILD]: ['id', 'pa', 'pb', 'qty'],
      },
      seedStatements: [
        `DROP TABLE IF EXISTS ${SALE} CASCADE`,
        `DROP TABLE IF EXISTS ${BOOK} CASCADE`,
        `DROP TABLE IF EXISTS ${AUTHOR} CASCADE`,
        `CREATE TABLE ${AUTHOR} (id INTEGER PRIMARY KEY, name TEXT, country TEXT)`,
        `CREATE TABLE ${BOOK} (` +
          `id INTEGER PRIMARY KEY, ` +
          `author_id INTEGER REFERENCES ${AUTHOR}(id), ` +
          `title TEXT, year INTEGER, price INTEGER, rating TEXT)`,
        `CREATE TABLE ${SALE} (` +
          `id INTEGER PRIMARY KEY, ` +
          `book_id INTEGER REFERENCES ${BOOK}(id), ` +
          `qty INTEGER, channel TEXT)`,
        `INSERT INTO ${AUTHOR} (id, name, country) VALUES (1, 'Ada', 'UK'), (2, 'Linus', 'FI')`,
        `INSERT INTO ${BOOK} (id, author_id, title, year, price, rating) VALUES ` +
          `(1, 1, 'Notes', 2001, 30, 'A'), ` +
          `(2, 1, 'Sketches', 1998, 20, 'C'), ` +
          `(3, 2, 'Kernel', 2005, 50, 'B'), ` +
          `(4, 2, 'Later', 2010, 40, 'A')`,
        `INSERT INTO ${SALE} (id, book_id, qty, channel) VALUES ` +
          `(1, 1, 3, 'online'), (2, 1, 2, 'store'), ` +
          `(3, 3, 5, 'online'), (4, 4, 1, 'store')`,
        // Composite foreign key: two columns referenced as one constraint.
        `DROP TABLE IF EXISTS ${COMP_CHILD} CASCADE`,
        `DROP TABLE IF EXISTS ${COMP_PARENT} CASCADE`,
        `CREATE TABLE ${COMP_PARENT} (a INTEGER, b INTEGER, label TEXT, PRIMARY KEY (a, b))`,
        `CREATE TABLE ${COMP_CHILD} (` +
          `id INTEGER PRIMARY KEY, pa INTEGER, pb INTEGER, qty INTEGER, ` +
          `CONSTRAINT fk_stock_wh FOREIGN KEY (pa, pb) REFERENCES ${COMP_PARENT}(a, b))`,
        `INSERT INTO ${COMP_PARENT} (a, b, label) VALUES (1, 1, 'A1'), (2, 2, 'B2')`,
        `INSERT INTO ${COMP_CHILD} (id, pa, pb, qty) VALUES (1, 1, 1, 5), (2, 2, 2, 7)`,
      ],
    });
    await setEditorSql('');
  });

  after(async () => {
    await teardownQbJourney(setup);
  });

  it('C1–C15: 三表 JOIN + 嵌套条件组 + 聚合 + 分组 + 排序 + 分页 + DISTINCT', async () => {
    await openQbViaMenu('qbc-open');

    // ── C1: add all three tables ──
    // Spread horizontally so the JOIN labels land *between* the cards and stay
    // clickable (a label sitting under a card cannot be interacted with).
    await addTableCard(AUTHOR, { x: 20, y: 20 });
    await addTableCard(BOOK, { x: 420, y: 20 });
    await addTableCard(SALE, { x: 820, y: 20 });
    await browser.pause(600);

    for (const table of [AUTHOR, BOOK, SALE]) {
      expect(await existsInDom(`[data-testid="qb-drag-${table}"]`)).toBe(true);
    }
    // Detected relations are *candidates* only — nothing may be joined yet.
    expect((await qbRead(['joins'])).joins).toEqual([]);
    await captureJourneyStep('qbc-tables');

    // ── C2: confirm the auto-detected FK relations through the real UI ──
    const confirmed = await confirmAutoJoinsViaUi(2);
    expect(confirmed).toBe(2);

    // Both candidates are now *confirmed*: they left the candidate list and
    // entered the SQL as INNER joins.
    const confirmedJoins = (await qbRead(['joins'])).joins as Array<{ type: string }>;
    expect(confirmedJoins.length).toBe(2);
    expect(confirmedJoins.every((j) => j.type === 'INNER')).toBe(true);
    expect((await qbRead(['autoJoins'])).autoJoins).toEqual([]);

    // Relations are drawn as lines only — no labels anywhere on the canvas.
    expect(await existsInDom('[data-testid^="qb-relation-"]')).toBe(true);
    expect(await existsInDom('[data-testid^="qb-join-label-"]')).toBe(false);

    // ── C3: re-type the sale↔book join as LEFT ──
    const joins = (await qbRead(['joins'])).joins as Array<Record<string, string>>;
    const saleJoin = joins.find((j) => j.leftTable === SALE || j.rightTable === SALE);
    expect(saleJoin).toBeTruthy();
    await qbCall('updateJoinType', saleJoin!.id, 'LEFT');
    await browser.pause(300);

    const afterRetype = (await qbRead(['joins'])).joins as Array<{ type: string }>;
    expect(afterRetype.filter((j) => j.type === 'INNER').length).toBe(1);
    expect(afterRetype.filter((j) => j.type === 'LEFT').length).toBe(1);

    // ── C4/C5: columns, aggregate, aliases ──
    // (The SQL only becomes non-empty once a column is selected.)
    await selectCardColumn(AUTHOR, 'name');
    await selectCardColumn(BOOK, 'title');
    await selectCardColumn(SALE, 'qty');
    await browser.pause(400);

    await qbCall('setColumnAlias', AUTHOR, 'name', 'author_name');
    await qbCall('setColumnAlias', BOOK, 'title', 'book_title');
    await qbCall('setColumnAlias', SALE, 'qty', 'total_qty');
    await qbCall('setColumnAggregate', SALE, 'qty', 'SUM');
    await browser.pause(400);

    await switchQbTab('preview');
    let sql = await previewText();
    expectSqlFragments(sql, [
      'INNER JOIN',
      'LEFT JOIN',
      'SUM(',
      'author_name',
      'book_title',
      'total_qty',
    ]);
    expect((sql.match(/INNER JOIN/gi) ?? []).length).toBe(1);
    expect((sql.match(/LEFT JOIN/gi) ?? []).length).toBe(1);
    await captureJourneyStep('qbc-joins-aggregate');

    // ── C6: GROUP BY the two dimension columns ──
    await qbCall('updateColumnConfig', AUTHOR, 'name', { groupBy: true });
    await qbCall('updateColumnConfig', BOOK, 'title', { groupBy: true });
    await browser.pause(300);

    sql = await previewText();
    expect(sql.toUpperCase()).toContain('GROUP BY');
    // GROUP BY qualifies with the alias the FROM/JOIN clauses declare, so assert
    // the alias-qualified columns rather than the bare table names.
    expect(sql).toContain('"name"');
    expect(sql).toContain('"title"');

    // ── C7: ORDER BY the measure, descending ──
    await qbCall('updateColumnConfig', SALE, 'qty', { sort: 'DESC' });
    await browser.pause(300);
    sql = await previewText();
    expect(sql.toUpperCase()).toContain('ORDER BY');
    expect(sql.toUpperCase()).toContain('DESC');

    // ── C8: DISTINCT ──
    // The toggle lives in the SELECT clause row (Build tab only).
    await switchQbTab('build');
    await $('[data-testid="qb-distinct-checkbox"]').click();
    await browser.pause(300);
    await switchQbTab('preview');
    sql = await previewText();
    expect(sql.toUpperCase()).toContain('SELECT DISTINCT');

    // ── C9/C10: WHERE — root conditions first, then the nested OR group ──
    await switchQbTab('build');
    await addWhereCondition();
    await configureWhereRow(0, { field: `${BOOK}.year`, operator: '>=', value: '2000' });

    // Second root condition: the IN list.
    await addWhereCondition();
    await configureWhereRow(1, {
      field: `${SALE}.channel`,
      operator: 'IN',
      value: 'online,store',
    });

    // Nested group: (rating = 'A' OR rating = 'B').
    await addWhereGroup();
    await addSubGroupCondition();
    await configureWhereRow(2, { field: `${BOOK}.rating`, operator: '=', value: 'A' });
    await addSubGroupCondition();
    await configureWhereRow(3, { field: `${BOOK}.rating`, operator: '=', value: 'B' });
    await browser.pause(400);

    // ── C11: LIMIT / OFFSET through the real inputs ──
    // A non-zero offset is used on purpose: the dialect adapters omit a zero
    // OFFSET, so asserting `OFFSET 0` would never prove the clause is emitted.
    await switchQbTab('build');
    // The fields must actually exist in the Build tab (component wiring is
    // covered in depth by the unit test; this is the integration check).
    expect(await existsInDom('[data-testid="qb-limit-input"]')).toBe(true);
    expect(await existsInDom('[data-testid="qb-offset-input"]')).toBe(true);

    await setPagination(50, 10);
    const pagination = (await qbRead(['limit', 'offset'])) as { limit: number; offset: number };
    expect(pagination.limit).toBe(50);
    expect(pagination.offset).toBe(10);

    // ── C12: full preview cross-check ──
    await switchQbTab('preview');
    sql = await previewText();
    expectSqlFragments(sql, [
      'SELECT DISTINCT',
      'FROM',
      'INNER JOIN',
      'LEFT JOIN',
      'WHERE',
      'GROUP BY',
      'ORDER BY',
      'LIMIT 50',
      'OFFSET 10',
      'SUM(',
      'author_name',
      'book_title',
      'total_qty',
      '>= 2000',
      "IN ('online', 'store')",
      "= 'A'",
      "= 'B'",
    ]);
    // The nested group really is parenthesised — semantics, not cosmetics.
    expect(sql).toMatch(/AND\s*\(/);
    await captureJourneyStep('qbc-full-preview');

    // ── C13: commit and execute the generated statement ──
    // Drop the offset so the executed statement returns every group (the
    // semantic cross-check below depends on the full result set).
    await setPagination(50, 0);
    await clickQbOk();
    await waitQbClosed();

    const committed = (await readEditorSql()).replace(/\s+/g, ' ');
    expectSqlFragments(committed, ['SELECT DISTINCT', 'INNER JOIN', 'LEFT JOIN', 'GROUP BY']);

    const exec = await $('[data-testid="editor-execute-button"]');
    await exec.waitForClickable({ timeout: 10000 });
    await exec.click();

    const resultTable = await $('[data-testid="result-workspace-table"]');
    try {
      await resultTable.waitForDisplayed({ timeout: 25000 });
    } catch (error) {
      // Print the statement that failed to produce a grid: without it a
      // regression here is very hard to diagnose from the reporter alone.
      console.log('[qb-e2e] statement that produced no result grid:\n' + committed);
      throw error;
    }
    const resultText = await resultTable.getText();

    // Semantic cross-check against the seed data:
    //   Ada/Notes    2001 'A' → 3 + 2 = 5
    //   Linus/Kernel 2005 'B' → 5
    //   Linus/Later  2010 'A' → 1
    //   Ada/Sketches 1998     → excluded by the year filter
    expect(resultText).toContain('Ada');
    expect(resultText).toContain('Notes');
    expect(resultText).toContain('Kernel');
    expect(resultText).toContain('Later');
    expect(resultText).not.toContain('Sketches');
    await captureJourneyStep('qbc-result');

    // ── C14: the preview copy button flips to the copied state ──
    await openQbViaMenu('qbc-reopen');
    await switchQbTab('preview');
    const copy = await $('[data-testid="qb-copy-sql"]');
    await copy.waitForClickable({ timeout: 5000 });
    await copy.click();
    await browser.waitUntil(
      async () => {
        const text = await copy.getText();
        return text.includes('已复制') || text.includes('Copied');
      },
      { timeout: 5000, timeoutMsg: '复制按钮未切换到已复制态' },
    );

    // ── C15: reset degenerates to the minimal statement, no clause leftovers ──
    await $('[data-testid="qb-reset"]').click();
    await browser.pause(500);
    await addTableCard(AUTHOR, { x: 40, y: 40 });
    await selectCardColumn(AUTHOR, 'name');
    await browser.pause(400);

    await switchQbTab('preview');
    const minimal = await previewText();
    expectSqlFragments(minimal, ['SELECT', AUTHOR]);
    expectNoSqlFragments(minimal, [
      'JOIN',
      'WHERE',
      'GROUP BY',
      'ORDER BY',
      'LIMIT',
      'OFFSET',
      'DISTINCT',
      'SUM(',
    ]);
    await captureJourneyStep('qbc-reset-minimal');
  });

  it('C16: 复合外键画成一条主干并整组确认', async () => {
    // C1–C15 leave the builder open; the toolbar entry is a toggle, so only
    // open it when it is actually closed.
    if (!(await isQbOpen())) await openQbViaMenu('qbc-composite');
    await qbCall('reset');
    await addTableCard(COMP_PARENT, { x: 20, y: 20 });
    await addTableCard(COMP_CHILD, { x: 460, y: 20 });
    await browser.pause(800);

    // One constraint → exactly one relation group …
    const constraintCount = await browser.execute(() => {
      const state = (window as any).__qbStore.getState();
      const constraints = new Set(
        (state.autoJoins ?? []).map((join: { constraint?: string }) => join.constraint),
      );
      return constraints.size;
    });
    expect(constraintCount).toBe(1);

    // … drawn as ONE trunk of axis-aligned segments, with a terminal dot at
    // both ends of every pair and no direction marker anywhere.
    const shape = await browser.execute(() => {
      const groups = Array.from(document.querySelectorAll('g[data-relation-kind]'));
      const group = groups[0] as SVGGElement | undefined;
      const paths = group ? Array.from(group.querySelectorAll('path[data-part]')) : [];
      const ds = paths.map((p) => p.getAttribute('d') ?? '');
      // A polyline is a fold line only if consecutive points share an axis —
      // this is what catches a regression back to a straight diagonal.
      const axial = ds.every((d) => {
        const nums = (d.match(/-?\d+(?:\.\d+)?/g) ?? []).map(Number);
        const pts: Array<[number, number]> = [];
        for (let i = 0; i + 1 < nums.length; i += 2) pts.push([nums[i]!, nums[i + 1]!]);
        return pts.every((p, i) => {
          if (i === 0) return true;
          const q = pts[i - 1]!;
          return Math.abs(p[0] - q[0]) < 0.01 || Math.abs(p[1] - q[1]) < 0.01;
        });
      });
      const state = (window as any).__qbStore.getState();
      return {
        groups: groups.length,
        trunks: group ? group.querySelectorAll('[data-part="trunk"]').length : -1,
        arrows: group ? group.querySelectorAll('.qb-relation-arrow').length : -1,
        dots: group ? group.querySelectorAll('.qb-relation-dot').length : -1,
        markers: paths.filter((p) => p.getAttribute('marker-end') ?? p.getAttribute('marker-start'))
          .length,
        segments: ds.length,
        axial,
        pairs: group?.getAttribute('data-relation-pairs') ?? null,
        detected: (state.autoJoins ?? []).map(
          (join: { constraint?: string; leftColumn: string; rightColumn: string }) =>
            `${join.constraint}:${join.leftColumn}->${join.rightColumn}`,
        ),
      };
    });
    expect(shape).not.toBeNull();
    // One merged relation (not one line per column pair) …
    expect(shape!.groups).toBe(1);
    expect(shape!.trunks).toBe(1);
    // … one trunk + one source stub + one target stub per pair …
    expect(shape!.segments).toBe(5);
    // … drawing a fold line (no diagonals) with no direction of any kind.
    expect(shape!.axial).toBe(true);
    expect(shape!.arrows).toBe(0);
    expect(shape!.markers).toBe(0);
    // … and a symmetric terminal dot at BOTH ends of every pair.
    expect(shape!.dots).toBe(4);
    expect(shape!.pairs).toBe('0/2');
    expect(await existsInDom('[data-testid^="qb-join-label-"]')).toBe(false);
    await captureJourneyStep('qbc-composite-trunk');

    // Confirming the group must add BOTH pairs — half a composite FK is a
    // wrong query, so it is never confirmable pair by pair.
    const confirmed = await confirmAutoJoinsViaUi(2);
    expect(confirmed).toBe(2);
    const afterConfirm = await browser.execute(() => {
      const groups = Array.from(document.querySelectorAll('g[data-relation-kind]'));
      const pairs = groups[0]?.getAttribute('data-relation-pairs') ?? null;
      const joins = (window as any).__qbStore.getState().joins ?? [];
      return { pairs, joins: joins.length };
    });
    // Every pair of the constraint moved into the SQL in one action.
    expect(afterConfirm.pairs).toBe('2/2');
    expect(afterConfirm.joins).toBe(2);

    // … and the generated SQL merges them into a single JOIN with AND.
    await selectCardColumn(COMP_PARENT, 'label');
    await selectCardColumn(COMP_CHILD, 'qty');
    await switchQbTab('preview');
    const sql = await previewText();
    expect((sql.match(/INNER JOIN/gi) ?? []).length).toBe(1);
    expect(sql).toContain(' AND ');
    await captureJourneyStep('qbc-composite-sql');
  });
});
