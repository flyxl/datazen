/**
 * Redis console transcript — the REPL scrollback that replaced the old
 * single-slot result panel.
 *
 * The load-bearing behaviour here is *persistence across unmount*: the host
 * remounts a panel every time the user switches top-level tab, and the scrollback
 * has to come back. These cases unmount and re-render the real component to
 * prove it, rather than asserting on the store in isolation.
 */
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { create } from 'zustand';
import {
  bindConfirmDialog,
  bindConnectionStore,
  bindSettingsStore,
  type ConnectionBridgeState,
  type ConfirmDialogFn,
  type SettingsBridgeState,
} from '@datazen/driver-sdk';

const scanKeys = vi.fn();
const commandInvoke = vi.fn();

vi.mock('@datazen/ui', async (importOriginal) => ({
  ...(await importOriginal<typeof import('@datazen/ui')>()),
  useI18n: () => ({ t: (key: string) => key }),
}));

vi.mock('../shared/redisInvoke', () => ({
  redisCommandInvoke: (...args: unknown[]) => commandInvoke(...args),
  invokeScanKeys: (...args: unknown[]) => scanKeys(...args),
}));

import { RedisConsole } from '../console/RedisConsole';
import { RedisRightPanel } from '../key-browser/RedisRightPanel';
import { resetTranscript } from '../console/consoleTranscript';
import { resetRightTab } from '../shared/rightTabState';

// One connection session, shared by every db tab — the wire id, NOT the state key.
// State is keyed by the host-issued panel id, unique per tab.
const WIRE = 'sess-1';
const PANEL_0 = 'panel-1';
const PANEL_1 = 'panel-2';

const detailProps = {
  selectedKey: null,
  detail: null,
  detailLoading: false,
  modules: null,
  onRefresh: () => {},
  onRenamed: () => {},
  onClose: () => {},
};

// Shaped like the safety suite's bindings: the store holds settings, not getters.
bindSettingsStore(
  create<SettingsBridgeState>(() => ({
    settings: { safeMode: false, editorFontFamily: '', driverSettings: { redis: {} } },
  })),
);
bindConnectionStore(create<ConnectionBridgeState>(() => ({ connections: [] })));

let confirmResult = true;

function stubConfirm(result = true) {
  confirmResult = result;
  const confirmFn = vi.fn<ConfirmDialogFn>(() => Promise.resolve(confirmResult));
  bindConfirmDialog(() => [confirmFn, null]);
}

function typeInto(input: HTMLTextAreaElement, value: string) {
  fireEvent.change(input, {
    target: { value, selectionStart: value.length, selectionEnd: value.length },
  });
}

function ok(value: string) {
  commandInvoke.mockResolvedValue({
    results: [{ command: 'GET k', ok: true, value, resultType: 'scalar' }],
  });
}

beforeEach(() => {
  commandInvoke.mockReset();
  scanKeys.mockResolvedValue({ keys: [], cursor: 0, done: true });
  stubConfirm();
  resetTranscript(PANEL_0);
  resetTranscript(PANEL_1);
  resetRightTab(PANEL_0);
  resetRightTab(PANEL_1);
});

afterEach(() => {
  cleanup();
});

