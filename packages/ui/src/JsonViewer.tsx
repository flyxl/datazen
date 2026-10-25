import { useMemo, useState } from 'react';
import { cn } from './cn';
import { Tabs } from './Tabs';
import { t, useI18n } from './i18n';

/**
 * The JSON viewer shared by the host grid and the driver UIs (DB-13).
 *
 * One source of truth — the raw text — and four ways to look at it:
 * `tree` (a collapsible, read-only tree), `raw` (the text exactly as it came),
 * `pretty` (two-space indent) and `minify` (one line). The mode names and
 * their semantics are the ones the Redis value viewer already speaks, so a
 * user who learned one panel already knows the other.
 *
 * The tree is read-only on purpose: the host's write path is the cell editor
 * (an editor that refuses invalid JSON), and the driver editors that can write
 * JSON keep their own implementations. This component is the thing you look
 * at when the value is too big or too nested to read as text.
 *
 * A large document does not freeze the frame: the tree renders one node per
 * value, collapsed nodes render nothing below them, and the first levels open
 * — so a 1 MB JSON renders in one pass over the first two levels, not over
 * every leaf.
 */
export type JsonDisplayMode = 'tree' | 'raw' | 'pretty' | 'minify';

export const JSON_DISPLAY_MODES: readonly JsonDisplayMode[] = ['tree', 'raw', 'pretty', 'minify'];

/** How deep the tree opens by default — enough to see structure, not every leaf. */
const DEFAULT_OPEN_DEPTH = 2;

export interface JsonViewerProps {
  /** The raw JSON text — the only source; every mode renders from it. */
  text: string;
  defaultMode?: JsonDisplayMode;
  testId?: string;
}

interface ParseResult {
  /** The parsed document; `null` when the text does not parse. */
  parsed: unknown | null;
  /** Why the parse failed, when it did. */
  error: string | null;
}

function parseJson(text: string): ParseResult {
  try {
    return { parsed: JSON.parse(text) as unknown, error: null };
  } catch (error) {
    return {
      parsed: null,
      error: error instanceof Error ? error.message : String(error),
    };
  }
}

export function JsonViewer({ text, defaultMode = 'tree', testId }: JsonViewerProps) {
  useI18n();
  const parse = useMemo(() => parseJson(text), [text]);
  // Modes that need a parsed document are unavailable when the text does not
  // parse — falling back to `raw` shows exactly what is wrong instead of a
  // blank panel.
  const firstMode: JsonDisplayMode = parse.error === null ? defaultMode : 'raw';
  const [mode, setMode] = useState<JsonDisplayMode>(firstMode);

  const modes = useMemo<JsonDisplayMode[]>(() => {
    if (parse.error !== null) return ['raw'];
    return [...JSON_DISPLAY_MODES];
  }, [parse.error]);

  const prettyText = useMemo(
    () =>
      parse.parsed === null || parse.parsed === undefined
        ? text
        : (JSON.stringify(parse.parsed, null, 2) ?? text),
    [parse, text],
  );
  const minifiedText = useMemo(
    () =>
      parse.parsed === null || parse.parsed === undefined
        ? text
        : (JSON.stringify(parse.parsed) ?? text),
    [parse, text],
  );

  const activeText = mode === 'raw' ? text : mode === 'pretty' ? prettyText : minifiedText;

  return (
    <div data-testid={testId ?? 'json-viewer'} className="flex min-h-0 flex-col">
      <div className="flex items-center gap-2 pb-2">
        <Tabs
          items={modes.map((entry) => ({
            id: entry,
            label: t(`jsonViewer.mode.${entry}`),
            testId: `${testId ?? 'json-viewer'}-mode-${entry}`,
          }))}
          activeId={mode}
          onChange={(id) => {
            const next = modes.find((entry) => entry === id);
            if (next) setMode(next);
          }}
          className="inline-flex items-center gap-1 rounded-md border border-edge p-0.5"
          getTabClassName={({ selected }) =>
            cn(
              'rounded px-2 py-0.5 text-[11px] transition-colors',
              selected ? 'bg-accent/15 text-accent' : 'text-fg-secondary hover:bg-surface-raised',
            )
          }
          testId={`${testId ?? 'json-viewer'}-mode-bar`}
        />
        {parse.error !== null && (
          <span
            className="min-w-0 truncate rounded bg-warning/15 px-1.5 py-px text-[10px] text-warning"
            title={parse.error}
            data-testid={`${testId ?? 'json-viewer'}-parse-error`}
          >
            {t('jsonViewer.invalid')} · {parse.error}
          </span>
        )}
      </div>
      {mode === 'tree' && parse.error === null ? (
        <div
          data-testid={`${testId ?? 'json-viewer'}-tree`}
          className="max-h-[480px] min-h-0 overflow-auto rounded-md border border-edge bg-surface-alt p-2 font-mono text-xs"
        >
          <JsonTreeNode name={null} value={parse.parsed} depth={0} openDepth={DEFAULT_OPEN_DEPTH} />
        </div>
      ) : (
        <pre
          data-testid={`${testId ?? 'json-viewer'}-text`}
          className="selectable max-h-[480px] min-h-0 overflow-auto whitespace-pre-wrap break-words rounded-md border border-edge bg-surface-alt p-2 font-mono text-xs text-fg"
        >
          {activeText}
        </pre>
      )}
    </div>
  );
}

