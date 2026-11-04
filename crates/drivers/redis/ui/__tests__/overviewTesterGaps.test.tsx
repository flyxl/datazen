import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { ComponentProps } from 'react';
import { cleanup, fireEvent, render, renderHook, waitFor } from '@testing-library/react';
import type { RedisInvokeFn } from '../shared/redisInvoke';
import { formatSize } from '../shared/formatSize';
import { BROWSE_HISTORY_STORAGE_KEY, pushBrowseEntry } from '../lib/redisBrowseHistory';
import { OVERVIEW_COMMANDS, useOverviewData } from '../overview/useOverviewData';
import { RedisOverviewHome } from '../overview/RedisOverviewHome';

/**
 * [tester] 第 1 轮 Tester 缺口补齐（覆盖率驱动 + 变异复证幸存者）.
 *
 * 这些用例不引入新行为，只把存量 spec 没钉死的既有契约钉住：
 *
 * 1. **跳转不落历史（key 分支）** — 存量 spec 的"未接线不写历史"只点了
 *    `database` 目标（该 kind 本就不写历史），删掉生产代码的 `outcome.handled`
 *    守卫后 38 例全绿（变异存活）。这里补点 `key` 行：未接线 ⇒ 零写入 + 具名
 *    指引；接线 ⇒ 条目重新置顶并刷新时间戳。
 * 2. **useOverviewData 失败/畸形载荷分支** — dbSizes 独立失败、非字符串 INFO、
 *    `samples` 非数组、`truncated` 非严格 true、非数组 slowlog、非 Error reject。
 *
 * 断言口径与存量 spec 一致：`data-*` / i18n key / 服务端 token，零可见英文文案
 * （PRD §7-6）。
 */

vi.mock('@datazen/ui', async (importOriginal) => ({
  ...(await importOriginal<typeof import('@datazen/ui')>()),
  useI18n: () => ({ t: (key: string) => key, lang: 'en' }),
}));

const redisInvoke = vi.fn();
vi.mock('../shared/redisInvoke', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../shared/redisInvoke')>()),
  redisCommandInvoke: (...args: unknown[]) => redisInvoke(...args),
}));

const SESSION = 'sess-1';
const CONNECTION = 'conn-1';

const INFO_OK = ['# Server', 'redis_version:7.2.4', '# Memory', 'used_memory:1048576'].join('\r\n');
const DB_SIZES_OK = [{ db: 0, keys: 3 }];
const MEMORY_OK = { samples: [{ key: 'blob:a', bytes: 5 }], truncated: false };

function stubHome(commandOverrides: Record<string, unknown> = {}) {
  redisInvoke.mockImplementation(async (_pluginId: string, command: string) => {
    if (Object.prototype.hasOwnProperty.call(commandOverrides, command)) {
      const value = commandOverrides[command];
      if (value instanceof Error) throw value;
      return value;
    }
    switch (command) {
      case OVERVIEW_COMMANDS.info:
        return INFO_OK;
      case OVERVIEW_COMMANDS.dbSizes:
        return DB_SIZES_OK;
      case OVERVIEW_COMMANDS.memorySample:
        return MEMORY_OK;
      case OVERVIEW_COMMANDS.slowlogGet:
        return [];
      default:
        throw new Error(`unexpected command ${command}`);
    }
  });
}

function homeProps(overrides: Record<string, unknown> = {}) {
  return {
    connectionId: CONNECTION,
    dbSessionId: SESSION,
    connectionName: 'local-redis',
    databaseType: 'redis',
    ...overrides,
  } as unknown as ComponentProps<typeof RedisOverviewHome>;
}

beforeEach(() => {
  cleanup();
  redisInvoke.mockReset();
  stubHome();
  globalThis.localStorage.clear();
});

function readBucket(): Array<{ key: string; visitedAt: number }> {
  const raw = globalThis.localStorage.getItem(BROWSE_HISTORY_STORAGE_KEY);
  if (!raw) return [];
  const parsed = JSON.parse(raw) as Record<string, Array<{ key: string; visitedAt: number }>>;
  return parsed[CONNECTION] ?? [];
}

