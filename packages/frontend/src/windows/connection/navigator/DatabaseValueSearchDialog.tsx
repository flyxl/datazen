import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { Locate, Search, Square, TextSearch } from 'lucide-react';
import { useI18n } from '../../../hooks/useI18n';
import { getCachedTableSchema } from '../../../lib/schemaCache';
import {
  ValueSearchDriver,
  cellTypesForScope,
  rankTables,
  targetKey,
  typeScopesForDriver,
  type ValueSearchLocateIntent,
  type ValueSearchProgress,
  type ValueSearchTableResult,
  type ValueSearchTypeScope,
  valueSearchTargets,
} from '../../../lib/valueSearch';
import { Dialog } from '../../../components/ui/Dialog';
import { Select, type SelectOption } from '@datazen/ui';
import { Button } from '../../../components/ui/Button';
import type { ValueSearchMode } from '@datazen/backend-client';
import type { TableInfo } from '../../../types';
import type { I18nKey } from '../../../locales';

/**
 * The full-database value search (DB-19 + DBR-03): "which table holds this
 * value?".
 *
 * The dialog is the scan's cockpit: the value and its match mode, the scope
 * (every plain table or a picked list, schema-qualified so same-named tables
 * cannot be confused), the searchable type scope (the safe text set, plus
 * the type groups the driver declared it can text-match), the cost note that
 * says what a scan costs ("one query per table at least"), the pause and
 * resume of the driver, and the answers — the most likely tables first, every
 * table's hit count, zero-hit and failed tables each in their own list, and
 * a click that opens the table's own tab carrying the search's own
 * conditions.
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
  /** The connection's driver type — decides the offered type scopes. */
  driverType?: string | null;
  /** The tables of the database, straight from the schema store. */
  tables: readonly TableInfo[];
  /** Open one answer's table tab, carrying the search's own conditions. */
  onLocate: (intent: ValueSearchLocateIntent) => void;
}

const MODES: ReadonlyArray<{ id: ValueSearchMode; labelKey: I18nKey }> = [
  { id: 'contains', labelKey: 'valueSearch.mode.contains' },
  { id: 'notContains', labelKey: 'valueSearch.mode.notContains' },
  { id: 'exact', labelKey: 'valueSearch.mode.exact' },
  { id: 'prefix', labelKey: 'valueSearch.mode.prefix' },
  { id: 'suffix', labelKey: 'valueSearch.mode.suffix' },
];

const TYPE_SCOPE_LABELS: Record<ValueSearchTypeScope, I18nKey> = {
  text: 'valueSearch.types.text',
  'text+number': 'valueSearch.types.textNumber',
  'text+datetime': 'valueSearch.types.textDatetime',
  'text+number+datetime': 'valueSearch.types.textNumberDatetime',
};

