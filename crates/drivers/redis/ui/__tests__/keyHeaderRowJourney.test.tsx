/**
 * 键头行连续旅程（本轨 E-4，PRD §3.3 键头行 + 徽标行，裁定 8-4）。
 *
 * 覆盖「状态机」而非静态快照：
 * - 键头行进入：mono 键名 + 刷新分体按钮 + 复制 + 内联改名 + danger 删除；
 * - 自动刷新：选间隔 ⇒ 按节奏 tick；关 ⇒ 停；被拒（I-1 草稿守卫，E-5 接线）
 *   ⇒ 自动落回关（退出跃迁，不再每拍骚扰）；
 * - 内联改名：进入（输入框替换键名）→ 确认成功关闭 / 确认失败保持打开 / 取消退出；
 * - 删除：绑定确认框先过，取消则删除命令根本不发；
 * - 复制插入语句：由 detail 形状构造（stream 等无法忠实重建的形状直接隐藏）。
 *
 * 断言口径（§4-6 / 裁定 8-4）：定位 `data-testid`，状态 `data-*`，
 * 文案只断言 `data-i18n-key`；键名与剪贴板载荷是服务器数据，可以钉死。
 */
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { act, cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { create } from 'zustand';
import {
  bindConfirmDialog,
  bindConnectionStore,
  bindSettingsStore,
  type ConnectionBridgeState,
  type SettingsBridgeState,
} from '@datazen/driver-sdk';

const getKeyRaw = vi.fn();
const decodeValue = vi.fn();
vi.mock('../shared/redisInvoke', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../shared/redisInvoke')>()),
  invokeGetKeyRaw: (...args: unknown[]) => getKeyRaw(...args),
  invokeDecodeValue: (...args: unknown[]) => decodeValue(...args),
}));

const renameKey = vi.fn();
const deleteKey = vi.fn();
vi.mock('../value-editors/keyEditorsInvokes', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../value-editors/keyEditorsInvokes')>()),
  invokeRename: (...args: unknown[]) => renameKey(...args),
  invokeDeleteKey: (...args: unknown[]) => deleteKey(...args),
}));

import type { KeyDetail, ValueFrame } from '../shared/types';
import { KeyDetailEditor } from '../value-editors/KeyEditors';
import { KeyHeaderRow } from '../value-editors/KeyHeaderRow';
import { bytesToBase64 } from '../__testing__/bytes';

bindSettingsStore(
  create<SettingsBridgeState>(() => ({
    settings: { safeMode: false, editorFontFamily: '', driverSettings: {} },
  })),
);
bindConnectionStore(create<ConnectionBridgeState>(() => ({ connections: [] })));

// 删除确认：测试内可切换接受/拒绝，确认框节点为 null（不需要点第二个按钮）。
let confirmAnswer = true;
const confirmFn = vi.fn(async () => confirmAnswer);
bindConfirmDialog(() => [confirmFn, null]);

const RAW = bytesToBase64(new TextEncoder().encode('hello'));

function frame(over: Partial<ValueFrame> = {}): ValueFrame {
  return {
    key: 'user:1',
    keyType: 'string',
    ttl: -1,
    logicalLen: RAW.length,
    memBytes: 5,
    rawB64: RAW,
    truncated: false,
    ...over,
  };
}

function stringDetail(value = 'hello'): KeyDetail {
  return {
    key: 'user:1',
    keyType: 'string',
    ttl: -1,
    value,
  } as unknown as KeyDetail;
}

const writeText = vi.fn(async () => undefined);

function renderEditor(onRefresh = vi.fn(() => {})) {
  const onRenamed = vi.fn();
  const result = render(
    <KeyDetailEditor
      dbSessionId="sess-e4"
      dbIndex={0}
      detail={stringDetail()}
      modules={[]}
      onRefresh={onRefresh}
      onRenamed={onRenamed}
    />,
  );
  return { onRefresh, onRenamed, ...result };
}