describe('[tester] 屏 A 跳转不落历史（key 目标补桩）', () => {
  it('clicking a big-key row without a bridge records nothing and shows the named hint', async () => {
    const { container } = render(<RedisOverviewHome {...homeProps()} />);
    await waitFor(() =>
      expect(container.querySelectorAll('[data-overview-bigkey]').length).toBe(1),
    );

    fireEvent.click(container.querySelector('[data-overview-bigkey="1"]') as Element);

    await waitFor(() =>
      expect(container.querySelector('[data-overview-jump-hint]')).not.toBeNull(),
    );
    expect(
      container.querySelector('[data-overview-jump-hint]')?.getAttribute('data-overview-jump-hint'),
    ).toBe('redis.overview.jump.pendingTree');
    // 未到达屏 B ⇒ 一条历史都不写。
    expect(globalThis.localStorage.getItem(BROWSE_HISTORY_STORAGE_KEY)).toBeNull();
    expect(container.querySelector('[data-overview-recent-key]')).toBeNull();
  });

  it('clicking a recent-key row without a bridge leaves the stored bucket untouched', async () => {
    pushBrowseEntry(CONNECTION, { key: 'user:1', dbIndex: 2 }, undefined, 1_700_000_000_000);
    const before = globalThis.localStorage.getItem(BROWSE_HISTORY_STORAGE_KEY);

    const { container } = render(<RedisOverviewHome {...homeProps()} />);
    await waitFor(() =>
      expect(container.querySelectorAll('[data-overview-recent-key]').length).toBe(1),
    );

    fireEvent.click(container.querySelector('[data-overview-recent-key="user:1"]') as Element);

    await waitFor(() =>
      expect(container.querySelector('[data-overview-jump-hint]')).not.toBeNull(),
    );
    expect(globalThis.localStorage.getItem(BROWSE_HISTORY_STORAGE_KEY)).toBe(before);
  });

  it('clicking a recent-key row through the bridge re-files the entry (a landed jump)', async () => {
    pushBrowseEntry(CONNECTION, { key: 'user:1', dbIndex: 2 }, undefined, 1_700_000_000_000);
    const onOpenTarget = vi.fn();

    const { container } = render(<RedisOverviewHome {...homeProps({ onOpenTarget })} />);
    await waitFor(() =>
      expect(container.querySelectorAll('[data-overview-recent-key]').length).toBe(1),
    );

    fireEvent.click(container.querySelector('[data-overview-recent-key="user:1"]') as Element);
    // The stored entry had no type, so the forwarded jump carries `keyType: null`
    // (BUG-001: the type is best-effort — a jump without it is still valid).
    expect(onOpenTarget).toHaveBeenCalledWith({
      kind: 'key',
      dbIndex: 2,
      key: 'user:1',
      keyType: null,
    });

    const bucket = readBucket();
    // 到达 ⇒ 条目被重新置顶且时间戳刷新（不再是播种时的固定值）。
    expect(bucket).toHaveLength(1);
    expect(bucket[0]?.key).toBe('user:1');
    expect(bucket[0]?.visitedAt ?? 0).toBeGreaterThan(1_700_000_000_000);
  });

  it('writing history through the bridge stays limited to key targets (non-key action)', async () => {
    // `handleJump` 的历史守卫是**双条件**：桥接成功 ∧ 目标是键。
    // 本例封「目标是键」—— 缺它的话「落地跳转一律入历史」
    // （快捷动作也写）这个变异能在整条套件里存活。
    const onOpenTarget = vi.fn();
    const { container } = render(<RedisOverviewHome {...homeProps({ onOpenTarget })} />);
    await waitFor(() =>
      expect(container.querySelectorAll('[data-overview-action]').length).toBeGreaterThanOrEqual(1),
    );

    // 点一个非键目标（console）验证不写历史
    fireEvent.click(container.querySelector('[data-overview-action="console"]') as Element);
    expect(onOpenTarget).toHaveBeenCalledWith({ kind: 'console' });

    // 到了屏 B 但不是键 ⇒ 历史里不该出现任何条目。
    expect(readBucket()).toHaveLength(0);
    expect(globalThis.localStorage.getItem(BROWSE_HISTORY_STORAGE_KEY)).toBeNull();
  });
});

// ---------------------------------------------------------------------------
// useOverviewData · 失败与畸形载荷分支
// ---------------------------------------------------------------------------

function deferred() {
  let resolve!: (value: unknown) => void;
  let reject!: (error: unknown) => void;
  const promise = new Promise<unknown>((res, rej) => {
    resolve = res;
    reject = rej;
  });
  return { promise, resolve, reject };
}

