import { useEffect } from 'react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { act, cleanup, fireEvent, render, screen } from '@testing-library/react';
import { useSettingsStore } from '../../../../../src/stores/settingsStore';
import { useRedisGate } from '../useRedisGate';

vi.mock('../../../../../src/hooks/useI18n', () => ({
  useI18n: () => ({
    t: (key: string) => key,
  }),
}));

let latestGateWrite: ((level: string, command?: string) => Promise<boolean>) | null = null;

function Harness() {
  const { gateWrite, gateDialog } = useRedisGate();
  useEffect(() => {
    latestGateWrite = gateWrite;
  }, [gateWrite]);
  return <>{gateDialog}</>;
}

function setSafeMode(value: boolean) {
  useSettingsStore.setState((s) => ({
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
    setSafeMode(false);
    vi.clearAllMocks();
  });

  it('Safe Mode ON blocks a write-op and resolves false after dismiss', async () => {
    setSafeMode(true);
    render(<Harness />);
    let result: boolean | undefined;
    await act(async () => {
      gateCall('write-op').then((r) => {
        result = r;
      });
    });
    const ok = await screen.findByTestId('confirm-dialog-ok');
    await act(async () => {
      fireEvent.click(ok);
    });
    expect(result).toBe(false);
  });

  it('Safe Mode OFF lets a write-op through without a dialog', async () => {
    setSafeMode(false);
    render(<Harness />);
    let result: boolean | undefined;
    await act(async () => {
      result = await gateCall('write-op');
    });
    expect(result).toBe(true);
    expect(screen.queryByTestId('confirm-dialog-ok')).toBeNull();
  });

  it('danger level confirms when Safe Mode is OFF', async () => {
    setSafeMode(false);
    render(<Harness />);
    let result: boolean | undefined;
    await act(async () => {
      gateCall('danger', 'DEL key').then((r) => {
        result = r;
      });
    });
    const ok = await screen.findByTestId('confirm-dialog-ok');
    await act(async () => {
      fireEvent.click(ok);
    });
    expect(result).toBe(true);
  });

  it('danger level cancels to false', async () => {
    setSafeMode(false);
    render(<Harness />);
    let result: boolean | undefined;
    await act(async () => {
      gateCall('danger', 'DEL key').then((r) => {
        result = r;
      });
    });
    const cancel = await screen.findByTestId('confirm-dialog-cancel');
    await act(async () => {
      fireEvent.click(cancel);
    });
    expect(result).toBe(false);
  });
});