describe('transcript accumulates instead of replacing', () => {
  it('keeps every earlier command and result on screen after a second execution', async () => {
    ok('first');
    render(<RedisConsole dbSessionId={WIRE} panelId={PANEL_0} dbIndex={0} />);
    const input = screen.getByTestId('redis-console-input') as HTMLTextAreaElement;

    typeInto(input, 'GET a');
    fireEvent.keyDown(input, { key: 'Enter' });
    await waitFor(() => expect(screen.getByText('first')).toBeTruthy());

    ok('second');
    typeInto(input, 'GET b');
    fireEvent.keyDown(input, { key: 'Enter' });
    await waitFor(() => expect(screen.getByText('second')).toBeTruthy());

    // The old panel overwrote on each execute, so only `second` would be here.
    expect(screen.getByText('first')).toBeTruthy();
    expect(screen.getAllByTestId('redis-console-cmd')).toHaveLength(2);
  });

  it('echoes each command with its dbN prompt', async () => {
    ok('v');
    render(<RedisConsole dbSessionId={WIRE} panelId={PANEL_0} dbIndex={3} />);
    const input = screen.getByTestId('redis-console-input') as HTMLTextAreaElement;

    typeInto(input, 'GET a');
    fireEvent.keyDown(input, { key: 'Enter' });

    await waitFor(() => expect(screen.getByTestId('redis-console-cmd')).toBeTruthy());
    expect(screen.getByTestId('redis-console-cmd').textContent).toContain('db3>');
    expect(screen.getByTestId('redis-console-cmd').textContent).toContain('GET a');
  });

  it('leaves a server error in place, in red, and still runs the next command', async () => {
    commandInvoke.mockResolvedValueOnce({
      results: [{ command: 'SET t v', ok: false, error: 'syntax error', resultType: 'error' }],
    });
    render(<RedisConsole dbSessionId={WIRE} panelId={PANEL_0} dbIndex={0} />);
    const input = screen.getByTestId('redis-console-input') as HTMLTextAreaElement;

    typeInto(input, 'SET t v');
    fireEvent.keyDown(input, { key: 'Enter' });
    await waitFor(() => expect(screen.getByText('syntax error')).toBeTruthy());

    ok('recovered');
    typeInto(input, 'GET t');
    fireEvent.keyDown(input, { key: 'Enter' });
    await waitFor(() => expect(screen.getByText('recovered')).toBeTruthy());

    // The failure is history, not a transient banner that gets replaced.
    expect(screen.getByText('syntax error')).toBeTruthy();
  });

  it('shows the welcome banner only while nothing has run', async () => {
    ok('v');
    render(<RedisConsole dbSessionId={WIRE} panelId={PANEL_0} dbIndex={0} />);
    expect(screen.getByText('redis.console.welcome')).toBeTruthy();

    const input = screen.getByTestId('redis-console-input') as HTMLTextAreaElement;
    typeInto(input, 'GET a');
    fireEvent.keyDown(input, { key: 'Enter' });

    await waitFor(() => expect(screen.queryByText('redis.console.welcome')).toBeNull());
  });
});

describe('scrollback survives the unmount a tab switch causes', () => {
  it('restores entries and the in-progress draft after a full remount', async () => {
    ok('kept');
    const first = render(<RedisConsole dbSessionId={WIRE} panelId={PANEL_0} dbIndex={0} />);
    const input = screen.getByTestId('redis-console-input') as HTMLTextAreaElement;
    typeInto(input, 'GET a');
    fireEvent.keyDown(input, { key: 'Enter' });
    await waitFor(() => expect(screen.getByText('kept')).toBeTruthy());

    typeInto(input, 'half-typed command');
    first.unmount();

    // The host does exactly this: drop the subtree, mount it again later.
    render(<RedisConsole dbSessionId={WIRE} panelId={PANEL_0} dbIndex={0} />);
    expect(screen.getByText('kept')).toBeTruthy();
    expect((screen.getByTestId('redis-console-input') as HTMLTextAreaElement).value).toBe(
      'half-typed command',
    );
  });

  it('keeps two db tabs completely separate although they share one dbSessionId', async () => {
    // Both tabs carry the same wire session id (one connection), so only the
    // panel scope can keep the scrollback apart. Keying by `dbSessionId` leaked
    // db0's output into db1.
    ok('from-db0');
    const db0 = render(<RedisConsole dbSessionId={WIRE} panelId={PANEL_0} dbIndex={0} />);
    const input0 = screen.getByTestId('redis-console-input') as HTMLTextAreaElement;
    typeInto(input0, 'GET a');
    fireEvent.keyDown(input0, { key: 'Enter' });
    await waitFor(() => expect(screen.getByText('from-db0')).toBeTruthy());
    db0.unmount();

    render(<RedisConsole dbSessionId={WIRE} panelId={PANEL_1} dbIndex={1} />);
    // A different db is a different tab: no leaked scrollback, no leaked draft.
    expect(screen.getByText('redis.console.welcome')).toBeTruthy();
    expect((screen.getByTestId('redis-console-input') as HTMLTextAreaElement).value).toBe('');
  });
});

