/**
 * Shared helpers for the Visual Query Builder E2E journeys (QB-JOURNEY-A/B/C).
 *
 * Conventions
 * -----------
 * - Panel chrome (tabs, OK, Cancel, WHERE rows) is driven through **real DOM
 *   interactions** — that is the behaviour under test.
 * - Canvas state is driven through the exposed `__qbStore`, the same shortcut the
 *   pre-existing QB journey uses: WebKit does not fire React's synthetic
 *   drag/drop handlers, so a synthetic `DragEvent` would silently no-op.
 * - Assertions use stable `data-*` locators only (never copy or geometry).
 */
import { browser, $, expect } from '@wdio/globals';
import {
  captureJourneyStep,
  clickCardConnectButton,
  closeExtraWindows,
  executeSQL,
  expandConnectedConnectionInNavigator,
  invokeBackend,
  openConnectionsWorkspace,
  openQueryTab,
  waitForConnectionToolbar,
} from '../../helpers.js';

// ── Store access ───────────────────────────────────────────────────

/** Invoke a query-builder store action by name. */
export async function qbCall(method: string, ...args: unknown[]): Promise<void> {
  await browser.execute(
    (m: string, a: unknown[]) => {
      const store = (window as any).__qbStore;
      if (!store?.getState) throw new Error('__qbStore is not exposed');
      const state = store.getState();
      const action = state[m];
      if (typeof action !== 'function') throw new Error(`unknown qb action: ${m}`);
      action.apply(state, a);
    },
    method,
    args,
  );
}

/** Patch raw fields on the query-builder store. */
export async function qbPatch(patch: Record<string, unknown>): Promise<void> {
  await browser.execute((p: Record<string, unknown>) => {
    const store = (window as any).__qbStore;
    if (!store?.setState) throw new Error('__qbStore is not exposed');
    store.setState(p);
  }, patch);
}

/** Read raw fields off the query-builder store. */
export async function qbRead(keys: string[]): Promise<Record<string, unknown>> {
  return browser.execute((ks: string[]) => {
    const store = (window as any).__qbStore;
    const state = store?.getState?.() ?? {};
    const out: Record<string, unknown> = {};
    for (const k of ks) out[k] = state[k];
    return out;
  }, keys);
}

/** Fully reset the builder store (canvas + view state). */
export async function resetQbStore(): Promise<void> {
  await browser.execute(() => {
    const store = (window as any).__qbStore;
    store.setState(store.getInitialState());
  });
}

/** Preload the schema-store column map so table cards render deterministically. */
export async function seedSchemaColumns(table: string, columns: string[]): Promise<void> {
  await browser.execute(
    (t: string, cols: string[]) => {
      const store = (window as any).__schemaStore;
      if (!store?.setState) throw new Error('__schemaStore is not exposed');
      const state = store.getState();
      store.setState({ columnMap: { ...(state.columnMap ?? {}), [t]: cols } });
    },
    table,
    columns,
  );
}

// ── Panel ──────────────────────────────────────────────────────────

export async function waitQbOpen(timeout = 8000): Promise<void> {
  const panel = await $('[data-testid="qb-panel"]');
  await panel.waitForDisplayed({ timeout, timeoutMsg: '可视化构建器面板未打开' });
}

export async function isQbOpen(): Promise<boolean> {
  return (
    (await $('[data-testid="qb-panel"]')
      .isDisplayed()
      .catch(() => false)) === true
  );
}

export async function waitQbClosed(timeout = 8000): Promise<void> {
  await browser.waitUntil(async () => !(await isQbOpen()), {
    timeout,
    timeoutMsg: '可视化构建器面板未关闭',
  });
}

/** Open the builder from the query toolbar's More menu. */
export async function openQbViaMenu(label = 'qb-open'): Promise<void> {
  const moreMenu = await $('[data-testid="query-toolbar-more-menu-trigger"]');
  await moreMenu.waitForClickable({ timeout: 10000 });
  await moreMenu.click();
  await browser.pause(250);

  const item = await $('[data-testid="more-menu-visual-builder"]');
  await item.waitForClickable({ timeout: 5000 });
  await item.click();

  await waitQbOpen();
  await captureJourneyStep(label);
}

export async function switchQbTab(tab: 'build' | 'preview'): Promise<void> {
  const button = await $(`[data-testid="qb-tab-${tab}"]`);
  await button.waitForClickable({ timeout: 5000 });
  await button.click();
  await browser.waitUntil(
    () =>
      browser.execute(
        (t: string) =>
          document
            .querySelector('[data-testid="qb-tab-content"]')
            ?.getAttribute('data-active-tab') === t,
        tab,
      ),
    { timeout: 5000, timeoutMsg: `切换到 ${tab} Tab 失败` },
  );
}

/** Text of the read-only SQL preview. Empty string when on the empty state. */
export async function previewText(): Promise<string> {
  return browser.execute(() => {
    const pre = document.querySelector('[data-testid="qb-sql-preview"]');
    return pre ? (pre.textContent ?? '').replace(/\u00a0/g, ' ') : '';
  });
}

export async function clickQbOk(): Promise<void> {
  const ok = await $('[data-testid="qb-ok"]');
  await ok.waitForClickable({ timeout: 5000 });
  await ok.click();
}

export async function clickQbCancel(): Promise<void> {
  const cancel = await $('[data-testid="qb-cancel"]');
  await cancel.waitForClickable({ timeout: 5000 });
  await cancel.click();
}

export async function clickQbClose(): Promise<void> {
  const close = await $('[data-testid="qb-close"]');
  await close.waitForClickable({ timeout: 5000 });
  await close.click();
}

