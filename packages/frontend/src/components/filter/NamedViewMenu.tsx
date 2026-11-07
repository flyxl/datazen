import { useCallback, useMemo, useState } from 'react';
import { Bookmark, ChevronDown, Trash2 } from 'lucide-react';
import { useI18n } from '../../hooks/useI18n';
import { useConfirmDialog } from '../../hooks/useConfirmDialog';
import { cn } from '../../lib/cn';
import { classifyGridError } from '../../lib/gridErrors';
import { hostErrorMessage } from '../../lib/hostErrorMessage';
import {
  buildNamedViewExport,
  namedViewScopeKey,
  normalizeViewName,
  parseNamedViewExport,
  readViewFilters,
  resolveImportName,
  sortNamedViews,
  viewFromTableState,
  viewNamesEqual,
  type NamedView,
  type NamedViewScope,
  type TableStateForView,
} from '../../lib/namedViews';
import type { FilterCondition } from '../../types';

/**
 * The saved-views menu (08 §4.6).
 *
 * The menu is a small state machine with six states — `idle`, `loading`,
 * `list`, `naming`, `importing`, `error` — and every state has an exit: the
 * user can always get back to the list, and a failure can always be retried.
 *
 * The menu holds **no** command layer: the three operations and the two file
 * operations arrive as props, so the component can be exercised without an
 * IPC and the wiring stays in the toolbar that owns it.
 */

/** The three host commands the menu drives. */
export interface NamedViewOperations {
  /** The views of a scope. */
  list: (scope: NamedViewScope) => Promise<readonly NamedView[]>;
  /** Save a view; the answer carries the minted id and the timestamps. */
  save: (view: NamedView) => Promise<NamedView>;
  /** Delete a view; the answer carries the remaining ones. */
  remove: (id: string) => Promise<readonly NamedView[]>;
}

/** The file operations the export and the import need. */
export interface NamedViewFileOperations {
  /** Write the export file. Resolves `false` when the user cancelled. */
  write: (fileName: string, content: string) => Promise<boolean>;
  /** Read an import file. Resolves `null` when the user cancelled. */
  read: () => Promise<{ fileName: string; content: string } | null>;
}

export interface NamedViewMenuProps {
  /**
   * The scope the menu lists: the persistent connection id, the database, the
   * schema and the table.
   */
  scope: NamedViewScope;
  /** The **applied** state — what "Save as…" saves. */
  applied: TableStateForView;
  /** The draft state, when the user has unapplied changes. */
  draft?: { filters: FilterCondition[]; filterLogic: 'and' | 'or' } | null;
  /** Whether the draft differs from the applied state. */
  dirty?: boolean;
  /** The host commands. */
  operations: NamedViewOperations;
  /** The file commands. */
  files: NamedViewFileOperations;
  /** Apply a view to the table. */
  onApply: (view: NamedView) => void;
  /** Apply the draft filters (the "Apply first" action). */
  onApplyDraft?: () => void;
  /** The columns of the table, to spot a view that names a column that is gone. */
  columns?: readonly string[];
}

/** The states of the menu. */
type MenuState = 'idle' | 'loading' | 'list' | 'naming' | 'importing' | 'error';