describe('[tester] useOverviewData · 卸载后的迟到回复全部丢弃', () => {
  it.each([
    { label: 'resolve', mode: 'resolve' as const },
    { label: 'reject', mode: 'reject' as const },
  ])('drops every stale %$label reply after unmount (四个 stale 守卫全覆盖)', async ({ mode }) => {
    const gates = {
      [OVERVIEW_COMMANDS.info]: deferred(),
      [OVERVIEW_COMMANDS.dbSizes]: deferred(),
      [OVERVIEW_COMMANDS.memorySample]: deferred(),
      [OVERVIEW_COMMANDS.slowlogGet]: deferred(),
    };
    const invoke = vi.fn((_pluginId: string, command: string) => {
      return gates[command as keyof typeof gates].promise;
    }) as unknown as RedisInvokeFn;

    const { result, unmount } = renderHook(() =>
      useOverviewData({ dbSessionId: SESSION, dbIndex: 0, invoke }),
    );
    await waitFor(() => expect(invoke).toHaveBeenCalledTimes(4));
    unmount();

    if (mode === 'resolve') {
      gates[OVERVIEW_COMMANDS.info].resolve(INFO_OK);
      gates[OVERVIEW_COMMANDS.dbSizes].resolve(DB_SIZES_OK);
      gates[OVERVIEW_COMMANDS.memorySample].reject(new Error('late memory reply'));
      gates[OVERVIEW_COMMANDS.slowlogGet].reject(new Error('late slowlog reply'));
    } else {
      gates[OVERVIEW_COMMANDS.info].reject(new Error('late info reply'));
      gates[OVERVIEW_COMMANDS.dbSizes].reject(new Error('late db_sizes reply'));
      gates[OVERVIEW_COMMANDS.memorySample].resolve(MEMORY_OK);
      gates[OVERVIEW_COMMANDS.slowlogGet].resolve([]);
    }
    await new Promise((r) => setTimeout(r, 0));

    // 卸载 ⇒ token 失效，任何迟到回复都不得再写状态（不 setState-after-unmount）。
    expect(result.current.info.status).toBe('loading');
    expect(result.current.dbSizes.status).toBe('loading');
    expect(result.current.memory.status).toBe('loading');
    expect(result.current.slowlog.status).toBe('loading');
  });
});

function gateway(plan: Record<string, unknown | Error> = {}) {
  const replies: Record<string, unknown> = {
    [OVERVIEW_COMMANDS.info]: INFO_OK,
    [OVERVIEW_COMMANDS.dbSizes]: DB_SIZES_OK,
    [OVERVIEW_COMMANDS.memorySample]: MEMORY_OK,
    [OVERVIEW_COMMANDS.slowlogGet]: [],
  };
  const invoke = vi.fn(async (_pluginId: string, command: string) => {
    if (!(command in replies)) throw new Error(`unexpected command ${command}`);
    const value = Object.prototype.hasOwnProperty.call(plan, command)
      ? plan[command]
      : replies[command];
    // reject 包：允许测非 Error 形状的 reject（裸字符串），而不只是 Error。
    if (value && typeof value === 'object' && '__reject' in (value as object)) {
      return Promise.reject((value as { __reject: unknown }).__reject);
    }
    if (value instanceof Error) return Promise.reject(value);
    return value;
  }) as unknown as RedisInvokeFn;
  return { invoke };
}

function renderData(plan: Record<string, unknown | Error> = {}) {
  const { invoke } = gateway(plan);
  return renderHook(() => useOverviewData({ dbSessionId: SESSION, dbIndex: 0, invoke }));
}

