import { useEffect, useImperativeHandle, useRef, forwardRef } from 'react';
import { EditorView, keymap, placeholder as cmPlaceholder, lineNumbers } from '@codemirror/view';
import { EditorState, Compartment } from '@codemirror/state';
import { defaultKeymap, history, historyKeymap } from '@codemirror/commands';
import { sql, PostgreSQL, MySQL, MariaSQL, SQLite, StandardSQL } from '@codemirror/lang-sql';
import type { SQLDialect } from '@codemirror/lang-sql';
import { contextualSchemaCompletion } from '../lib/sqlCompletionContext';
import {
  autocompletion,
  closeBrackets,
  acceptCompletion,
  completeFromList,
} from '@codemirror/autocomplete';
import { sqlFunctionCompletions } from '../lib/sqlCompletions';
import { searchKeymap } from '@codemirror/search';
import { resolveEditorFontFamily, HOST_DEFAULT_EDITOR_FONT } from '../lib/resolveEditorFontFamily';
import {
  editorSyntaxHighlighting,
  readEditorColorsFromElement,
  type EditorColorContract,
} from '../lib/themeEditorColors';
import { useSettingsStore } from '../stores/settingsStore';
import { DB_REGISTRY } from '../lib/databaseTypes';
import { parseQualifiedPathParents } from '../lib/sqlPathPrefix';
import { namespaceLoadingCompletionSource } from '../lib/namespaceLoadingCompletion';
import { t } from '../locales/t';
import type { SqlNamespace } from '../lib/sqlNamespace';
import type { DatabaseType } from '../types';
import { toggleSqlLineComments } from '../lib/sqlEditorContextMenu';
import { getStatementAtCursor } from '../lib/sqlStatementRange';

interface ThemeConfig {
  dark: boolean;
  fontSize: number;
  fontFamily: string;
}

function makeEditorTheme({ dark, fontSize, fontFamily }: ThemeConfig, colors: EditorColorContract) {
  return EditorView.theme(
    {
      '&': {
        height: '100%',
        fontSize: `${fontSize}px`,
        backgroundColor: colors.background,
        color: colors.foreground,
      },
      '.cm-content': {
        fontFamily: `${fontFamily}, ui-monospace, SFMono-Regular, Menlo, monospace`,
        padding: '12px 0',
        caretColor: colors.cursor,
      },
      '.cm-cursor': {
        borderLeftColor: colors.cursor,
      },
      '.cm-activeLine': {
        backgroundColor: dark ? 'rgba(30,41,59,0.5)' : 'rgba(241,245,249,0.5)',
      },
      '.cm-selectionBackground, &.cm-focused .cm-selectionBackground': {
        backgroundColor: colors.selection,
      },
      '.cm-gutters': {
        backgroundColor: dark ? '#1e293b' : '#f8fafc',
        color: dark ? '#64748b' : '#94a3b8',
        border: 'none',
        borderRight: `1px solid ${dark ? '#334155' : '#e2e8f0'}`,
      },
      '.cm-activeLineGutter': {
        backgroundColor: dark ? 'rgba(51,65,85,0.5)' : 'rgba(226,232,240,0.5)',
      },
      '.cm-tooltip': {
        backgroundColor: dark ? '#1e293b' : '#ffffff',
        border: `1px solid ${dark ? '#334155' : '#e2e8f0'}`,
        color: dark ? '#f1f5f9' : '#0f172a',
        borderRadius: '6px',
        boxShadow: dark ? '0 4px 12px rgba(0,0,0,0.4)' : '0 4px 12px rgba(0,0,0,0.1)',
      },
      '.cm-tooltip-autocomplete': {
        '& > ul': { maxHeight: '240px' },
        '& > ul > li': {
          padding: '2px 8px',
          fontSize: '12px',
          lineHeight: '1.6',
        },
      },
      '.cm-tooltip-autocomplete ul li[aria-selected]': {
        backgroundColor: dark ? '#334155' : '#dbeafe',
        color: dark ? '#f1f5f9' : '#1e3a5f',
      },
      '.cm-completionIcon': {
        width: '1.2em',
        opacity: '0.7',
      },
      '.cm-placeholder': {
        color: dark ? '#64748b' : '#94a3b8',
      },
    },
    { dark },
  );
}

function themeExtensions(config: ThemeConfig) {
  const colors = readEditorColorsFromElement();
  return [makeEditorTheme(config, colors), editorSyntaxHighlighting(colors, config.dark)];
}

/** Nested schema for CodeMirror SQL autocompletion */
export type SqlSchema = SqlNamespace;