/** Confirm the "discard changes?" dialog, if it appeared. */
export async function confirmDiscardIfAsked(): Promise<boolean> {
  const appeared = await browser
    .waitUntil(
      () =>
        browser.execute(() =>
          Array.from(document.querySelectorAll('button')).some((b) =>
            ['放弃更改', 'Discard'].includes((b.textContent ?? '').trim()),
          ),
        ),
      { timeout: 2500, timeoutMsg: 'no discard dialog' },
    )
    .then(() => true)
    .catch(() => false);

  if (!appeared) return false;
  await browser.execute(() => {
    const btn = Array.from(document.querySelectorAll('button')).find((b) =>
      ['放弃更改', 'Discard'].includes((b.textContent ?? '').trim()),
    ) as HTMLButtonElement | undefined;
    btn?.click();
  });
  await browser.pause(400);
  return true;
}

// ── Select / input primitives ──────────────────────────────────────

/** Open a Select (by trigger testid) and pick the option matching `optionText`. */
export async function pickSelectOption(triggerTestId: string, optionText: string): Promise<void> {
  await browser.execute((id: string) => {
    // A wrapper may host the testid while the clickable trigger is a child
    // (CriteriaRow does exactly that), so try the inner trigger first.
    const el =
      document.querySelector<HTMLElement>(`[data-testid="${id}"] button`) ??
      document.querySelector<HTMLElement>(`[data-testid="${id}"] [role="combobox"]`) ??
      document.querySelector<HTMLElement>(`[data-testid="${id}"]`);
    el?.click();
  }, triggerTestId);

  await browser.waitUntil(
    () => browser.execute(() => document.querySelector('[data-testid="select-listbox"]') !== null),
    { timeout: 5000, timeoutMsg: `Select "${triggerTestId}" 未打开` },
  );

  const picked = await browser.execute((text: string) => {
    const list = document.querySelector('[data-testid="select-listbox"]');
    if (!list) return false;
    const options = Array.from(list.querySelectorAll('[data-testid="select-option"]'));
    const norm = (el: Element) => (el.textContent ?? '').replace(/\u00a0/g, ' ').trim();
    // Column options are labelled with the *effective qualifier*, which is the
    // table's alias (auto-assigned) rather than its name — accept either form.
    const candidates = [text];
    const dot = text.indexOf('.');
    if (dot > 0) {
      const table = text.slice(0, dot);
      const column = text.slice(dot + 1);
      const aliases = (window as any).__qbStore?.getState?.().tableAliases ?? {};
      if (aliases[table]) candidates.push(`${aliases[table]}.${column}`);
      candidates.push(column);
    }
    const hit =
      candidates.map((c) => options.find((o) => norm(o) === c)).find(Boolean) ??
      candidates.map((c) => options.find((o) => norm(o).includes(c))).find(Boolean);
    if (!hit) return false;
    hit.dispatchEvent(new MouseEvent('mousedown', { bubbles: true, cancelable: true }));
    return true;
  }, optionText);

  if (!picked) {
    throw new Error(`选项 "${optionText}" 未在 ${triggerTestId} 的下拉中找到`);
  }
  await browser.pause(250);
}

/**
 * Set a controlled `<input data-testid=...>` value.
 *
 * Tries the genuine typing path first. A field inside a scrollable region can be
 * reported as "not displayed" by WKWebView once it is clipped, so the fallback
 * drives the same controlled input through React's own value-tracker bypass
 * (native setter + `input` event) — identical to what a real keystroke produces
 * for React, without depending on hit-testing.
 *
 * Deliberately does NOT scroll the field into view first: that shifts the layout
 * between WebDriver's aim and its click, which sent the click onto the tab bar
 * instead of the input (a real, reproducible cross-element mis-click).
 */
/**
 * Set LIMIT and OFFSET together.
 *
 * Both fields are driven in a single in-page call: they are controlled React
 * inputs, so the native setter + `input` event is exactly what a keystroke
 * produces for React — while a WebDriver click on either one proved unstable
 * (WebDriver aims, then performs a layout pass, then clicks; in this panel that
 * landed the click on the tab bar above the sticky pagination row).
 */
export async function setPagination(limit: number | null, offset: number | null): Promise<void> {
  const write = () =>
    browser.execute(
      (l: number | null, o: number | null) => {
        const setter = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value')?.set;
        const set = (id: string, value: string) => {
          const el = document.querySelector<HTMLInputElement>(`[data-testid="${id}"]`);
          if (!el) return false;
          setter?.call(el, value);
          el.dispatchEvent(new Event('input', { bubbles: true }));
          return true;
        };
        return {
          limit: set('qb-limit-input', l === null ? '' : String(l)),
          offset: set('qb-offset-input', o === null ? '' : String(o)),
        };
      },
      limit,
      offset,
    );

  // The fields only exist while the Build tab is mounted. WebDriver's own input
  // operations can leave the tab bar focused, which occasionally activates the
  // Preview tab mid-sequence, so make sure we are on Build and retry rather than
  // asserting on harness-side focus artefacts.
  for (let attempt = 0; attempt < 3; attempt += 1) {
    const applied = await write();
    if (applied.limit && applied.offset) {
      await browser.pause(250);
      return;
    }
    await switchQbTab('build');
  }
  throw new Error('LIMIT/OFFSET 输入框在 Build Tab 中始终不可用');
}

