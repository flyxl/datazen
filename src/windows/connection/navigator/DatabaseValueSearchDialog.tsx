import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { Search, Square, TextSearch } from 'lucide-react';
import { useI18n } from '../../../hooks/useI18n';
import { getCachedTableSchema } from '../../../lib/schemaCache';
import {
  ValueSearchDriver,
  rankTables,
  valueSearchTargets,
  type ValueSearchProgress,
  type ValueSearchTableResult,
} from '../../../lib/valueSearch';
import { Dialog } from '../../../components/ui/Dialog';
import { Select, type SelectOption } from '@datazen/ui';
import { Button } from '../../../components/ui/Button';
import type { ValueSearchMode } from '@datazen/backend-client';
import type { TableInfo } from '../../../types';
import type { I18nKey } from '../../../locales';

/**
 * The full-database value search (DB-19): "which table holds this value?".
 *
 * The dialog is the scan's cockpit: the value and its match mode, the cost
 * note that says what a scan costs ("one query per table"), the pause and
 * resume of the driver, and the answers — the most likely tables first, every
 * table's hit count, and a click that opens the table's own tab.
 *
 * The scan itself lives in `src/lib/valueSearch.ts`: one command per table,
 * driven here, because that is what makes pause and resume real.
 */

export interface DatabaseValueSearchDialogProps {
  open: boolean;
  onClose: () => void;
  /** The runtime session of the connection whose database is searched. */
  dbSessionId: string;
  /** The connection, part of the schema cache's key. */
  connectionId: string | null;
  /** The database the navigator shows for this session. */
  database: string;
  /** The tables of the database, straight from the schema store. */
  tables: readonly TableInfo[];
  /** Open one result table's own tab. */
  onOpenTable: (tableName: string, schema: string | null) => void;
}

const MODES: ReadonlyArray<{ id: ValueSearchMode; labelKey: I18nKey }> = [
  { id: 'contains', labelKey: 'valueSearch.mode.contains' },
  { id: 'exact', labelKey: 'valueSearch.mode.exact' },
  { id: 'prefix', labelKey: 'valueSearch.mode.prefix' },
  { id: 'suffix', labelKey: 'valueSearch.mode.suffix' },
];