function renderHeader(overrides: Partial<Parameters<typeof KeyHeaderRow>[0]> = {}) {
  return render(
    <KeyHeaderRow
      keyName="user:1"
      onRefresh={vi.fn(async () => true)}
      onRename={vi.fn(async () => true)}
      onDelete={vi.fn(async () => true)}
      insertStatement="SET user:1 hello"
      {...overrides}
    />,
  );
}

beforeEach(() => {
  getKeyRaw.mockResolvedValue(frame());
  decodeValue.mockResolvedValue({ ok: true, json: '{}' });
  confirmFn.mockClear();
  confirmAnswer = true;
  renameKey.mockReset().mockResolvedValue(undefined);
  deleteKey.mockReset().mockResolvedValue(undefined);
  writeText.mockClear();
  Object.defineProperty(navigator, 'clipboard', {
    value: { writeText },
    configurable: true,
    writable: true,
  });
});

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
  vi.useRealTimers();
});

// ============================================================================
// Journey 1: 键头行 + 徽标行整体落地（经 KeyDetailEditor 的真实装配）
// ============================================================================
describe('Journey: 键头行与徽标行装配', () => {
  it('renders the header row above a single merged badge row (type | size | TTL | truncated)', async () => {
    renderEditor();

    // 键头行：mono 键名（服务器数据可钉），初始自动刷新为关。
    const header = screen.getByTestId('redis-key-header');
    expect(header.getAttribute('data-refresh-interval-ms')).toBe('0');
    const name = screen.getByTestId('redis-header-key-name');
    expect(name.textContent).toBe('user:1');
    expect(name.getAttribute('title')).toBe('user:1');

    // 徽标行：类型 + 大小（i18n key 断言）+ TTL 胶囊住在同一容器里（合并成一行）。
    // 大小徽标依赖 `get_key_raw` 异步回填的 frame，先等它落地。
    const badges = screen.getByTestId('redis-key-badges');
    await waitFor(() => {
      expect(screen.getByTestId('redis-key-badge-size')).toBeTruthy();
    });
    expect(screen.getByTestId('redis-key-badge-type').getAttribute('data-key-type')).toBe('string');
    expect(screen.getByTestId('redis-key-badge-size').getAttribute('data-i18n-key')).toBe(
      'redis.detail.badge.size',
    );
    expect(badges.querySelector('[data-testid="redis-ttl-value"]')).not.toBeNull();

    // 旧的独立 rename 输入框（常驻第二输入框）已被键头行取代，只在进入改名时出现。
    expect(screen.queryByTestId('redis-header-rename-input')).toBeNull();
  });

  it('copies the key name and the built insert statement to the clipboard', async () => {
    renderEditor();

    fireEvent.click(screen.getByTestId('redis-header-copy-key'));
    await waitFor(() => {
      expect(writeText).toHaveBeenCalledWith('user:1');
    });
    expect(screen.getByTestId('redis-header-copy-key').getAttribute('data-copied')).toBe('true');

    // 语句由 detail 形状构造（string ⇒ SET），载荷是服务器数据。
    fireEvent.click(screen.getByTestId('redis-header-copy-insert'));
    await waitFor(() => {
      expect(writeText).toHaveBeenCalledWith('SET user:1 hello');
    });
  });

  it('manual refresh routes to onRefresh once per click', async () => {
    const { onRefresh } = renderEditor();

    fireEvent.click(screen.getByTestId('redis-header-refresh'));
    await waitFor(() => {
      expect(onRefresh).toHaveBeenCalledTimes(1);
    });
    fireEvent.click(screen.getByTestId('redis-header-refresh'));
    await waitFor(() => {
      expect(onRefresh).toHaveBeenCalledTimes(2);
    });
  });
});