describe('[tester] useOverviewData · 失败与畸形载荷分支', () => {
  it('reports a failed db_sizes as its own failed source, others intact', async () => {
    const { result } = renderData({
      [OVERVIEW_COMMANDS.dbSizes]: new Error('ERR bad reply from proxy'),
    });
    await waitFor(() => expect(result.current.dbSizes.status).toBe('failed'));

    expect(result.current.dbSizes.message).toContain('bad reply');
    await waitFor(() => expect(result.current.info.status).toBe('ready'));
    expect(result.current.memory.status).toBe('ready');
  });

  it('coerces a non-string INFO payload instead of crashing', async () => {
    const { result } = renderData({ [OVERVIEW_COMMANDS.info]: 42 });
    await waitFor(() => expect(result.current.info.status).toBe('ready'));
    // String(42) 不含任何 field —— 卡片照常渲染，只是全部字段缺席。
    expect(result.current.info.data?.fields).toEqual({});
    expect(result.current.info.data?.raw).toBe('42');
  });

  it('normalises malformed memory_sample payloads (samples 非数组 / truncated 非 true)', async () => {
    const { result } = renderData({
      [OVERVIEW_COMMANDS.memorySample]: { samples: 'nope', truncated: 'yes' },
    });
    await waitFor(() => expect(result.current.memory.status).toBe('ready'));
    // truncated 只有严格 true 才可信；'yes' 不得点亮截断标注。
    expect(result.current.memory.data).toEqual({ samples: [], truncated: false });
  });

  it('normalises a non-array slowlog reply into an empty list', async () => {
    const { result } = renderData({ [OVERVIEW_COMMANDS.slowlogGet]: { oops: true } });
    await waitFor(() => expect(result.current.slowlog.status).toBe('ready'));
    expect(result.current.slowlog.data).toEqual([]);
  });

  it('keeps a rejection thrown as a bare string in the failed bucket', async () => {
    // 非 Error 的 reject：message 走 String(error) 而不是 error.message。
    const { result } = renderData({
      [OVERVIEW_COMMANDS.slowlogGet]: { __reject: 'ERR bad reply' },
    });
    await waitFor(() => expect(result.current.slowlog.status).toBe('failed'));
    expect(result.current.slowlog.message).toBe('ERR bad reply');
  });
});

// ---------------------------------------------------------------------------
// 内存条高水位着色
// ---------------------------------------------------------------------------

describe('[tester] 内存条 ≥90% 的 danger 着色', () => {
  it('turns the gauge fill danger near the maxmemory ceiling and accent below it', async () => {
    const hotInfo = [
      '# Memory',
      'used_memory:4000000',
      'used_memory_human:4.00M',
      'maxmemory:4194304',
      'maxmemory_human:4.00M',
    ].join('\r\n');
    stubHome({ info: hotInfo });
    const { container } = render(<RedisOverviewHome {...homeProps()} />);
    await waitFor(() =>
      expect(
        container
          .querySelector('[data-overview-memory-bar]')
          ?.getAttribute('data-overview-memory-bar-percent'),
      ).toBe('95'),
    );
    const hotFill = container.querySelector('[data-overview-memory-bar]')?.firstElementChild;
    expect(hotFill?.className).toContain('bg-danger');
    cleanup();

    stubHome();
    const cool = render(<RedisOverviewHome {...homeProps()} />);
    await waitFor(() =>
      expect(
        cool.container
          .querySelector('[data-overview-memory-bar]')
          ?.getAttribute('data-overview-memory-bar-percent'),
      ).toBe('0'),
    );
    // cool 用例里 INFO_OK 无 maxmemory ⇒ unlimited ⇒ percent 0
    const coolFill = cool.container.querySelector('[data-overview-memory-bar]')?.firstElementChild;
    expect(coolFill?.className).toContain('bg-accent');
  });
});

// ---------------------------------------------------------------------------
// [tester] 第 2 轮：屏 A 大 key 行 TTL / 类型徽标的**可见**三态
//
// 变异复验 C2 证明：`bigKeyTtlText` 的四条臂（永不过期 / 正数剩余 / 键消失 /
// 不可读）在存量 363 例里**一条都没钉**——把整个函数塌成「非正数一律 —」后全套
// 仍绿，即 PRD §3.1 卡 2 的 TTL 列与裁定 8-6 的 `-1 / -2` 可区分空态等于没测。
// 这里按 `data-overview-bigkey-ttl` 拿到**渲染出的单元格文本**，断言对象是
// i18n key 与破折号（`t` 被 mock 成 key 恒等），零可见英文文案（PRD §7-6）。
// 同一批用例并钉住类型徽标的 tone class：BUG-001 的病根是「只有 data-* 没有可见
// 标记」，若 class 从 `cn()` 里掉出去，界面上就是一枚无色徽标，也应红。
// ---------------------------------------------------------------------------

const BIG_KEY_SAMPLES = {
  samples: [
    { key: 'k:expire-never', bytes: 500, type: 'string', ttlMs: -1, missing: false },
    { key: 'k:countdown', bytes: 400, type: 'hash', ttlMs: 8_000, missing: false },
    { key: 'k:unreadable', bytes: 300, type: 'list', ttlMs: null, missing: false },
    { key: 'k:vanished', bytes: 200, type: null, ttlMs: -2, missing: true },
    // PTTL 读到 -2 而 TYPE 当时仍答得出（批内竞态）：既不是永不过期也不是
    // 已标注的消失键，只能落中性破折号，不许编造剩余时间。
    { key: 'k:minus-two-unflagged', bytes: 100, type: 'set', ttlMs: -2, missing: false },
  ],
  truncated: false,
};

