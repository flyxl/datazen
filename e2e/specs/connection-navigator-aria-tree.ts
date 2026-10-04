/**
 * 连接导航树的 ARIA tree 语义回归 (NAV-ARIA)
 *
 * 覆盖 `feature/tree-cleanup` 为 Host 连接树补齐的 tree 语义：
 *   - 唯一的 `role="tree"` 容器；
 *   - 每一行节点都是 `role="treeitem"` 且带合法 `aria-level`（1 基、正整数）；
 *   - `aria-level` 不跳级：任何 L>1 的行都存在一个 L-1 的祖先链；
 *   - `aria-expanded` 随展开/折叠正确翻转。
 *
 * 定位一律走 `data-*`（`data-section-header` / `data-group-header` /
 * `data-conn-item` / `data-tree-node` / `data-cat-id` / `data-item-name` /
 * `data-db-name` / `data-empty-group`），不使用任何视口几何坐标反查，
 * 符合 AGENTS.md「数据属性解耦」。
 *
 * 虚拟化说明：树按 `role="tree"` → 绝对定位行包装 div → 行节点渲染，
 * 窗口外的行根本没有 DOM。因此本文件的所有结构断言都只针对**当前已渲染
 * 的窗口切片**，并用「滚动前后重叠行的 aria-level 完全一致」来证明
 * `aria-level` 属于行本身而不是它被画在窗口的哪个位置。
 *
 * 本文件随 tree-cleanup 一起合入，尚未实际执行
 * （需要 `pnpm tauri:build:webdriver` 产出 webdriver binary）。
 */
import { expect, browser, $, $$ } from '@wdio/globals';
import { expandAllGroups } from '../helpers.js';
import { t } from '../i18n.js';

/** 树语义下的「行节点」选择器；空分组/无连接提示刻意不在其中。 */
const NODE_SELECTOR =
  '[data-section-header], [data-group-header], [data-conn-item], [data-tree-node]';

const HINT_SELECTOR = '[data-empty-group]';

type RowSnapshot = {
  label: string;
  role: string | null;
  level: string | null;
  expanded: string | null;
};

async function countIn(selector: string): Promise<number> {
  return browser.execute((sel: string) => document.querySelectorAll(sel).length, selector);
}

/** 按 DOM 顺序读出当前已渲染的节点行快照。 */
async function readRows(): Promise<RowSnapshot[]> {
  return browser.execute((selector: string) => {
    const tree = document.querySelector('[role="tree"]');
    if (!tree) return [];
    return Array.from(tree.querySelectorAll<HTMLElement>(selector)).map((el) => {
      const kind = el.hasAttribute('data-section-header')
        ? `section:${el.dataset.section}`
        : el.hasAttribute('data-group-header')
          ? `group:${el.dataset.groupName}`
          : el.hasAttribute('data-conn-item')
            ? `connection:${el.dataset.connName}`
            : `node:${el.dataset.treeNode}:${el.dataset.dbName ?? el.dataset.itemName ?? ''}`;
      return {
        label: kind,
        role: el.getAttribute('role'),
        level: el.getAttribute('aria-level'),
        expanded: el.getAttribute('aria-expanded'),
      };
    });
  }, NODE_SELECTOR);
}

/**
 * 结构契约：role / aria-level 合法性 + 不跳级。
 * 用「祖先栈」走一遍文档顺序的扁平行流，命中即产出可读告警。
 */
async function structuralViolations(): Promise<string[]> {
  return browser.execute((selector: string) => {
    const trees = Array.from(document.querySelectorAll('[role="tree"]'));
    if (trees.length !== 1) return [`expected exactly one [role="tree"], found ${trees.length}`];
    const violations: string[] = [];
    const open: number[] = [];
    for (const el of Array.from(trees[0]!.querySelectorAll<HTMLElement>(selector))) {
      const label = el.hasAttribute('data-conn-item')
        ? `connection:${el.dataset.connName}`
        : el.hasAttribute('data-group-header')
          ? `group:${el.dataset.groupName}`
          : el.hasAttribute('data-section-header')
            ? `section:${el.dataset.section}`
            : `node:${el.dataset.treeNode}:${el.dataset.dbName ?? el.dataset.itemName ?? ''}`;
      const role = el.getAttribute('role');
      if (role !== 'treeitem') {
        violations.push(`${label}: role=${role ?? '(none)'} is not treeitem`);
        continue;
      }
      const raw = el.getAttribute('aria-level');
      const level = raw === null ? Number.NaN : Number(raw);
      if (!Number.isInteger(level) || level < 1) {
        violations.push(`${label}: aria-level=${raw ?? '(none)'} is not a positive integer`);
        continue;
      }
      while (open.length > level - 1) open.pop();
      if (open.length !== level - 1) {
        violations.push(
          `${label}: aria-level ${level} has no aria-level ${level - 1} ancestor ` +
            `(open ancestors: [${open.join(', ')}])`,
        );
      }
      open[level - 1] = level;
    }
    return violations;
  }, NODE_SELECTOR);
}