// ============================================================================
// Journey 2: 内联改名（进入 → 确认成功 / 确认失败保持 / 取消退出）
// ============================================================================
describe('Journey: 内联改名', () => {
  it('confirms with a trimmed name, then closes and reloads', async () => {
    const { onRenamed, onRefresh } = renderEditor();

    // 进入：输入框替换键名显示，预填当前键名；未改动 ⇒ 确认禁用（退出条件之一）。
    fireEvent.click(screen.getByTestId('redis-header-rename'));
    expect(screen.getByTestId('redis-key-header').getAttribute('data-renaming')).toBe('true');
    const input = screen.getByTestId('redis-header-rename-input') as HTMLInputElement;
    expect(input.value).toBe('user:1');
    expect(screen.getByTestId('redis-header-rename-confirm')).toBeDisabled();

    // 空名同样是非法进入态。
    fireEvent.change(input, { target: { value: '   ' } });
    expect(screen.getByTestId('redis-header-rename-confirm')).toBeDisabled();

    // 合法改动 ⇒ 确认可用；Enter 也能提交（同一路径）。
    fireEvent.change(input, { target: { value: '  user:2  ' } });
    const confirm = screen.getByTestId('redis-header-rename-confirm');
    expect(confirm).not.toBeDisabled();
    fireEvent.click(confirm);

    await waitFor(() => {
      expect(renameKey).toHaveBeenCalledWith('sess-e4', 0, 'user:1', 'user:2');
    });
    await waitFor(() => {
      expect(onRenamed).toHaveBeenCalledWith('user:2');
    });
    await waitFor(() => {
      expect(onRefresh).toHaveBeenCalled();
    });
    // 成功退出：输入框收起。
    expect(screen.getByTestId('redis-key-header').getAttribute('data-renaming')).toBe('false');
  });

  it('cancel exits without touching the server', async () => {
    const { onRenamed } = renderEditor();

    fireEvent.click(screen.getByTestId('redis-header-rename'));
    fireEvent.change(screen.getByTestId('redis-header-rename-input'), {
      target: { value: 'user:x' },
    });
    fireEvent.click(screen.getByTestId('redis-header-rename-cancel'));

    expect(screen.getByTestId('redis-key-header').getAttribute('data-renaming')).toBe('false');
    expect(renameKey).not.toHaveBeenCalled();
    expect(onRenamed).not.toHaveBeenCalled();
  });

  it('a refused rename (guard / error) keeps the input open — no silent exit', async () => {
    renderHeader({
      onRename: vi.fn(async () => false),
    });

    fireEvent.click(screen.getByTestId('redis-header-rename'));
    fireEvent.change(screen.getByTestId('redis-header-rename-input'), {
      target: { value: 'user:2' },
    });
    fireEvent.click(screen.getByTestId('redis-header-rename-confirm'));

    await waitFor(() => {
      expect(screen.getByTestId('redis-key-header').getAttribute('data-renaming')).toBe('true');
    });
    // 输入还在，改动没有被吃掉。
    expect((screen.getByTestId('redis-header-rename-input') as HTMLInputElement).value).toBe(
      'user:2',
    );
  });
});

// ============================================================================
// Journey 3: 删除（确认框先过；取消 ⇒ 命令不发）
// ============================================================================
describe('Journey: 删除键', () => {
  it('runs the bound confirm first, then delete + reload on accept', async () => {
    const { onRefresh } = renderEditor();

    fireEvent.click(screen.getByTestId('redis-header-delete'));
    await waitFor(() => {
      expect(confirmFn).toHaveBeenCalledTimes(1);
    });
    await waitFor(() => {
      expect(deleteKey).toHaveBeenCalledWith('sess-e4', 0, 'user:1');
    });
    await waitFor(() => {
      expect(onRefresh).toHaveBeenCalled();
    });
  });

  it('cancelling the confirm never dispatches delete_keys', async () => {
    const { onRefresh } = renderEditor();
    confirmAnswer = false;

    fireEvent.click(screen.getByTestId('redis-header-delete'));
    await waitFor(() => {
      expect(confirmFn).toHaveBeenCalledTimes(1);
    });
    expect(deleteKey).not.toHaveBeenCalled();
    expect(onRefresh).not.toHaveBeenCalled();
  });
});

