import { useEffect, useRef } from 'react';
import { EditorView, lineNumbers } from '@codemirror/view';
import { EditorState, Transaction } from '@codemirror/state';
import { sql, PostgreSQL, MySQL, MariaSQL, SQLite, StandardSQL } from '@codemirror/lang-sql';
import type { SQLDialect } from '@codemirror/lang-sql';
import {
  editorSyntaxHighlighting,
  readEditorColorsFromElement,
  type EditorColorContract,
} from '../lib/themeEditorColors';

function makeTheme(dark: boolean, colors: EditorColorContract) {
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
      '.cm-gutters': {
        backgroundColor: dark ? '#1e293b' : '#f8fafc',
        color: dark ? '#64748b' : '#94a3b8',
        border: 'none',
        borderRight: `1px solid ${dark ? '#334155' : '#e2e8f0'}`,
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

function codeBlockExtensions(
  dark: boolean,
  sqlDialect: SQLDialect,
  readOnly: boolean,
  onDocChange?: (code: string) => void,
) {
  const colors = readEditorColorsFromElement();
  const extensions = [
    lineNumbers(),
    sql({ dialect: sqlDialect }),
    editorSyntaxHighlighting(colors, dark),
    makeTheme(dark, colors),
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
  const onChangeRef = useRef(onChange);
  onChangeRef.current = onChange;
  const readOnly = !onChange;

  useEffect(() => {
    if (!containerRef.current) return;
    const dark = document.documentElement.classList.contains('dark');
    const sqlDialect = resolveDialect(dialect);

    const state = EditorState.create({
      doc: code,
      extensions: codeBlockExtensions(dark, sqlDialect, readOnly, (next) =>
        onChangeRef.current?.(next),
      ),
    });
    const view = new EditorView({ state, parent: containerRef.current });
    viewRef.current = view;

    const observer = new MutationObserver(() => {
      const nowDark = document.documentElement.classList.contains('dark');
      const doc = view.state.doc.toString();
      const newState = EditorState.create({
        doc,
        extensions: codeBlockExtensions(nowDark, sqlDialect, readOnly, (next) =>
          onChangeRef.current?.(next),
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