/** Shorten a branch label: keys and items are counted, never listed — a
 * collapsed node shows its size, not its contents. */
function childSummary(node: unknown): string | null {
  if (Array.isArray(node)) return `${node.length} item${node.length === 1 ? '' : 's'}`;
  if (node !== null && typeof node === 'object') {
    const count = Object.keys(node as Record<string, unknown>).length;
    return `${count} key${count === 1 ? '' : 's'}`;
  }
  return null;
}

function JsonTreeNode({
  name,
  value,
  depth,
  openDepth,
}: {
  name: string | null;
  value: unknown;
  depth: number;
  openDepth: number;
}) {
  const [open, setOpen] = useState(depth < openDepth);
  const isBranch = value !== null && (Array.isArray(value) || typeof value === 'object');

  const scalar =
    value === null ? (
      <span className="text-fg-muted italic">null</span>
    ) : typeof value === 'string' ? (
      <span className="text-success">&quot;{value}&quot;</span>
    ) : typeof value === 'number' ? (
      <span className="text-dt-number">{value}</span>
    ) : typeof value === 'boolean' ? (
      <span className="text-accent">{String(value)}</span>
    ) : null;

  return (
    <div className="leading-5" role="treeitem" aria-expanded={isBranch ? open : undefined}>
      <div className="flex items-start gap-1">
        {isBranch ? (
          <button
            type="button"
            onClick={() => setOpen((next) => !next)}
            className="shrink-0 rounded px-0.5 text-[10px] text-fg-muted hover:bg-surface-raised hover:text-fg"
            aria-label={open ? t('jsonViewer.collapse') : t('jsonViewer.expand')}
          >
            {open ? '▾' : '▸'}
          </button>
        ) : (
          <span className="shrink-0 px-0.5 text-[10px] text-fg-muted">·</span>
        )}
        {name !== null && <span className="shrink-0 font-medium text-fg">{name}:</span>}
        {isBranch ? (
          <span className="min-w-0">
            <span className="text-fg-secondary">{Array.isArray(value) ? '[…]' : '{…}'}</span>{' '}
            <span className="text-[10px] text-fg-muted">{childSummary(value)}</span>
          </span>
        ) : (
          scalar
        )}
      </div>
      {isBranch && open && (
        <div className="ml-4 border-l border-edge pl-1">
          {Array.isArray(value)
            ? value.map((item, index) => (
                <JsonTreeNode
                  key={String(index)}
                  name={String(index)}
                  value={item}
                  depth={depth + 1}
                  openDepth={openDepth}
                />
              ))
            : Object.entries(value as Record<string, unknown>).map(([key, item]) => (
                <JsonTreeNode
                  key={key}
                  name={key}
                  value={item}
                  depth={depth + 1}
                  openDepth={openDepth}
                />
              ))}
        </div>
      )}
    </div>
  );
}