export function DatabaseValueSearchDialog({
  open,
  onClose,
  dbSessionId,
  connectionId,
  database,
  driverType,
  tables,
  onLocate,
}: DatabaseValueSearchDialogProps) {
  const { t } = useI18n();
  const [value, setValue] = useState('');
  const [mode, setMode] = useState<ValueSearchMode>('contains');
  const [scope, setScope] = useState<'all' | 'selected'>('all');
  const [selectedKeys, setSelectedKeys] = useState<ReadonlySet<string>>(() => new Set());
  const [pickerFilter, setPickerFilter] = useState('');
  const [typeScope, setTypeScope] = useState<ValueSearchTypeScope>('text');
  const [progress, setProgress] = useState<ValueSearchProgress | null>(null);
  /** What the finished (or running) scan actually asked — the locate seeds
   * this, never the input's current text (editable again once a scan ends). */
  const [lastRun, setLastRun] = useState<{
    value: string;
    mode: ValueSearchMode;
  } | null>(null);
  const driverRef = useRef<ValueSearchDriver | null>(null);

  const typeScopes = useMemo(() => typeScopesForDriver(driverType), [driverType]);
  const allTargets = useMemo(() => valueSearchTargets(tables), [tables]);
  const targets = useMemo(
    () =>
      scope === 'all'
        ? allTargets
        : allTargets.filter((target) => selectedKeys.has(targetKey(target))),
    [scope, allTargets, selectedKeys],
  );

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
      cellTypes: cellTypesForScope(typeScope),
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
    setLastRun({ value: trimmed, mode });
    driver.subscribe(setProgress);
    driver.start();
  }, [connectionId, database, dbSessionId, mode, targets, typeScope, value]);

  const busy = progress?.status === 'running' || progress?.status === 'paused';
  const ranked = useMemo(() => rankTables(progress?.results ?? []), [progress]);
  const withHits = useMemo(
    () =>
      (progress?.results ?? [])
        .filter((result) => (result.matches ?? 0) > 0)
        .sort((left, right) => (right.matches ?? 0) - (left.matches ?? 0)),
    [progress],
  );
  const zeroHits = useMemo(
    () => (progress?.results ?? []).filter((result) => result.matches === 0),
    [progress],
  );
  const errors = useMemo(
    () => (progress?.results ?? []).filter((result) => result.matches === null),
    [progress],
  );

  const locate = useCallback(
    (result: ValueSearchTableResult) => {
      // The seeded conditions are the scan's own question — the value and
      // mode of the run that produced this answer, and the very columns the
      // statement searched.
      const run = lastRun;
      if (!run) return;
      onLocate({
        table: result.table,
        schema: result.schema,
        database,
        value: run.value,
        mode: run.mode,
        columns: result.columns,
      });
      onClose();
    },
    [database, lastRun, onClose, onLocate],
  );

  const pickerRows = useMemo(() => {
    const needle = pickerFilter.trim().toLowerCase();
    if (!needle) return allTargets;
    return allTargets.filter((target) => targetKey(target).toLowerCase().includes(needle));
  }, [allTargets, pickerFilter]);

  const toggleTarget = useCallback((key: string) => {
    setSelectedKeys((previous) => {
      const next = new Set(previous);
      if (next.has(key)) next.delete(key);
      else next.add(key);
      return next;
    });
  }, []);

  const resultRow = (result: ValueSearchTableResult, keyPrefix: string) => (
    <div
      key={`${keyPrefix}-${targetKey(result)}`}
      data-testid={`value-search-result-${targetKey(result)}`}
      className="flex w-full items-center justify-between gap-2 rounded-md px-2 py-1.5 hover:bg-surface-raised"
    >
      <button
        type="button"
        className="flex min-w-0 flex-1 items-center gap-1 text-left"
        onClick={() => locate(result)}
        title={t('valueSearch.locate')}
      >
        {result.schema && (
          <span className="shrink-0 text-[10px] text-fg-muted">{result.schema}.</span>
        )}
        <span className="truncate text-xs text-fg">{result.table}</span>
      </button>
      <span className="shrink-0 rounded bg-surface-raised px-1.5 text-[10px] text-fg-secondary">
        {result.matches}
      </span>
      <Button
        data-testid={`value-search-locate-${targetKey(result)}`}
        className="h-6 px-1.5 text-[10px]"
        variant="ghost"
        disabled={result.columns.length === 0}
        onClick={() => locate(result)}
        title={
          result.columns.length === 0 ? t('valueSearch.locateNoColumns') : t('valueSearch.locate')
        }
      >
        <Locate className="h-3 w-3" />
        {t('valueSearch.locate')}
      </Button>
    </div>
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

        {/* The scope line: every plain table, or a picked list; and the
            searchable type scope the driver can serve. */}
        <div className="flex items-center gap-2">
          <Select
            triggerDataAttrs={{ 'data-testid': 'value-search-scope' }}
            className="h-7 text-xs"
            value={scope}
            options={[
              { value: 'all', label: t('valueSearch.scope.all', { count: allTargets.length }) },
              {
                value: 'selected',
                label: t('valueSearch.scope.selected', { count: selectedKeys.size }),
              },
            ]}
            disabled={busy}
            onChange={(value) => setScope(value as 'all' | 'selected')}
            title={t('valueSearch.scopeLabel')}
          />
          <Select
            triggerDataAttrs={{ 'data-testid': 'value-search-types' }}
            className="h-7 text-xs"
            value={typeScope}
            options={typeScopes.map(
              (entry): SelectOption => ({ value: entry, label: t(TYPE_SCOPE_LABELS[entry]) }),
            )}
            disabled={busy}
            onChange={(value) => setTypeScope(value as ValueSearchTypeScope)}
            title={t('valueSearch.typesLabel')}
          />
        </div>

        {/* The picked list: schema-qualified rows, because a bare name is
            ambiguous the moment two schemas hold the same one (DBR-03). */}
        {scope === 'selected' && (
          <div
            className="flex flex-col gap-1.5 rounded-md border border-edge p-1.5"
            data-testid="value-search-table-picker"
          >
            <input
              type="text"
              data-testid="value-search-table-filter"
              className="h-6 w-full rounded-md bg-surface px-2 text-xs text-fg placeholder:text-fg-muted focus:outline-none focus:ring-1 focus:ring-accent"
              placeholder={t('valueSearch.pickerPlaceholder')}
              value={pickerFilter}
              disabled={busy}
              onChange={(event) => setPickerFilter(event.target.value)}
            />
            <div className="max-h-40 overflow-y-auto">
              {pickerRows.map((target) => {
                const key = targetKey(target);
                const checked = selectedKeys.has(key);
                return (
                  <label
                    key={`picker-${key}`}
                    data-testid={`value-search-table-option-${key}`}
                    className="flex cursor-pointer items-center gap-2 rounded px-1 py-0.5 text-xs hover:bg-surface-raised"
                  >
                    <input
                      type="checkbox"
                      className="h-3 w-3 accent-accent"
                      checked={checked}
                      disabled={busy}
                      onChange={() => toggleTarget(key)}
                    />
                    {target.schema && (
                      <span className="shrink-0 text-[10px] text-fg-muted">{target.schema}.</span>
                    )}
                    <span className="truncate text-fg">{target.table}</span>
                  </label>
                );
              })}
              {pickerRows.length === 0 && (
                <p
                  className="px-1 py-1 text-[10px] text-fg-muted"
                  data-testid="value-search-picker-empty"
                >
                  {t('valueSearch.pickerEmpty')}
                </p>
              )}
            </div>
            <p className="px-1 text-[10px] text-fg-muted" data-testid="value-search-picker-count">
              {t('valueSearch.pickerCount', {
                selected: selectedKeys.size,
                total: allTargets.length,
              })}
            </p>
          </div>
        )}

        {/* The type note: numbers and date/times are matched by their text
            rendering — the PRD's "say which kind of match it is" (DBR-03). */}
        {typeScope !== 'text' && (
          <p className="text-[10px] text-fg-muted" data-testid="value-search-types-note">
            {t('valueSearch.types.note')}
          </p>
        )}

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
            <span className="shrink-0" data-testid="value-search-progress-count">
              {progress.completed} / {progress.total}
            </span>
          </div>
        )}

        {/* The scan's own cost, as measured — tables and wall-clock, never a
            made-up ETA (DBR-03's "呈现扫描成本"). */}
        {progress?.status === 'done' && progress.elapsedMs !== null && (
          <p className="px-1 text-[10px] text-fg-muted" data-testid="value-search-done-note">
            {t('valueSearch.doneNote', {
              count: progress.total,
              seconds: Math.max(1, Math.round(progress.elapsedMs / 1000)),
            })}
          </p>
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

        {/* The tables the scan read and found clean — its own list, apart
            from the ones it could not read at all (DBR-03: zero-hit and
            failed tables are different facts). */}
        {zeroHits.length > 0 && (
          <details className="px-1 text-[10px] text-fg-muted">
            <summary data-testid="value-search-zero-hits-summary">
              {t('valueSearch.zeroHits', { count: zeroHits.length })}
            </summary>
            {zeroHits.map((result) => (
              <p key={`zero-${targetKey(result)}`} className="truncate">
                {targetKey(result)}
              </p>
            ))}
          </details>
        )}

        {/* The tables the scan could not read; the scan keeps its promise
            about the rest, and the user can see which promise fell short. */}
        {errors.length > 0 && (
          <details className="px-1 text-[10px] text-fg-muted">
            <summary data-testid="value-search-errors-summary">
              {t('valueSearch.errors', { count: errors.length })}
            </summary>
            {errors.map((result) => (
              <p key={`error-${targetKey(result)}`} className="truncate">
                {targetKey(result)}: {result.reason}
              </p>
            ))}
          </details>
        )}

        {allTargets.length === 0 && (
          <p className="flex items-center gap-1.5 px-1 text-xs text-fg-muted">
            <TextSearch className="h-3.5 w-3.5" />
            {t('valueSearch.noTables')}
          </p>
        )}
      </div>
    </Dialog>
  );
}
