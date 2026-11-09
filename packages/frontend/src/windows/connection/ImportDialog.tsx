import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { AlertTriangle, FileText } from 'lucide-react';
import { Dialog } from '../../components/ui/Dialog';
import { CopyableError } from '../../components/ui/CopyableError';
import { Button } from '../../components/ui/Button';
import { Select } from '../../components/ui/Select';
import { Spinner } from '../../components/ui/Spinner';
import { fileCommands } from '../../commands/file';
import {
  IMPORT_ENCODINGS,
  ImportParseError,
  decodeImportBase64,
  detectImportFormat,
  parseCsvImport,
  parseJsonImport,
  parseXlsxImport,
  type ImportDiagnostic,
  type ImportParseResult,
} from '../../lib/importData';
import {
  mappingIsExecutable,
  planImportMapping,
  type ImportMappingMode,
  type ImportMappingPlan,
} from '../../lib/importMapping';
import {
  createIpcImportTransport,
  executeImport,
  type ImportProgress,
  type ImportRunResult,
  type ImportTransport,
} from '../../lib/importExecution';
import { useConnectionSchemaField } from '../../stores/schemaStore';
import { cn } from '../../lib/cn';
import { useI18n } from '../../hooks/useI18n';

/**
 * The import wizard (DBR-01).
 *
 * Steps in one dialog: file → format options → target table + column mapping
 * → preview with diagnostics → conflict strategy and commit mode → batched
 * execution with progress, cancel and an honest report (committed rows, per-row
 * failures with source lines). The run never builds one giant SQL string; see
 * `lib/importExecution`.
 */

const PREVIEW_ROWS = 100;
const SAMPLE_ROWS = 5;

interface ImportDialogProps {
  open: boolean;
  onClose: () => void;
  dbSessionId: string;
  tableName: string | null;
  /** Target schema (PG family) — threaded from the tree node. */
  schema?: string | null;
  /** Target database — threaded from the tree node. */
  database?: string | null;
  /** Connection id, for the relation-schema cache key. */
  connectionId?: string;
  /** Called after a finished import; a rejection becomes "refresh failed". */
  onImported: (target: {
    table: string;
    schema?: string | null;
    database?: string | null;
  }) => void | Promise<void>;
  databaseType?: string;
}

interface LoadedFile {
  name: string;
  dataBase64: string;
}

interface TargetColumnsState {
  columns: { name: string; dataType: string }[] | null;
  loading: boolean;
  error: string | null;
}

type WizardPhase = 'config' | 'running' | 'done';

