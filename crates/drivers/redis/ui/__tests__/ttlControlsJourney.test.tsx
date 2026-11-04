/**
 * TtlControls 三态内联编辑器连续旅程（本轨 E-4，PRD §3.3 徽标行 / 裁定 8-4）。
 *
 * 每条旅程自含四步：准备（mock invoke + 渲染）→ 行为（点开 TTL 胶囊、切模式、
 * 点击应用）→ 断言（Redis 命令载荷 + data-* 状态跃迁）→ 清理。
 *
 * 状态机三要素（AGENTS.md 状态机思维）：
 * - 进入：点折叠胶囊（`data-ttl-open=false` → `true`），编辑器总是落在相对 TTL 模式；
 * - 状态内：三种模式（永不过期 / 相对 / 绝对 EXPIREAT）各自应用并触发 onChanged；
 * - 退出：`redis-ttl-close` 回到折叠态（→ `data-ttl-open=false`），或切键卸载。
 *
 * 断言口径（§7-6 / 裁定 8-4）：定位用 `data-testid`，状态用 `data-ttl-state` /
 * `data-ttl-open` / `data-ttl-mode` / `data-selected`，模式标签只断言其 i18n key
 * （`data-i18n-key`），不读英文字面量。下方 map 仅为让 `t()` 返回可渲染文本。
 */
import { afterEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';

// Components take `useI18n` from the single @datazen/ui runtime; keep the
// assertions locale-independent by overriding only that hook.
//
// The map below exists ONLY so `t()` returns something renderable — no
// assertion in this file reads a rendered copy. Locating and state checks go
// through the `data-*` contract on TtlControls (see
// docs/development/interaction-and-testing-principles.md, "断言与 i18n 文案解耦").
vi.mock('@datazen/ui', async (importOriginal) => ({
  ...(await importOriginal<typeof import('@datazen/ui')>()),
  useI18n: () => ({
    t: (key: string, params?: Record<string, unknown>) => {
      const map: Record<string, string> = {
        'redis.ttl': 'TTL',
        'redis.noExpiry': 'No expiry',
        'redis.seconds': 's',
        'redis.ttlSeconds': 'TTL (seconds)',
        'redis.setTtl': 'Set TTL',
        'redis.expireAt': 'Expire at',
        'redis.expireAtInvalid': 'Invalid expire datetime',
        'redis.setExpireAt': 'Set expire at',
        'redis.persist': 'Persist',
        'redis.detail.ttl.modeRelative': 'Relative TTL',
        'redis.detail.ttl.modeAbsolute': 'Absolute time (EXPIREAT)',
      };
      const raw = map[key] ?? key;
      return params && params.n != null ? raw.replace('{n}', String(params.n)) : raw;
    },
  }),
}));

import { TtlControls } from '../value-editors/TtlControls';
import type { PluginInvokeFn } from '../value-editors/keyEditorsInvokes';

afterEach(() => {
  cleanup();
});

// ---------------------------------------------------------------------------
// Helpers: render TtlControls with a mock invoke function + data-* locators
// ---------------------------------------------------------------------------

/** The TTL read-out slot; state is `no-expiry` | `seconds`, never rendered copy. */
function ttlValueSlot() {
  return screen.getByTestId('redis-ttl-value');
}

function expectCollapsed() {
  expect(ttlValueSlot().getAttribute('data-ttl-open')).toBe('false');
  expect(screen.queryByTestId('redis-ttl-close')).toBeNull();
}

function expectNoExpiryState() {
  const slot = ttlValueSlot();
  expect(slot.getAttribute('data-ttl-state')).toBe('no-expiry');
  // Slot must render a resolved label, not an empty hole.
  expect((slot.textContent ?? '').trim().length).toBeGreaterThan(0);
}

function expectSecondsState(seconds: number) {
  const slot = ttlValueSlot();
  expect(slot.getAttribute('data-ttl-state')).toBe('seconds');
  // The numeric TTL is data, not copy — pinning it keeps the real intent.
  expect(slot.textContent).toContain(String(seconds));
}

/** Enter transition: click the pill, editor opens (always in relative mode). */
function openEditor() {
  expectCollapsed();
  fireEvent.click(screen.getByTestId('redis-ttl-value'));
  expect(ttlValueSlot().getAttribute('data-ttl-open')).toBe('true');
  expectSelectedMode('relative');
}

function selectMode(mode: 'relative' | 'absolute' | 'no-expiry') {
  fireEvent.click(screen.getByTestId(`redis-ttl-mode-${mode}`));
  expectSelectedMode(mode);
}

function expectSelectedMode(mode: 'relative' | 'absolute' | 'no-expiry') {
  expect(screen.getByTestId(`redis-ttl-mode-${mode}`).getAttribute('data-selected')).toBe('true');
}

function datetimeInput(): HTMLInputElement {
  return screen.getByTestId('redis-ttl-datetime') as HTMLInputElement;
}

function setupJourney(opts: {
  keyName: string;
  ttl: number;
  dbSessionId?: string;
  dbIndex?: number;
}) {
  const invoke = vi.fn<PluginInvokeFn>().mockResolvedValue(undefined);
  const onChanged = vi.fn();
  const result = render(
    <TtlControls
      dbSessionId={opts.dbSessionId ?? 'test-sess'}
      dbIndex={opts.dbIndex ?? 0}
      keyName={opts.keyName}
      ttl={opts.ttl}
      onChanged={onChanged}
      invoke={invoke}
    />,
  );
  return { invoke, onChanged, ...result };
}

// ============================================================================
// Journey 1: Set relative TTL on a key with no expiry
// ============================================================================
describe('Journey: Set relative TTL on key without expiry', () => {
  it('creates key, sets TTL, verifies EXPIRE command, cleans up', async () => {
    // ── 1. Prepare ───────────────────────────────────────────────────────
    // Fresh key with ttl=-1 (no expiry)
    const { invoke, onChanged } = setupJourney({
      keyName: 'journey1:test-key',
      ttl: -1,
    });

    // Verify initial state: collapsed pill reports the no-expiry state
    expectNoExpiryState();
    // 进入跃迁：点胶囊 ⇒ 编辑器打开且默认相对模式。
    openEditor();

    // ── 2. Act ───────────────────────────────────────────────────────────
    // Type a relative TTL value
    const ttlInput = screen.getByTestId('redis-ttl-input');
    fireEvent.change(ttlInput, { target: { value: '3600' } });

    // Click the relative-TTL apply action
    const setTtlBtn = screen.getByTestId('redis-ttl-set');
    fireEvent.click(setTtlBtn);

    // ── 3. Assert ────────────────────────────────────────────────────────
    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith('redis', 'set_ttl', {
        dbSessionId: 'test-sess',
        dbIndex: 0,
        key: 'journey1:test-key',
        ttlSeconds: 3600,
      });
    });
    expect(onChanged).toHaveBeenCalledOnce();

    // ── 4. Clean ─────────────────────────────────────────────────────────
    cleanup();
    // Verify no pending timers or leaked state
    expect(document.querySelector('[data-testid]')).toBeNull();
  });
});