export async function setTestIdInput(testId: string, value: string): Promise<void> {
  const input = await $(`[data-testid="${testId}"]`);
  await input.waitForExist({ timeout: 5000 });

  try {
    await input.waitForDisplayed({ timeout: 2500 });
    await input.click();
    await input.setValue(value);
  } catch {
    await browser.execute(
      (id: string, v: string) => {
        const el = document.querySelector<HTMLInputElement>(`[data-testid="${id}"]`);
        if (!el) throw new Error(`input ${id} not found`);
        const setter = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value')?.set;
        setter?.call(el, v);
        el.dispatchEvent(new Event('input', { bubbles: true }));
      },
      testId,
      value,
    );
  }
  await browser.pause(150);
}

// ── Canvas ─────────────────────────────────────────────────────────

/**
 * Add a table card.
 *
 * Uses the store action (not a raw `setState`) so the table picks up its
 * default alias exactly as it would through the real drop handler, then places
 * the card. See the file header for why the drop event itself is synthesised
 * rather than driven through WebDriver's drag support.
 */
export async function addTableCard(table: string, pos = { x: 40, y: 40 }): Promise<void> {
  const already = await browser.execute(
    (t: string) => (window as any).__qbStore.getState().selectedTables.includes(t),
    table,
  );
  if (!already) {
    await qbCall('toggleTable', table);
  }
  await qbCall('updateTablePosition', table, pos);
  await browser.waitUntil(
    () =>
      browser.execute(
        (t: string) => !!document.querySelector(`[data-testid="qb-drag-${t}"]`),
        table,
      ),
    { timeout: 5000, timeoutMsg: `表卡片 ${table} 未渲染` },
  );
}

/** Drop a navigator table onto the canvas by dispatching the real drag payload. */
export async function dragNavigatorTableToCanvas(
  table: string,
  pos = { x: 60, y: 40 },
): Promise<boolean> {
  return browser.execute(
    (t: string, p: { x: number; y: number }) => {
      const canvas = document.querySelector<HTMLElement>('[data-testid="qb-diagram-canvas"]');
      if (!canvas) return false;
      const rect = canvas.getBoundingClientRect();
      const dt = new DataTransfer();
      dt.setData(
        'application/datazen-schema-object',
        JSON.stringify({
          version: 1,
          kind: 'table',
          namespace: { database: '', table: t },
          connectionId: 'e2e',
          databaseType: 'postgresql',
        }),
      );
      const init = {
        bubbles: true,
        cancelable: true,
        dataTransfer: dt,
        clientX: rect.left + p.x,
        clientY: rect.top + p.y,
      };
      canvas.dispatchEvent(new DragEvent('dragover', init));
      canvas.dispatchEvent(new DragEvent('drop', init));
      return true;
    },
    table,
    pos,
  );
}

/** Remove a table through the card's remove control (real UI path). */
export async function removeTableCard(table: string): Promise<void> {
  const clicked = await browser.execute((t: string) => {
    const btn = document.querySelector<HTMLElement>(`[data-testid="qb-remove-${t}"]`);
    if (!btn) return false;
    btn.click();
    return true;
  }, table);
  if (!clicked) throw new Error(`表卡片 ${table} 的移除按钮不存在`);
  await browser.pause(250);
}

/** Toggle every column of a card through its header checkbox. */
export async function toggleAllColumns(table: string): Promise<void> {
  const clicked = await browser.execute((t: string) => {
    const box = document.querySelector<HTMLInputElement>(`[data-testid="qb-selectall-${t}"]`);
    if (!box) return false;
    box.click();
    return true;
  }, table);
  if (!clicked) throw new Error(`表卡片 ${table} 的全选复选框不存在`);
  await browser.pause(250);
}

/** Drag a card by `delta` px, driving the real pointer sequence. */
export async function dragCard(table: string, delta: { x: number; y: number }): Promise<void> {
  const before = await browser.execute((t: string) => {
    const state = (window as any).__qbStore.getState();
    return state.tablePositions[t] ?? null;
  }, table);

  await browser.execute(
    (t: string, d: { x: number; y: number }) => {
      const card = document.querySelector<HTMLElement>(`[data-testid="qb-drag-${t}"]`);
      if (!card) return;
      // `useResizable`-style capture is not needed here: the card captures the
      // pointer itself, which WebKit refuses for synthetic pointers, so stub it.
      const anyCard = card as unknown as Record<string, unknown>;
      const originalSet = anyCard.setPointerCapture;
      const originalHas = anyCard.hasPointerCapture;
      const originalRelease = anyCard.releasePointerCapture;
      anyCard.setPointerCapture = () => {};
      anyCard.hasPointerCapture = () => true;
      anyCard.releasePointerCapture = () => {};

      const rect = card.getBoundingClientRect();
      const x = rect.left + 20;
      const y = rect.top + 8;
      const make = (type: string, cx: number, cy: number) =>
        new PointerEvent(type, {
          bubbles: true,
          cancelable: true,
          pointerId: 7,
          isPrimary: true,
          pointerType: 'mouse',
          buttons: 1,
          clientX: cx,
          clientY: cy,
        });

      card.dispatchEvent(make('pointerdown', x, y));
      card.dispatchEvent(make('pointermove', x + d.x, y + d.y));
      card.dispatchEvent(make('pointerup', x + d.x, y + d.y));

      anyCard.setPointerCapture = originalSet;
      anyCard.hasPointerCapture = originalHas;
      anyCard.releasePointerCapture = originalRelease;
    },
    table,
    delta,
  );
  await browser.pause(300);

  const after = await browser.execute((t: string) => {
    const state = (window as any).__qbStore.getState();
    return state.tablePositions[t] ?? null;
  }, table);
  if (JSON.stringify(before) === JSON.stringify(after)) {
    throw new Error(`拖动表卡片 ${table} 后位置未变化`);
  }
}

