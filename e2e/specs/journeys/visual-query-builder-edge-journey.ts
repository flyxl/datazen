/**
 * QB-JOURNEY-B — Visual Query Builder, abnormal / edge journey.
 *
 * The point of this journey is that the *wrong* things never happen:
 * OK never executes, OK never silently overwrites hand-written SQL, Cancel
 * never loses the editor content, and two query tabs never mirror each other.
 *
 * Covers PRD §13.3.2 (B1–B15).
 */
import { browser, $, expect } from '@wdio/globals';
import { captureJourneyStep, openQueryTab } from '../../helpers.js';
import {
  addTableCard,
  clickQbCancel,
  clickQbClose,
  clickQbOk,
  confirmDiscardIfAsked,
  existsInDom,
  addWhereCondition,
  applyConditionDialog,
  configureWhereRow,
  isQbOpen,
  openQbViaMenu,
  clickNavigatorTable,
  closeAllQueryPanels,
  previewText,
  readExecutionSeq,
  removeTableCard,
  setTestIdInput,
  tableDataViewIsOpen,
  waitForEmptyCanvas,
  qbCall,
  qbRead,
  readEditorSql,
  selectCardColumn,
  setEditorSql,
  setupQbJourney,
  switchQbTab,
  teardownQbJourney,
  waitQbClosed,
  waitQbOpen,
} from './visualQueryBuilderHelpers.js';
import type { QbJourneySetup } from './visualQueryBuilderHelpers.js';

const TABLE_A = `e2e_qb_b1_${Date.now().toString(36)}`;
const TABLE_B = `e2e_qb_b2_${Date.now().toString(36)}`;
const COLUMNS = ['id', 'name', 'score'];