export function DatabaseValueSearchDialog({
  open,
  onClose,
  dbSessionId,
  connectionId,
  database,
  tables,
  onOpenTable,
}: DatabaseValueSearchDialogProps) {
  const { t } = useI18n();
  const [value, setValue] = useState('');
  const [mode, setMode] = useState<ValueSearchMode>('contains');
  const [progress, setProgress] = useState<ValueSearchProgress | null>(null);
  const driverRef = useRef<ValueSearchDriver | null>(null);

  const targets = useMemo(() => valueSearchTargets(tables), [tables]);

  // The driver is built for the current inputs when the scan starts; closing
  // the dialog cancels the one in flight, because a scan nobody watches is a
  // scan that should not run.
  useEffect(() => {
    if (!open) {
      driverRef.current?.cancel();
      driverRef.current = null;
      setProgress(null);
    }
  }, [open]);

  const start = useCallback(() => {
    const trimmed = value.trim();
    if (!trimmed || targets.length === 0) return;
    driverRef.current?.cancel();
    const driver = new ValueSearchDriver({
      dbSessionId,
      database,
      targets,
      value: trimmed,
      mode,
      readSchema: (target) =>
        getCachedTableSchema(
          dbSessionId,
          target.table,
          database,
          target.schema,
          connectionId ?? undefined,
        ).then((schema) => schema.columns),
    });
    driverRef.current = driver;
    driver.subscribe(setProgress);
    driver.start();
  }, [connectionId, database, dbSessionId, mode, targets, value]);

  const busy = progress?.status === 'running' || progress?.status === 'paused';
  const ranked = useMemo(() => rankTables(progress?.results ?? []), [progress]);
  const withHits = useMemo(
    () =>
      (progress?.results ?? [])
        .filter((result) => (result.matches ?? 0) > 0)
        .sort((left, right) => (right.matches ?? 0) - (left.matches ?? 0)),
    [progress],
  );
  const errors = useMemo(
    () => (progress?.results ?? []).filter((result) => result.matches === null),
    [progress],
  );

  const resultRow = (result: ValueSearchTableResult, keyPrefix: string) => (
    <button
      key={`${keyPrefix}-${result.table}`}
      type="button"
      data-testid={`value-search-result-${result.table}`}
      className="flex w-full items-center justify-between gap-2 rounded-md px-2 py-1.5 text-left hover:bg-surface-raised"
      onClick={() => {
        const target = targets.find((item) => item.table === result.table);
        onOpenTable(result.table, target?.schema ?? null);
        onClose();
      }}
    >
      <span className="truncate text-xs text-fg">{result.table}</span>
      <span className="shrink-0 rounded bg-surface-raised px-1.5 text-[10px] text-fg-secondary">
        {result.matches}
      </span>
    </button>
  );

  return (
    <Dialog
      open={open}
      title={t('valueSearch.title')}
      description={t('valueSearch.description')}
      onClose={onClose}
      className="max-w-xl"
      testId="database-value-search-dialog"
    >
      <div className="flex flex-col gap-3">
        {/* The search line: the value, the match mode, the start. */}
        <div className="flex items-center gap-2">
          <div className="relative flex-1">
            <Search className="absolute left-2 top-1/2 h-3.5 w-3.5 -translate-y-1/2 text-fg-muted" />
            <input
              type="text"
              data-testid="value-search-input"
              className="h-7 w-full rounded-md bg-surface pl-7 pr-2 text-xs text-fg placeholder:text-fg-muted focus:outline-none focus:ring-1 focus:ring-accent"
              placeholder={t('valueSearch.placeholder')}
              value={value}
              disabled={busy}
              onChange={(event) => setValue(event.target.value)}
              onKeyDown={(event) => {
                if (event.key === 'Enter') {
                  event.preventDefault();
                  start();
                }
              }}
            />
          </div>
          <Select
            triggerDataAttrs={{ 'data-testid': 'value-search-mode' }}
            className="h-7 text-xs"
            value={mode}
            options={MODES.map(
              (entry): SelectOption => ({ value: entry.id, label: t(entry.labelKey) }),
            )}
            disabled={busy}
            onChange={(value) => setMode(value as ValueSearchMode)}
            title={t('valueSearch.modeLabel')}
          />
          {!busy && (
            <Button
              data-testid="value-search-start"
              disabled={value.trim() === '' || targets.length === 0}
              onClick={start}
            >
              {t('valueSearch.start')}
            </Button>
          )}
          {progress?.status === 'running' && (
            <Button data-testid="value-search-pause" onClick={() => driverRef.current?.pause()}>
              {t('valueSearch.pause')}
            </Button>
          )}
          {progress?.status === 'paused' && (
            <Button data-testid="value-search-resume" onClick={() => driverRef.current?.start()}>
              {t('valueSearch.resume')}
            </Button>
          )}
          {busy && (
            <Button
              data-testid="value-search-cancel"
              onClick={() => driverRef.current?.cancel()}
              variant="ghost"
            >
              <Square className="h-3 w-3" />
            </Button>
          )}
        </div>

        {/* The cost note: a scan queries every table, and the user should
            know that before the first statement, not after. */}
        <p className="text-[10px] text-fg-muted" data-testid="value-search-cost-note">
          {t('valueSearch.costNote', { count: targets.length })}
        </p>

        {/* The progress line: one line, live, while the scan runs. */}
        {progress && progress.status !== 'idle' && (
          <div
            className="flex items-center justify-between rounded-md border border-edge bg-surface-raised/50 px-2 py-1 text-[10px] text-fg-secondary"
            data-testid="value-search-progress"
          >
            <span className="truncate">
              {progress.status === 'running' && progress.current
                ? t('valueSearch.scanning', { table: progress.current })
                : t(`valueSearch.status.${progress.status}`)}
            </span>
            <span className="shrink-0">
              {progress.completed} / {progress.total}
            </span>
          </div>
        )}

        {/* The most likely tables: the three with the most hits answer the
            user's real question ("where is this value?"). */}
        {ranked.length > 0 && (
          <div className="rounded-md border border-accent/40 bg-accent/5 p-1.5">
            <p className="px-1 pb-1 text-[10px] font-medium text-accent">
              {t('valueSearch.mostLikely')}
            </p>
            {ranked.map((result) => resultRow(result, 'likely'))}
          </div>
        )}

        {/* Every table with at least one hit, most first. */}
        {withHits.length > 0 && (
          <div
            className="max-h-64 min-h-24 overflow-y-auto rounded-md border border-edge p-1"
            data-testid="value-search-results"
          >
            {withHits.map((result) => resultRow(result, 'hit'))}
          </div>
        )}

        {/* A finished scan with no hit says so — silence reads as "broken". */}
        {progress?.status === 'done' && withHits.length === 0 && (
          <p className="px-1 text-xs text-fg-muted" data-testid="value-search-empty">
            {t('valueSearch.noHits')}
          </p>
        )}

        {/* The tables the scan could not read; the scan keeps its promise
            about the rest, and the user can see which promise fell short. */}
        {errors.length > 0 && (
          <details className="px-1 text-[10px] text-fg-muted">
            <summary data-testid="value-search-errors-summary">
              {t('valueSearch.errors', { count: errors.length })}
            </summary>
            {errors.map((result) => (
              <p key={`error-${result.table}`} className="truncate">
                {result.table}: {result.reason}
              </p>
            ))}
          </details>
        )}

        {targets.length === 0 && (
          <p className="flex items-center gap-1.5 px-1 text-xs text-fg-muted">
            <TextSearch className="h-3.5 w-3.5" />
            {t('valueSearch.noTables')}
          </p>
        )}
      </div>
    </Dialog>
  );
}
