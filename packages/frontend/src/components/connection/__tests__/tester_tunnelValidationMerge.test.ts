/**
 * [tester] Round 2 — pins the *direction* of the driver-validator merge inside
 * `validate()` (tunnel-form-BUG-001 fix, commit `c9777246`).
 *
 * `Object.assign(errors, driverValidator(...))` runs **after** the tunnel-domain
 * prologue, so the driver result wins on any shared key. With the real redis
 * validator the two key spaces are disjoint — pinned by
 * `tester_tunnelValidationMatrix.test.ts` — so nothing is clobbered today.
 *
 * This file substitutes a synthetic validator whose key space deliberately
 * overlaps the tunnel domain to prove the merge is driver-wins. Consequence: the
 * "a dangling reference always blocks save" guarantee is *empirical* (no
 * colliding validator exists in this build), not structural. Recorded as a
 * latent risk in the track report, not as a defect.
 */
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { renderHook, act } from '@testing-library/react';
import { useConnectionForm } from '../useConnectionForm';
import { useTunnelStore } from '../../../stores/tunnelStore';
import type { ConnectionConfig } from '../../../types';

vi.mock('../../../hooks/useI18n', () => ({
  useI18n: () => ({ t: (key: string) => key }),
}));

vi.mock('../../../extensions/generated', async (importOriginal) => {
  const actual = await importOriginal<typeof import('../../../extensions/generated')>();
  /** Synthetic validator: `tunnelId` collides with the tunnel domain on purpose. */
  const collidingValidator = () => ({ tunnelId: 'driver-clobbered', host: 'driver-host' });
  return {
    ...actual,
    getDriverValidator: (formVariant: string) =>
      formVariant === 'redis' ? collidingValidator : actual.getDriverValidator(formVariant),
  };
});

const { saveConnectionMock } = vi.hoisted(() => ({ saveConnectionMock: vi.fn() }));

vi.mock('../../../commands/connection', () => ({
  connectionCommands: { testConnection: vi.fn(), saveConnection: saveConnectionMock },
}));

vi.mock('../../../stores/connectionStore', () => ({
  useConnectionStore: Object.assign(
    vi.fn((selector: (s: { saveConnection: typeof saveConnectionMock }) => unknown) =>
      selector({ saveConnection: saveConnectionMock }),
    ),
    { getState: () => ({ saveConnection: saveConnectionMock }) },
  ),
}));

const mockTunnelCommands = vi.hoisted(() => ({
  getTunnels: vi.fn(),
  getTunnelSummaries: vi.fn(),
  getTunnel: vi.fn(),
  getTunnelUsage: vi.fn(),
  saveTunnel: vi.fn(),
  deleteTunnel: vi.fn(),
  testTunnel: vi.fn(),
}));

vi.mock('../../../commands/tunnel', () => ({ tunnelCommands: mockTunnelCommands }));

beforeEach(() => {
  vi.clearAllMocks();
  useTunnelStore.setState({ summaries: [], loaded: false, loading: false, error: null });
  mockTunnelCommands.getTunnelSummaries.mockResolvedValue([]);
});

describe('[tester] merge direction of the driver validator', () => {
  it('test_tester a colliding driver key overwrites the tunnel-domain error', async () => {
    const { result } = renderHook(() =>
      useConnectionForm({
        editId: 'c-redis',
        existingConnections: [
          {
            id: 'c-redis',
            name: 'collision',
            databaseType: 'redis',
            host: 'cache.internal',
            port: 6379,
            sslMode: 'disable',
            tunnelId: 'tun_gone',
          } satisfies ConnectionConfig,
        ],
      }),
    );
    await act(async () => {
      await useTunnelStore.getState().load();
    });
    expect(result.current.tunnelRefMissing).toBe(true);

    act(() => {
      expect(result.current.validate()).toBe(false);
    });
    // Driver-wins: the tunnel domain's own message is replaced by the synthetic
    // driver key, which is exactly why a colliding validator would be a defect.
    expect(result.current.validationErrors.tunnelId).toBe('driver-clobbered');
    // The tunnel domain never clobbers the driver's keys (written first).
    expect(result.current.validationErrors.host).toBe('driver-host');

    await act(async () => {
      await result.current.onSave();
    });
    expect(saveConnectionMock).not.toHaveBeenCalled();
  });
});