describe('Visual Query Builder 异常旅程 (QB-JOURNEY-B)', () => {
  let setup: QbJourneySetup;

  before(async () => {
    setup = await setupQbJourney({
      connectionId: 'e2e_qb_b_conn',
      connectionName: 'E2E-QB-B',
      captureLabel: 'qbb-open-suite',
      tables: { [TABLE_A]: COLUMNS, [TABLE_B]: COLUMNS },
      seedStatements: [
        `DROP TABLE IF EXISTS ${TABLE_A} CASCADE`,
        `CREATE TABLE ${TABLE_A} (id INTEGER, name TEXT, score INTEGER)`,
        `INSERT INTO ${TABLE_A} (id, name, score) VALUES (1, 'Alice', 90)`,
        // No foreign key to TABLE_A — the "no relation" case.
        `DROP TABLE IF EXISTS ${TABLE_B} CASCADE`,
        `CREATE TABLE ${TABLE_B} (id INTEGER, name TEXT, score INTEGER)`,
        `INSERT INTO ${TABLE_B} (id, name, score) VALUES (1, 'Zed', 10)`,
      ],
    });
    await setEditorSql('');
    await openQbViaMenu('qbb-open');
  });

  after(async () => {
    await teardownQbJourney(setup);
  });

  it('B1: 空态下 OK 禁用，预览给出空态提示', async () => {
    await qbCall('reset');
    await browser.pause(300);

    const ok = await $('[data-testid="qb-ok"]');
    expect(await ok.isEnabled()).toBe(false);

    await switchQbTab('preview');
    expect(await existsInDom('[data-testid="qb-sql-preview-empty"]')).toBe(true);
    expect(await existsInDom('[data-testid="qb-sql-preview"]')).toBe(false);

    await switchQbTab('build');
    expect(await existsInDom('[data-testid="qb-where-empty"]')).toBe(true);
    await captureJourneyStep('qbb-b1-empty');
  });

  it('B2: 只加表不选列 → 构建 Tab 打红点且 OK 仍禁用', async () => {
    await addTableCard(TABLE_A);
    await browser.pause(400);

    expect(await existsInDom('[data-testid="qb-tab-build-badge"]')).toBe(true);
    expect(await $('[data-testid="qb-ok"]').isEnabled()).toBe(false);
    await captureJourneyStep('qbb-b2-badge');
  });

  it('B3: OK 只写 SQL，绝不执行查询', async () => {
    await selectCardColumn(TABLE_A, 'name');
    await browser.pause(300);

    // The panel's execution counter is the precise "did a query run?" signal.
    // Asserting on the *absence* of a result grid would be wrong here: the
    // journey seeds its tables through this same panel, so a grid from the
    // seed statements is expected to still be on screen.
    const executionSeqBefore = await readExecutionSeq();
    const runningBefore = await browser.execute(() =>
      document.querySelector('[data-testid="query-panel"]')?.getAttribute('data-query-running'),
    );
    expect(runningBefore).toBe('false');

    // The editor is already empty and is hidden while the builder is open.
    await clickQbOk();
    await waitQbClosed();
    await browser.pause(600);

    // OK wrote SQL and started nothing.
    expect(await readExecutionSeq()).toBe(executionSeqBefore);
    const runningAfter = await browser.execute(() =>
      document.querySelector('[data-testid="query-panel"]')?.getAttribute('data-query-running'),
    );
    expect(runningAfter).toBe('false');

    const sql = (await readEditorSql()).replace(/\s+/g, ' ');
    expect(sql).toContain('SELECT');
    expect(sql).toContain(TABLE_A);
    await captureJourneyStep('qbb-b3-no-execute');
  });

  it('B4–B6: 编辑器已有内容时 OK 必须询问，替换/追加/保留三种都正确', async () => {
    const sentinel = 'SELECT 1 AS keep_me;';

    // ── B4: replace ──
    await setEditorSql(sentinel);
    await openQbViaMenu('qbb-b4-reopen');
    await clickQbOk();

    await $('[data-testid="qb-conflict-replace"]').waitForDisplayed({ timeout: 5000 });
    expect(await existsInDom('[data-testid="qb-conflict-append"]')).toBe(true);
    expect(await existsInDom('[data-testid="qb-conflict-keep"]')).toBe(true);
    await captureJourneyStep('qbb-b4-conflict');

    await $('[data-testid="qb-conflict-replace"]').click();
    await waitQbClosed();

    const replaced = (await readEditorSql()).replace(/\s+/g, ' ');
    expect(replaced).toContain('SELECT');
    expect(replaced).toContain(TABLE_A);
    expect(replaced).not.toContain('keep_me');

    // ── B5: keep → stays in the builder, editor untouched ──
    await setEditorSql(sentinel);
    await openQbViaMenu('qbb-b5-reopen');
    await clickQbOk();
    await $('[data-testid="qb-conflict-keep"]').waitForDisplayed({ timeout: 5000 });
    await $('[data-testid="qb-conflict-keep"]').click();
    await browser.pause(400);

    await waitQbOpen(3000);
    expect(await existsInDom('[data-testid="qb-conflict-replace"]')).toBe(false);
    expect((await readEditorSql()).replace(/\s+/g, ' ')).toContain('keep_me');

    // ── B6: append ──
    await clickQbOk();
    await $('[data-testid="qb-conflict-append"]').waitForDisplayed({ timeout: 5000 });
    await $('[data-testid="qb-conflict-append"]').click();
    await waitQbClosed();

    const appended = (await readEditorSql()).replace(/\s+/g, ' ');
    expect(appended).toContain('keep_me');
    expect(appended).toContain(TABLE_A);
    await captureJourneyStep('qbb-b6-appended');
  });

  it('B7: 编辑器内容与生成结果一致时不弹冲突框', async () => {
    // Deterministic setup: commit once with an empty editor, then commit the
    // very same canvas again. The editor already holds exactly this SQL, so
    // the second OK must not prompt.
    await setEditorSql('');
    await openQbViaMenu('qbb-b7-reopen');
    await clickQbOk();
    await waitQbClosed();
    const first = (await readEditorSql()).replace(/\s+/g, ' ').trim();
    expect(first.length).toBeGreaterThan(0);

    await openQbViaMenu('qbb-b7-reopen-again');
    await clickQbOk();

    await waitQbClosed();
    expect(await existsInDom('[data-testid="qb-conflict-replace"]')).toBe(false);
    const second = (await readEditorSql()).replace(/\s+/g, ' ').trim();
    expect(second).toBe(first);
  });

  it('B8–B9: 取消会回滚画布且不动编辑器', async () => {
    const editorBefore = (await readEditorSql()).replace(/\s+/g, ' ');

    await openQbViaMenu('qbb-b8-reopen');
    // A real change after opening → the discard confirmation must appear.
    await addTableCard(TABLE_B);
    await browser.pause(400);

    await clickQbCancel();
    const asked = await confirmDiscardIfAsked();
    expect(asked).toBe(true);
    await waitQbClosed();

    // B9: the editor content is exactly what it was before the builder session.
    const editorAfter = (await readEditorSql()).replace(/\s+/g, ' ');
    expect(editorAfter).toBe(editorBefore);

    // Reopening proves the rollback: TABLE_B is gone from the canvas.
    await openQbViaMenu('qbb-b8-rollback');
    expect(await existsInDom(`[data-testid="qb-drag-${TABLE_B}"]`)).toBe(false);
    expect(await existsInDom(`[data-testid="qb-drag-${TABLE_A}"]`)).toBe(true);
  });

  it('B10: 无外键关系时给出提示且不生成 JOIN', async () => {
    await addTableCard(TABLE_B);
    await browser.pause(1500); // let FK detection settle for both tables

    expect(await existsInDom('[data-testid="qb-no-relation-hint"]')).toBe(true);

    await switchQbTab('preview');
    const sql = await previewText();
    expect(sql).toContain(TABLE_A);
    // No relation detected ⇒ nothing may be pulled in implicitly: the second
    // table must not appear at all (no automatic JOIN, no phantom FROM entry).
    expect(sql.toUpperCase()).not.toContain('JOIN');
    expect(sql).not.toContain(TABLE_B);
    await captureJourneyStep('qbb-b10-no-relation');
  });

  it('B11: 移除表会一并清理其列与条件引用', async () => {
    await switchQbTab('build');
    // Through the card's own remove control — the path a user has.
    await removeTableCard(TABLE_B);
    expect(await existsInDom(`[data-testid="qb-drag-${TABLE_B}"]`)).toBe(false);

    await switchQbTab('preview');
    const sql = await previewText();
    expect(sql).toContain(TABLE_A);
    expect(sql).not.toContain(TABLE_B);
    expect(sql.toUpperCase()).not.toContain('JOIN');
  });

  it('B12: 两个查询 Tab 的构建器状态互不串扰', async () => {
    const firstPanelId = await browser.execute(
      () =>
        document.querySelector('[data-testid="qb-panel"]')?.getAttribute('data-qb-panel-id') ?? '',
    );
    expect(firstPanelId.length).toBeGreaterThan(0);

    // Open a second query tab. The pre-existing global `isOpen` bug showed the
    // builder in whichever panel was active — it must not appear here.
    await openQueryTab();
    await browser.pause(800);
    expect(await existsInDom('[data-testid="qb-panel"]')).toBe(false);

    // Opening it here is panel-scoped: a different panel id.
    await openQbViaMenu('qbb-b12-second-panel');
    const secondPanelId = await browser.execute(
      () =>
        document.querySelector('[data-testid="qb-panel"]')?.getAttribute('data-qb-panel-id') ?? '',
    );
    expect(secondPanelId.length).toBeGreaterThan(0);
    expect(secondPanelId).not.toBe(firstPanelId);

    // Going back to the first panel must not resurrect a stale builder.
    const switched = await browser.execute(() => {
      const tabs = Array.from(
        document.querySelectorAll<HTMLElement>('[data-testid="panel-tab"] button[role="tab"]'),
      );
      const first = tabs[0];
      if (!first) return false;
      first.click();
      return true;
    });
    expect(switched).toBe(true);
    await browser.pause(800);
    expect(await existsInDom('[data-testid="qb-panel"]')).toBe(false);
    await captureJourneyStep('qbb-b12-isolation');
  });

  it('B13–B15: Esc 取消、收起不破坏画布、无残留异常', async () => {
    // ── B15: the toolbar toggle is a non-destructive "hide" ──
    await openQbViaMenu('qbb-b15-open');
    await addTableCard(TABLE_A);
    await browser.pause(400);
    expect(await existsInDom(`[data-testid="qb-drag-${TABLE_A}"]`)).toBe(true);

    // Clicking the menu entry again hides the builder…
    const moreMenu = await $('[data-testid="query-toolbar-more-menu-trigger"]');
    await moreMenu.waitForClickable({ timeout: 5000 });
    await moreMenu.click();
    await browser.pause(250);
    await $('[data-testid="more-menu-visual-builder"]').click();
    await waitQbClosed();

    // …and the canvas state survived (hide ≠ cancel).
    await openQbViaMenu('qbb-b15-reopen');
    expect(await existsInDom(`[data-testid="qb-drag-${TABLE_A}"]`)).toBe(true);

    // ── B14: Esc walks the cancel path ──
    await addTableCard(TABLE_B);
    await browser.pause(400);
    await browser.keys(['Escape']);
    const asked = await confirmDiscardIfAsked();
    expect(asked).toBe(true);
    await waitQbClosed();

    // ── B13: still fully interactive after all the edge paths ──
    await openQbViaMenu('qbb-b13-liveness');
    expect(await existsInDom('[data-testid="qb-tab-build"]')).toBe(true);
    expect(await existsInDom('[data-testid="qb-tab-preview"]')).toBe(true);
    expect(await existsInDom('[data-testid="qb-footer"]')).toBe(true);
    await switchQbTab('preview');
    const alive = await previewText();
    expect(alive).toContain('SELECT');
    await switchQbTab('build');
    await clickQbClose();
    await waitQbClosed();
    await captureJourneyStep('qbb-done');
  });

  it('B16: 打开 QB 时点击导航树的表应打开表数据，而不是加入画布', async () => {
    // The builder is open (B13 closed it, so reopen) with a clean canvas.
    await openQbViaMenu('qbb-b16-open');
    await qbCall('reset');
    await waitForEmptyCanvas();

    const before = await browser.execute(
      () => ((window as any).__qbStore.getState().selectedTables ?? []).length,
    );
    expect(before).toBe(0);

    await clickNavigatorTable(TABLE_A);

    // A table data workspace opened …
    await browser.waitUntil(() => tableDataViewIsOpen(TABLE_A), {
      timeout: 15000,
      timeoutMsg: '点击导航树中的表后未打开表数据视图',
    });
    // … and the builder canvas was left untouched.
    const after = await browser.execute(
      () => ((window as any).__qbStore.getState().selectedTables ?? []).length,
    );
    expect(after).toBe(0);
    await captureJourneyStep('qbb-b16-click-opens-data');
  });

  it('B17: 销毁 Query 面板应销毁其 QB，重新打开是全新状态', async () => {
    // Remove the stray table-data panel so only the query panel is left, then
    // close every panel: the builder belongs to the panel, so it must go too.
    await closeAllQueryPanels();
    await waitForEmptyCanvas();

    // With no panel alive, clicking a navigator table opens table data again.
    await clickNavigatorTable(TABLE_A);
    await browser.waitUntil(() => tableDataViewIsOpen(TABLE_A), {
      timeout: 15000,
      timeoutMsg: '面板销毁后点击表未打开表数据',
    });

    // A brand-new query panel must start from a blank builder canvas.
    await openQueryTab();
    await openQbViaMenu('qbb-b17-fresh');
    await waitForEmptyCanvas();
    expect(await existsInDom(`[data-testid="qb-drag-${TABLE_A}"]`)).toBe(false);
    expect(await existsInDom(`[data-testid="qb-drag-${TABLE_B}"]`)).toBe(false);
    await captureJourneyStep('qbb-b17-fresh-builder');
    await clickQbClose();
    await waitQbClosed();
  });

  it('B18: 条件值为空会阻断 OK 并给出诊断', async () => {
    await openQbViaMenu('qbb-b18-open');
    await qbCall('reset');
    await addTableCard(TABLE_A);
    await selectCardColumn(TABLE_A, 'name');
    await browser.pause(300);

    // A condition applied with an empty value — `col = ` is not buildable.
    // The dialog is a draft, so the empty value has to be confirmed on purpose.
    await switchQbTab('build');
    await addWhereCondition();
    await applyConditionDialog();
    await switchQbTab('preview');

    expect(await existsInDom('[data-testid="qb-diagnostics"]')).toBe(true);
    expect(await $('[data-testid="qb-ok"]').isEnabled()).toBe(false);
    await captureJourneyStep('qbb-b18-blocked');

    // Filling the value clears the block.
    await switchQbTab('build');
    await configureWhereRow(0, { value: 'Alice' });
    await switchQbTab('preview');
    expect(await existsInDom('[data-testid="qb-diagnostics"]')).toBe(false);
    expect(await $('[data-testid="qb-ok"]').isEnabled()).toBe(true);
  });

  it('B19: 两张表使用相同别名会阻断 OK', async () => {
    await switchQbTab('build');
    await addTableCard(TABLE_B);
    await browser.pause(300);

    await setTestIdInput(`qb-alias-${TABLE_A}`, 'dup');
    await setTestIdInput(`qb-alias-${TABLE_B}`, 'dup');
    await browser.pause(300);

    await switchQbTab('preview');
    expect(await existsInDom('[data-testid="qb-diagnostics"]')).toBe(true);
    expect(await $('[data-testid="qb-ok"]').isEnabled()).toBe(false);

    // Renaming one of them unblocks the query.
    await switchQbTab('build');
    await setTestIdInput(`qb-alias-${TABLE_B}`, 'other');
    await switchQbTab('preview');
    expect(await existsInDom('[data-testid="qb-diagnostics"]')).toBe(false);
    expect(await $('[data-testid="qb-ok"]').isEnabled()).toBe(true);
    await captureJourneyStep('qbb-b19-alias-collision');
  });

  it('B20: 从列拖到另一列建立手动 JOIN', async () => {
    // B18/B19 leave TABLE_A (name selected) and TABLE_B on the canvas. They have
    // no foreign key between them, so any relation here is a manual join.
    await switchQbTab('build');
    const before = (await qbRead(['joins'])).joins as unknown[];

    // The drag has to be driven in steps: React attaches the window listeners in
    // an effect *after* the pointerdown state update, so a pointermove sent in
    // the same JS turn would land before anything is listening.
    const pointer = (
      selector: string,
      type: 'pointerdown' | 'pointermove' | 'pointerup',
      onWindow: boolean,
    ) =>
      browser.execute(
        (sel: string, kind: string, useWindow: boolean) => {
          const el = document.querySelector<HTMLElement>(sel);
          if (!el) return { ok: false, reason: `missing ${sel}` };
          const rect = el.getBoundingClientRect();
          const x = rect.left + rect.width / 2;
          const y = rect.top + rect.height / 2;
          const target: EventTarget = useWindow ? window : el;
          target.dispatchEvent(
            new PointerEvent(kind, {
              bubbles: true,
              cancelable: true,
              pointerId: 11,
              isPrimary: true,
              pointerType: 'mouse',
              buttons: 1,
              clientX: x,
              clientY: y,
            }),
          );
          return { ok: true, x, y };
        },
        selector,
        type,
        onWindow,
      );

    const handleSelector = `[data-testid="qb-connect-${TABLE_A}-name"]`;
    const targetSelector = `[data-testid="qb-col-${TABLE_B}-id"]`;

    expect((await pointer(handleSelector, 'pointerdown', false)).ok).toBe(true);
    // The preview path only renders after the pointerdown state commits, and the
    // window pointermove listener attaches in that same commit's effect. Waiting
    // on it beats a fixed pause: under parallel-session load the debug webview
    // can take longer than any constant allows.
    await browser.waitUntil(() => existsInDom('[data-testid="qb-manual-join-preview"]'), {
      timeout: 5000,
      interval: 100,
    });
    await pointer(targetSelector, 'pointermove', true);

    // During a drag the canvas draws a preview line and highlights the column
    // under the pointer, so the drop target is unambiguous.
    await browser.waitUntil(
      () =>
        browser.execute(
          (sel: string) => !!document.querySelector(sel)?.classList.contains('qb-drop-target'),
          targetSelector,
        ),
      { timeout: 3000, interval: 100 },
    );
    const highlighted = await browser.execute(
      (sel: string) => !!document.querySelector(sel)?.classList.contains('qb-drop-target'),
      targetSelector,
    );
    expect(highlighted).toBe(true);
    await captureJourneyStep('qbb-b20-drag-preview');

    await pointer(targetSelector, 'pointerup', true);
    await browser.pause(250);

    const after = (await qbRead(['joins'])).joins as Array<Record<string, unknown>>;
    expect(after.length).toBe(before.length + 1);
    const manual = after.find((join) => join.isManual === true);
    expect(manual).toBeTruthy();
    expect(manual!.leftTable).toBe(TABLE_A);
    expect(manual!.leftColumn).toBe('name');
    expect(manual!.rightTable).toBe(TABLE_B);
    expect(manual!.rightColumn).toBe('id');

    // Drawn as a manual relation (its own style) and still no text anywhere.
    expect(await existsInDom('[data-relation-kind="manual"]')).toBe(true);
    expect(await existsInDom('[data-testid^="qb-join-label-"]')).toBe(false);

    // …and it reaches the generated SQL.
    await switchQbTab('preview');
    const sql = await previewText();
    expect((sql.match(/INNER JOIN/gi) ?? []).length).toBe(1);
    await captureJourneyStep('qbb-b20-manual-join');
  });

  it('B-close: × 走取消路径', async () => {
    // B18/B19 leave the builder open with a modified canvas, so × must ask
    // before discarding rather than closing outright.
    if (!(await isQbOpen())) await openQbViaMenu('qbb-close-open');
    await clickQbClose();
    await confirmDiscardIfAsked();
    await waitQbClosed();
  });
});