export interface DroppedTablePayload {
  tables: Array<{ tableName: string; schema?: string }>;
  connectionId?: string;
  dbSessionId?: string;
  databaseType?: string;
}

export interface SqlEditorHandle {
  getSelection: () => string;
  /** Toggle `-- ` comments on selected lines (or the line containing the cursor). */
  toggleLineComment: () => void;
  /** Insert SQL text at the given position (or append at the end if pos is null/undefined). */
  insertAt: (text: string, pos?: number | null) => void;
}

const CM_DIALECT_MAP: Record<string, SQLDialect> = {
  postgresql: PostgreSQL,
  mysql: MySQL,
  mariadb: MariaSQL,
  sqlite: SQLite,
};

/** Resolve CodeMirror SQL dialect; plugins may map via `sqlDialect` (e.g. kiwi → mysql). */
function sqlEditorExtensions(
  databaseType?: string,
  schema?: SqlSchema,
  namespaceLoading?: boolean,
  defaultSchema?: string,
  defaultTable?: string,
) {
  const dialect = resolveCmDialect(databaseType);
  return [
    // Do not pass `schema` into sql() — its built-in source mixes tables into WHERE.
    sql({
      dialect,
      upperCaseKeywords: true,
    }),
    dialect.language.data.of({
      autocomplete: contextualSchemaCompletion({
        dialect,
        schema: schema ?? {},
        defaultSchema,
        defaultTable,
      }),
    }),
    dialect.language.data.of({
      autocomplete: completeFromList(sqlFunctionCompletions(databaseType)),
    }),
    // CompletionSource is not an Extension — must be registered via language data.
    dialect.language.data.of({
      autocomplete: namespaceLoadingCompletionSource(
        Boolean(namespaceLoading),
        t('query.namespaceLoading'),
      ),
    }),
  ];
}

export function resolveCmDialect(dbType?: string): SQLDialect {
  if (!dbType) return StandardSQL;
  if (CM_DIALECT_MAP[dbType]) return CM_DIALECT_MAP[dbType];
  const mapped = DB_REGISTRY[dbType as DatabaseType]?.sqlDialect;
  if (mapped && CM_DIALECT_MAP[mapped]) return CM_DIALECT_MAP[mapped];
  return StandardSQL;
}

interface SqlEditorProps {
  value: string;
  onChange: (value: string) => void;
  onExecute?: () => void;
  onExecuteSelection?: (sql: string) => void;
  /** Execute all statements in the query (Cmd/Ctrl + Shift + Enter). */
  onExecuteAll?: () => void;
  /** Save query (Cmd/Ctrl + S). */
  onSaveQuery?: () => void;
  onContextMenu?: (e: MouseEvent, selectedSql: string) => void;
  onQualifiedPath?: (parents: string[]) => void;
  placeholder?: string;
  schema?: SqlSchema;
  databaseType?: string;
  /** True while a lazy namespace path is fetching for autocomplete. */
  namespaceLoading?: boolean;
  /** CodeMirror: tables in this schema complete without a prefix (`public.users` → `users`). */
  defaultSchema?: string;
  /** CodeMirror: columns of this table complete at the top level (WHERE / SELECT). */
  defaultTable?: string;
  className?: string;
  /** Callback when tables are dropped into the editor from SchemaTree. */
  onDropTable?: (payload: DroppedTablePayload, pos: number | null) => void;
}

function parentsEqual(a: readonly string[], b: readonly string[]): boolean {
  return a.length === b.length && a.every((seg, i) => seg === b[i]);
}

