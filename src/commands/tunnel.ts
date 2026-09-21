import { invoke } from '@tauri-apps/api/core';
import type { SavedTunnel } from '../types';

export const tunnelCommands = {
  getTunnels: () => invoke<SavedTunnel[]>('get_tunnels'),

  getTunnel: (id: string) => invoke<SavedTunnel | null>('get_tunnel', { id }),

  saveTunnel: (tunnel: SavedTunnel) => invoke<void>('save_tunnel', { tunnel }),

  deleteTunnel: (id: string) => invoke<void>('delete_tunnel', { id }),
};