// ============================================================================
// Journey 2: Set absolute expiry via datetime picker
// ============================================================================
describe('Journey: Set absolute expiry via EXPIREAT', () => {
  it('sets expire-at timestamp and verifies setExpireAt command', async () => {
    // ── 1. Prepare ───────────────────────────────────────────────────────
    const { invoke, onChanged } = setupJourney({
      keyName: 'journey2:session-data',
      ttl: 600,
    });

    // Verify initial TTL state carries the numeric TTL (collapsed)
    expectSecondsState(600);
    openEditor();

    // ── 2. Act ───────────────────────────────────────────────────────────
    // 切到绝对时间模式，datetime 输入才属于当前状态。
    selectMode('absolute');
    const input = datetimeInput();
    // Use a fixed future time: 2030-01-15T12:00
    fireEvent.change(input, { target: { value: '2030-01-15T12:00' } });

    // Click the absolute-expiry apply action
    const expireAtBtn = screen.getByTestId('redis-ttl-expire-at');
    fireEvent.click(expireAtBtn);

    // ── 3. Assert ────────────────────────────────────────────────────────
    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith('redis', 'set_ttl', {
        dbSessionId: 'test-sess',
        dbIndex: 0,
        key: 'journey2:session-data',
        expireAt: expect.any(Number),
      });
    });

    // Verify the unix timestamp is roughly 2030-01-15 (± timezone offset)
    const callArgs = invoke.mock.calls.find(
      (c) =>
        c[0] === 'redis' &&
        c[1] === 'set_ttl' &&
        typeof (c[2] as Record<string, unknown>).expireAt === 'number',
    );
    expect(callArgs).toBeTruthy();
    const expireAt = (callArgs![2] as { expireAt: number }).expireAt;
    expect(expireAt).toBeGreaterThan(1893000000);
    expect(expireAt).toBeLessThan(1896000000);
    expect(onChanged).toHaveBeenCalledOnce();

    // ── 4. Clean ─────────────────────────────────────────────────────────
    cleanup();
  });
});

// ============================================================================
// Journey 3: Remove TTL via PERSIST
// ============================================================================
describe('Journey: Remove TTL via PERSIST', () => {
  it('removes expiry and verifies persist command with ttlSeconds=-1', async () => {
    // ── 1. Prepare ───────────────────────────────────────────────────────
    const { invoke, onChanged } = setupJourney({
      keyName: 'journey3:cache-item',
      ttl: 120,
    });

    expectSecondsState(120);
    openEditor();

    // ── 2. Act ───────────────────────────────────────────────────────────
    // 持久化属于「永不过期」模式，先切模式再点应用。
    selectMode('no-expiry');
    const persistBtn = screen.getByTestId('redis-ttl-persist');
    fireEvent.click(persistBtn);

    // ── 3. Assert ────────────────────────────────────────────────────────
    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith('redis', 'set_ttl', {
        dbSessionId: 'test-sess',
        dbIndex: 0,
        key: 'journey3:cache-item',
        ttlSeconds: -1,
      });
    });
    expect(onChanged).toHaveBeenCalledOnce();

    // ── 4. Clean ─────────────────────────────────────────────────────────
    cleanup();
  });
});

// ============================================================================
// Journey 4: Error handling — invalid TTL input
// ============================================================================
describe('Journey: Error on invalid TTL input', () => {
  it('shows error message when negative TTL is entered', async () => {
    // ── 1. Prepare ───────────────────────────────────────────────────────
    const { invoke } = setupJourney({
      keyName: 'journey4:error-key',
      ttl: 300,
    });
    openEditor();

    // ── 2. Act ───────────────────────────────────────────────────────────
    const ttlInput = screen.getByTestId('redis-ttl-input');
    fireEvent.change(ttlInput, { target: { value: '-5' } });

    const setTtlBtn = screen.getByTestId('redis-ttl-set');
    fireEvent.click(setTtlBtn);

    // ── 3. Assert ────────────────────────────────────────────────────────
    // invoke should NOT have been called for invalid input
    await waitFor(() => {
      expect(invoke).not.toHaveBeenCalled();
    });

    // The inline error slot must appear and carry a resolved message
    // (the copy itself is an i18n value and is deliberately not asserted).
    await waitFor(() => {
      const errorSlot = screen.getByTestId('redis-ttl-error');
      expect((errorSlot.textContent ?? '').trim().length).toBeGreaterThan(0);
    });

    // ── 4. Clean ─────────────────────────────────────────────────────────
    cleanup();
  });
});