/** Click a column checkbox on a table card (real DOM interaction). */
export async function selectCardColumn(table: string, column: string): Promise<void> {
  const clicked = await browser.execute(
    (t: string, c: string) => {
      const row = document.querySelector(`[data-testid="qb-col-${t}-${c}"]`);
      const box = row?.querySelector('input[type="checkbox"]') as HTMLInputElement | null;
      if (!box) return false;
      box.click();
      return true;
    },
    table,
    column,
  );
  if (!clicked) throw new Error(`列 ${table}.${column} 的复选框不存在`);
  await browser.pause(200);
}

/**
 * Confirm every auto-detected (dashed) FK candidate through its real
 * "confirm join" affordance, so it enters the SQL.
 *
 * The click is dispatched in-page: the button lives inside an SVG
 * `foreignObject`, where WebDriver hit-testing is unreliable — the JSX handler
 * exercised is the same one a user click reaches.
 */
export async function confirmAutoJoinsViaUi(expected: number): Promise<number> {
  await waitForAutoJoins(expected, 25000);

  // One group per constraint: a composite FK is a single line with a single
  // trunk, so it is confirmed once, not once per column pair.
  const constraints = (await browser.execute(() =>
    Array.from(
      new Set(
        ((window as any).__qbStore.getState().autoJoins ?? [])
          .map((join: { constraint?: string }) => join.constraint)
          .filter(Boolean),
      ),
    ),
  )) as string[];

  for (const constraint of constraints) {
    // Open the actions popover by activating the relation line, then confirm.
    const opened = await browser.execute((id: string) => {
      const hits = Array.from(
        document.querySelectorAll<SVGPathElement>('[data-testid^="qb-relation-hit-"]'),
      );
      const hit = hits.find((node) => node.getAttribute('data-testid') === `qb-relation-hit-${id}`);
      if (!hit) return false;
      hit.dispatchEvent(new MouseEvent('click', { bubbles: true, cancelable: true }));
      return true;
    }, constraint);
    if (!opened) throw new Error(`关系组 ${constraint} 的连线命中区域不存在`);

    await browser.waitUntil(
      () => browser.execute(() => !!document.querySelector('[data-testid="qb-join-confirm"]')),
      { timeout: 5000, timeoutMsg: `关系组 ${constraint} 的浮层未打开` },
    );
    await browser.execute(() => {
      document.querySelector<HTMLElement>('[data-testid="qb-join-confirm"]')?.click();
    });
    await browser.pause(200);
  }

  return browser.execute(() => ((window as any).__qbStore.getState().joins ?? []).length);
}

/** Wait until auto-detected FK candidates have been computed. */
export async function waitForAutoJoins(minCount = 1, timeout = 15000): Promise<number> {
  let count = 0;
  await browser.waitUntil(
    async () => {
      count = await browser.execute(() => {
        const state = (window as any).__qbStore.getState();
        return (state.autoJoins ?? []).length;
      });
      return count >= minCount;
    },
    { timeout, timeoutMsg: `未检测到外键候选（期望 ≥ ${minCount}）` },
  );
  return count;
}

// ── Condition chips + dialog (WHERE / HAVING) ─────────────────────

/**
 * Every clause item is a chip: clicking it opens that item's dialog. These
 * helpers drive that dialog, which replaced the old inline per-row controls.
 */

/**
 * Click a control inside the Build tab's scroll container.
 *
 * The lower clauses (HAVING, ORDER BY) sit below the fold at the default
 * splitter height, and WebKit reports a scrolled-out element as not
 * interactable — so scroll it into view first, then click.
 */
export async function clickInBuildTab(testId: string): Promise<void> {
  await browser.execute((id: string) => {
    document
      .querySelector<HTMLElement>(`[data-testid="${id}"]`)
      ?.scrollIntoView({ block: 'center' });
  }, testId);
  await browser.pause(200);
  const el = await $(`[data-testid="${testId}"]`);
  await el.waitForClickable({ timeout: 5000 });
  await el.click();
  await browser.pause(200);
}

/** True while a condition dialog is open. */
export async function conditionDialogOpen(): Promise<boolean> {
  return existsInDom('[data-testid="qb-cond-apply"]');
}

async function waitForConditionDialog(): Promise<void> {
  await browser.waitUntil(() => conditionDialogOpen(), {
    timeout: 5000,
    timeoutMsg: '条件弹窗未打开',
  });
  await browser.pause(200);
}

/** Click a condition chip (document order) to edit it. */
export async function openConditionChip(clause: 'where' | 'having', index: number): Promise<void> {
  const clicked = await browser.execute(
    (c: string, i: number) => {
      const chips = Array.from(
        document.querySelectorAll<HTMLElement>(`[data-testid^="qb-${c}-chip-"]`),
      );
      const chip = chips[i];
      // The options trigger carries no testid of its own; the × does.
      const trigger = chip?.querySelector<HTMLElement>('button:not([data-testid])');
      if (!trigger) return false;
      trigger.click();
      return true;
    },
    clause,
    index,
  );
  if (!clicked) throw new Error(`${clause} 条件 chip #${index} 不存在或不可点击`);
  await waitForConditionDialog();
}

