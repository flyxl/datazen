import { legacyInvoke as invoke } from '@datazen/backend-client';
import type { SavedTunnel, SavedTunnelSummary, TunnelUsage } from '../types';

export const tunnelCommands = {
  /** Full entities, credentials included — only for editing a single tunnel. */
  getTunnels: () => invoke<SavedTunnel[]>('get_tunnels'),

  /** Secret-free projection for lists and pickers. */
  getTunnelSummaries: () => invoke<SavedTunnelSummary[]>('get_tunnel_summaries'),

  getTunnel: (id: string) => invoke<SavedTunnel | null>('get_tunnel', { id }),

  getTunnelUsage: (id: string) => invoke<TunnelUsage>('get_tunnel_usage', { id }),

  saveTunnel: (tunnel: SavedTunnel) => invoke<void>('save_tunnel', { tunnel }),

  deleteTunnel: (id: string) => invoke<void>('delete_tunnel', { id }),

  /** Round-trip probe through a saved tunnel; resolves to elapsed milliseconds. */
  testTunnel: (id: string, targetHost: string, targetPort: number) =>
    invoke<number>('test_tunnel', { id, targetHost, targetPort }),
};