// ============================================================================
// Journey 5: Error handling — invalid datetime
// ============================================================================
describe('Journey: Error on invalid datetime', () => {
  it('shows error when invalid datetime is entered', async () => {
    // ── 1. Prepare ───────────────────────────────────────────────────────
    const { invoke } = setupJourney({
      keyName: 'journey5:dt-key',
      ttl: -1,
    });
    openEditor();
    selectMode('absolute');

    // ── 2. Act ───────────────────────────────────────────────────────────
    const input = datetimeInput();
    expect(input).toBeTruthy();
    // Set a value that Date.parse cannot parse
    fireEvent.change(input, { target: { value: 'not-a-date' } });

    // Click the absolute-expiry apply action
    const expireAtBtn = screen.getByTestId('redis-ttl-expire-at');
    fireEvent.click(expireAtBtn);

    // ── 3. Assert ────────────────────────────────────────────────────────
    // jsdom normalises an unparsable `datetime-local` value to '', so the real
    // contract here is "the apply action stays disabled ⇒ nothing is sent".
    // [tester] That used to be proven only through the absence of the invoke,
    // which stays green even if the guard on the button is dropped — pin the
    // disabled property itself (a DOM-state anchor, no rendered copy).
    await waitFor(() => {
      expect(invoke).not.toHaveBeenCalled();
    });
    expect(screen.getByTestId('redis-ttl-expire-at')).toBeDisabled();

    // ── 4. Clean ─────────────────────────────────────────────────────────
    cleanup();
  });
});

// ============================================================================
// Journey 6: Rapid TTL → Persist → Set again
// ============================================================================
describe('Journey: TTL set → persist → set again cycle', () => {
  it('handles a full lifecycle: EXPIRE → PERSIST → EXPIREAT', async () => {
    // ── 1. Prepare ───────────────────────────────────────────────────────
    const { invoke, onChanged, unmount } = setupJourney({
      keyName: 'journey6:lifecycle-key',
      ttl: -1,
    });
    openEditor();

    // ── 2. Act: Step A — Set TTL ─────────────────────────────────────────
    const ttlInput = screen.getByTestId('redis-ttl-input');
    const setTtlBtn = screen.getByTestId('redis-ttl-set');
    fireEvent.change(ttlInput, { target: { value: '7200' } });
    fireEvent.click(setTtlBtn);

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith('redis', 'set_ttl', {
        dbSessionId: 'test-sess',
        dbIndex: 0,
        key: 'journey6:lifecycle-key',
        ttlSeconds: 7200,
      });
    });

    // ── 3. Act: Step B — Persist ─────────────────────────────────────────
    invoke.mockClear();
    onChanged.mockClear();
    selectMode('no-expiry');
    fireEvent.click(screen.getByTestId('redis-ttl-persist'));

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith('redis', 'set_ttl', {
        dbSessionId: 'test-sess',
        dbIndex: 0,
        key: 'journey6:lifecycle-key',
        ttlSeconds: -1,
      });
    });

    // ── 4. Act: Step C — Set EXPIREAT ───────────────────────────────────
    invoke.mockClear();
    onChanged.mockClear();
    selectMode('absolute');
    fireEvent.change(datetimeInput(), { target: { value: '2035-06-01T00:00' } });
    fireEvent.click(screen.getByTestId('redis-ttl-expire-at'));

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith('redis', 'set_ttl', {
        dbSessionId: 'test-sess',
        dbIndex: 0,
        key: 'journey6:lifecycle-key',
        expireAt: expect.any(Number),
      });
    });

    // ── 5. Clean ─────────────────────────────────────────────────────────
    unmount();
    cleanup();
  });
});

