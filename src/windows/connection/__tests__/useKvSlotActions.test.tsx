/**
 * The host's single KV slot action dispatcher.
 *
 * The `useI18n` mock echoes keys back verbatim, so every copy assertion below
 * targets an i18n *key* rather than visible English.
 */
import { act, fireEvent, render, screen, waitFor, cleanup } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { KvSlotAction } from '@datazen/driver-sdk';
import { registerTranslations } from '@datazen/ui';
import { useKvSlotActions } from '../useKvSlotActions';

const { settings, windowManager } = vi.hoisted(() => ({
  settings: { safeMode: false },
  windowManager: { openSettingsWindow: vi.fn() },
}));

vi.mock('../../../hooks/useI18n', () => ({
  // Key-style mock: a mocked `t` echoes the key, so tests can assert on keys.
  useI18n: () => ({ t: (key: string) => key }),
}));

vi.mock('../../../stores/settingsStore', () => ({
  useSettingsStore: { getState: () => ({ settings }) },
}));

vi.mock('../../../lib/windowManager', () => windowManager);

/**
 * The `useI18n` mock above only replaces the HOST hook. The dispatcher's dialog
 * is a real `@datazen/ui` ConfirmDialog, which resolves its own three button
 * labels through the package's i18n engine — and a unit test runs with a fresh
 * module registry holding no host dictionaries at all. Seed them (the same
 * PROBE pattern the package's own dialog tests use) so the "this flow must stay
 * silent" guard below keeps testing the dispatcher, not the missing fixture.
 * The assertion itself is untouched: it is still a blanket "no console.warn".
 */
registerTranslations({
  en: {
    'common.close': 'PROBE::Close',
    'common.cancel': 'PROBE::Cancel',
    'common.confirm': 'PROBE::Confirm',
  },
});

/** Every action this build has no executor for (degradation path). */
const UNWIRED: KvSlotAction[] = [
  { type: 'newKey' },
  { type: 'import' },
  { type: 'export' },
  { type: 'openMonitor' },
  { type: 'setScanBudget', value: 10_000 },
];

/** Where the mounted dispatcher's `request` lives, for calls outside render. */
let request: (action: KvSlotAction) => void;

function Harness({
  onRefresh,
  onSelectDatabase,
}: {
  onRefresh: () => void;
  onSelectDatabase?: (database: string) => void;
}) {
  const dispatcher = useKvSlotActions({ onRefresh, onSelectDatabase });
  request = dispatcher.request;
  // The dialog is part of the dispatcher's return value: a host that forgot to
  // render it could not gate a dangerous action, so the tests drive it for real.
  return <>{dispatcher.dialog}</>;
}

const warnSpy = vi.spyOn(console, 'warn').mockImplementation(() => {});

beforeEach(() => {
  settings.safeMode = false;
  warnSpy.mockClear();
  windowManager.openSettingsWindow.mockClear();
});

afterEach(() => {
  cleanup();
});

describe('useKvSlotActions — wired actions', () => {
  it('routes refresh to the panel refresh sink and to nothing else', () => {
    const onRefresh = vi.fn();
    render(<Harness onRefresh={onRefresh} />);

    act(() => request({ type: 'refresh' }));

    expect(onRefresh).toHaveBeenCalledTimes(1);
    expect(warnSpy).not.toHaveBeenCalled();
    expect(windowManager.openSettingsWindow).not.toHaveBeenCalled();
    expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
  });

  it('routes openSettings to the section that hosts driver settings', () => {
    render(<Harness onRefresh={vi.fn()} />);

    act(() => request({ type: 'openSettings' }));

    expect(windowManager.openSettingsWindow).toHaveBeenCalledWith('extensions');
    expect(warnSpy).not.toHaveBeenCalled();
  });
});

describe('useKvSlotActions — selectDatabase is really wired, not warned', () => {
  it('forwards the db label to the host sink and warns about nothing', () => {
    const onSelectDatabase = vi.fn();
    render(<Harness onRefresh={vi.fn()} onSelectDatabase={onSelectDatabase} />);

    act(() => request({ type: 'selectDatabase', database: 'db3' }));

    expect(onSelectDatabase).toHaveBeenCalledTimes(1);
    expect(onSelectDatabase).toHaveBeenCalledWith('db3');
    // The whole point of the ruling: this is no longer a degradation path.
    expect(warnSpy).not.toHaveBeenCalled();
    expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
  });

  it('degrades to a warning when the host threaded no sink through', () => {
    const onRefresh = vi.fn();
    render(<Harness onRefresh={onRefresh} />);

    expect(() => {
      act(() => request({ type: 'selectDatabase', database: 'db3' }));
    }).not.toThrow();

    expect(warnSpy).toHaveBeenCalledTimes(1);
    expect(warnSpy.mock.calls[0]?.[0]).toContain('"selectDatabase"');
    // The warning must advertise that this one *is* wireable, so a host that
    // dropped the prop can tell a missing sink from a missing capability.
    expect(warnSpy.mock.calls[0]?.[0]).toContain('selectDatabase');
    expect(onRefresh).not.toHaveBeenCalled();
  });

  it('reaches the newest sink through the frozen request identity', () => {
    const stale = vi.fn();
    const fresh = vi.fn();
    const { rerender } = render(<Harness onRefresh={vi.fn()} onSelectDatabase={stale} />);
    const initial = request;

    rerender(<Harness onRefresh={vi.fn()} onSelectDatabase={fresh} />);
    act(() => initial({ type: 'selectDatabase', database: 'db9' }));

    expect(stale).not.toHaveBeenCalled();
    expect(fresh).toHaveBeenCalledWith('db9');
    expect(request).toBe(initial);
  });
});

