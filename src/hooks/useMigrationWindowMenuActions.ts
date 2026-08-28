import { useEffect } from 'react';
import { settingsCommands } from '../commands/settings';
import { listenCrossWindow } from '../lib/crossWindowBus';
import {
  openBackupWindow,
  openDataSyncWindow,
  openDataTransferWindow,
  openSchemaDiffWindow,
} from '../lib/windowManager';

/**
 * Open migration tool sub-windows from native / in-app menus.
 * Mounted on the main window shell only (not ConnectionPage) so a single
 * listener survives ConnectionPage remounts and StrictMode races.
 */
export function useMigrationWindowMenuActions() {
  useEffect(() => {
    let cancelled = false;
    const cleanups: Array<() => void> = [];

    const subscribe = (event: string, handler: () => void) => {
      void listenCrossWindow(event, handler)
        .then((unlisten) => {
          if (cancelled) unlisten();
          else cleanups.push(unlisten);
        })
        .catch(() => {
          // Non-Tauri / tests
        });
    };

    subscribe('menu:data-sync', () => openDataSyncWindow());
    subscribe('menu:data-transfer', () => openDataTransferWindow());
    subscribe('menu:schema-diff', () => openSchemaDiffWindow());
    subscribe('menu:backup', () => openBackupWindow('backup'));
    subscribe('menu:restore', () => openBackupWindow('restore'));
    subscribe('menu:view-logs', () => {
      void settingsCommands.openLogDir();
    });

    return () => {
      cancelled = true;
      for (const fn of cleanups) fn();
    };
  }, []);
}