export const SqlEditor = forwardRef<SqlEditorHandle, SqlEditorProps>(function SqlEditor(
  {
    value,
    onChange,
    onExecute,
    onExecuteSelection,
    onExecuteAll,
    onSaveQuery,
    onContextMenu: onCtxMenu,
    onQualifiedPath,
    placeholder,
    schema,
    databaseType,
    namespaceLoading,
    defaultSchema,
    defaultTable,
    className,
    onDropTable,
  },
  ref,
) {
  const containerRef = useRef<HTMLDivElement>(null);
  const viewRef = useRef<EditorView | null>(null);
  const themeCompartment = useRef(new Compartment());
  const sqlCompartment = useRef(new Compartment());
  const onChangeRef = useRef(onChange);
  const onExecuteRef = useRef(onExecute);
  const onExecuteSelectionRef = useRef(onExecuteSelection);
  const onExecuteAllRef = useRef(onExecuteAll);
  const onSaveQueryRef = useRef(onSaveQuery);
  const onCtxMenuRef = useRef(onCtxMenu);
  const onQualifiedPathRef = useRef(onQualifiedPath);
  const onDropTableRef = useRef(onDropTable);
  const lastParentsRef = useRef<string[]>([]);

  const editorFontSize = useSettingsStore((s) => s.settings.editorFontSize);
  const editorFontFamily = useSettingsStore((s) => s.settings.editorFontFamily);

  onChangeRef.current = onChange;
  onExecuteRef.current = onExecute;
  onExecuteSelectionRef.current = onExecuteSelection;
  onExecuteAllRef.current = onExecuteAll;
  onSaveQueryRef.current = onSaveQuery;
  onCtxMenuRef.current = onCtxMenu;
  onQualifiedPathRef.current = onQualifiedPath;
  onDropTableRef.current = onDropTable;

  useImperativeHandle(ref, () => ({
    getSelection: () => {
      const view = viewRef.current;
      if (!view) return '';
      return view.state.sliceDoc(view.state.selection.main.from, view.state.selection.main.to);
    },
    toggleLineComment: () => {
      const view = viewRef.current;
      if (!view) return;
      const { state } = view;
      const sel = state.selection.main;
      const fromLine = state.doc.lineAt(sel.from);
      const toLine = state.doc.lineAt(sel.to > sel.from ? sel.to - 1 : sel.to);
      const from = fromLine.from;
      const to = toLine.to;
      const original = state.sliceDoc(from, to);
      const next = toggleSqlLineComments(original);
      if (next === original) return;
      view.dispatch({
        changes: { from, to, insert: next },
        selection: {
          anchor: from,
          head: from + next.length,
        },
      });
    },
    insertAt: (text: string, pos?: number | null) => {
      const view = viewRef.current;
      if (!view) return;
      const docLen = view.state.doc.length;
      const docStr = view.state.doc.toString();
      if (docStr.trim().length === 0) {
        view.dispatch({
          changes: { from: 0, to: docLen, insert: text },
          selection: { anchor: text.length },
        });
        return;
      }
      const targetPos = pos != null ? Math.max(0, Math.min(pos, docLen)) : docLen;
      const before = docStr.slice(0, targetPos);
      const after = docStr.slice(targetPos);
      const needLeadingNewline = before.length > 0 && !before.endsWith('\n\n');
      const prefix = needLeadingNewline ? (before.endsWith('\n') ? '\n' : '\n\n') : '';
      const needTrailingNewline = after.length > 0 && !after.startsWith('\n\n');
      const suffix = needTrailingNewline ? (after.startsWith('\n') ? '\n' : '\n\n') : '';
      const insertText = `${prefix}${text}${suffix}`;
      view.dispatch({
        changes: { from: targetPos, insert: insertText },
        selection: { anchor: targetPos + insertText.length },
      });
    },
  }));

  function currentThemeConfig(): ThemeConfig {
    const { editorFontSize: fs, editorFontFamily: ff } = useSettingsStore.getState().settings;
    const computedEditorVar = getComputedStyle(document.documentElement)
      .getPropertyValue('--font-editor')
      .trim();
    return {
      dark: document.documentElement.classList.contains('dark'),
      fontSize: fs,
      fontFamily: resolveEditorFontFamily(ff, computedEditorVar, HOST_DEFAULT_EDITOR_FONT),
    };
  }

  useEffect(() => {
    if (!containerRef.current) return;

    const config = currentThemeConfig();

    const state = EditorState.create({
      doc: value,
      extensions: [
        keymap.of([
          {
            key: 'Mod-Enter',
            run: (view) => {
              const sel = view.state.sliceDoc(
                view.state.selection.main.from,
                view.state.selection.main.to,
              );
              if (sel.trim()) {
                onExecuteSelectionRef.current?.(sel);
              } else {
                const docText = view.state.doc.toString();
                const stmt = getStatementAtCursor(docText, view.state.selection.main.head);
                if (stmt) {
                  onExecuteSelectionRef.current?.(stmt);
                } else {
                  onExecuteRef.current?.();
                }
              }
              return true;
            },
          },
          {
            key: 'Mod-Shift-Enter',
            run: () => {
              if (onExecuteAllRef.current) {
                onExecuteAllRef.current();
              } else {
                onExecuteRef.current?.();
              }
              return true;
            },
          },
          {
            key: 'Mod-s',
            run: () => {
              onSaveQueryRef.current?.();
              return true;
            },
          },
          { key: 'Tab', run: acceptCompletion },
        ]),
        history(),
        keymap.of([...defaultKeymap, ...historyKeymap, ...searchKeymap]),
        lineNumbers(),
        closeBrackets(),
        autocompletion({
          activateOnTyping: true,
          defaultKeymap: true,
        }),
        sqlCompartment.current.of(
          sqlEditorExtensions(databaseType, schema, namespaceLoading, defaultSchema, defaultTable),
        ),
        themeCompartment.current.of(themeExtensions(config)),
        EditorView.lineWrapping,
        EditorView.domEventHandlers({
          contextmenu: (e, view) => {
            const handler = onCtxMenuRef.current;
            if (!handler) return false;
            const sel = view.state.sliceDoc(
              view.state.selection.main.from,
              view.state.selection.main.to,
            );
            // Prefer selection; fall back to full doc (may be empty so Paste still works).
            const sqlText = sel.trim() || view.state.doc.toString().trim();
            e.preventDefault();
            e.stopPropagation();
            handler(e, sqlText);
            return true;
          },
          dragover: (e) => {
            if (e.dataTransfer?.types.includes('application/datazen-table')) {
              e.preventDefault();
              e.dataTransfer.dropEffect = 'copy';
              return true;
            }
            return false;
          },
          drop: (e, view) => {
            const rawData = e.dataTransfer?.getData('application/datazen-table');
            if (!rawData) return false;
            e.preventDefault();
            e.stopPropagation();
            try {
              const parsed = JSON.parse(rawData);
              const payload: DroppedTablePayload = parsed.tables
                ? parsed
                : { tables: [parsed] };
              const pos =
                e.clientX != null && e.clientY != null
                  ? (view.posAtCoords({ x: e.clientX, y: e.clientY }) ?? view.state.selection.main.head)
                  : view.state.selection.main.head;
              onDropTableRef.current?.(payload, pos);
              return true;
            } catch {
              return false;
            }
          },
        }),
        EditorView.updateListener.of((update) => {
          if (update.docChanged) {
            onChangeRef.current(update.state.doc.toString());
          }
          if (update.docChanged || update.selectionSet) {
            const parents = parseQualifiedPathParents(
              update.state.doc.toString(),
              update.state.selection.main.head,
            );
            if (!parentsEqual(lastParentsRef.current, parents)) {
              lastParentsRef.current = parents;
              onQualifiedPathRef.current?.(parents);
            }
          }
        }),
        ...(placeholder ? [cmPlaceholder(placeholder)] : []),
      ],
    });

    const view = new EditorView({ state, parent: containerRef.current });
    viewRef.current = view;
    const initialParents = parseQualifiedPathParents(
      view.state.doc.toString(),
      view.state.selection.main.head,
    );
    lastParentsRef.current = initialParents;
    onQualifiedPathRef.current?.(initialParents);

    const observer = new MutationObserver(() => {
      view.dispatch({
        effects: themeCompartment.current.reconfigure(themeExtensions(currentThemeConfig())),
      });
    });
    observer.observe(document.documentElement, { attributes: true, attributeFilter: ['class'] });

    return () => {
      observer.disconnect();
      view.destroy();
      viewRef.current = null;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // Reconfigure theme when font settings change
  useEffect(() => {
    const view = viewRef.current;
    if (!view) return;
    view.dispatch({
      effects: themeCompartment.current.reconfigure(themeExtensions(currentThemeConfig())),
    });
  }, [editorFontSize, editorFontFamily]);

  // Reconfigure when theme pack CSS / editor.json overlay changes
  useEffect(() => {
    const reconfigure = () => {
      const view = viewRef.current;
      if (!view) return;
      view.dispatch({
        effects: themeCompartment.current.reconfigure(themeExtensions(currentThemeConfig())),
      });
    };
    document.addEventListener('datazen:theme-pack-changed', reconfigure);
    return () => document.removeEventListener('datazen:theme-pack-changed', reconfigure);
  }, []);

  // Dynamically update schema / loading completions
  useEffect(() => {
    const view = viewRef.current;
    if (!view) return;
    view.dispatch({
      effects: sqlCompartment.current.reconfigure(
        sqlEditorExtensions(databaseType, schema, namespaceLoading, defaultSchema, defaultTable),
      ),
    });
  }, [schema, databaseType, namespaceLoading, defaultSchema, defaultTable]);

  useEffect(() => {
    const view = viewRef.current;
    if (!view) return;
    const current = view.state.doc.toString();
    if (current !== value) {
      view.dispatch({
        changes: { from: 0, to: current.length, insert: value },
      });
    }
  }, [value]);

  return (
    <div
      ref={containerRef}
      className={`h-full w-full overflow-hidden${className ? ` ${className}` : ''}`}
    />
  );
});