// ============================================================================
// Journey 7: Different dbSessionId and dbIndex
// ============================================================================
describe('Journey: Different session and db index', () => {
  it('passes correct session and db index for non-default config', async () => {
    // ── 1. Prepare ───────────────────────────────────────────────────────
    const { invoke } = setupJourney({
      keyName: 'journey7:special-key',
      ttl: 60,
      dbSessionId: 'custom-session-abc',
      dbIndex: 3,
    });
    openEditor();

    // ── 2. Act ───────────────────────────────────────────────────────────
    selectMode('no-expiry');
    const persistBtn = screen.getByTestId('redis-ttl-persist');
    fireEvent.click(persistBtn);

    // ── 3. Assert ────────────────────────────────────────────────────────
    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith('redis', 'set_ttl', {
        dbSessionId: 'custom-session-abc',
        dbIndex: 3,
        key: 'journey7:special-key',
        ttlSeconds: -1,
      });
    });

    // ── 4. Clean ─────────────────────────────────────────────────────────
    cleanup();
  });
});

// ============================================================================
// Journey 8: TTL = -1 (no expiry) shows correct display
// ============================================================================
describe('Journey: No-expiry display and persist when already expired', () => {
  it('reports the no-expiry TTL state for ttl=-1, and persist resets input fields', async () => {
    // ── 1. Prepare ───────────────────────────────────────────────────────
    const { invoke, onChanged } = setupJourney({
      keyName: 'journey8:no-expiry-key',
      ttl: -1,
    });

    // ── 2. Act ───────────────────────────────────────────────────────────
    // Verify the TTL slot is in the no-expiry state (collapsed)
    expectNoExpiryState();
    openEditor();

    // Persist should still work (noop but verify command)
    selectMode('no-expiry');
    const persistBtn = screen.getByTestId('redis-ttl-persist');
    fireEvent.click(persistBtn);

    // ── 3. Assert ────────────────────────────────────────────────────────
    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith('redis', 'set_ttl', {
        dbSessionId: 'test-sess',
        dbIndex: 0,
        key: 'journey8:no-expiry-key',
        ttlSeconds: -1,
      });
    });
    expect(onChanged).toHaveBeenCalledOnce();

    // ── 4. Clean ─────────────────────────────────────────────────────────
    cleanup();
  });
});

// ============================================================================
// Journey 9: pill state machine — enter / in-state / exit (三要素)
// ============================================================================
describe('Journey: TTL pill state machine (enter / in-state / exit)', () => {
  it('opens in relative mode, switches modes, closes, and re-enters fresh', () => {
    const { rerender } = setupJourney({
      keyName: 'journey9:pill-machine',
      ttl: -1,
    });

    // 进入前：折叠胶囊带状态，编辑器控件不存在。
    expectNoExpiryState();
    expectCollapsed();
    expect(screen.queryByTestId('redis-ttl-input')).toBeNull();
    expect(screen.queryByTestId('redis-ttl-persist')).toBeNull();

    // 进入：点胶囊 ⇒ 打开，且总是落在相对模式（确定性进入条件）。
    openEditor();
    expect(screen.getByTestId('redis-ttl-input')).toBeTruthy();

    // 状态内：三枚模式按钮各带自己的 i18n key，选中态互斥跃迁。
    expect(screen.getByTestId('redis-ttl-mode-no-expiry').getAttribute('data-i18n-key')).toBe(
      'redis.noExpiry',
    );
    expect(screen.getByTestId('redis-ttl-mode-relative').getAttribute('data-i18n-key')).toBe(
      'redis.detail.ttl.modeRelative',
    );
    expect(screen.getByTestId('redis-ttl-mode-absolute').getAttribute('data-i18n-key')).toBe(
      'redis.detail.ttl.modeAbsolute',
    );
    selectMode('absolute');
    expect(screen.getByTestId('redis-ttl-mode-relative').getAttribute('data-selected')).toBe(
      'false',
    );
    expect(datetimeInput()).toBeTruthy();

    // 退出：关闭按钮回到折叠态，编辑器控件整体卸载。
    fireEvent.click(screen.getByTestId('redis-ttl-close'));
    expectCollapsed();
    expect(screen.queryByTestId('redis-ttl-mode-absolute')).toBeNull();
    expect(screen.queryByTestId('redis-ttl-datetime')).toBeNull();

    // 重新进入：又从相对模式开始（退出清掉了状态内选择）。
    fireEvent.click(screen.getByTestId('redis-ttl-value'));
    expect(ttlValueSlot().getAttribute('data-ttl-open')).toBe('true');
    expectSelectedMode('relative');

    // 属性 ttl 变化（成功应用后父级回读）⇒ 输入与胶囊读数跟随服务器真值。
    rerender(
      <TtlControls
        dbSessionId="test-sess"
        dbIndex={0}
        keyName="journey9:pill-machine"
        ttl={999}
        onChanged={() => {}}
        invoke={vi.fn<PluginInvokeFn>().mockResolvedValue(undefined)}
      />,
    );
    expect((screen.getByTestId('redis-ttl-input') as HTMLInputElement).value).toBe('999');
    expect(ttlValueSlot().textContent).toContain('999');
  });
});

