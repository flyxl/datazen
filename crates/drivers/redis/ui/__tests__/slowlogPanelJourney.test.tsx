/**
 * E-1（裁定 8-1 + I-11）：`SlowlogPanel` 的状态机旅程。
 *
 * 一次挂载走完全程：loading → ready → 手动刷新 → 未授权具名空态 → 真·0 条具名空态，
 * 逐步断言 `data-slowlog-state` 跃迁；另测失败态与重置确认弹层（取消不发消息）。
 *
 * 断言只读 `data-*` / `data-i18n-key`（PRD §7-6），不钉任何翻译后的英文字面量。
 */
import { afterEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { create } from 'zustand';
import {
  bindConfirmDialog,
  bindSettingsStore,
  type SettingsBridgeState,
} from '@datazen/driver-sdk';

import {
  SlowlogPanel,
  formatSlowlogDuration,
  formatSlowlogTimestamp,
  resolveSlowlogState,
} from '../observe/SlowlogPanel';
import type { PluginInvokeFn } from '../value-editors/keyEditorsInvokes';

// Harness capability bindings: `SlowlogPanel` runs writes through the Redis Safe
// Mode gate, which reads the host settings store / confirm dialog bridges that
// the host injects at startup. Safe Mode stays off so `write-op` proceeds.
bindSettingsStore(
  create<SettingsBridgeState>(() => ({
    settings: { safeMode: false, editorFontFamily: '', driverSettings: {} },
  })),
);
bindConfirmDialog(() => [async () => true, null]);

afterEach(() => cleanup());

function entry(id: number, durationUs: number, command: string[], clientAddr?: string | null) {
  return {
    id,
    timestamp: 1_700_000_000 + id,
    durationUs,
    command,
    clientAddr: clientAddr ?? null,
    clientName: null,
  };
}

function panel() {
  return screen.getByTestId('redis-slowlog-panel');
}

function panelState(): string | null {
  return panel().getAttribute('data-slowlog-state');
}

describe('resolveSlowlogState（I-11 具名空态判定）', () => {
  it('folds payload and error shapes into the four settled states', () => {
    expect(resolveSlowlogState([{ id: 1 }], null)).toBe('ready');
    expect(resolveSlowlogState([], null)).toBe('empty');
    expect(resolveSlowlogState(null, null)).toBe('empty');
    expect(resolveSlowlogState(undefined, null)).toBe('empty');
    // ACL 拒绝与 Redis 不支持（unknown command / disabled）在协议层不可区分，
    // 两者都必须落到"未授权"具名空态，绝不能显示成 0 条。
    expect(resolveSlowlogState(null, new Error('NOPERM this user has no permissions'))).toBe(
      'unauthorized',
    );
    expect(resolveSlowlogState(null, new Error('ERR unknown command `slowlog`'))).toBe(
      'unauthorized',
    );
    expect(resolveSlowlogState(null, new Error('FLUSHALL is not allowed'))).toBe('unauthorized');
    expect(resolveSlowlogState(null, new Error('connection reset by peer'))).toBe('failed');
    expect(resolveSlowlogState(null, 'boom')).toBe('failed');
  });

  it('formats durations and timestamps as data', () => {
    expect(formatSlowlogDuration(500)).toBe('500 µs');
    expect(formatSlowlogDuration(1500)).toBe('1.50 ms');
    expect(formatSlowlogDuration(2_500_000)).toBe('2.50 s');
    expect(formatSlowlogDuration(-1)).toBe('—');
    expect(formatSlowlogDuration(Number.NaN)).toBe('—');
    expect(formatSlowlogTimestamp(0)).toBe('—');
    expect(formatSlowlogTimestamp(-5)).toBe('—');
    expect(formatSlowlogTimestamp(1_700_000_000)).not.toBe('—');
  });
});

describe('Journey: loading → ready → refresh → unauthorized → empty', () => {
  it('walks the whole state machine through clicks', async () => {
    const invoke = vi
      .fn<PluginInvokeFn>()
      .mockResolvedValue([entry(7, 12_345, ['GET', 'big:key'], '127.0.0.1:55')]);
    render(<SlowlogPanel dbSessionId="sess-slow" invoke={invoke} />);

    // 进入条件：挂载即 loading。
    expect(panelState()).toBe('loading');
    await waitFor(() => expect(panelState()).toBe('ready'));
    expect(panel().getAttribute('data-row-count')).toBe('1');
    expect(screen.getByTestId('redis-slowlog-row').getAttribute('data-slowlog-id')).toBe('7');
    expect(invoke).toHaveBeenCalledWith('redis', 'slowlog_get', {
      dbSessionId: 'sess-slow',
      count: 100,
    });

    // 状态内行为：手动刷新 ⇒ 立刻回 loading ⇒ 新数据到位。
    invoke.mockResolvedValue([entry(8, 900, ['SLOWLOG', 'GET']), entry(9, 42, ['KEYS', '*'])]);
    fireEvent.click(screen.getByTestId('redis-slowlog-refresh'));
    expect(panelState()).toBe('loading');
    await waitFor(() => expect(panelState()).toBe('ready'));
    expect(panel().getAttribute('data-row-count')).toBe('2');
    expect(screen.getAllByTestId('redis-slowlog-row')[0].getAttribute('data-slowlog-id')).toBe('9');
    expect(invoke).toHaveBeenCalledTimes(2);

    // 退出跃迁 1：服务端拒绝 ⇒ 具名"未授权"空态（不是 0 条、不是留白）。
    invoke.mockRejectedValue(new Error('NOPERM this user has no permissions'));
    fireEvent.click(screen.getByTestId('redis-slowlog-refresh'));
    await waitFor(() => expect(panelState()).toBe('unauthorized'));
    expect(screen.getByTestId('redis-slowlog-empty').getAttribute('data-i18n-key')).toBe(
      'redis.overview.slowlog.unauthorized',
    );
    expect(panel().getAttribute('data-row-count')).toBe('0');
    expect(screen.queryByTestId('redis-slowlog-row')).toBeNull();

    // 退出跃迁 2：权限恢复但确实没有慢查询 ⇒ 与未授权可区分的第二个具名空态。
    invoke.mockResolvedValue([]);
    fireEvent.click(screen.getByTestId('redis-slowlog-refresh'));
    await waitFor(() => expect(panelState()).toBe('empty'));
    expect(screen.getByTestId('redis-slowlog-empty').getAttribute('data-i18n-key')).toBe(
      'redis.slowlogEmpty',
    );

    // 卸载后不残留 DOM。
    cleanup();
    expect(document.querySelector('[data-testid="redis-slowlog-panel"]')).toBeNull();
  });
});

describe('Journey: 传输失败态', () => {
  it('keeps the raw message visible in the failed state', async () => {
    const invoke = vi.fn<PluginInvokeFn>().mockRejectedValue(new Error('connection reset by peer'));
    render(<SlowlogPanel dbSessionId="sess-err" invoke={invoke} />);
    await waitFor(() => expect(panelState()).toBe('failed'));
    expect(screen.getByTestId('redis-slowlog-empty').textContent).toContain(
      'connection reset by peer',
    );
  });
});

describe('Journey: SLOWLOG RESET 确认弹层', () => {
  it('sends nothing before confirmation and reloads after it lands', async () => {
    const invoke = vi.fn<PluginInvokeFn>().mockResolvedValue([entry(1, 10, ['KEYS', '*'])]);
    render(<SlowlogPanel dbSessionId="sess-reset" invoke={invoke} />);
    await waitFor(() => expect(panelState()).toBe('ready'));
    const callsBefore = invoke.mock.calls.length;

    fireEvent.click(screen.getByTestId('redis-slowlog-reset'));
    expect(invoke.mock.calls.length).toBe(callsBefore);

    fireEvent.click(screen.getByTestId('redis-slowlog-reset-cancel'));
    expect(invoke.mock.calls.length).toBe(callsBefore);

    fireEvent.click(screen.getByTestId('redis-slowlog-reset'));
    fireEvent.click(screen.getByTestId('redis-slowlog-reset-confirm'));
    await waitFor(() =>
      expect(invoke.mock.calls.some((call) => call[1] === 'slowlog_reset')).toBe(true),
    );
    expect(invoke).toHaveBeenCalledWith('redis', 'slowlog_reset', {
      dbSessionId: 'sess-reset',
      confirm: true,
    });
    // 重置成功后回到列表态（重新 GET 一次）。
    await waitFor(() => expect(panelState()).toBe('ready'));
    expect(
      invoke.mock.calls.filter((call) => call[1] === 'slowlog_get').length,
    ).toBeGreaterThanOrEqual(2);
  });
});
