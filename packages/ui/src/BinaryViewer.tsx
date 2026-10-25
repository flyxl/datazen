import { useMemo } from 'react';
import { Button } from './Button';
import { t, useI18n } from './i18n';

/**
 * The binary viewer shared by the host grid and the driver UIs (DB-13).
 *
 * Three answers to "what is in these bytes": a bounded hex dump (16 bytes per
 * row with an ASCII gutter, the shape every hex editor speaks), an inline
 * image preview when the bytes carry a known image signature, and a
 * save-as-file exit — the one way to inspect a BLOB with the tool that
 * actually understands it.
 *
 * The hex rendering itself lives in {@link hexRows} so the Redis value view
 * renders the exact same rows from the exact same code (R9's `renderHex` moved
 * here verbatim): one renderer, two consumers, no drift.
 */
export interface HexRow {
  offset: number;
  hex: string;
  ascii: string;
}

/** Max rows rendered for hex views (keeps the DOM bounded on multi-MB blobs). */
export const MAX_HEX_ROWS = 4096;

const BYTES_PER_HEX_ROW = 16;

/** Printable ASCII 0x21..0x7e; space and controls render as '.'. */
function asciiGutter(byte: number): string {
  return byte >= 0x21 && byte <= 0x7e ? String.fromCharCode(byte) : '.';
}

/**
 * The hex rows of a payload, up to `maxRows`. The row shape is the one the
 * Redis value viewer renders (`offset` / `hex` / `ascii`), so the two hex
 * dumps of the same bytes are byte-identical.
 */
export function hexRows(bytes: Uint8Array, maxRows: number = MAX_HEX_ROWS): HexRow[] {
  const rows: HexRow[] = [];
  const rowCount = Math.ceil(bytes.length / BYTES_PER_HEX_ROW);
  const limit = Math.min(rowCount, maxRows);
  for (let r = 0; r < limit; r++) {
    const start = r * BYTES_PER_HEX_ROW;
    const slice = bytes.subarray(start, start + BYTES_PER_HEX_ROW);
    let hex = '';
    let ascii = '';
    for (let i = 0; i < BYTES_PER_HEX_ROW; i++) {
      if (i < slice.length) {
        hex += slice[i].toString(16).padStart(2, '0') + ' ';
        ascii += asciiGutter(slice[i]);
      } else {
        hex += '   ';
      }
    }
    rows.push({ offset: start, hex: hex.trimEnd(), ascii });
  }
  return rows;
}

/** The image kinds a byte signature can prove — anything else renders as hex only. */
const IMAGE_SIGNATURES: { mime: string; ext: string; head: number[] }[] = [
  { mime: 'image/png', ext: 'png', head: [0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a] },
  { mime: 'image/jpeg', ext: 'jpg', head: [0xff, 0xd8, 0xff] },
  { mime: 'image/gif', ext: 'gif', head: [0x47, 0x49, 0x46, 0x38] },
  { mime: 'image/webp', ext: 'webp', head: [0x52, 0x49, 0x46, 0x46] },
  { mime: 'image/bmp', ext: 'bmp', head: [0x42, 0x4d] },
];

function sniffImage(bytes: Uint8Array): { mime: string; ext: string } | null {
  for (const signature of IMAGE_SIGNATURES) {
    if (bytes.length < signature.head.length) continue;
    if (signature.head.every((byte, index) => bytes[index] === byte)) {
      return { mime: signature.mime, ext: signature.ext };
    }
  }
  return null;
}

function bytesToBase64(bytes: Uint8Array): string {
  let binary = '';
  const chunk = 0x8000;
  for (let i = 0; i < bytes.length; i += chunk) {
    binary += String.fromCharCode(...bytes.subarray(i, i + chunk));
  }
  return btoa(binary);
}

export interface BinaryViewerProps {
  /** The payload; nothing is fetched here — the caller already holds the bytes. */
  bytes: Uint8Array;
  /** A file name for the save exit; the extension comes from the sniffed kind. */
  fileName?: string;
  /** The save exit, injected by the caller (the host opens its save dialog). */
  onSaveAsFile?: (bytes: Uint8Array, suggestedName: string) => void;
  testId?: string;
}

export function BinaryViewer({
  bytes,
  fileName = 'binary-data',
  onSaveAsFile,
  testId,
}: BinaryViewerProps) {
  useI18n();
  const base = testId ?? 'binary-viewer';
  const rows = useMemo(() => hexRows(bytes), [bytes]);
  const image = useMemo(() => sniffImage(bytes), [bytes]);
  const totalRows = Math.ceil(bytes.length / BYTES_PER_HEX_ROW);

  return (
    <div data-testid={base} className="flex min-h-0 flex-col gap-2">
      <div className="flex items-center gap-2 text-[11px] text-fg-muted">
        <span data-testid={`${base}-size`}>{t('binaryViewer.bytes', { count: bytes.length })}</span>
        {image && (
          <span
            className="rounded bg-accent/15 px-1.5 py-px text-accent"
            data-testid={`${base}-kind`}
          >
            {image.mime}
          </span>
        )}
        {onSaveAsFile && (
          <Button
            variant="ghost"
            className="ml-auto h-6 px-2 text-xs"
            onClick={() => onSaveAsFile(bytes, image ? `${fileName}.${image.ext}` : fileName)}
            data-testid={`${base}-save`}
          >
            {t('binaryViewer.saveAsFile')}
          </Button>
        )}
      </div>
      {image && (
        <div
          data-testid={`${base}-image`}
          className="flex max-h-64 items-center justify-center overflow-hidden rounded-md border border-edge bg-surface-alt p-2"
        >
          <img
            src={`data:${image.mime};base64,${bytesToBase64(bytes)}`}
            alt={t('binaryViewer.imagePreview')}
            className="max-h-60 max-w-full object-contain"
          />
        </div>
      )}
      <div
        data-testid={`${base}-hex`}
        className="max-h-[360px] min-h-0 overflow-auto rounded-md border border-edge bg-surface-alt p-2 font-mono text-xs"
      >
        {rows.map((row) => (
          <div key={row.offset} className="flex gap-3 whitespace-pre leading-5">
            <span className="text-fg-muted">{row.offset.toString(16).padStart(8, '0')}</span>
            <span data-testid={`${base}-hex-bytes`} className="text-fg">
              {row.hex}
            </span>
            <span className="text-fg-muted">{row.ascii}</span>
          </div>
        ))}
        {totalRows > rows.length && (
          <div data-testid={`${base}-hex-truncated`} className="pt-1 text-fg-muted">
            {t('binaryViewer.hexTruncated', { shown: rows.length, total: totalRows })}
          </div>
        )}
      </div>
    </div>
  );
}
