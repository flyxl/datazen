/**
 * Desktop tables exporter (form-split F4 dual-flow demonstration, plan §3.3).
 *
 * The desktop implementation of `TablesExporterPort`: the whole export streams
 * on the Rust side through its own native save dialog (the path never returns
 * to JS), and writer progress rides the `EventChannel` rail — the host
 * broadcast bus — instead of a direct host-runtime import.
 */

import { invoke } from '@tauri-apps/api/core';
import type {
  TablesExporterPort,
  TablesExportProgressEvent,
  TablesExportRequest,
  TablesExportResult,
} from '@datazen/frontend/src/platform/contracts';
import { requirePlatform } from '@datazen/frontend/src/platform/contracts';

export function createDesktopTablesExporter(): TablesExporterPort {
  return {
    exportTables(request: TablesExportRequest): Promise<TablesExportResult | null> {
      return invoke<TablesExportResult | null>('export_tables_stream', { request });
    },

    async onExportProgress(handler: (event: TablesExportProgressEvent) => void) {
      // Host broadcast — the same rail the menu and cross-window events ride.
      return requirePlatform().events.listenCrossWindow('batch-export-progress', (payload) =>
        handler(payload as TablesExportProgressEvent),
      );
    },
  };
}
