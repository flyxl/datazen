import { useCallback, useEffect, useState } from 'react';
import { Button } from '../ui/Button';
import { Dialog } from '../ui/Dialog';
import { BinaryViewer, JsonViewer } from '@datazen/ui';
import type { ColumnDef } from './TableHeader';
import { toEditString } from './EditableCell';
import { fileCommands } from '../../commands/file';
import { bytesToBase64 } from '../../lib/base64';
import { useI18n } from '../../hooks/useI18n';

/**
 * Quick Look (DBR-05): the quick viewer of one cell's content, shared by the
 * detail drawer's per-field button and the grid's own quick entries — the
 * middle-click on a cell and the `Cmd/Ctrl+Shift+L` shortcut.
 *
 * Extracted from `DetailPanel` when the grid got its own entries: two owners
 * means the dialog cannot live inside one of them.
 */

const BLOB_TYPE_KEYWORDS = ['blob', 'binary', 'bytea', 'image', 'raw'];

/**
 * Whether a column *type name* reads as binary.
 *
 * Kept as a substring test on the type name — the same verdict the detail
 * drawer has always given — because the dialog may be opened from a surface
 * that has no `cellTypeHints` (a query-result grid); a cell resolved as
 * `bytes` through the driver rules shows the viewer either way.
 */
export function isBlobType(type: string): boolean {
  return BLOB_TYPE_KEYWORDS.some((k) => type.includes(k));
}

/**
 * The bytes of a binary value, when they are bytes (DB-13).
 *
 * The wire carries binary as an untagged array of numbers; a driver that
 * reports a blob as a string (an escaped `\x…` text, say) is left to the
 * text rendering — guessing an encoding here would invent bytes that are not
 * in the cell.
 */
export function binaryValueBytes(value: unknown): Uint8Array | null {
  if (Array.isArray(value) && value.every((item) => typeof item === 'number')) {
    return new Uint8Array(value);
  }
  return null;
}

export function isJsonValue(v: unknown): boolean {
  if (v === null || v === undefined) return false;
  if (typeof v === 'object') return true;
  if (typeof v === 'string') {
    const s = v.trim();
    if (!s.startsWith('{') && !s.startsWith('[')) return false;
    try {
      JSON.parse(s);
      return true;
    } catch {
      return false;
    }
  }
  return false;
}

export function prettyJson(v: unknown): string | null {
  if (v === null || v === undefined) return null;
  let parsed: unknown = v;
  if (typeof v === 'string') {
    const s = v.trim();
    if (!s.startsWith('{') && !s.startsWith('[')) return null;
    try {
      parsed = JSON.parse(s);
    } catch {
      return null;
    }
  }
  if (typeof parsed !== 'object') return null;
  try {
    return JSON.stringify(parsed, null, 2);
  } catch {
    return null;
  }
}

/**
 * The raw JSON text of a value (DB-13): an object serialises compactly, a
 * string passes through untouched — the JsonViewer's `raw` mode is exactly
 * what the cell held, not a re-format of it.
 */
function rawJsonText(value: unknown): string {
  if (typeof value === 'string') return value;
  if (value === null || value === undefined) return '';
  return JSON.stringify(value) ?? '';
}

/**
 * Whether a value is worth a Quick Look at all (DBR-05): binary content,
 * structured JSON, or a long text. The quick entries on the grid ask this
 * before they open the dialog — the same verdict the detail drawer's
 * per-field button is built on.
 */
export function isQuickLookableValue(value: unknown, columnType?: string): boolean {
  if (value === null || value === undefined) return false;
  const type = (columnType ?? '').toLowerCase();
  if (isBlobType(type)) return true;
  if (isJsonValue(value)) return true;
  return typeof value === 'string' && value.length > 120;
}

export interface QuickLookDialogProps {
  open: boolean;
  column: ColumnDef;
  value: unknown;
  editable?: boolean;
  onCommit?: (value: unknown) => void;
  onClose: () => void;
}

export function QuickLookDialog({
  open,
  column,
  value,
  editable,
  onCommit,
  onClose,
}: QuickLookDialogProps) {
  const { t } = useI18n();
  const pretty = prettyJson(value);
  const [local, setLocal] = useState(pretty ?? toEditString(value));
  const type = (column.type ?? '').toLowerCase();
  const isJson = type.includes('json') || isJsonValue(value);
  const blobBytes = isBlobType(type) ? binaryValueBytes(value) : null;
  const canEdit = editable === true && onCommit !== undefined;

  useEffect(() => {
    if (open) setLocal(pretty ?? toEditString(value));
  }, [open, value, pretty]);

  /** The save exit of the binary viewer (DB-13): the host owns the file dialog. */
  const saveBlobAsFile = useCallback((bytes: Uint8Array, suggestedName: string) => {
    const extension = suggestedName.includes('.')
      ? (suggestedName.split('.').pop() ?? 'bin')
      : 'bin';
    void fileCommands.saveBase64WithDialog(
      bytesToBase64(bytes),
      suggestedName,
      extension.toUpperCase(),
      [extension],
    );
  }, []);

  const handleSave = useCallback(() => {
    if (!onCommit) return;
    if (local === '') {
      onCommit(null);
      onClose();
      return;
    }
    if (pretty !== null) {
      try {
        onCommit(JSON.parse(local));
      } catch {
        onCommit(local);
      }
    } else if (type.includes('int') || type.includes('serial') || type.includes('bigint')) {
      onCommit(Number(local));
    } else if (type.includes('bool')) {
      onCommit(local === 'true');
    } else if (
      type.includes('float') ||
      type.includes('double') ||
      type.includes('numeric') ||
      type.includes('decimal') ||
      type.includes('real')
    ) {
      onCommit(Number(local));
    } else {
      onCommit(local);
    }
    onClose();
  }, [onCommit, local, pretty, type, onClose]);

  return (
    <Dialog
      open={open}
      title={column.name}
      description={
        column.type ? `${t('detail.quickLook')} · ${column.type}` : t('detail.quickLook')
      }
      onClose={onClose}
      className="max-w-2xl"
      footer={
        <>
          <Button variant="secondary" onClick={onClose}>
            {t('common.cancel')}
          </Button>
          {/* Binary is viewed and saved, never hand-typed (DBR-05): a blob's
              "edit" entry is the file loader, not a textarea of the wire's
              number array. */}
          {canEdit && blobBytes == null && (
            <Button variant="primary" onClick={handleSave}>
              {t('detail.save')}
            </Button>
          )}
        </>
      }
    >
      {blobBytes != null ? (
        <BinaryViewer
          bytes={blobBytes}
          fileName={column.name}
          onSaveAsFile={saveBlobAsFile}
          testId="detail-quick-look-binary"
        />
      ) : canEdit ? (
        <textarea
          className="max-h-[50vh] w-full resize-y rounded-md border border-edge bg-surface p-3 font-mono text-xs text-fg focus:border-accent focus:outline-none focus:ring-1 focus:ring-accent/30"
          value={local}
          onChange={(e) => setLocal(e.target.value)}
          rows={12}
          wrap="off"
        />
      ) : isJson ? (
        // DB-13: the structured read — tree / raw / pretty / minify, from the
        // same raw text the cell holds. The edit path above keeps its editor.
        <JsonViewer text={rawJsonText(value)} testId="detail-quick-look-json" />
      ) : (
        <pre className="max-h-[50vh] overflow-auto whitespace-pre-wrap break-all rounded-md border border-edge bg-surface p-3 font-mono text-xs text-fg">
          {local || 'NULL'}
        </pre>
      )}
    </Dialog>
  );
}