/** Patch the open condition dialog. Any field left undefined is not touched. */
export interface ConditionPatch {
  /**
   * Aggregate by position — the option labels are translated, so the index is
   * the only language-independent handle: 0 = none, 1 = COUNT, 2 = SUM, 3 = AVG,
   * 4 = MIN, 5 = MAX.
   */
  aggregateIndex?: number;
  field?: string;
  operator?: string;
  value?: string;
  conjunction?: string;
}

export async function patchConditionDialog(patch: ConditionPatch): Promise<void> {
  if (patch.aggregateIndex !== undefined) {
    await pickSelectOptionByIndex('qb-cond-aggregate', patch.aggregateIndex);
  }
  if (patch.field !== undefined) {
    await pickSelectOption('qb-cond-field', patch.field);
  }
  if (patch.operator !== undefined) {
    await pickSelectOption('qb-cond-operator', patch.operator);
  }
  if (patch.conjunction !== undefined) {
    await pickSelectOption('qb-cond-conjunction', patch.conjunction);
  }
  if (patch.value !== undefined) {
    const input = await $('[data-testid="qb-cond-value"]');
    await input.waitForDisplayed({ timeout: 5000 });
    await input.click();
    await input.setValue(patch.value);
    await browser.pause(150);
  }
}

/** Confirm the condition dialog. A new condition only lands here. */
export async function applyConditionDialog(): Promise<void> {
  const btn = await $('[data-testid="qb-cond-apply"]');
  await btn.waitForClickable({ timeout: 5000 });
  await btn.click();
  await browser.pause(300);
}

/** Dismiss the condition dialog without writing anything. */
export async function cancelConditionDialog(): Promise<void> {
  const btn = await $('[data-testid="qb-cond-cancel"]');
  await btn.waitForClickable({ timeout: 5000 });
  await btn.click();
  await browser.pause(250);
}

/**
 * Open the WHERE "add condition" affordance.
 *
 * This only opens the dialog — nothing is in the query until
 * `applyConditionDialog()`. A draft cannot leave a half-filled condition behind.
 */
export async function addWhereCondition(): Promise<void> {
  await clickInBuildTab('qb-where-add-condition');
  await waitForConditionDialog();
}

/** Same for HAVING. */
export async function addHavingCondition(): Promise<void> {
  await clickInBuildTab('qb-having-add-condition');
  await waitForConditionDialog();
}

export async function addWhereGroup(): Promise<void> {
  await clickInBuildTab('qb-where-add-group');
  await browser.pause(250);
}

export async function addHavingGroup(): Promise<void> {
  await clickInBuildTab('qb-having-add-group');
  await browser.pause(250);
}

/** Add a condition directly inside the first nested group. */
export async function addSubGroupCondition(): Promise<void> {
  await clickInBuildTab('qb-where-subgroup-add-condition');
  await waitForConditionDialog();
}

/**
 * Configure the WHERE condition at `index` (document order) and confirm it.
 *
 * If a dialog is already open — the usual case right after `addWhereCondition()`
 * — that draft is patched instead of opening another one.
 */
export async function configureWhereRow(index: number, patch: ConditionPatch): Promise<void> {
  if (!(await conditionDialogOpen())) await openConditionChip('where', index);
  await patchConditionDialog(patch);
  await applyConditionDialog();
}

/** Same for HAVING. `aggregate: '—'` picks "no aggregate". */
export async function configureHavingRow(index: number, patch: ConditionPatch): Promise<void> {
  if (!(await conditionDialogOpen())) await openConditionChip('having', index);
  await patchConditionDialog(patch);
  await applyConditionDialog();
}

// ── SQL editor ─────────────────────────────────────────────────────

/** Text currently in the SQL editor (CodeMirror content). */
export async function readEditorSql(): Promise<string> {
  return browser.execute(() => {
    const host = document.querySelector('[data-testid="sql-editor-content"]');
    if (!host) return '';
    const content = host.querySelector('.cm-content') ?? host;
    return (content.textContent ?? '').replace(/\u00a0/g, ' ').trim();
  });
}

/** Replace the whole editor document with `sql`. */
export async function setEditorSql(sql: string): Promise<void> {
  const editor = await $('[data-testid="sql-editor-content"]');
  await editor.waitForDisplayed({ timeout: 10000 });
  await editor.click();
  await browser.pause(150);
  await browser.execute((text: string) => {
    const host = document.querySelector('[data-testid="sql-editor-content"]') as HTMLElement | null;
    if (!host) return;
    host.focus();
    const sel = window.getSelection();
    if (sel) {
      sel.selectAllChildren(host);
      sel.deleteFromDocument();
    }
    document.execCommand('insertText', false, text);
  }, sql);
  await browser.pause(300);
}

export async function clearEditorSql(): Promise<void> {
  await setEditorSql('');
}

// ── Test data / journey setup ──────────────────────────────────────

export interface QbJourneySetup {
  /** Window handle to keep alive across the spec (`closeExtraWindows`). */
  mainWindow: string;
  connectionId: string;
  connectionName: string;
  /** Table → column names, preloaded into the schema store for the canvas. */
  tables: Record<string, string[]>;
}

/**
 * Bring up an isolated connection + query tab for one QB journey and run the
 * seed statements through the live panel session.
 *
 * Self-contained on purpose: the runner recreates the worker database between
 * spec files, so a spec that reuses the shared `conn_e2e_pg` config can be
 * handed a session pointing at a database the previous spec just dropped.
 * A disposable connection with an explicit database avoids that entirely
 * (same pattern as `query-row-limit-journey.ts`).
 */
