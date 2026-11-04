/**
 * 键头行复制反馈收敛（本轨 `copy-feedback-converge`）。
 *
 * 收敛前这里自带 `copiedKind` + 裸 `setTimeout`；收敛后改用 `@datazen/ui` 导出的
 * `useCopyFeedback`，行内 id 仍在本地，标志位由 hook 持有并按请求绑定。
 *
 * 窗口刻意保持 1200ms（不是 1500/2000）：`data-copied` 是 Redis 专属的按钮态，
 * 改窗口就是改用户可见行为。
 *
 * 断言口径：定位 `data-testid`，状态 `data-copied`，只观察运行时行为。
 */
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { act, cleanup, fireEvent, render, screen } from '@testing-library/react';
import { create } from 'zustand';
import {
  bindConfirmDialog,
  bindConnectionStore,
  bindSettingsStore,
  type ConnectionBridgeState,
  type SettingsBridgeState,
} from '@datazen/driver-sdk';
import { KeyHeaderRow } from '../value-editors/KeyHeaderRow';

// 驱动 UI 不依赖宿主，测试自行把 SDK 桥绑定到最小 store。
bindSettingsStore(
  create<SettingsBridgeState>(() => ({
    settings: { safeMode: false, editorFontFamily: '', driverSettings: {} },
  })),
);
bindConnectionStore(create<ConnectionBridgeState>(() => ({ connections: [] })));
bindConfirmDialog(() => [vi.fn(async () => true), null]);

interface Deferred {
  promise: Promise<void>;
  resolve: () => void;
  reject: (reason?: unknown) => void;
}

/**
 * 每次 `writeText` 一个独立 deferred，便于「先点击、后失败」这种时序断言。
 * 注意：拒绝用例不能用 `waitFor`——本套件的 fake timer 不会被 `waitFor` 推进。
 */
function installControlledClipboard() {
  const deferreds: Deferred[] = [];
  const writeText = vi.fn((_text: string) => {
    let resolve!: () => void;
    let reject!: (reason?: unknown) => void;
    const promise = new Promise<void>((res, rej) => {
      resolve = res;
      reject = rej;
    });
    deferreds.push({ promise, resolve, reject });
    return promise;
  });
  Object.defineProperty(navigator, 'clipboard', {
    value: { writeText },
    configurable: true,
    writable: true,
  });
  return {
    writeText,
    settle: async (index: number, outcome: 'resolve' | 'reject') => {
      const entry = deferreds[index];
      if (outcome === 'reject') entry.reject(new Error('denied'));
      else entry.resolve();
      await entry.promise.catch(() => undefined);
      await Promise.resolve();
    },
  };
}

function installResolvedClipboard() {
  const writeText = vi.fn(async (_text: string) => undefined);
  Object.defineProperty(navigator, 'clipboard', {
    value: { writeText },
    configurable: true,
    writable: true,
  });
  return writeText;
}

/**
 * 监听 window 定时器 API，好让测试指认出「这次点击到底排了哪个句柄」。
 * 卸载前后直接比 `getTimerCount()` 不可靠：挂载本身也可能排队定时器，
 * 卸载会把它们一并清掉，计数下降与复制窗口无关。
 */
function spyOnWindowTimers() {
  const setSpy = vi.spyOn(window, 'setTimeout');
  const clearSpy = vi.spyOn(window, 'clearTimeout');
  return {
    lastArmedHandle: () => setSpy.mock.results.at(-1)?.value as unknown as number,
    clearedHandles: () => clearSpy.mock.calls.map((call) => call[0] as unknown as number),
  };
}

/** 该行刻意保持的非默认窗口。 */
const FEEDBACK_MS = 1200;

/** 推进时间必须包在 `act` 里，React 才会把回退后的状态刷到 DOM 上。 */
function advanceBy(ms: number): void {
  act(() => {
    vi.advanceTimersByTime(ms);
  });
}

const keyButton = () => screen.getByTestId('redis-header-copy-key');
const insertButton = () => screen.getByTestId('redis-header-copy-insert');
const isCopied = (testId: 'key' | 'insert') =>
  screen.getByTestId(`redis-header-copy-${testId}`).getAttribute('data-copied') === 'true';

function renderHeader() {
  return render(
    <KeyHeaderRow
      keyName="user:1"
      onRefresh={vi.fn(async () => true)}
      onRename={vi.fn(async () => true)}
      onDelete={vi.fn(async () => true)}
      insertStatement="SET user:1 hello"
    />,
  );
}

beforeEach(() => {
  vi.useFakeTimers();
});

afterEach(() => {
  cleanup();
  vi.useRealTimers();
  vi.restoreAllMocks();
  vi.clearAllMocks();
});

describe('KeyHeaderRow copy feedback', () => {
  it('只标记被复制的那一个按钮', () => {
    const writeText = installResolvedClipboard();
    renderHeader();

    fireEvent.click(keyButton());
    expect(writeText).toHaveBeenCalledWith('user:1');
    expect(isCopied('key')).toBe(true);
    expect(isCopied('insert')).toBe(false);

    fireEvent.click(insertButton());
    expect(writeText).toHaveBeenLastCalledWith('SET user:1 hello');
    expect(isCopied('key')).toBe(false);
    expect(isCopied('insert')).toBe(true);
  });

  it('窗口保持 1200ms（非默认时长）', () => {
    installResolvedClipboard();
    renderHeader();

    fireEvent.click(keyButton());

    advanceBy(FEEDBACK_MS - 1);
    expect(isCopied('key')).toBe(true);

    advanceBy(1);
    expect(isCopied('key')).toBe(false);
  });

  it('剪贴板写入失败时回滚标记', async () => {
    const clipboard = installControlledClipboard();
    renderHeader();

    fireEvent.click(keyButton());
    expect(isCopied('key')).toBe(true);

    await clipboard.settle(0, 'reject');
    expect(isCopied('key')).toBe(false);
  });

  it('窗口期内卸载会清掉定时器', () => {
    installResolvedClipboard();
    const timers = spyOnWindowTimers();
    const { unmount } = renderHeader();

    fireEvent.click(keyButton());
    const windowHandle = timers.lastArmedHandle();
    expect(timers.clearedHandles()).not.toContain(windowHandle);

    unmount();

    expect(timers.clearedHandles()).toContain(windowHandle);
  });

  it('前一次点击的迟到失败不会抹掉后一次的成功标记', async () => {
    const clipboard = installControlledClipboard();
    renderHeader();

    fireEvent.click(keyButton());
    fireEvent.click(insertButton());

    await clipboard.settle(0, 'reject');
    expect(isCopied('key')).toBe(false);
    expect(isCopied('insert')).toBe(true);

    advanceBy(FEEDBACK_MS);
    expect(isCopied('insert')).toBe(false);
  });
});
