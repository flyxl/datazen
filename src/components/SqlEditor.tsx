import { useEffect, useImperativeHandle, useRef, forwardRef } from 'react';
import { EditorView, keymap, placeholder as cmPlaceholder, lineNumbers } from '@codemirror/view';
import { EditorState, Compartment } from '@codemirror/state';
import { defaultKeymap, history, historyKeymap } from '@codemirror/commands';
import { sql, PostgreSQL, MySQL, MariaSQL, SQLite, StandardSQL } from '@codemirror/lang-sql';
import type { SQLDialect } from '@codemirror/lang-sql';
import { autocompletion, closeBrackets, acceptCompletion } from '@codemirror/autocomplete';
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
import type { SqlNamespace } from '../lib/sqlNamespace';
import type { DatabaseType } from '../types';

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
        boxShadow: dark
          ? '0 4px 12px rgba(0,0,0,0.4)'
          : '0 4px 12px rgba(0,0,0,0.1)',
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
  return [
    makeEditorTheme(config, colors),
    editorSyntaxHighlighting(colors, config.dark),
  ];
}

/** Nested schema for CodeMirror SQL autocompletion */
export type SqlSchema = SqlNamespace;

export interface SqlEditorHandle {
  getSelection: () => string;
}

const CM_DIALECT_MAP: Record<string, SQLDialect> = {
  postgresql: PostgreSQL,
  mysql: MySQL,
  mariadb: MariaSQL,
  sqlite: SQLite,
};

/** Resolve CodeMirror SQL dialect; plugins may map via `sqlDialect` (e.g. kiwi → mysql). */
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
  onContextMenu?: (e: MouseEvent, selectedSql: string) => void;
  onQualifiedPath?: (parents: string[]) => void;
  placeholder?: string;
  schema?: SqlSchema;
  databaseType?: string;
}

function parentsEqual(a: readonly string[], b: readonly string[]): boolean {
  return a.length === b.length && a.every((seg, i) => seg === b[i]);
}

export const SqlEditor = forwardRef<SqlEditorHandle, SqlEditorProps>(function SqlEditor({ value, onChange, onExecute, onExecuteSelection, onContextMenu: onCtxMenu, onQualifiedPath, placeholder, schema, databaseType }, ref) {
  const containerRef = useRef<HTMLDivElement>(null);
  const viewRef = useRef<EditorView | null>(null);
  const themeCompartment = useRef(new Compartment());
  const sqlCompartment = useRef(new Compartment());
  const onChangeRef = useRef(onChange);
  const onExecuteRef = useRef(onExecute);
  const onExecuteSelectionRef = useRef(onExecuteSelection);
  const onCtxMenuRef = useRef(onCtxMenu);
  const onQualifiedPathRef = useRef(onQualifiedPath);
  const lastParentsRef = useRef<string[]>([]);

  const editorFontSize = useSettingsStore((s) => s.settings.editorFontSize);
  const editorFontFamily = useSettingsStore((s) => s.settings.editorFontFamily);

  onChangeRef.current = onChange;
  onExecuteRef.current = onExecute;
  onExecuteSelectionRef.current = onExecuteSelection;
  onCtxMenuRef.current = onCtxMenu;
  onQualifiedPathRef.current = onQualifiedPath;

  useImperativeHandle(ref, () => ({
    getSelection: () => {
      const view = viewRef.current;
      if (!view) return '';
      return view.state.sliceDoc(
        view.state.selection.main.from,
        view.state.selection.main.to
      );
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
                view.state.selection.main.to
              );
              if (sel.trim()) {
                onExecuteSelectionRef.current?.(sel);
              } else {
                onExecuteRef.current?.();
              }
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
          sql({
            dialect: resolveCmDialect(databaseType),
            upperCaseKeywords: true,
            schema: schema ?? {},
          }),
        ),
        themeCompartment.current.of(themeExtensions(config)),
        EditorView.lineWrapping,
        EditorView.domEventHandlers({
          contextmenu: (e, view) => {
            const handler = onCtxMenuRef.current;
            if (!handler) return false;
            const sel = view.state.sliceDoc(
              view.state.selection.main.from,
              view.state.selection.main.to
            );
            const sqlText = sel.trim() || view.state.doc.toString().trim();
            if (sqlText) {
              e.preventDefault();
              e.stopPropagation();
              handler(e, sqlText);
              return true;
            }
            return false;
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

  // Dynamically update schema when it changes
  useEffect(() => {
    const view = viewRef.current;
    if (!view) return;
    view.dispatch({
      effects: sqlCompartment.current.reconfigure(
        sql({
          dialect: resolveCmDialect(databaseType),
          upperCaseKeywords: true,
          schema: schema ?? {},
        }),
      ),
    });
  }, [schema, databaseType]);

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

  return <div ref={containerRef} className="h-full w-full overflow-hidden" />;
});