export async function setupQbJourney(options: {
  connectionId: string;
  connectionName: string;
  /** DDL / INSERT statements, executed in order after the tab is open. */
  seedStatements: string[];
  /** table → columns, mirrored into the schema store so cards render at once. */
  tables: Record<string, string[]>;
  captureLabel?: string;
}): Promise<QbJourneySetup> {
  const mainWindow = await browser.getWindowHandle();

  await invokeBackend('save_connection', {
    config: {
      id: options.connectionId,
      name: options.connectionName,
      databaseType: 'postgresql',
      host: process.env.E2E_PG_HOST || '127.0.0.1',
      port: Number(process.env.E2E_PG_PORT) || 5432,
      username: process.env.E2E_PG_USER || 'postgres',
      password: process.env.E2E_PG_PASSWORD || '',
      database: process.env.E2E_PG_DB || 'postgres',
      schema: process.env.E2E_WORKER_SCHEMA || undefined,
      group: 'E2E 测试',
      colorTag: 'blue',
      sslMode: 'disable',
      options: {},
    },
  });
  await browser.refresh();
  await browser.pause(1500);

  // The canvas/tab splitter persists its height in localStorage, which survives
  // the app-data reset — a previous spec's drag would then decide this spec's
  // layout and make geometry assertions depend on execution order. Drop it so
  // every journey starts from the shipped default.
  await browser.execute(() => {
    try {
      localStorage.removeItem('resize:qb-split-height');
    } catch {
      /* storage unavailable — nothing to clear */
    }
  });

  await openConnectionsWorkspace();
  await clickCardConnectButton(options.connectionName);
  await waitForConnectionToolbar();
  await expandConnectedConnectionInNavigator(options.connectionName);
  await browser.pause(800);

  await openQueryTab();

  for (const statement of options.seedStatements) {
    await executeSQL(statement);
  }
  await browser.pause(500);

  // Refresh the tree so the seeded tables are reachable from the navigator.
  const refresh = await $('[data-testid="navigator-refresh"]');
  if (await refresh.isExisting()) {
    await refresh.click().catch(() => {});
    await browser.pause(800);
  }

  for (const [table, columns] of Object.entries(options.tables)) {
    await seedSchemaColumns(table, columns);
  }

  await resetQbStore();
  if (options.captureLabel) await captureJourneyStep(options.captureLabel);

  return {
    mainWindow,
    connectionId: options.connectionId,
    connectionName: options.connectionName,
    tables: options.tables,
  };
}

/** Close the extra windows and drop the journey's disposable connection. */
export async function teardownQbJourney(setup: QbJourneySetup): Promise<void> {
  try {
    await closeExtraWindows(setup.mainWindow);
  } catch {
    /* window already gone */
  }
  try {
    await invokeBackend('delete_connection', { id: setup.connectionId });
  } catch {
    /* best effort — the runner also cleans app data */
  }
}

// ── Assertions ─────────────────────────────────────────────────────

/**
 * Assert the generated SQL contains every fragment.
 *
 * Fragments are compared with ALL whitespace removed, because the preview and
 * the committed statement are pretty-printed: `LIMIT 50` is legitimately
 * rendered as `LIMIT\n  50`. The assertion is about the token sequence, not
 * about where the formatter chose to break lines.
 */
export function expectSqlFragments(sql: string, fragments: string[]): void {
  const compact = sql.toUpperCase().replace(/\s+/g, '');
  for (const fragment of fragments) {
    expect(compact).toContain(fragment.toUpperCase().replace(/\s+/g, ''));
  }
}

/** Assert the generated SQL contains none of the fragments (whitespace-insensitive). */
export function expectNoSqlFragments(sql: string, fragments: string[]): void {
  const compact = sql.toUpperCase().replace(/\s+/g, '');
  for (const fragment of fragments) {
    expect(compact).not.toContain(fragment.toUpperCase().replace(/\s+/g, ''));
  }
}

// ── Statement clause list (Navicat-style Build tab) ────────────────

/** Click a SELECT field chip to open its options dialog. */
export async function openColumnOptions(table: string, column: string): Promise<void> {
  const clicked = await browser.execute(
    (t: string, c: string) => {
      const chip = document.querySelector<HTMLElement>(`[data-testid="qb-field-chip-${t}-${c}"]`);
      const trigger = chip?.querySelector('button') ?? chip;
      if (!trigger) return false;
      trigger.click();
      return true;
    },
    table,
    column,
  );
  if (!clicked) throw new Error(`字段 chip ${table}.${column} 不存在`);
  await browser.waitUntil(
    () => browser.execute(() => !!document.querySelector('[data-testid="qb-col-opt-apply"]')),
    { timeout: 5000, timeoutMsg: '字段选项弹窗未打开' },
  );
  await browser.pause(200);
}

/** Remove a field from SELECT through its chip's × . */
export async function removeFieldChip(table: string, column: string): Promise<void> {
  const clicked = await browser.execute(
    (t: string, c: string) => {
      const btn = document.querySelector<HTMLElement>(`[data-testid="qb-field-remove-${t}-${c}"]`);
      if (!btn) return false;
      btn.click();
      return true;
    },
    table,
    column,
  );
  if (!clicked) throw new Error(`字段 chip ${table}.${column} 的移除按钮不存在`);
  await browser.pause(250);
}

/** True while the column options dialog is open. */
export async function columnOptionsOpen(): Promise<boolean> {
  return existsInDom('[data-testid="qb-col-opt-apply"]');
}

/** Apply the column options dialog. */
export async function applyColumnOptions(): Promise<void> {
  const btn = await $('[data-testid="qb-col-opt-apply"]');
  await btn.waitForClickable({ timeout: 5000 });
  await btn.click();
  await browser.pause(300);
}

