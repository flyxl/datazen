import { create } from 'zustand';
import { tunnelCommands } from '../commands/tunnel';
import { t } from '../locales/t';
import type { SavedTunnel, SavedTunnelSummary, TunnelUsage } from '../types';

/**
 * Tunnel collection — the single source of truth for saved-tunnel summaries
 * across the app. Replaces the mount-time-only fetch that made every open
 * connection form show a stale list (G5).
 *
 * Only the secret-free `SavedTunnelSummary` projection is kept in state: full
 * entities (with decrypted credentials) are fetched on demand for editing (G9).
 */
export interface TunnelStore {
  summaries: SavedTunnelSummary[];
  /** True once the first load attempt settled (success or failure). */
  loaded: boolean;
  loading: boolean;
  error: string | null;

  /** Fetch summaries; short-circuits when already loaded unless `force`. Never throws. */
  load: (force?: boolean) => Promise<void>;
  /** Persist a new entity (id generated when absent) and refresh the collection. */
  create: (input: SavedTunnelInput) => Promise<SavedTunnel>;
  /** Persist changes to an existing entity and refresh the collection. */
  update: (tunnel: SavedTunnel) => Promise<void>;
  /** Delete an entity and refresh the collection. */
  remove: (id: string) => Promise<void>;
  /** Connections referencing this tunnel (`get_tunnel_usage`). */
  usage: (id: string) => Promise<TunnelUsage>;
}

/** New tunnel entities get a `tun_` prefix so they are never confused with connection ids. */
export function newTunnelId(): string {
  return `tun_${Math.random().toString(36).slice(2, 10)}`;
}

export type SavedTunnelInput = Omit<SavedTunnel, 'id'> & { id?: string };

function errorMessage(e: unknown): string {
  if (typeof e === 'string' && e) return e;
  if (e instanceof Error && e.message) return e.message;
  return t('tunnelStore.loadFailed');
}

export const useTunnelStore = create<TunnelStore>((set, get) => ({
  summaries: [],
  loaded: false,
  loading: false,
  error: null,

  load: async (force = false) => {
    if (get().loaded && !force) return;
    set({ loading: true, error: null });
    try {
      const summaries = await tunnelCommands.getTunnelSummaries();
      set({ summaries, loaded: true, loading: false });
    } catch (e) {
      // Degrade to an empty collection; callers render the "no saved tunnel" guidance.
      set({ summaries: [], loaded: true, loading: false, error: errorMessage(e) });
    }
  },

  create: async (input) => {
    const tunnel: SavedTunnel = { ...input, id: input.id ?? newTunnelId() };
    await tunnelCommands.saveTunnel(tunnel);
    await get().load(true);
    return tunnel;
  },

  update: async (tunnel) => {
    await tunnelCommands.saveTunnel(tunnel);
    await get().load(true);
  },

  remove: async (id) => {
    await tunnelCommands.deleteTunnel(id);
    await get().load(true);
  },

  usage: (id) => tunnelCommands.getTunnelUsage(id),
}));