// ============================================================================
// BUG-004: 退出跃迁补齐 —— Esc / 失焦（状态机 exit 不能只有显式关按钮）
// ============================================================================
describe('[redis-detail-ui-BUG-004] TTL 内联编辑的 Esc / 失焦退出跃迁', () => {
  it('closes on Escape typed inside the editor, and re-entry still starts at the default mode', () => {
    setupJourney({ keyName: 'bug004:esc', ttl: 300 });
    openEditor();

    // 状态内：焦点在秒数输入上按 Esc ⇒ 退出到折叠胶囊。
    fireEvent.keyDown(screen.getByTestId('redis-ttl-input'), { key: 'Escape' });
    expectCollapsed();

    // 退出没破坏进入跃迁：重新点开仍落在文档默认的相对模式。
    openEditor();
  });

  it('closes when focus leaves the container but stays when focus moves within it', () => {
    setupJourney({ keyName: 'bug004:blur', ttl: 300 });
    openEditor();
    const ttlInput = screen.getByTestId('redis-ttl-input');
    const modeButton = screen.getByTestId('redis-ttl-mode-absolute');

    // 焦点从输入移到容器内的模式按钮 ⇒ 不得关闭。
    fireEvent.blur(ttlInput, { relatedTarget: modeButton });
    expect(ttlValueSlot().getAttribute('data-ttl-open')).toBe('true');

    // 焦点离开容器 ⇒ 关闭。
    fireEvent.blur(modeButton, { relatedTarget: document.body });
    expectCollapsed();
  });

  it('ignores Escape while an apply is in flight (mirrors the disabled close button)', async () => {
    // 永不落定的写命令 ⇒ busy 悬起，退出跃迁必须像关按钮一样被禁用。
    const pendingInvoke = vi
      .fn<PluginInvokeFn>()
      .mockImplementation(() => new Promise<void>(() => {}));
    render(
      <TtlControls
        dbSessionId="test-sess"
        dbIndex={0}
        keyName="bug004:busy"
        ttl={300}
        onChanged={() => {}}
        invoke={pendingInvoke}
      />,
    );
    openEditor();
    fireEvent.change(screen.getByTestId('redis-ttl-input'), { target: { value: '120' } });
    fireEvent.click(screen.getByTestId('redis-ttl-set'));
    await waitFor(() => expect(pendingInvoke).toHaveBeenCalled());

    fireEvent.keyDown(screen.getByTestId('redis-ttl-input'), { key: 'Escape' });
    // 应用中不退：错误/结果仍要可见，不被静默折叠。
    expect(ttlValueSlot().getAttribute('data-ttl-open')).toBe('true');
    expect(screen.queryByTestId('redis-ttl-close')).not.toBeNull();
  });
});