describe('input bar', () => {
  it('is one line tall at rest, matching the db prompt row', () => {
    render(<RedisConsole dbSessionId={WIRE} panelId={PANEL_0} dbIndex={0} />);
    const input = screen.getByTestId('redis-console-input') as HTMLTextAreaElement;
    expect(input.getAttribute('rows')).toBe('1');
    expect(input.className).toContain('py-2');
    // The prompt and the field share h-9 so the two baselines line up.
    const row = input.parentElement;
    expect(row?.className).toContain('h-9');
  });

  it('cannot grow to multiple lines or scroll vertically', () => {
    render(<RedisConsole dbSessionId={WIRE} panelId={PANEL_0} dbIndex={0} />);
    const input = screen.getByTestId('redis-console-input') as HTMLTextAreaElement;

    // `wrap="off"` is the load-bearing part: a textarea soft-wraps by default,
    // which is what let the field grow past three lines and start scrolling.
    expect(input.getAttribute('wrap')).toBe('off');
    // A fixed height clamps the box; the earlier `height: auto` undid `rows=1`.
    expect(input.className).toContain('h-9');
    expect(input.style.height).toBe('');
    // Horizontal scrolling is kept so a long command stays reachable.
    expect(input.className).toContain('overflow-y-hidden');
    expect(input.className).toContain('overflow-x-auto');
    expect(input.className).not.toContain('overflow-y-auto');
  });

  it('still runs a pasted multi-line batch as one graded batch', async () => {
    commandInvoke.mockResolvedValue({
      results: [
        { command: 'GET a', ok: true, value: '1', resultType: 'scalar' },
        { command: 'PING', ok: true, value: 'PONG', resultType: 'ok' },
      ],
    });
    render(<RedisConsole dbSessionId={WIRE} panelId={PANEL_0} dbIndex={0} />);
    const input = screen.getByTestId('redis-console-input') as HTMLTextAreaElement;

    // One line tall must not mean one line per command: the batch semantics that
    // PRD I-7 grades as a whole are what the textarea exists for.
    typeInto(input, 'GET a\nPING');
    fireEvent.keyDown(input, { key: 'Enter' });

    await waitFor(() =>
      expect(screen.getAllByTestId('redis-console-entry-result')).toHaveLength(2),
    );
    expect(commandInvoke).toHaveBeenCalledWith(
      'redis',
      'exec',
      expect.objectContaining({ commands: 'GET a\nPING' }),
    );
  });
});

describe('active sub-tab survives a top-level tab switch', () => {
  it('comes back on the console, not the item tab', async () => {
    ok('v');
    const first = render(
      <RedisRightPanel dbSessionId={WIRE} panelId={PANEL_0} dbIndex={0} {...detailProps} />,
    );

    // Land on the console sub-tab, like a user typing a command would.
    await waitFor(() => expect(screen.getByTestId('redis-right-tab-console')).toBeTruthy());
    fireEvent.click(screen.getByTestId('redis-right-tab-console'));
    await waitFor(() => expect(screen.getByTestId('redis-console-input')).toBeTruthy());
    const input = screen.getByTestId('redis-console-input') as HTMLTextAreaElement;
    typeInto(input, 'GET a');
    fireEvent.keyDown(input, { key: 'Enter' });
    await waitFor(() => expect(screen.getByText('v')).toBeTruthy());

    // The host drops the subtree and mounts it again on a top-level tab switch.
    first.unmount();
    render(<RedisRightPanel dbSessionId={WIRE} panelId={PANEL_0} dbIndex={0} {...detailProps} />);

    // Bug: this came back on 'detail' (the item sub-tab) with an empty console.
    expect(screen.getByTestId('redis-right-tab-console').getAttribute('data-active')).toBe('true');
    expect(screen.getByText('v')).toBeTruthy();
  });

  it('gives each db tab its own sub-tab although they share one dbSessionId', async () => {
    // Repro: db0 tab → console, open db1 tab, then return to db0. db1 must open
    // on the item sub-tab and db0 must still be on the console. Both tabs carry
    // the SAME `dbSessionId` (one connection), so keying state by it leaks the
    // console choice into the sibling — the bug this scopes by panel identity.
    const db0 = render(
      <RedisRightPanel dbSessionId={WIRE} panelId={PANEL_0} dbIndex={0} {...detailProps} />,
    );
    await waitFor(() => expect(screen.getByTestId('redis-right-tab-console')).toBeTruthy());
    fireEvent.click(screen.getByTestId('redis-right-tab-console'));
    await waitFor(() => expect(screen.getByTestId('redis-console-input')).toBeTruthy());
    expect(screen.getByTestId('redis-right-tab-console').getAttribute('data-active')).toBe('true');

    // Open the sibling db tab: the host unmounts db0 and mounts db1.
    db0.unmount();
    const db1 = render(
      <RedisRightPanel dbSessionId={WIRE} panelId={PANEL_1} dbIndex={1} {...detailProps} />,
    );
    await waitFor(() => expect(screen.getByTestId('redis-right-tab-detail')).toBeTruthy());
    expect(screen.getByTestId('redis-right-tab-detail').getAttribute('data-active')).toBe('true');
    expect(screen.getByTestId('redis-right-tab-console').getAttribute('data-active')).toBe('false');

    // Back to db0: its console is still the one that was left.
    db1.unmount();
    render(<RedisRightPanel dbSessionId={WIRE} panelId={PANEL_0} dbIndex={0} {...detailProps} />);
    await waitFor(() => expect(screen.getByTestId('redis-right-tab-console')).toBeTruthy());
    expect(screen.getByTestId('redis-right-tab-console').getAttribute('data-active')).toBe('true');
  });
});
