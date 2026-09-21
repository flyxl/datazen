import { useEffect } from 'react';
import { afterEach, beforeAll, describe, expect, it, vi } from 'vitest';
import { act, cleanup, render } from '@testing-library/react';
import { create } from 'zustand';
import {
  bindSettingsStore,
  bindConfirmDialog,
  useBoundSettingsStore,
  type ConfirmDialogFn,
  type ConfirmDialogOptions,
  type SettingsBridgeState,
} from '@datazen/driver-sdk';
import { useRedisGate } from '../shared/useRedisGate';

// Components take `useI18n` from the single @datazen/ui runtime; keep the
// assertions locale-independent by overriding only that hook.
vi.mock('@datazen/ui', async (importOriginal) => ({
  ...(await importOriginal<typeof import('@datazen/ui')>()),
  useI18n: () => ({
    t: (key: string) => key,
  }),
}));

// Harness capability bindings: drivers consume the settings store and the
// confirm dialog through @datazen/driver-sdk bridges (the host injects its
// real implementations at startup; tests bind fakes here).
const harnessSettingsStore = create<SettingsBridgeState>(() => ({
  settings: {
    safeMode: false,
    editorFontFamily: '',
    driverSettings: {},
  },
}));

beforeAll(() => {
  bindSettingsStore(harnessSettingsStore);
});

let latestGateWrite: ((level: string, command?: string) => Promise<boolean>) | null = null;
let lastConfirmOptions: ConfirmDialogOptions | null = null;
let resolveLastConfirm: ((value: boolean) => void) | null = null;

function Harness() {
  const { gateWrite, gateDialog } = useRedisGate();
  useEffect(() => {
    latestGateWrite = gateWrite;
  }, [gateWrite]);
  return <>{gateDialog}</>;
}

function setSafeMode(value: boolean) {
  useBoundSettingsStore.setState((s) => ({
    settings: { ...s.settings, safeMode: value },
  }));
}

function gateCall(level: string, command?: string): Promise<boolean> {
  if (!latestGateWrite) throw new Error('gate not mounted');
  return latestGateWrite(level, command);
}

describe('useRedisGate', () => {
  afterEach(() => {
    cleanup();
    latestGateWrite = null;
    lastConfirmOptions = null;
    resolveLastConfirm = null;
    setSafeMode(false);
    vi.clearAllMocks();
  });

  it('Safe Mode ON blocks a write-op and resolves false after dismiss', async () => {
    setSafeMode(true);
    const confirmFn = vi.fn<ConfirmDialogFn>(
      (options) =>
        new Promise<boolean>((resolve) => {
          lastConfirmOptions = options;
          resolveLastConfirm = resolve;
        }),
    );
    bindConfirmDialog(() => [confirmFn, null]);
    render(<Harness />);
    let result: boolean | undefined;
    await act(async () => {
      gateCall('write-op').then((r) => {
        result = r;
      });
    });
    expect(lastConfirmOptions?.title).toBe('settings.safeMode');
    await act(async () => {
      resolveLastConfirm?.(true);
    });
    expect(result).toBe(false);
  });

  it('Safe Mode OFF lets a write-op through without a dialog', async () => {
    setSafeMode(false);
    const confirmFn = vi.fn<ConfirmDialogFn>(async () => true);
    bindConfirmDialog(() => [confirmFn, null]);
    render(<Harness />);
    let result: boolean | undefined;
    await act(async () => {
      result = await gateCall('write-op');
    });
    expect(result).toBe(true);
    expect(confirmFn).not.toHaveBeenCalled();
  });

  it('danger level confirms when Safe Mode is OFF', async () => {
    setSafeMode(false);
    const confirmFn = vi.fn<ConfirmDialogFn>(
      (options) =>
        new Promise<boolean>((resolve) => {
          lastConfirmOptions = options;
          resolveLastConfirm = resolve;
        }),
    );
    bindConfirmDialog(() => [confirmFn, null]);
    render(<Harness />);
    let result: boolean | undefined;
    await act(async () => {
      gateCall('danger', 'DEL key').then((r) => {
        result = r;
      });
    });
    expect(lastConfirmOptions?.codePreview).toBe('DEL key');
    await act(async () => {
      resolveLastConfirm?.(true);
    });
    expect(result).toBe(true);
  });

  it('danger level cancels to false', async () => {
    setSafeMode(false);
    const confirmFn = vi.fn<ConfirmDialogFn>(
      () =>
        new Promise<boolean>((resolve) => {
          resolveLastConfirm = resolve;
        }),
    );
    bindConfirmDialog(() => [confirmFn, null]);
    render(<Harness />);
    let result: boolean | undefined;
    await act(async () => {
      gateCall('danger', 'DEL key').then((r) => {
        result = r;
      });
    });
    await act(async () => {
      resolveLastConfirm?.(false);
    });
    expect(result).toBe(false);
  });
});