describe('useKvSlotActions — unwired actions degrade, never throw', () => {
  for (const action of UNWIRED) {
    it(`warns once and ignores "${action.type}"`, () => {
      const onRefresh = vi.fn();
      render(<Harness onRefresh={onRefresh} />);

      expect(() => {
        act(() => request(action));
      }).not.toThrow();

      expect(onRefresh).not.toHaveBeenCalled();
      expect(warnSpy).toHaveBeenCalledTimes(1);
      // The warning names the missing handler *and* what is wired, so a slot
      // that offers more than this build supports is diagnosable from the log.
      expect(warnSpy.mock.calls[0]?.[0]).toContain(`"${action.type}"`);
      expect(warnSpy.mock.calls[0]?.[0]).toContain('refresh');
      expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
    });
  }

  it('ignores an action type this build does not know at all', () => {
    render(<Harness onRefresh={vi.fn()} />);

    // A newer driver slot may send a union member the host has not seen yet.
    expect(() => {
      act(() => request({ type: 'compactKeys' } as unknown as KvSlotAction));
    }).not.toThrow();

    expect(warnSpy).toHaveBeenCalledTimes(1);
    expect(warnSpy.mock.calls[0]?.[0]).toContain('"compactKeys"');
  });
});

describe('useKvSlotActions — flushDb passes the existing write gate (I-6)', () => {
  it('asks through the shared confirm dialog before anything else happens', () => {
    const onRefresh = vi.fn();
    render(<Harness onRefresh={onRefresh} />);

    act(() => request({ type: 'flushDb' }));

    const dialog = screen.getByRole('dialog');
    // Asserted by key: no English literal.
    expect(dialog.getAttribute('aria-label')).toBe('redis.kvSlot.flushTitle');
    expect(screen.getByText('redis.kvSlot.flushMessage')).toBeInTheDocument();
    expect(onRefresh).not.toHaveBeenCalled();
    expect(warnSpy).not.toHaveBeenCalled();
  });

  it('runs nothing when the user declines', async () => {
    const onRefresh = vi.fn();
    render(<Harness onRefresh={onRefresh} />);
    act(() => request({ type: 'flushDb' }));

    fireEvent.click(screen.getByTestId('confirm-dialog-cancel'));

    await waitFor(() => expect(screen.queryByRole('dialog')).not.toBeInTheDocument());
    expect(onRefresh).not.toHaveBeenCalled();
    expect(warnSpy).not.toHaveBeenCalled();
  });

  it('executes no driver command on the host side even after consent', async () => {
    // The gate is the host's job; running FLUSHDB is the driver's `flush_db`,
    // and the host naming a driver command is the hardcoding this layering forbids.
    // So consent ends at the warning, and the warning arrives only *after* it.
    const onRefresh = vi.fn();
    render(<Harness onRefresh={onRefresh} />);
    act(() => request({ type: 'flushDb' }));
    expect(warnSpy).not.toHaveBeenCalled();

    fireEvent.click(screen.getByTestId('confirm-dialog-ok'));

    await waitFor(() => expect(warnSpy).toHaveBeenCalledTimes(1));
    expect(warnSpy.mock.calls[0]?.[0]).toContain('"flushDb"');
    expect(onRefresh).not.toHaveBeenCalled();
  });

  it('hard-blocks the flush while Safe Mode is on', async () => {
    settings.safeMode = true;
    const onRefresh = vi.fn();
    render(<Harness onRefresh={onRefresh} />);

    act(() => request({ type: 'flushDb' }));

    expect(screen.getByRole('dialog').getAttribute('aria-label')).toBe('settings.safeMode');
    expect(screen.getByText('redis.kvSlot.flushBlocked')).toBeInTheDocument();
    // Safe Mode replaces the prompt: there is no confirm path to click through.
    expect(screen.queryByText('redis.kvSlot.flushMessage')).not.toBeInTheDocument();

    fireEvent.click(screen.getByTestId('confirm-dialog-ok'));

    await waitFor(() => expect(screen.queryByRole('dialog')).not.toBeInTheDocument());
    // "Dismiss" is not consent: nothing proceeds and nothing warns.
    expect(onRefresh).not.toHaveBeenCalled();
    expect(warnSpy).not.toHaveBeenCalled();
  });

  it('reads Safe Mode at request time, not when the hook mounted', async () => {
    render(<Harness onRefresh={vi.fn()} />);

    act(() => request({ type: 'flushDb' }));
    expect(screen.getByRole('dialog').getAttribute('aria-label')).toBe('redis.kvSlot.flushTitle');
    fireEvent.click(screen.getByTestId('confirm-dialog-cancel'));
    await waitFor(() => expect(screen.queryByRole('dialog')).not.toBeInTheDocument());

    settings.safeMode = true;
    act(() => request({ type: 'flushDb' }));
    expect(screen.getByRole('dialog').getAttribute('aria-label')).toBe('settings.safeMode');
  });
});

describe('useKvSlotActions — request identity is stable', () => {
  it('keeps one function identity across re-renders with a new sink', () => {
    const { rerender } = render(<Harness onRefresh={vi.fn()} />);
    const initial = request;

    // A churning identity rides inside the memoised slot props bundle and would
    // re-render every driver slot on every workspace render.
    rerender(<Harness onRefresh={vi.fn()} />);
    expect(request).toBe(initial);
  });

  it('forwards through the original identity to the newest sink', () => {
    const stale = vi.fn();
    const fresh = vi.fn();
    const { rerender } = render(<Harness onRefresh={stale} />);
    const initial = request;

    rerender(<Harness onRefresh={fresh} />);
    act(() => initial({ type: 'refresh' }));

    expect(stale).not.toHaveBeenCalled();
    expect(fresh).toHaveBeenCalledTimes(1);
  });
});
