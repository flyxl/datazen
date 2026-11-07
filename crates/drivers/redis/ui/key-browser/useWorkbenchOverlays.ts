import { useCallback, useState } from 'react';
import {
  openKeyCtxDelete,
  openKeyCtxRename,
  openKeyCtxTtl,
  type KeyCtxDialog,
} from './KeyWorkbenchDialogs';

/**
 * Transient overlays of the key workbench: the batch-result banner, the import /
 * export dialog, the create-key dialog, the FLUSH confirmation target and the
 * row context-menu dialog queue.
 *
 * Grouped in one hook (D-0) because they share exactly one property: none of
 * them owns data, they are all "which sheet is on top right now". Keeping them
 * together is what lets `RedisWorkbench` stay a composition instead of a wall of
 * `useState` calls.
 */

export interface WorkbenchOverlays {
  /** `null` ⇒ hidden; otherwise the one-line result of the last write. */
  batchSummary: string | null;
  setBatchSummary: (message: string | null) => void;
  importExportOpen: boolean;
  setImportExportOpen: (open: boolean) => void;
  createOpen: boolean;
  setCreateOpen: (open: boolean) => void;
  /** `null` ⇒ closed; `'db' | 'all'` ⇒ which FLUSH is being confirmed. */
  flushDialog: 'db' | 'all' | null;
  setFlushDialog: (target: 'db' | 'all' | null) => void;
  keyCtxDialog: KeyCtxDialog;
  setKeyCtxDialog: (dialog: KeyCtxDialog) => void;
  /** Row context-menu entry point: opens exactly one of the three key dialogs. */
  openKeyCtx: (key: string, action: KeyCtxAction) => void;
}

export type KeyCtxAction = 'ttl' | 'rename' | 'delete';

const KEY_CTX_OPENERS: Record<KeyCtxAction, (key: string) => KeyCtxDialog> = {
  ttl: openKeyCtxTtl,
  rename: openKeyCtxRename,
  delete: openKeyCtxDelete,
};

export function useWorkbenchOverlays(): WorkbenchOverlays {
  const [batchSummary, setBatchSummary] = useState<string | null>(null);
  const [importExportOpen, setImportExportOpen] = useState(false);
  const [createOpen, setCreateOpen] = useState(false);
  const [flushDialog, setFlushDialog] = useState<'db' | 'all' | null>(null);
  const [keyCtxDialog, setKeyCtxDialog] = useState<KeyCtxDialog>(null);

  const openKeyCtx = useCallback((key: string, action: KeyCtxAction) => {
    setKeyCtxDialog(KEY_CTX_OPENERS[action](key));
  }, []);

  return {
    batchSummary,
    setBatchSummary,
    importExportOpen,
    setImportExportOpen,
    createOpen,
    setCreateOpen,
    flushDialog,
    setFlushDialog,
    keyCtxDialog,
    setKeyCtxDialog,
    openKeyCtx,
  };
}