// ============================================================================
// Journey 4: 自动刷新分体按钮（进入 / 状态内 tick / 退出：关 & 被拒）
// ============================================================================
describe('Journey: 自动刷新分体按钮', () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });

  it('ticks on the chosen interval and stops when 关 is picked', async () => {
    const onRefresh = vi.fn(async () => true);
    renderHeader({ onRefresh });

    // 进入前：关（0）。
    expect(screen.getByTestId('redis-key-header').getAttribute('data-refresh-interval-ms')).toBe(
      '0',
    );

    // 打开菜单：5 枚档位，各带 i18n key，选中态挂在当前档上。
    fireEvent.click(screen.getByTestId('redis-header-refresh-menu'));
    expect(screen.getAllByRole('menuitem')).toHaveLength(5);
    expect(screen.getByTestId('redis-refresh-interval-5000').getAttribute('data-i18n-key')).toBe(
      'redis.detail.refresh.interval',
    );
    expect(screen.getByTestId('redis-refresh-interval-0').getAttribute('data-i18n-key')).toBe(
      'redis.detail.refresh.off',
    );
    expect(screen.getByTestId('redis-refresh-interval-5000').getAttribute('data-selected')).toBe(
      'false',
    );

    // 进入：选 5s ⇒ 菜单收起、容器记下间隔。
    fireEvent.click(screen.getByTestId('redis-refresh-interval-5000'));
    expect(screen.queryByTestId('redis-refresh-menu')).toBeNull();
    expect(screen.getByTestId('redis-key-header').getAttribute('data-refresh-interval-ms')).toBe(
      '5000',
    );

    // 状态内：每 5000ms 拍一次，15000ms ⇒ 3 拍。
    await act(async () => {
      await vi.advanceTimersByTimeAsync(5000);
    });
    expect(onRefresh).toHaveBeenCalledTimes(1);
    await act(async () => {
      await vi.advanceTimersByTimeAsync(10000);
    });
    expect(onRefresh).toHaveBeenCalledTimes(3);

    // 退出跃迁：选关 ⇒ 立刻停拍。
    fireEvent.click(screen.getByTestId('redis-header-refresh-menu'));
    expect(screen.getByTestId('redis-refresh-interval-5000').getAttribute('data-selected')).toBe(
      'true',
    );
    fireEvent.click(screen.getByTestId('redis-refresh-interval-0'));
    expect(screen.getByTestId('redis-key-header').getAttribute('data-refresh-interval-ms')).toBe(
      '0',
    );
    await act(async () => {
      await vi.advanceTimersByTimeAsync(60000);
    });
    expect(onRefresh).toHaveBeenCalledTimes(3);
  });

  it('a refused refresh (I-1 guard) turns auto-refresh off instead of nagging', async () => {
    const onRefresh = vi.fn(async () => false);
    renderHeader({ onRefresh });

    fireEvent.click(screen.getByTestId('redis-header-refresh-menu'));
    fireEvent.click(screen.getByTestId('redis-refresh-interval-1000'));
    expect(screen.getByTestId('redis-key-header').getAttribute('data-refresh-interval-ms')).toBe(
      '1000',
    );

    await act(async () => {
      await vi.advanceTimersByTimeAsync(1000);
    });
    expect(onRefresh).toHaveBeenCalledTimes(1);
    // 退出跃迁：false ⇒ 自动落回关（E-5 草稿守卫的回答就是这个信号）。
    expect(screen.getByTestId('redis-key-header').getAttribute('data-refresh-interval-ms')).toBe(
      '0',
    );

    await act(async () => {
      await vi.advanceTimersByTimeAsync(10000);
    });
    expect(onRefresh).toHaveBeenCalledTimes(1);
  });
});

// ============================================================================
// Journey 5: 插入语句缺席 ⇒ 动作隐藏（宁缺勿错）
// ============================================================================
describe('Journey: 插入语句为 null 时隐藏复制动作', () => {
  it('hides the insert button when the shape cannot be rebuilt faithfully', () => {
    renderHeader({ insertStatement: null });

    expect(screen.queryByTestId('redis-header-copy-insert')).toBeNull();
    // 复制键名仍在：语句缺席不影响键名复制。
    expect(screen.getByTestId('redis-header-copy-key')).toBeTruthy();
  });
});