export function ImportDialog({
  open: isOpen,
  onClose,
  dbSessionId,
  tableName,
  schema = null,
  database = null,
  connectionId,
  onImported,
  databaseType,
}: ImportDialogProps) {
  const { t } = useI18n();
  const [file, setFile] = useState<LoadedFile | null>(null);
  const [encoding, setEncoding] = useState('utf-8');
  const [delimiter, setDelimiter] = useState<'auto' | ',' | ';' | '\t'>('auto');
  const [quoteChar, setQuoteChar] = useState('"');
  const [firstRowIsHeader, setFirstRowIsHeader] = useState(true);
  const [emptyUnquotedAs, setEmptyUnquotedAs] = useState<'null' | 'empty'>('null');
  const [sheetName, setSheetName] = useState<string | null>(null);
  const [targetTable, setTargetTable] = useState(tableName ?? '');
  const [mappingMode, setMappingMode] = useState<ImportMappingMode>('auto');
  const [overrides, setOverrides] = useState<Record<string, string | null>>({});
  const [onError, setOnError] = useState<'stop' | 'skip'>('stop');
  const [commitMode, setCommitMode] = useState<'transaction' | 'autocommit'>('transaction');
  const [phase, setPhase] = useState<WizardPhase>('config');
  const [progress, setProgress] = useState<ImportProgress | null>(null);
  const [result, setResult] = useState<ImportRunResult | null>(null);
  const [refreshFailed, setRefreshFailed] = useState(false);
  const [parseError, setParseError] = useState<string | null>(null);
  const [targets, setTargets] = useState<TargetColumnsState>({
    columns: null,
    loading: false,
    error: null,
  });

  const signalRef = useRef({ aborted: false });
  const transportRef = useRef<ImportTransport | null>(null);
  const tables = useConnectionSchemaField(dbSessionId, 'tables');

  const format = file ? detectImportFormat(file.name) : null;

  useEffect(() => {
    if (tableName) setTargetTable(tableName);
  }, [tableName]);

  // Reset the run state whenever the dialog reopens.
  useEffect(() => {
    if (!isOpen) return;
    setPhase('config');
    setProgress(null);
    setResult(null);
    setRefreshFailed(false);
    setParseError(null);
    setOverrides({});
    signalRef.current = { aborted: false };
    transportRef.current = null;
  }, [isOpen]);

  const handlePickFile = useCallback(async () => {
    setParseError(null);
    try {
      const opened = await fileCommands.openBase64WithDialog('Data Files', ['csv', 'json', 'xlsx']);
      if (!opened) return;
      setFile({ name: opened.fileName, dataBase64: opened.dataBase64 });
      setSheetName(null);
      setOverrides({});
    } catch (e) {
      setParseError(e instanceof Error ? e.message : String(e));
    }
  }, []);

  // The parse pipeline: re-runs for every format-affecting option so the
  // preview always reflects the settings the import will actually use.
  const parsed: (ImportParseResult & { sheetNames?: string[] }) | null = useMemo(() => {
    if (!file || !format) return null;
    try {
      if (format === 'xlsx') {
        const result = parseXlsxImport(file.dataBase64, { sheetName: sheetName ?? undefined });
        return result;
      }
      const text = decodeImportBase64(file.dataBase64, encoding);
      if (format === 'csv') {
        return parseCsvImport(text, {
          delimiter,
          quoteChar: quoteChar.charAt(0) || '"',
          firstRowIsHeader,
          emptyUnquotedAs,
        });
      }
      return parseJsonImport(text);
    } catch (error) {
      return null;
    }
  }, [file, format, encoding, delimiter, quoteChar, firstRowIsHeader, emptyUnquotedAs, sheetName]);

  // Parse errors need their own channel: the memo above swallows them for
  // render safety, so recompute the error alongside it.
  const parseErrorMessage = useMemo(() => {
    if (!file || !format) return null;
    try {
      if (format === 'xlsx') {
        parseXlsxImport(file.dataBase64, { sheetName: sheetName ?? undefined });
      } else {
        const text = decodeImportBase64(file.dataBase64, encoding);
        if (format === 'csv') {
          parseCsvImport(text, {
            delimiter,
            quoteChar: quoteChar.charAt(0) || '"',
            firstRowIsHeader,
            emptyUnquotedAs,
          });
        } else {
          parseJsonImport(text);
        }
      }
      return null;
    } catch (error) {
      if (error instanceof ImportParseError) {
        return t('import.parseFailed', {
          message: error.line != null ? `${error.message} (line ${error.line})` : error.message,
        });
      }
      if (error instanceof RangeError) {
        return t('import.parseFailed', { message: error.message });
      }
      return t('import.parseFailed', {
        message: error instanceof Error ? error.message : String(error),
      });
    }
  }, [
    file,
    format,
    encoding,
    delimiter,
    quoteChar,
    firstRowIsHeader,
    emptyUnquotedAs,
    sheetName,
    t,
  ]);

  // Load the target table's columns whenever the target changes.
  useEffect(() => {
    if (!isOpen || !targetTable.trim()) {
      setTargets({ columns: null, loading: false, error: null });
      return;
    }
    let cancelled = false;
    setTargets((state) => ({ ...state, loading: true, error: null }));
    void (async () => {
      try {
        const loaded = await import('../../lib/schemaCache').then((m) =>
          m.getCachedTableSchema(
            dbSessionId,
            targetTable.trim(),
            database ?? '',
            schema,
            connectionId,
          ),
        );
        if (cancelled) return;
        setTargets({
          columns: loaded.columns.map((column) => ({
            name: column.name,
            dataType: column.dataType,
          })),
          loading: false,
          error: null,
        });
      } catch (error) {
        if (cancelled) return;
        setTargets({
          columns: null,
          loading: false,
          error: error instanceof Error ? error.message : String(error),
        });
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [isOpen, targetTable, dbSessionId, database, schema, connectionId]);

  const mappingPlan: ImportMappingPlan | null = useMemo(() => {
    if (!parsed || parsed.columns.length === 0 || !targets.columns) return null;
    return planImportMapping({
      sourceColumns: parsed.columns,
      targetColumns: targets.columns.map((column) => column.name),
      mode: mappingMode,
      overrides,
    });
  }, [parsed, targets.columns, mappingMode, overrides]);

  const mappedPairs = useMemo(() => {
    if (!mappingPlan) return [];
    return mappingPlan.entries
      .filter((entry) => entry.target != null)
      .map((entry) => ({ source: entry.source, target: entry.target as string }));
  }, [mappingPlan]);

  const diagnostics: ImportDiagnostic[] = parsed?.diagnostics ?? [];
  const duplicates = diagnostics.filter((d) => d.kind === 'duplicate-column');
  const ragged = diagnostics.filter((d) => d.kind === 'ragged-row');

  // Duplicate source column names are a hard stop: the mapping is keyed by
  // source name, so two sources with one name cannot be represented — the
  // file must be fixed before anything is written (PRD 4.3).
  const blockedByDuplicates = duplicates.length > 0;

  const canStart =
    phase === 'config' &&
    parsed != null &&
    parsed.rows.length > 0 &&
    parseErrorMessage == null &&
    !blockedByDuplicates &&
    targets.columns != null &&
    targets.error == null &&
    mappingPlan != null &&
    mappingIsExecutable(mappingPlan);

  const handleStart = useCallback(async () => {
    if (!parsed || !canStart || !targets.columns) return;
    setPhase('running');
    setProgress(null);
    setResult(null);
    setRefreshFailed(false);
    signalRef.current = { aborted: false };
    const transport = createIpcImportTransport({ dbSessionId, database, schema });
    transportRef.current = transport;
    try {
      const run = await executeImport({
        table: targetTable.trim(),
        targetColumns: mappedPairs.map((pair) => pair.target),
        columnPairs: mappedPairs,
        rows: parsed.rows,
        rowOrigins: parsed.rowOrigins,
        databaseType,
        onError,
        commitMode,
        signal: signalRef.current,
        transport,
        onProgress: setProgress,
      });
      setResult(run);
      setPhase('done');
      try {
        await onImported({ table: targetTable.trim(), schema, database });
      } catch {
        setRefreshFailed(true);
      }
    } catch (error) {
      // executeImport never throws for run outcomes; reaching here means the
      // transport itself broke (session loss) — surface it as a stopped run.
      setResult({
        status: 'stopped',
        committedRows: progress?.committedRows ?? 0,
        failedRows: [],
        stoppedBy: {
          rowIndex: 0,
          sourceLine: null,
          message: error instanceof Error ? error.message : String(error),
        },
      });
      setPhase('done');
    }
  }, [
    parsed,
    canStart,
    targets.columns,
    targetTable,
    mappedPairs,
    dbSessionId,
    database,
    schema,
    databaseType,
    onError,
    commitMode,
    onImported,
    progress,
  ]);

  const handleCancelImport = useCallback(() => {
    signalRef.current.aborted = true;
    void transportRef.current?.cancelInFlight();
  }, []);

  const handleResetForAnother = useCallback(() => {
    setFile(null);
    setParseError(null);
    setSheetName(null);
    setOverrides({});
    setResult(null);
    setProgress(null);
    setRefreshFailed(false);
    setPhase('config');
  }, []);

  const tableOptions = useMemo(
    () =>
      tables
        .filter((entry) => entry.tableType === 'table')
        .map((entry) => ({
          value: entry.name,
          label: entry.schema ? `${entry.schema}.${entry.name}` : entry.name,
        })),
    [tables],
  );

  const busy = phase === 'running';

  return (
    <Dialog
      open={isOpen}
      title={t('common.importData')}
      description={t('import.description')}
      onClose={busy ? () => undefined : onClose}
      className="max-w-3xl"
      footer={
        phase === 'running' ? (
          <>
            <Button variant="secondary" onClick={handleCancelImport}>
              {t('import.cancelImport')}
            </Button>
            <Button variant="primary" disabled>
              <Spinner size="sm" />
              {t('import.importing')}
            </Button>
          </>
        ) : phase === 'done' ? (
          <>
            <Button variant="secondary" onClick={onClose}>
              {t('common.close')}
            </Button>
            <Button variant="primary" onClick={handleResetForAnother}>
              {t('import.importAnother')}
            </Button>
          </>
        ) : (
          <>
            <Button variant="secondary" onClick={onClose}>
              {t('common.cancel')}
            </Button>
            <Button
              variant="primary"
              onClick={() => void handleStart()}
              disabled={!canStart}
              title={
                !canStart &&
                parsed != null &&
                parsed.rows.length > 0 &&
                mappingPlan != null &&
                !mappingIsExecutable(mappingPlan)
                  ? t('import.needsMapping')
                  : undefined
              }
            >
              {parsed && parsed.rows.length > 0
                ? t('import.start', { count: parsed.rows.length })
                : t('import.import')}
            </Button>
          </>
        )
      }
    >
      <div className="space-y-4">
        {/* ── 文件与格式 ─────────────────────────────────────────── */}
        <section className="space-y-2">
          <div className="flex flex-wrap items-center gap-2">
            <Button
              variant="secondary"
              className="h-8 text-xs"
              onClick={() => void handlePickFile()}
              disabled={busy}
            >
              <FileText className="h-3.5 w-3.5" />
              {file ? file.name.split('/').pop() : t('import.selectFile')}
            </Button>
            {file && format && (
              <span className="rounded bg-surface-alt px-2 py-0.5 text-[11px] font-medium uppercase text-fg-muted">
                {format}
              </span>
            )}
            {file && (
              <Button
                variant="secondary"
                className="h-8 text-xs"
                onClick={() => void handlePickFile()}
                disabled={busy}
              >
                {t('import.replaceFile')}
              </Button>
            )}
          </div>

          {file && format === 'csv' && (
            <div className="flex flex-wrap items-center gap-x-4 gap-y-2 text-xs text-fg-secondary">
              <label className="flex items-center gap-1.5">
                {t('import.encoding')}
                <Select
                  className="h-7 w-28"
                  value={encoding}
                  onChange={setEncoding}
                  options={IMPORT_ENCODINGS.map((value) => ({ value, label: value }))}
                />
              </label>
              <label className="flex items-center gap-1.5">
                {t('import.delimiter')}
                <Select
                  className="h-7 w-28"
                  value={delimiter}
                  onChange={(next) => setDelimiter(next as typeof delimiter)}
                  options={[
                    { value: 'auto', label: t('import.delimiter.auto') },
                    { value: ',', label: t('import.delimiter.comma') },
                    { value: ';', label: t('import.delimiter.semicolon') },
                    { value: '\t', label: t('import.delimiter.tab') },
                  ]}
                />
              </label>
              <label className="flex items-center gap-1.5">
                {t('import.quoteChar')}
                <Select
                  className="h-7 w-16"
                  value={quoteChar}
                  onChange={setQuoteChar}
                  options={[
                    { value: '"', label: '"' },
                    { value: "'", label: "'" },
                  ]}
                />
              </label>
              <label className="flex items-center gap-1.5">
                {t('import.emptyUnquotedAs')}
                <Select
                  className="h-7 w-40"
                  value={emptyUnquotedAs}
                  onChange={(next) => setEmptyUnquotedAs(next as 'null' | 'empty')}
                  options={[
                    { value: 'null', label: t('import.emptyUnquotedAs.null') },
                    { value: 'empty', label: t('import.emptyUnquotedAs.empty') },
                  ]}
                />
              </label>
              <label className="flex items-center gap-1.5">
                <input
                  type="checkbox"
                  checked={firstRowIsHeader}
                  onChange={(e) => setFirstRowIsHeader(e.target.checked)}
                  className="h-3.5 w-3.5 accent-[var(--color-accent)]"
                />
                {t('import.firstRowIsHeader')}
              </label>
            </div>
          )}

          {file && format === 'json' && (
            <div className="flex items-center gap-1.5 text-xs text-fg-secondary">
              {t('import.encoding')}
              <Select
                className="h-7 w-28"
                value={encoding}
                onChange={setEncoding}
                options={IMPORT_ENCODINGS.map((value) => ({ value, label: value }))}
              />
            </div>
          )}

          {file && format === 'xlsx' && parsed?.sheetNames && parsed.sheetNames.length > 0 && (
            <div className="flex items-center gap-1.5 text-xs text-fg-secondary">
              {t('import.sheet')}
              <Select
                className="h-7 w-44"
                value={sheetName ?? parsed.sheetNames[0]}
                onChange={setSheetName}
                options={parsed.sheetNames.map((name) => ({ value: name, label: name }))}
              />
            </div>
          )}
        </section>

        {/* ── 解析错误 ───────────────────────────────────────────── */}
        {parseErrorMessage && (
          <CopyableError
            message={parseErrorMessage}
            className="rounded-md border border-red-500/20 bg-red-500/10 px-3 py-2 text-xs text-red-400"
          />
        )}
        {!file && parseError && (
          <CopyableError
            message={parseError}
            className="rounded-md border border-red-500/20 bg-red-500/10 px-3 py-2 text-xs text-red-400"
          />
        )}

        {/* ── 目标表与列映射 ─────────────────────────────────────── */}
        {parsed && parsed.columns.length > 0 && (
          <section className="space-y-2">
            <div className="flex flex-wrap items-center gap-x-4 gap-y-2">
              <label className="flex items-center gap-1.5 text-xs text-fg-secondary">
                {t('import.targetTable')}
                {tableOptions.length > 0 ? (
                  <Select
                    className="h-7 w-56"
                    searchable
                    value={targetTable}
                    onChange={setTargetTable}
                    options={tableOptions}
                  />
                ) : (
                  <input
                    type="text"
                    value={targetTable}
                    onChange={(e) => setTargetTable(e.target.value)}
                    placeholder={t('import.tableNamePlaceholder')}
                    className="h-7 w-56 rounded-md border border-edge bg-surface px-2 text-xs text-fg outline-none focus:border-accent"
                  />
                )}
              </label>
              <label className="flex items-center gap-1.5 text-xs text-fg-secondary">
                {t('import.matchMode')}
                <Select
                  className="h-7 w-44"
                  value={mappingMode}
                  onChange={(next) => setMappingMode(next as ImportMappingMode)}
                  options={[
                    { value: 'auto', label: t('import.matchMode.auto') },
                    { value: 'name', label: t('import.matchMode.name') },
                    { value: 'position', label: t('import.matchMode.position') },
                  ]}
                />
              </label>
            </div>

            {targets.loading && (
              <div className="flex items-center gap-2 text-xs text-fg-muted">
                <Spinner size="sm" />
                {t('import.loadingTargets')}
              </div>
            )}
            {targets.error && (
              <CopyableError
                message={t('import.loadTargetsFailed', { message: targets.error })}
                className="rounded-md border border-red-500/20 bg-red-500/10 px-3 py-2 text-xs text-red-400"
              />
            )}

            {targets.columns && mappingPlan && (
              <div className="max-h-56 overflow-auto rounded-md border border-edge">
                <table
                  className="w-full border-collapse text-xs"
                  data-testid="import-mapping-table"
                >
                  <thead className="sticky top-0">
                    <tr className="bg-surface-alt">
                      <th className="border-b border-edge px-2 py-1.5 text-left font-medium text-fg-secondary">
                        {t('import.sourceColumn')}
                      </th>
                      <th className="border-b border-edge px-2 py-1.5 text-left font-medium text-fg-secondary">
                        {t('import.targetColumn')}
                      </th>
                      <th className="border-b border-edge px-2 py-1.5 text-left font-medium text-fg-secondary">
                        {t('import.sampleValue')}
                      </th>
                    </tr>
                  </thead>
                  <tbody>
                    {mappingPlan.entries.map((entry) => {
                      const sample = parsed.rows
                        .slice(0, SAMPLE_ROWS)
                        .map((row) => row[entry.source])
                        .find((value) => value !== null && value !== undefined);
                      return (
                        <tr key={entry.source} className={cn(entry.target == null && 'opacity-60')}>
                          <td className="border-b border-edge px-2 py-1 font-mono">
                            {entry.source}
                          </td>
                          <td className="border-b border-edge px-2 py-1">
                            <Select
                              className="h-6 w-48"
                              value={entry.target ?? '__none__'}
                              onChange={(next) =>
                                setOverrides((state) => ({
                                  ...state,
                                  [entry.source]: next === '__none__' ? null : next,
                                }))
                              }
                              options={[
                                { value: '__none__', label: t('import.dontImport') },
                                ...targets.columns!.map((column) => ({
                                  value: column.name,
                                  label: `${column.name} (${column.dataType})`,
                                })),
                              ]}
                            />
                          </td>
                          <td className="max-w-40 truncate border-b border-edge px-2 py-1 font-mono text-fg-muted">
                            {sample == null
                              ? '—'
                              : typeof sample === 'object'
                                ? JSON.stringify(sample)
                                : String(sample)}
                          </td>
                        </tr>
                      );
                    })}
                  </tbody>
                </table>
              </div>
            )}

            {mappingPlan && (
              <p className="text-[11px] text-fg-muted" data-testid="import-summary">
                {t('import.summary', {
                  sources: parsed.columns.length,
                  mapped: mappedPairs.length,
                  skipped: mappingPlan.entries.length - mappedPairs.length,
                  unwritten: mappingPlan.untargeted.length,
                })}
              </p>
            )}

            {mappingPlan && mappingPlan.conflicts.length > 0 && (
              <div
                className="flex items-start gap-2 rounded-md border border-red-500/20 bg-red-500/10 px-3 py-2 text-xs text-red-400"
                data-testid="import-conflict-warning"
              >
                <AlertTriangle className="mt-0.5 h-3.5 w-3.5 shrink-0" />
                <span>
                  {t('import.conflictWarning', { count: mappingPlan.conflicts.length })}{' '}
                  {mappingPlan.conflicts
                    .map((c) => `${c.target} ← ${c.sources.join(', ')}`)
                    .join('；')}
                </span>
              </div>
            )}
            {duplicates.length > 0 && (
              <div
                className="flex items-start gap-2 rounded-md border border-yellow-500/20 bg-yellow-500/10 px-3 py-2 text-xs text-yellow-600 dark:text-yellow-400"
                data-testid="import-duplicate-warning"
              >
                <AlertTriangle className="mt-0.5 h-3.5 w-3.5 shrink-0" />
                <span>
                  {t('import.duplicateColumnWarning', {
                    columns: [
                      ...new Set(
                        duplicates.map((d) => (d.kind === 'duplicate-column' ? d.column : '')),
                      ),
                    ]
                      .filter(Boolean)
                      .join(', '),
                  })}
                </span>
              </div>
            )}
            {ragged.length > 0 && (
              <div className="flex items-start gap-2 rounded-md border border-yellow-500/20 bg-yellow-500/10 px-3 py-2 text-xs text-yellow-600 dark:text-yellow-400">
                <AlertTriangle className="mt-0.5 h-3.5 w-3.5 shrink-0" />
                <span>
                  {t('import.raggedRowWarning', {
                    count: ragged.length,
                    line: ragged[0].kind === 'ragged-row' ? ragged[0].line : 0,
                  })}
                </span>
              </div>
            )}
            {mappingPlan && mappingPlan.unmapped.length > 0 && (
              <p className="text-[11px] text-fg-muted">
                {t('import.unmappedColumns', { columns: mappingPlan.unmapped.join(', ') })}
              </p>
            )}
          </section>
        )}

        {/* ── 预览 ───────────────────────────────────────────────── */}
        {parsed && parsed.rows.length > 0 && (
          <section className="space-y-1">
            <label className="block text-xs font-medium text-fg-secondary">
              {t('import.previewFirstRows', {
                count: Math.min(PREVIEW_ROWS, parsed.rows.length),
                total: parsed.rows.length,
              })}
            </label>
            <div className="max-h-44 overflow-auto rounded-md border border-edge bg-surface">
              <table className="w-full border-collapse text-xs" data-testid="import-preview">
                <thead className="sticky top-0">
                  <tr className="bg-surface-alt">
                    {parsed.columns.map((col) => (
                      <th
                        key={col}
                        className="border-b border-r border-edge px-2 py-1.5 text-left font-medium text-fg-secondary"
                      >
                        {col}
                      </th>
                    ))}
                  </tr>
                </thead>
                <tbody>
                  {parsed.rows.slice(0, PREVIEW_ROWS).map((row, ri) => (
                    <tr key={ri} className={cn(ri % 2 === 0 ? 'bg-surface' : 'bg-surface-alt/30')}>
                      {parsed.columns.map((col) => (
                        <td
                          key={col}
                          className="max-w-48 truncate border-b border-r border-edge px-2 py-1 font-mono text-fg-secondary"
                        >
                          {row[col] === null ? (
                            <span className="italic text-fg-muted">NULL</span>
                          ) : typeof row[col] === 'object' ? (
                            JSON.stringify(row[col])
                          ) : (
                            String(row[col])
                          )}
                        </td>
                      ))}
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          </section>
        )}

        {/* ── 执行设置 ───────────────────────────────────────────── */}
        {parsed && parsed.rows.length > 0 && phase === 'config' && targets.columns && (
          <section className="grid grid-cols-1 gap-3 sm:grid-cols-2">
            <fieldset className="space-y-1">
              <legend className="mb-1 text-xs font-medium text-fg-secondary">
                {t('import.conflictStrategy')}
              </legend>
              {(['stop', 'skip'] as const).map((value) => (
                <label key={value} className="flex items-center gap-1.5 text-xs">
                  <input
                    type="radio"
                    name="import-on-error"
                    checked={onError === value}
                    onChange={() => setOnError(value)}
                    className="h-3.5 w-3.5 accent-[var(--color-accent)]"
                  />
                  {t(`import.conflictStrategy.${value}`)}
                </label>
              ))}
            </fieldset>
            <fieldset className="space-y-1">
              <legend className="mb-1 text-xs font-medium text-fg-secondary">
                {t('import.commitMode')}
              </legend>
              {(['transaction', 'autocommit'] as const).map((value) => (
                <label key={value} className="flex items-center gap-1.5 text-xs">
                  <input
                    type="radio"
                    name="import-commit-mode"
                    checked={commitMode === value}
                    onChange={() => setCommitMode(value)}
                    className="h-3.5 w-3.5 accent-[var(--color-accent)]"
                  />
                  {t(`import.commitMode.${value}`)}
                </label>
              ))}
              <p className="text-[11px] text-fg-muted">
                {commitMode === 'transaction'
                  ? t('import.transactionNote')
                  : t('import.autocommitNote')}
              </p>
            </fieldset>
          </section>
        )}

        {/* ── 进度与结果 ─────────────────────────────────────────── */}
        {busy && progress && (
          <div
            className="rounded-md border border-edge bg-surface px-3 py-2 text-xs text-fg-secondary"
            data-testid="import-progress"
          >
            {t('import.progress', {
              processed: progress.processedRows,
              committed: progress.committedRows,
              failed: progress.failedRows,
            })}
          </div>
        )}

        {phase === 'done' && result && (
          <section className="space-y-2" data-testid="import-result">
            {result.status === 'completed' && (
              <div className="rounded-md border border-green-500/20 bg-green-500/10 px-3 py-2 text-xs text-green-600 dark:text-green-400">
                {t('import.result.completed', { committed: result.committedRows })}
              </div>
            )}
            {result.status === 'stopped' && result.stoppedBy && (
              <CopyableError
                message={t('import.result.stopped', {
                  committed: result.committedRows,
                  line: result.stoppedBy.sourceLine ?? '—',
                  message: result.stoppedBy.message,
                })}
                className="rounded-md border border-red-500/20 bg-red-500/10 px-3 py-2 text-xs text-red-400"
              />
            )}
            {result.status === 'cancelled' && (
              <div className="rounded-md border border-yellow-500/20 bg-yellow-500/10 px-3 py-2 text-xs text-yellow-600 dark:text-yellow-400">
                {t('import.result.cancelled', { committed: result.committedRows })}
              </div>
            )}
            {result.failedRows.length > 0 && (
              <div className="max-h-40 overflow-auto rounded-md border border-edge">
                <table className="w-full border-collapse text-xs">
                  <thead className="sticky top-0">
                    <tr className="bg-surface-alt">
                      <th className="border-b border-edge px-2 py-1 text-left font-medium text-fg-secondary">
                        #
                      </th>
                      <th className="border-b border-edge px-2 py-1 text-left font-medium text-fg-secondary">
                        {t('import.sourceLine')}
                      </th>
                      <th className="border-b border-edge px-2 py-1 text-left font-medium text-fg-secondary">
                        Error
                      </th>
                    </tr>
                  </thead>
                  <tbody>
                    {result.failedRows.map((failure) => (
                      <tr key={failure.rowIndex}>
                        <td className="border-b border-edge px-2 py-1 font-mono">
                          {failure.rowIndex + 1}
                        </td>
                        <td className="border-b border-edge px-2 py-1 font-mono">
                          {failure.sourceLine ?? '—'}
                        </td>
                        <td className="border-b border-edge px-2 py-1 font-mono text-red-400">
                          {failure.message}
                        </td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
            )}
            {refreshFailed && (
              <div className="flex items-start gap-2 rounded-md border border-yellow-500/20 bg-yellow-500/10 px-3 py-2 text-xs text-yellow-600 dark:text-yellow-400">
                <AlertTriangle className="mt-0.5 h-3.5 w-3.5 shrink-0" />
                <span>{t('import.refreshFailed')}</span>
              </div>
            )}
          </section>
        )}
      </div>
    </Dialog>
  );
}