/** Cancel the column options dialog. */
export async function cancelColumnOptions(): Promise<void> {
  const btn = await $('[data-testid="qb-col-opt-cancel"]');
  await btn.waitForClickable({ timeout: 5000 });
  await btn.click();
  await browser.pause(250);
}

/** Pick an option from one of the clause-level `<Click here to add …>` links. */
export async function addClauseItem(linkTestId: string, optionText: string): Promise<void> {
  await pickSelectOption(linkTestId, optionText);
}

/**
 * Pick a Select option by position.
 *
 * Used where the option labels are translated (`Ascending` / `降序`): matching
 * text would make the journey depend on the UI language.
 */
export async function pickSelectOptionByIndex(triggerTestId: string, index: number): Promise<void> {
  await browser.execute((id: string) => {
    const el =
      document.querySelector<HTMLElement>(`[data-testid="${id}"] button`) ??
      document.querySelector<HTMLElement>(`[data-testid="${id}"] [role="combobox"]`) ??
      document.querySelector<HTMLElement>(`[data-testid="${id}"]`);
    el?.click();
  }, triggerTestId);

  await browser.waitUntil(
    () => browser.execute(() => document.querySelector('[data-testid="select-listbox"]') !== null),
    { timeout: 5000, timeoutMsg: `Select "${triggerTestId}" 未打开` },
  );

  const picked = await browser.execute((i: number) => {
    const list = document.querySelector('[data-testid="select-listbox"]');
    if (!list) return false;
    const options = Array.from(list.querySelectorAll<HTMLElement>('[data-testid="select-option"]'));
    const hit = options[i];
    if (!hit) return false;
    hit.dispatchEvent(new MouseEvent('mousedown', { bubbles: true, cancelable: true }));
    return true;
  }, index);

  if (!picked) throw new Error(`Select "${triggerTestId}" 没有第 ${index} 个选项`);
  await browser.pause(250);
}

/** Click an ORDER BY chip to open its sort options dialog. */
export async function openOrderByChip(table: string, column: string): Promise<void> {
  const clicked = await browser.execute(
    (t: string, c: string) => {
      const chip = document.querySelector<HTMLElement>(`[data-testid="qb-order-chip-${t}-${c}"]`);
      const trigger = chip?.querySelector<HTMLElement>('button:not([data-testid])');
      if (!trigger) return false;
      trigger.click();
      return true;
    },
    table,
    column,
  );
  if (!clicked) throw new Error(`ORDER BY chip ${table}.${column} 不存在或不可点击`);
  await browser.waitUntil(() => existsInDom('[data-testid="qb-sort-opt-apply"]'), {
    timeout: 5000,
    timeoutMsg: '排序选项弹窗未打开',
  });
  await browser.pause(200);
}

/** Set the direction in the open sort dialog and confirm it. */
export async function setSortDirection(direction: 'ASC' | 'DESC'): Promise<void> {
  // 0 = ASC, 1 = DESC — by position, because the labels are translated.
  await pickSelectOptionByIndex('qb-sort-opt-direction', direction === 'ASC' ? 0 : 1);
  const btn = await $('[data-testid="qb-sort-opt-apply"]');
  await btn.waitForClickable({ timeout: 5000 });
  await btn.click();
  await browser.pause(300);
}

/**
 * Is the clause row fully rendered inside the Build tab's scroll viewport?
 *
 * This is the regression guard for "after picking columns there is no room left
 * for WHERE and the clauses below it" — a row that exists but sits below the
 * fold is exactly the bug being tested.
 */
export async function clauseIsVisible(testId: string): Promise<boolean> {
  return browser.execute((id: string) => {
    const el = document.querySelector<HTMLElement>(`[data-testid="${id}"]`);
    const region = document.querySelector<HTMLElement>('[data-testid="qb-build-scroll"]');
    if (!el || !region) return false;
    const a = el.getBoundingClientRect();
    const b = region.getBoundingClientRect();
    return a.height > 0 && a.top >= b.top - 1 && a.bottom <= b.bottom + 1;
  }, testId);
}

/** Which clause rows exist in the DOM at all. */
export async function clauseRowsPresent(): Promise<string[]> {
  return browser.execute(() => {
    const ids = ['select', 'from', 'where', 'group-by', 'having', 'order-by'];
    return ids
      .filter((id) => !!document.querySelector(`[data-testid="qb-clause-${id}"]`))
      .map((id) => `qb-clause-${id}`);
  });
}

// ── Navigator integration ──────────────────────────────────────────

/**
 * Click a table row in the host `ConnectionNavigatorTree`.
 *
 * Clicking must always open the table's data view — including while the builder
 * is open. Tables only enter the builder by drag & drop, so this helper asserts
 * the opposite of the old (removed) click-to-add behaviour.
 */
export async function clickNavigatorTable(table: string): Promise<void> {
  const aside = await $('[data-testid="connection-navigator-aside"]');
  await aside.waitForDisplayed({ timeout: 10000 });

  const search = await aside.$('[data-testid="connection-search-input"]');
  if (await search.isExisting()) {
    await search.click();
    await search.setValue(table);
    await browser.pause(700);
  }

  await browser.waitUntil(
    () =>
      browser.execute((t: string) => {
        const nav = document.querySelector('[data-testid="connection-navigator-aside"]');
        if (!nav) return false;
        const node = Array.from(
          nav.querySelectorAll<HTMLElement>(
            '[data-testid="schema-tree-node"][data-tree-node="table"], ' +
              '[data-testid="schema-tree-node"][data-tree-node="view"]',
          ),
        ).find((n) => (n.getAttribute('data-item-name') ?? '') === t);
        if (!node) return false;
        node.scrollIntoView({ block: 'center' });
        return true;
      }, table),
    { timeout: 10000, timeoutMsg: `导航树中未找到表 ${table}` },
  );

  const node = await $(
    `[data-testid="schema-tree-node"][data-item-name="${table}"][data-tree-node="table"]`,
  );
  await node.waitForDisplayed({ timeout: 5000 });
  await node.click();
  await browser.pause(600);
}

