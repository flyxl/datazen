import { invoke } from '@tauri-apps/api/core';
import type { ThemePackSummary } from '../types/themePack';

export const themeCommands = {
  listThemePacks: () => invoke<ThemePackSummary[]>('list_theme_packs'),

  installThemePackWithDialog: () =>
    invoke<ThemePackSummary>('install_theme_pack_with_dialog'),

  removeThemePack: (id: string) => invoke<void>('remove_theme_pack', { id }),

  readThemePackFile: (id: string, relativePath: string) =>
    invoke<number[]>('read_theme_pack_file', { id, relativePath }),

  setSurfaceBackground: (hex: string, dark: boolean) =>
    invoke<void>('set_surface_background', { hex, dark }),
};