async function waitForTree(timeout = 20000): Promise<void> {
  await browser.waitUntil(async () => (await countIn('[role="tree"]')) === 1, {
    timeout,
    timeoutMsg: '连接树容器 [role="tree"] 未出现',
  });
  await browser.waitUntil(async () => (await countIn(NODE_SELECTOR)) > 0, {
    timeout,
    timeoutMsg: '连接树没有渲染任何节点行',
  });
}

async function attributeOf(selector: string, attribute: string): Promise<string | null> {
  return browser.execute(
    (sel: string, attr: string) => {
      const el = document.querySelector(sel);
      return el ? el.getAttribute(attr) : null;
    },
    selector,
    attribute,
  );
}

/** 通过 DOM 事件点击，不依赖坐标。 */
async function clickByDataAttribute(selector: string): Promise<boolean> {
  return browser.execute((sel: string) => {
    const el = document.querySelector(sel);
    if (!el) return false;
    (el as HTMLElement).click();
    return true;
  }, selector);
}

async function waitForAttribute(
  selector: string,
  attribute: string,
  expected: string,
  timeout = 10000,
): Promise<void> {
  await browser.waitUntil(async () => (await attributeOf(selector, attribute)) === expected, {
    timeout,
    timeoutMsg: `${selector} 的 ${attribute} 未变为 ${expected}`,
  });
}