/** True when the table's data workspace is the active panel. */
export async function tableDataViewIsOpen(table: string): Promise<boolean> {
  return browser.execute((t: string) => {
    const activeTab = document.querySelector<HTMLElement>(
      '[data-testid="panel-tab"] button[role="tab"][aria-selected="true"]',
    );
    const label = activeTab?.getAttribute('aria-label') ?? activeTab?.textContent ?? '';
    const dataTab = document.querySelector<HTMLElement>('[data-testid="sub-tab-data"]');
    return label.includes(t) && !!dataTab && dataTab.offsetParent !== null;
  }, table);
}

/** Close every open query panel tab. */
export async function closeAllQueryPanels(): Promise<void> {
  await browser.execute(() => {
    const closers = Array.from(
      document.querySelectorAll<HTMLElement>('[data-testid="panel-tab-close"]'),
    );
    for (const closer of closers) closer.click();
  });
  await browser.pause(600);
}

/** Wait for the builder canvas to be blank. */
export async function waitForEmptyCanvas(timeout = 8000): Promise<void> {
  await browser.waitUntil(
    () =>
      browser.execute(
        () => ((window as any).__qbStore.getState().selectedTables ?? []).length === 0,
      ),
    { timeout, timeoutMsg: '构建器画布未清空' },
  );
}

/** The query panel's execution counter — increments once per execution. */
export async function readExecutionSeq(): Promise<string | null> {
  return browser.execute(
    () =>
      document.querySelector('[data-testid="query-panel"]')?.getAttribute('data-execution-seq') ??
      null,
  );
}

/** Measure an element's rendered height. */
export async function heightOf(testId: string): Promise<number> {
  return browser.execute(
    (id: string) =>
      document.querySelector(`[data-testid="${id}"]`)?.getBoundingClientRect().height ?? 0,
    testId,
  );
}

/** Is the element present in the DOM at all? */
export async function existsInDom(selector: string): Promise<boolean> {
  return browser.execute((sel: string) => !!document.querySelector(sel), selector);
}

/**
 * Drag the builder splitter by `deltaY` px (negative = grow the bottom region).
 *
 * Two strategies, because the handle relies on pointer capture:
 *  1. real WebDriver pointer input (preferred — exercises the genuine path);
 *  2. an in-page synthetic pointer sequence with the capture calls stubbed,
 *     used only when WebKit refuses to associate the synthesised pointer with
 *     the element (which makes `setPointerCapture` throw and aborts the drag).
 * The resize logic itself is identical in both cases, and the caller asserts
 * the observable outcome (a height change) either way.
 */
export async function dragSplitter(deltaY: number): Promise<void> {
  const el = await $('[data-testid="qb-splitter"]');
  await el.waitForDisplayed({ timeout: 5000 });

  const before = await heightOf('qb-bottom-region');

  const loc = await el.getLocation();
  const size = await el.getSize();
  const cx = Math.round(loc.x + size.width / 2);
  const cy = Math.round(loc.y + size.height / 2);

  await browser
    .action('pointer', { parameters: { pointerType: 'mouse' } })
    .move({ x: cx, y: cy })
    .down()
    .pause(80)
    .move({ x: cx, y: cy + deltaY, duration: 250 })
    .pause(80)
    .up()
    .perform();
  await browser.pause(300);

  if ((await heightOf('qb-bottom-region')) !== before) return;

  // Fallback: synthesise the pointer sequence in-page.
  await browser.execute((dy: number) => {
    const handle = document.querySelector<HTMLElement>('[data-testid="qb-splitter"]');
    if (!handle) return;

    // jsdom/WebKit reject capture for pointers that were never "active", and
    // `useResizable` bails out of its move handler without capture. Stubbing
    // the capture bookkeeping is what lets the synthetic sequence through.
    const anyHandle = handle as unknown as Record<string, unknown>;
    const originalSet = anyHandle.setPointerCapture;
    const originalHas = anyHandle.hasPointerCapture;
    const originalRelease = anyHandle.releasePointerCapture;
    anyHandle.setPointerCapture = () => {};
    anyHandle.hasPointerCapture = () => true;
    anyHandle.releasePointerCapture = () => {};

    const rect = handle.getBoundingClientRect();
    const x = rect.left + rect.width / 2;
    const y0 = rect.top + rect.height / 2;
    const make = (type: string, y: number) =>
      new PointerEvent(type, {
        bubbles: true,
        cancelable: true,
        pointerId: 1,
        isPrimary: true,
        pointerType: 'mouse',
        buttons: 1,
        clientX: x,
        clientY: y,
      });

    handle.dispatchEvent(make('pointerdown', y0));
    handle.dispatchEvent(make('pointermove', y0 + dy));
    handle.dispatchEvent(make('pointerup', y0 + dy));

    anyHandle.setPointerCapture = originalSet;
    anyHandle.hasPointerCapture = originalHas;
    anyHandle.releasePointerCapture = originalRelease;
  }, deltaY);

  await browser.pause(300);
}