export function NamedViewMenu({
  scope,
  applied,
  draft = null,
  dirty = false,
  operations,
  files,
  onApply,
  onApplyDraft,
  columns,
}: NamedViewMenuProps) {
  const { t } = useI18n();
  const [state, setState] = useState<MenuState>('idle');
  const [views, setViews] = useState<NamedView[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [name, setName] = useState('');
  const [renamingId, setRenamingId] = useState<string | null>(null);
  const [renameValue, setRenameValue] = useState('');
  const [confirmDelete, confirmDeleteDialog] = useConfirmDialog();
  const [confirmOverwrite, confirmOverwriteDialog] = useConfirmDialog();

  const open = state !== 'idle';

  /** The message of a failure, classified by the contract code. */
  const describe = useCallback(
    (caught: unknown): string => {
      const raw = hostErrorMessage(caught);
      if (classifyGridError(raw) === 'grid.view.nameExists') {
        return t('filter.view.errorNameExists');
      }
      return raw;
    },
    [t],
  );

  const load = useCallback(async () => {
    setState('loading');
    setError(null);
    try {
      const loaded = await operations.list(scope);
      setViews(sortNamedViews(loaded));
      setState('list');
    } catch (caught) {
      setError(describe(caught));
      setState('error');
    }
  }, [describe, operations, scope]);

  /** The views of this scope, in the order the menu shows them. */
  const scoped = useMemo(() => {
    const key = namedViewScopeKey(scope);
    return sortNamedViews(views.filter((view) => namedViewScopeKey(view.scope) === key));
  }, [scope, views]);

  /** The name is free when no other view of this scope carries it. */
  const nameTaken = useMemo(
    () => scoped.some((view) => viewNamesEqual(view.name, name)),
    [name, scoped],
  );

  const close = useCallback(() => {
    setState('idle');
    setError(null);
    setNotice(null);
    setRenamingId(null);
  }, []);

  /** Replace one entry of the list with the record the host returned. */
  const replaceInList = useCallback((saved: NamedView) => {
    setViews((current) =>
      sortNamedViews([...current.filter((entry) => entry.id !== saved.id), saved]),
    );
  }, []);

  /** Save a new view under a name. */
  const saveNamed = useCallback(
    async (viewName: string, source: TableStateForView) => {
      const record = viewFromTableState(source, { name: normalizeViewName(viewName) });
      try {
        const saved = await operations.save(record);
        replaceInList(saved);
        setState('list');
        setError(null);
      } catch (caught) {
        setError(describe(caught));
        setState('error');
      }
    },
    [describe, operations, replaceInList],
  );

  const handleSaveAs = useCallback(async () => {
    const trimmed = normalizeViewName(name);
    if (trimmed === '' || nameTaken) return;
    await saveNamed(trimmed, applied);
    setName('');
  }, [applied, name, nameTaken, saveNamed]);

  /** Apply first, then save: the view carries the filters the user applied. */
  const handleApplyFirstAndSave = useCallback(async () => {
    const trimmed = normalizeViewName(name);
    if (trimmed === '' || nameTaken) return;
    onApplyDraft?.();
    const source: TableStateForView = {
      ...applied,
      filters: draft?.filters ?? applied.filters,
      filterLogic: draft?.filterLogic ?? applied.filterLogic,
    };
    await saveNamed(trimmed, source);
    setName('');
  }, [applied, draft, name, nameTaken, onApplyDraft, saveNamed]);

  /**
   * Rename a view in place; the id never changes.
   *
   * A name that another view of the scope already carries is an overwrite:
   * the user is asked, and on confirmation the renamed view takes over the
   * other view's id and the other record is dropped — the list never shows
   * two views with the same name.
   */
  const handleRename = useCallback(
    async (view: NamedView) => {
      const trimmed = normalizeViewName(renameValue);
      setRenamingId(null);
      if (trimmed === '' || viewNamesEqual(view.name, trimmed)) return;
      const conflict = scoped.find(
        (entry) => entry.id !== view.id && viewNamesEqual(entry.name, trimmed),
      );
      if (conflict) {
        const overwrite = await confirmOverwrite({
          title: t('filter.view.overwrite'),
          message: t('filter.view.nameExists'),
          confirmLabel: t('filter.view.overwrite'),
          kind: 'warning',
        });
        if (!overwrite) return;
        try {
          const saved = await operations.save({ ...view, name: trimmed, id: conflict.id });
          replaceInList(saved);
          // The renamed view's own record is dropped, so only one view of
          // that name is left.
          const remaining = await operations.remove(view.id);
          setViews(sortNamedViews([...remaining]));
          setState('list');
          setError(null);
        } catch (caught) {
          setError(describe(caught));
          setState('error');
        }
        return;
      }
      try {
        const saved = await operations.save({ ...view, name: trimmed });
        replaceInList(saved);
        setState('list');
        setError(null);
      } catch (caught) {
        setError(describe(caught));
        setState('error');
      }
    },
    [confirmOverwrite, describe, operations, renameValue, replaceInList, scoped, t],
  );

  /** Make a view the default of its scope. */
  const handleSetDefault = useCallback(
    async (view: NamedView) => {
      try {
        const saved = await operations.save({ ...view, isDefault: true });
        const key = namedViewScopeKey(saved.scope);
        setViews((current) =>
          sortNamedViews(
            current.map((entry) =>
              namedViewScopeKey(entry.scope) === key
                ? { ...entry, isDefault: entry.id === saved.id }
                : entry,
            ),
          ),
        );
        setState('list');
      } catch (caught) {
        setError(describe(caught));
        setState('error');
      }
    },
    [describe, operations],
  );

  const handleDelete = useCallback(
    async (view: NamedView) => {
      const confirmed = await confirmDelete({
        title: t('filter.view.delete'),
        message: t('filter.view.deleteConfirm', { name: view.name }),
        confirmLabel: t('filter.view.delete'),
        kind: 'warning',
      });
      if (!confirmed) return;
      try {
        const remaining = await operations.remove(view.id);
        setViews(sortNamedViews([...remaining]));
        setState('list');
      } catch (caught) {
        setError(describe(caught));
        setState('error');
      }
    },
    [confirmDelete, describe, operations, t],
  );

  /** Write the views of this scope to a file. */
  const handleExport = useCallback(async () => {
    try {
      const file = buildNamedViewExport(scoped);
      await files.write(`${scope.table}-views.json`, JSON.stringify(file, null, 2));
      setState('list');
    } catch (caught) {
      setError(describe(caught));
      setState('error');
    }
  }, [describe, files, scope.table, scoped]);

  /** Read a file and import its views into this scope. */
  const handleImport = useCallback(async () => {
    setState('importing');
    setError(null);
    try {
      const opened = await files.read();
      if (!opened) {
        setState('list');
        return;
      }
      const file = parseNamedViewExport(opened.content);
      const taken = new Set(scoped.map((view) => view.name));
      let imported = 0;
      for (const entry of file.views) {
        const name = resolveImportName(entry.name, taken);
        taken.add(name);
        // The scope of an imported view is rewritten to the current table:
        // the columns of another table are not this table's columns.
        const readout = readViewFilters(entry.filters);
        if (readout.unknownOperators.length > 0) {
          // The host's operator set is closed, so a condition the host does
          // not know cannot be stored: the whole import is refused rather
          // than half of it.
          throw new Error(
            t('filter.errorUnsupportedOperator', {
              operator: readout.unknownOperators[0],
              alternatives: readout.unknownOperators.slice(1).join(', '),
            }),
          );
        }
        const record = viewFromTableState(
          {
            scope,
            filters: readout.filters,
            filterLogic: entry.filterLogic,
            sorts: entry.sorts,
            visibleColumns: entry.visibleColumns,
            columnOrder: entry.columnOrder,
          },
          { name },
        );
        const saved = await operations.save(record);
        replaceInList(saved);
        imported += 1;
      }
      setNotice(t('filter.view.importDone', { count: imported }));
      setState('list');
    } catch (caught) {
      setError(t('filter.view.importFailed', { error: describe(caught) }));
      setState('error');
    }
  }, [describe, files, operations, replaceInList, scope, scoped, t]);

  /** Apply a view: the filters, the sorts and the column layout. */
  const handleApply = useCallback(
    (view: NamedView) => {
      const readout = readViewFilters(view.filters);
      if (readout.unknownOperators.length > 0) {
        setError(
          t('filter.errorUnsupportedOperator', {
            operator: readout.unknownOperators[0],
            alternatives: readout.unknownOperators.slice(1).join(', '),
          }),
        );
        setState('error');
        return;
      }
      const missing = columns
        ? readout.filters.filter((condition) => !columns.includes(condition.column))
        : [];
      if (missing.length > 0) {
        setError(t('filter.view.missingColumn', { column: missing[0].column }));
        setState('error');
        return;
      }
      onApply(view);
      close();
    },
    [close, columns, onApply, t],
  );

  return (
    <div className="relative shrink-0" data-testid="named-view-menu">
      <button
        type="button"
        onMouseDown={(event) => event.preventDefault()}
        onClick={() => (open ? close() : void load())}
        className={cn(
          'mt-0 flex h-7 items-center gap-1 rounded px-1.5 text-xs transition-colors hover:bg-surface-alt',
          open ? 'bg-surface-alt text-fg' : 'text-fg-muted hover:text-fg',
        )}
        title={t('filter.view.savedViews')}
        aria-label={t('filter.view.savedViews')}
        aria-expanded={open}
        data-testid="named-view-trigger"
      >
        <Bookmark className="h-3.5 w-3.5" />
        <span>{t('filter.view.savedViews')}</span>
        <ChevronDown className="h-3 w-3" />
      </button>

      {open && (
        <div
          className="absolute right-0 top-8 z-30 w-72 rounded-md border border-edge bg-surface p-2 shadow-lg"
          data-testid="named-view-panel"
          data-state={state}
        >
          {state === 'loading' && (
            <div className="px-2 py-3 text-xs text-fg-muted" data-testid="named-view-loading">
              {t('common.loading')}
            </div>
          )}

          {state === 'error' && (
            <div className="space-y-2 px-2 py-2">
              <p className="text-xs text-danger" data-testid="named-view-error">
                {error}
              </p>
              <button
                type="button"
                className="rounded border border-edge px-2 py-1 text-xs text-fg hover:bg-surface-alt"
                onClick={() => void load()}
                data-testid="named-view-retry"
              >
                {t('common.retry')}
              </button>
            </div>
          )}

          {state === 'naming' && (
            <div className="space-y-2 px-2 py-2">
              <label className="block text-xs text-fg-muted" htmlFor="named-view-name">
                {t('filter.view.nameLabel')}
              </label>
              <input
                id="named-view-name"
                type="text"
                value={name}
                autoFocus
                onChange={(event) => setName(event.target.value)}
                onKeyDown={(event) => {
                  if (event.key === 'Enter' && !event.nativeEvent.isComposing) {
                    event.preventDefault();
                    void handleSaveAs();
                  }
                }}
                placeholder={t('filter.view.namePlaceholder')}
                className="h-7 w-full rounded border border-edge bg-surface px-2 text-xs text-fg focus:border-accent focus:outline-none"
                data-testid="named-view-name-input"
              />
              {nameTaken && name.trim() !== '' && (
                <p className="text-xs text-warning" data-testid="named-view-name-taken">
                  {t('filter.view.nameExists')}
                </p>
              )}
              {dirty && (
                <div className="space-y-1">
                  <p className="text-xs text-fg-muted" data-testid="named-view-draft-hint">
                    {t('filter.view.draftNotAppliedHint')}
                  </p>
                  <button
                    type="button"
                    className="rounded border border-edge px-2 py-1 text-xs text-fg hover:bg-surface-alt"
                    onClick={() => void handleApplyFirstAndSave()}
                    data-testid="named-view-apply-first"
                  >
                    {t('filter.view.applyAnyway')}
                  </button>
                </div>
              )}
              <div className="flex justify-end gap-1">
                <button
                  type="button"
                  className="rounded border border-edge px-2 py-1 text-xs text-fg hover:bg-surface-alt"
                  onClick={() => {
                    setName('');
                    setState('list');
                  }}
                  data-testid="named-view-cancel"
                >
                  {t('common.cancel')}
                </button>
                <button
                  type="button"
                  disabled={name.trim() === '' || nameTaken}
                  className="rounded bg-accent px-2 py-1 text-xs text-white disabled:opacity-50"
                  onClick={() => void handleSaveAs()}
                  data-testid="named-view-save"
                >
                  {t('filter.view.save')}
                </button>
              </div>
            </div>
          )}

          {state === 'importing' && (
            <div className="px-2 py-3 text-xs text-fg-muted" data-testid="named-view-importing">
              {t('filter.view.import')}
            </div>
          )}

          {state === 'list' && (
            <>
              {notice && (
                <p className="px-2 pb-1 text-xs text-fg-muted" data-testid="named-view-notice">
                  {notice}
                </p>
              )}
              {scoped.length === 0 ? (
                <p className="px-2 py-3 text-xs text-fg-muted" data-testid="named-view-empty">
                  {t('filter.view.empty')}
                </p>
              ) : (
                <ul className="max-h-64 overflow-y-auto">
                  {scoped.map((view) => (
                    <li
                      key={view.id}
                      className="flex items-center gap-1 rounded px-1 py-1 text-xs hover:bg-surface-alt"
                      data-testid="named-view-item"
                      data-view-id={view.id}
                    >
                      {renamingId === view.id ? (
                        <input
                          type="text"
                          value={renameValue}
                          autoFocus
                          onChange={(event) => setRenameValue(event.target.value)}
                          onKeyDown={(event) => {
                            if (event.key === 'Enter' && !event.nativeEvent.isComposing) {
                              event.preventDefault();
                              void handleRename(view);
                            }
                            if (event.key === 'Escape') setRenamingId(null);
                          }}
                          className="h-6 min-w-0 flex-1 rounded border border-edge bg-surface px-1 text-xs text-fg focus:border-accent focus:outline-none"
                          data-testid="named-view-rename-input"
                        />
                      ) : (
                        <>
                          <button
                            type="button"
                            className="min-w-0 flex-1 truncate text-left text-fg"
                            onClick={() => handleApply(view)}
                            data-testid="named-view-apply"
                          >
                            {view.name}
                          </button>
                          {view.isDefault && (
                            <span
                              className="rounded bg-accent/15 px-1 text-[10px] text-accent"
                              data-testid="named-view-default-badge"
                            >
                              {t('filter.view.defaultBadge')}
                            </span>
                          )}
                          {!view.isDefault && (
                            <button
                              type="button"
                              className="rounded px-1 text-[10px] text-fg-muted hover:text-fg"
                              onClick={() => void handleSetDefault(view)}
                              data-testid="named-view-set-default"
                            >
                              {t('filter.view.setDefault')}
                            </button>
                          )}
                          <button
                            type="button"
                            className="rounded px-1 text-[10px] text-fg-muted hover:text-fg"
                            onClick={() => {
                              setRenamingId(view.id);
                              setRenameValue(view.name);
                            }}
                            data-testid="named-view-rename"
                          >
                            {t('filter.view.rename')}
                          </button>
                          <button
                            type="button"
                            className="rounded p-0.5 text-fg-muted hover:text-danger"
                            onClick={() => void handleDelete(view)}
                            title={t('filter.view.delete')}
                            aria-label={t('filter.view.delete')}
                            data-testid="named-view-delete"
                          >
                            <Trash2 className="h-3 w-3" />
                          </button>
                        </>
                      )}
                    </li>
                  ))}
                </ul>
              )}
              <div className="mt-1 flex items-center justify-between gap-1 border-t border-edge pt-1">
                <button
                  type="button"
                  className="rounded px-1 py-0.5 text-[11px] text-fg-muted hover:text-fg"
                  onClick={() => {
                    setName('');
                    setState('naming');
                  }}
                  data-testid="named-view-save-as"
                >
                  {t('filter.view.saveAs')}
                </button>
                <div className="flex items-center gap-1">
                  <button
                    type="button"
                    className="rounded px-1 py-0.5 text-[11px] text-fg-muted hover:text-fg"
                    onClick={() => void handleExport()}
                    data-testid="named-view-export"
                  >
                    {t('filter.view.export')}
                  </button>
                  <button
                    type="button"
                    className="rounded px-1 py-0.5 text-[11px] text-fg-muted hover:text-fg"
                    onClick={() => void handleImport()}
                    data-testid="named-view-import"
                  >
                    {t('filter.view.import')}
                  </button>
                </div>
              </div>
            </>
          )}
        </div>
      )}
      {confirmDeleteDialog}
      {confirmOverwriteDialog}
    </div>
  );
}