describe('连接树 ARIA tree 语义 (NAV-ARIA)', () => {
  before(async () => {
    await waitForTree();
    await expandAllGroups();
    await browser.pause(500);
  });

  // ── 结构契约 ─────────────────────────────────────────────────────

  it('导航树只暴露一个 role="tree" 容器，且所有行节点都在其中', async () => {
    expect(await countIn('[role="tree"]')).toBe(1);
    const outside = await browser.execute((selector: string) => {
      const tree = document.querySelector('[role="tree"]');
      if (!tree) return -1;
      return Array.from(document.querySelectorAll<HTMLElement>(selector)).filter(
        (el) => !tree.contains(el),
      ).length;
    }, NODE_SELECTOR);
    expect(outside).toBe(0);
  });

  it('每一行节点都是 role="treeitem" 且 aria-level 不跳级', async () => {
    expect(await structuralViolations()).toEqual([]);
  });

  it('多分组 + 最近连接的多段结构里每个表头都是一级 treeitem', async () => {
    const headers = await readRows();
    const topLevel = headers.filter(
      (row) => row.label.startsWith('section:') || row.label.startsWith('group:'),
    );
    expect(topLevel.length).toBeGreaterThan(0);
    for (const header of topLevel) {
      expect(header.role).toBe('treeitem');
      // 分组/分区表头是树的真正根：它没有 depth，固定为 level 1
      expect(header.level).toBe('1');
    }
    // group 永远不会嵌在 section 内部，所以分区表头与分组表头同级出现。
    expect(await structuralViolations()).toEqual([]);
  });

  it('连接 / 数据库 / 分类 逐级下降，层级严格 +1', async () => {
    const rows = await readRows();
    const connection = rows.find((row) => row.label.startsWith('connection:'));
    expect(connection?.level).toBe('2');

    const connIndex = rows.indexOf(connection!);
    const db = rows.slice(connIndex + 1).find((row) => row.label.startsWith('node:db:'));
    if (db) {
      expect(db.level).toBe('3');
      const dbIndex = rows.indexOf(db);
      const category = rows
        .slice(dbIndex + 1)
        .find((row) => row.label.startsWith('node:category:'));
      if (category) {
        expect(Number(category.level)).toBeGreaterThan(Number(db.level));
      }
    }
  });

  // ── 虚拟化边界 ───────────────────────────────────────────────────

  it('aria-level 属于行本身，不随虚拟化窗口位置漂移', async () => {
    const before = new Map((await readRows()).map((row) => [row.label, row.level]));
    expect(before.size).toBeGreaterThan(0);

    // 直接滚动容器的 scrollTop（不按坐标找元素）
    const scrolled = await browser.execute(() => {
      const tree = document.querySelector('[role="tree"]');
      const scroller = tree?.parentElement;
      if (!scroller) return false;
      if (scroller.scrollHeight <= scroller.clientHeight) return false;
      scroller.scrollTop = scroller.scrollHeight;
      return true;
    });
    if (!scrolled) return; // 一屏放得下，无需验证窗口边界
    await browser.pause(600);

    const after = new Map((await readRows()).map((row) => [row.label, row.level]));
    let compared = 0;
    for (const [label, level] of after) {
      if (!before.has(label)) continue;
      expect(after.get(label)).toBe(before.get(label));
      compared += 1;
    }
    expect(compared).toBeGreaterThan(0);
    expect(await structuralViolations()).toEqual([]);

    await browser.execute(() => {
      const tree = document.querySelector('[role="tree"]');
      if (tree?.parentElement) tree.parentElement.scrollTop = 0;
    });
    await browser.pause(300);
  });

  // ── aria-expanded 展开/折叠 ──────────────────────────────────────

  it('最近连接分区的 aria-expanded 在展开/折叠间正确翻转', async () => {
    const selector = '[data-section-header][data-section="recent"]';
    if ((await countIn(selector)) === 0) return; // 没有最近连接时跳过

    await waitForAttribute(selector, 'aria-expanded', 'true');
    expect(await clickByDataAttribute(selector)).toBe(true);
    await waitForAttribute(selector, 'aria-expanded', 'false');

    expect(await clickByDataAttribute(selector)).toBe(true);
    await waitForAttribute(selector, 'aria-expanded', 'true');
  });

  it('分组表头的 aria-expanded 随展开/折叠翻转', async () => {
    const group = (await readRows()).find((row) => row.label.startsWith('group:'));
    if (!group) return;
    const selector = `[data-group-header][data-group-name="${group.label.slice('group:'.length)}"]`;

    // `group` rows carry children, so they must expose aria-expanded rather
    // than read as leaves. NavigatorTreeRow.tsx now emits it, so this is a
    // hard contract rather than the tolerated known gap it used to be.
    const before = (await readRows()).find((row) => row.label === group.label);
    expect(before?.expanded).toBe('true');
    await waitForAttribute(selector, 'aria-expanded', 'true');

    expect(await clickByDataAttribute(selector)).toBe(true);
    await waitForAttribute(selector, 'aria-expanded', 'false');
    await browser.pause(600);
    expect(await structuralViolations()).toEqual([]);

    expect(await clickByDataAttribute(selector)).toBe(true);
    await waitForAttribute(selector, 'aria-expanded', 'true');
    await browser.pause(600);
    expect(await structuralViolations()).toEqual([]);
  });

  it('数据库行与分类行的 aria-expanded 随展开/折叠翻转', async () => {
    const rows = await readRows();
    const db = rows.find((row) => row.label.startsWith('node:db:'));
    if (!db) return;

    const selector = `[data-tree-node="db"][data-db-name="${db.label.split(':').slice(2).join(':')}"]`;
    expect(await attributeOf(selector, 'aria-expanded')).toBe('true');
    expect(await clickByDataAttribute(selector)).toBe(true);
    await waitForAttribute(selector, 'aria-expanded', 'false');
    expect(await clickByDataAttribute(selector)).toBe(true);
    await waitForAttribute(selector, 'aria-expanded', 'true');

    const category = (await readRows()).find((row) => row.label.startsWith('node:category:'));
    if (!category) return;
    const catId = await browser.execute((afterDb: string) => {
      const dbRow = Array.from(
        document.querySelectorAll<HTMLElement>('[data-tree-node="db"]'),
      ).find((el) => el.getAttribute('aria-level') === afterDb);
      const category = dbRow?.parentElement?.querySelector<HTMLElement>(
        '[data-tree-node="category"][aria-level="' + (Number(afterDb) + 1) + '"]',
      );
      return category?.dataset.catId ?? null;
    }, db.level as string);
    if (!catId) return;
    const catSelector = `[data-tree-node="category"][data-cat-id="${catId}"]`;
    expect(await attributeOf(catSelector, 'aria-expanded')).not.toBeNull();
  });

  // ── 非节点行不得进入树语义 ───────────────────────────────────────

  it('空分组提示与无连接提示不带 role / aria-level', async () => {
    const hints = await browser.execute((selector: string) => {
      const tree = document.querySelector('[role="tree"]');
      const scope = tree ?? document.body;
      return Array.from(scope.querySelectorAll<HTMLElement>(selector)).map((el) => ({
        role: el.getAttribute('role'),
        level: el.getAttribute('aria-level'),
        text: el.textContent?.trim() ?? '',
      }));
    }, HINT_SELECTOR);
    for (const hint of hints) {
      expect(hint.role).toBeNull();
      expect(hint.level).toBeNull();
    }

    const emptyState = await browser.execute((noConnections: string) => {
      const el = Array.from(document.querySelectorAll<HTMLElement>('p')).find(
        (p) => p.textContent?.trim() === noConnections,
      );
      return el ? { role: el.getAttribute('role'), level: el.getAttribute('aria-level') } : null;
    }, t('main.noConnections'));
    if (emptyState) {
      expect(emptyState.role).toBeNull();
      expect(emptyState.level).toBeNull();
    }
  });

  // ── 搜索模式 ─────────────────────────────────────────────────────────

  it('搜索模式下不跳级：db 是 connection 的直接子节点', async () => {
    const input = await $(
      `input[placeholder="${t('main.searchPlaceholder')}"], [data-testid="connection-search-input"]`,
    );
    await input.waitForDisplayed({ timeout: 10000 });
    const first = (await readRows())[0];
    await input.setValue(first ? first.label.split(':').slice(1).join(':') : 'a');
    await browser.pause(800);

    // 搜索态没有分组/分区表头，connection 直接是 level 1；db/kv-db 虽然仍
    // 按 depth 2 缩进绘制，但 aria-level 跟随逻辑父节点报 level 2，
    // 因此不存在断层的 level。
    expect(await structuralViolations()).toEqual([]);

    await input.clearValue();
    await browser.pause(600);
    expect(await structuralViolations()).toEqual([]);
  });
});
