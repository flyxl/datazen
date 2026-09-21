/**
 * RedisConsole completion journey (R11/R12)
 *
 * Drives the real textarea through a full keystroke sequence so the completion
 * state machine is exercised end-to-end (enter → navigate → accept → exit),
 * rather than asserting on a static legal statement.
 */
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { create } from 'zustand';
import {
  bindConfirmDialog,
  bindConnectionStore,
  bindSettingsStore,
  type ConnectionBridgeState,
  type SettingsBridgeState,
} from '@datazen/driver-sdk';

const scanKeys = vi.fn();
const commandInvoke = vi.fn();

// Components take `useI18n` from the single @datazen/ui runtime; keep the
// assertions locale-independent by overriding only that hook.
vi.mock('@datazen/ui', async (importOriginal) => ({
  ...(await importOriginal<typeof import('@datazen/ui')>()),
  useI18n: () => ({ t: (key: string) => key }),
}));

vi.mock('../shared/redisInvoke', () => ({
  redisCommandInvoke: (...args: unknown[]) => commandInvoke(...args),
  invokeScanKeys: (...args: unknown[]) => scanKeys(...args),
}));

import { RedisConsole } from '../console/RedisConsole';

// Harness capability bindings (host injects the real ones at startup).
bindSettingsStore(
  create<SettingsBridgeState>(() => ({
    settings: { safeMode: false, editorFontFamily: '', driverSettings: {} },
  })),
);
bindConnectionStore(create<ConnectionBridgeState>(() => ({ connections: [] })));
bindConfirmDialog(() => [async () => true, null]);

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
});

beforeEach(() => {
  commandInvoke.mockResolvedValue({ results: [] });
  scanKeys.mockResolvedValue({
    keys: [{ key: 'user:1' }, { key: 'user:2' }],
    cursor: 0,
    done: true,
  });
});

function setInput(input: HTMLTextAreaElement, value: string, cursor: number = value.length) {
  fireEvent.change(input, {
    target: { value, selectionStart: cursor, selectionEnd: cursor },
  });
}

describe('RedisConsole completion — command mode', () => {
  it('opens popup on "SE" listing SET-family candidates', async () => {
    render(<RedisConsole dbSessionId="sess-1" dbIndex={0} />);
    const input = screen.getByTestId('redis-console-input') as HTMLTextAreaElement;

    setInput(input, 'SE');

    await waitFor(() => {
      expect(screen.getByTestId('redis-completion-popup')).toBeTruthy();
    });
    const options = screen.getAllByRole('option');
    expect(options.length).toBeGreaterThan(0);
    expect(options.some((o) => o.textContent?.includes('SET'))).toBe(true);
    // exactly one active candidate on open
    expect(options.filter((o) => o.getAttribute('aria-selected') === 'true')).toHaveLength(1);
  });

  it('ArrowDown moves the highlight and Tab accepts (appends a space, closes popup)', async () => {
    render(<RedisConsole dbSessionId="sess-1" dbIndex={0} />);
    const input = screen.getByTestId('redis-console-input') as HTMLTextAreaElement;

    setInput(input, 'SE');
    await waitFor(() => {
      expect(screen.getByTestId('redis-completion-popup')).toBeTruthy();
    });

    const initialActive = screen
      .getAllByRole('option')
      .find((o) => o.getAttribute('aria-selected') === 'true')!;

    fireEvent.keyDown(input, { key: 'ArrowDown' });
    await waitFor(() => {
      const moved = screen
        .getAllByRole('option')
        .find((o) => o.getAttribute('aria-selected') === 'true');
      expect(moved).toBeTruthy();
      expect(moved).not.toBe(initialActive);
    });

    const active = screen
      .getAllByRole('option')
      .find((o) => o.getAttribute('aria-selected') === 'true')!;
    // Command name lives in the semibold span (first line of the option).
    const commandName = active.querySelector('span')!.textContent!.trim();
    expect(commandName).toMatch(/^[A-Z]+$/);

    fireEvent.keyDown(input, { key: 'Tab' });
    await waitFor(() => {
      expect(input.value).toBe(`${commandName} `);
    });
    expect(screen.queryByTestId('redis-completion-popup')).toBeNull();
  });

  it('Escape dismisses the popup without editing the text', async () => {
    render(<RedisConsole dbSessionId="sess-1" dbIndex={0} />);
    const input = screen.getByTestId('redis-console-input') as HTMLTextAreaElement;

    setInput(input, 'SE');
    await waitFor(() => {
      expect(screen.getByTestId('redis-completion-popup')).toBeTruthy();
    });

    fireEvent.keyDown(input, { key: 'Escape' });
    await waitFor(() => {
      expect(screen.queryByTestId('redis-completion-popup')).toBeNull();
    });
    expect(input.value).toBe('SE');
  });
});

describe('RedisConsole completion — key mode', () => {
  it('fetches key candidates for the argument token and accepts with Enter', async () => {
    render(<RedisConsole dbSessionId="sess-1" dbIndex={0} />);
    const input = screen.getByTestId('redis-console-input') as HTMLTextAreaElement;

    setInput(input, 'SET user');

    await waitFor(() => {
      expect(screen.getByTestId('redis-completion-popup')).toBeTruthy();
    });
    expect(scanKeys).toHaveBeenCalledWith('sess-1', 0, 'user*', 0, 200);

    const first = screen.getByTestId('redis-completion-item-0');
    expect(first.textContent).toContain('user:1');

    // Enter accepts the candidate (no execution, popup closes with trailing space).
    fireEvent.keyDown(input, { key: 'Enter' });
    await waitFor(() => {
      expect(input.value).toBe('SET user:1 ');
    });
    expect(commandInvoke).not.toHaveBeenCalled();
    expect(screen.queryByTestId('redis-completion-popup')).toBeNull();
  });
});
