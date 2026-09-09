import { useEffect, useRef } from 'react';
import { EditorView, lineNumbers } from '@codemirror/view';
import { Compartment, EditorState, Transaction } from '@codemirror/state';
import { sql, PostgreSQL, MySQL, MariaSQL, SQLite, StandardSQL } from '@codemirror/lang-sql';
import type { SQLDialect } from '@codemirror/lang-sql';
import {
  editorSyntaxHighlighting,
  readEditorColorsFromElement,
  applySqlSyntaxPreset,
  sqlPropertyNameHighlighting,
  type EditorColorContract,
} from '../lib/themeEditorColors';
import { useSettingsStore } from '../stores/settingsStore';

function makeTheme(dark: boolean, colors: EditorColorContract & { propertyName?: string }) {
  const propertyNameColor = colors.propertyName ?? (dark ? '#61afef' : '#2563eb');
  return EditorView.theme(
    {
      '&': {
        height: '100%',
        fontSize: '13px',
        backgroundColor: 'transparent',
        color: colors.foreground,
      },
      '.cm-content': {
        fontFamily: 'ui-monospace, SFMono-Regular, Menlo, monospace',
        padding: '12px 0',
      },
      '.cm-sql-property-name, .cm-sql-property-name *': {
        color: `${propertyNameColor} !important`,
      },
      '.cm-gutters': {
        backgroundColor: dark ? '#111827' : '#f1f5f9',
        color: dark ? '#9ca3af' : '#64748b',
        border: 'none',
        borderRight: `1px solid ${dark ? '#374151' : '#cbd5e1'}`,
      },
      '.cm-activeLine': { backgroundColor: 'transparent' },
      '.cm-activeLineGutter': { backgroundColor: 'transparent' },
      '&.cm-focused': { outline: 'none' },
      '.cm-selectionBackground, &.cm-focused .cm-selectionBackground': {
        backgroundColor: colors.selection,
      },
    },
    { dark },
  );
}

function resolveDialect(dialect: string): SQLDialect {
  const dialectMap: Record<string, SQLDialect> = {
    postgresql: PostgreSQL,
    mysql: MySQL,
    mariadb: MariaSQL,
    sqlite: SQLite,
  };
  return dialectMap[dialect] ?? StandardSQL;
}

function codeBlockThemeExtensions(dark: boolean, sqlSyntaxTheme?: string) {
  const baseColors = readEditorColorsFromElement();
  const colors = applySqlSyntaxPreset(baseColors, sqlSyntaxTheme, dark);
  return [
    editorSyntaxHighlighting(colors, dark),
    makeTheme(dark, colors),
    sqlPropertyNameHighlighting(),
  ];
}

function codeBlockExtensions(
  dark: boolean,
  sqlDialect: SQLDialect,
  readOnly: boolean,
  sqlSyntaxTheme?: string,
  themeCompartment?: Compartment,
  onDocChange?: (code: string) => void,
) {
  const themeExtensions = codeBlockThemeExtensions(dark, sqlSyntaxTheme);
  const extensions = [
    lineNumbers(),
    sql({ dialect: sqlDialect }),
    themeCompartment ? themeCompartment.of(themeExtensions) : themeExtensions,
  ];
  if (readOnly) {
    return [EditorState.readOnly.of(true), EditorView.editable.of(false), ...extensions];
  }
  if (onDocChange) {
    extensions.push(
      EditorView.updateListener.of((update) => {
        if (
          update.docChanged &&
          !update.transactions.some((tr) => tr.annotation(Transaction.remote))
        ) {
          onDocChange(update.state.doc.toString());
        }
      }),
    );
  }
  return extensions;
}

interface SqlCodeBlockProps {
  code: string;
  dialect?: string;
  /** When set, the block is editable and streams changes through this callback. */
  onChange?: (code: string) => void;
}

export function SqlCodeBlock({ code, dialect = 'postgresql', onChange }: SqlCodeBlockProps) {
  const containerRef = useRef<HTMLDivElement>(null);
  const viewRef = useRef<EditorView | null>(null);
  const themeCompartment = useRef(new Compartment());
  const onChangeRef = useRef(onChange);
  onChangeRef.current = onChange;
  const readOnly = !onChange;
  const sqlSyntaxTheme = useSettingsStore((s) => s.settings.sqlSyntaxTheme);

  useEffect(() => {
    if (!containerRef.current) return;
    const dark = document.documentElement.classList.contains('dark');
    const sqlDialect = resolveDialect(dialect);

    const state = EditorState.create({
      doc: code,
      extensions: codeBlockExtensions(
        dark,
        sqlDialect,
        readOnly,
        sqlSyntaxTheme,
        themeCompartment.current,
        (next) => onChangeRef.current?.(next),
      ),
    });
    const view = new EditorView({ state, parent: containerRef.current });
    viewRef.current = view;

    const observer = new MutationObserver(() => {
      const nowDark = document.documentElement.classList.contains('dark');
      const doc = view.state.doc.toString();
      const currentTheme = useSettingsStore.getState().settings.sqlSyntaxTheme;
      const newState = EditorState.create({
        doc,
        extensions: codeBlockExtensions(
          nowDark,
          sqlDialect,
          readOnly,
          currentTheme,
          themeCompartment.current,
          (next) => onChangeRef.current?.(next),
        ),
      });
      view.setState(newState);
    });
    observer.observe(document.documentElement, { attributes: true, attributeFilter: ['class'] });

    return () => {
      observer.disconnect();
      view.destroy();
      viewRef.current = null;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps -- mount once; code synced below
  }, [dialect, readOnly]);

  useEffect(() => {
    const view = viewRef.current;
    if (!view) return;
    const dark = document.documentElement.classList.contains('dark');
    view.dispatch({
      effects: themeCompartment.current.reconfigure(codeBlockThemeExtensions(dark, sqlSyntaxTheme)),
      annotations: Transaction.addToHistory.of(false),
    });
  }, [sqlSyntaxTheme]);

  useEffect(() => {
    const view = viewRef.current;
    if (!view) return;
    const current = view.state.doc.toString();
    if (current !== code) {
      view.dispatch({
        changes: { from: 0, to: current.length, insert: code },
        annotations: Transaction.remote.of(true),
      });
    }
  }, [code]);

  return <div ref={containerRef} className="h-full w-full overflow-hidden select-text" />;
}