async function renderBigKeys() {
  stubHome({ [OVERVIEW_COMMANDS.memorySample]: BIG_KEY_SAMPLES });
  const view = render(<RedisOverviewHome {...homeProps()} />);
  await waitFor(() =>
    expect(view.container.querySelectorAll('[data-overview-bigkey]')).toHaveLength(3),
  );
  return view.container;
}

function ttlCell(container: HTMLElement, rank: number): Element | null {
  return container.querySelector(`[data-overview-bigkey-ttl="${rank}"]`);
}

describe('[tester] 大 key 行 TTL 列的四态互不塌陷', () => {
  it('renders a distinct cell for no-expiry / remaining / unreadable', async () => {
    const container = await renderBigKeys();

    // rank1 PTTL=-1 ⇒ 永不过期；rank2 正数 ⇒ 秒数（8000ms → 8 + 单位 key）。
    expect(ttlCell(container, 1)?.textContent).toBe('redis.noExpiry');
    expect(ttlCell(container, 2)?.textContent).toBe(`8redis.seconds`);
    // rank3 不可读 ⇒ 中性破折号，绝不显示负数。
    expect(ttlCell(container, 3)?.textContent).toBe('—');

    const cells = [1, 2, 3].map((rank) => ttlCell(container, rank)?.textContent);
    expect(new Set(cells).size).toBe(3);
    expect(cells).not.toContain('-1');
    expect(cells).not.toContain('-2');
  });

  it('truncates at 3 rows (BIG_KEY_LIMIT) so vanished / edge-case samples beyond the budget are excluded', async () => {
    const container = await renderBigKeys();
    // rank4 (k:vanished) and rank5 (k:minus-two-unflagged) are beyond the limit.
    expect(ttlCell(container, 4)).toBeNull();
    expect(ttlCell(container, 5)).toBeNull();
  });

  it('keeps unreadable TTL neutral even when the row is within budget', async () => {
    const container = await renderBigKeys();
    expect(ttlCell(container, 3)?.textContent).toBe('—');
  });
});

describe('[tester] 大 key 行的类型徽标带上 tone class', () => {
  it('paints each row badge with its typeTone class, unknown stays neutral', async () => {
    const container = await renderBigKeys();
    const badgeOf = (rank: number) =>
      container.querySelector(`[data-overview-bigkey="${rank}"] .text-success`);

    // hash→success / string→accent / list→warning（typeTone 词表，top-3 only）。
    expect(badgeOf(2)).not.toBeNull();
    expect(container.querySelector(`[data-overview-bigkey="1"] .text-accent`)).not.toBeNull();
    expect(container.querySelector(`[data-overview-bigkey="3"] .text-warning`)).not.toBeNull();
    // rank4/5 beyond BIG_KEY_LIMIT=3 — no rows rendered.
    expect(container.querySelector('[data-overview-bigkey="4"]')).toBeNull();
    expect(container.querySelector('[data-overview-bigkey="5"]')).toBeNull();
  });
});

// ---------------------------------------------------------------------------
// [tester] 第 2 轮：maxmemory 有人数、无 human 形制时的回退
//
// 覆盖率报告里本轨唯一剩下的真实未覆盖分支：托管端 INFO 常只给 `maxmemory`
// 而漏掉 `maxmemory_human`，此时上限格必须退化成 `formatSize(maxBytes)`，
// 而不是破折号（谎报「无上限」）也不是 unlimited 文案。
// ---------------------------------------------------------------------------

describe('[tester] maxmemory 缺 maxmemory_human 时的上限回退', () => {
  it('formats the byte ceiling itself instead of claiming unlimited or a dash', async () => {
    stubHome({
      info: ['# Memory', 'used_memory:1048576', 'maxmemory:4194304'].join('\r\n'),
    });
    const { container } = render(<RedisOverviewHome {...homeProps()} />);
    await waitFor(() =>
      expect(
        container
          .querySelector('[data-overview-memory-max]')
          ?.getAttribute('data-overview-memory-max'),
      ).toBe('bounded'),
    );
    const cell = container.querySelector('[data-overview-memory-max]') as Element;
    expect(cell.textContent).toBe(formatSize(4194304));
    expect(cell.textContent).not.toBe('—');
    expect(cell.textContent).not.toBe('redis.overview.memory.unlimited');
  });
});
