import { useEffect, useRef } from 'react';
import { EditorView, lineNumbers } from '@codemirror/view';
import { EditorState } from '@codemirror/state';
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

function codeBlockExtensions(dark: boolean, sqlDialect: SQLDialect) {
  const colors = readEditorColorsFromElement();
  return [
    EditorState.readOnly.of(true),
    EditorView.editable.of(false),
    lineNumbers(),
    sql({ dialect: sqlDialect }),
    editorSyntaxHighlighting(colors, dark),
    makeTheme(dark, colors),
  ];
}

interface SqlCodeBlockProps {
  code: string;
  dialect?: string;
}

export function SqlCodeBlock({ code, dialect = 'postgresql' }: SqlCodeBlockProps) {
  const containerRef = useRef<HTMLDivElement>(null);
  const viewRef = useRef<EditorView | null>(null);

  useEffect(() => {
    if (!containerRef.current) return;
    const dark = document.documentElement.classList.contains('dark');
    const dialectMap: Record<string, SQLDialect> = {
      postgresql: PostgreSQL,
      mysql: MySQL,
      mariadb: MariaSQL,
      sqlite: SQLite,
    };
    const sqlDialect = dialectMap[dialect] ?? StandardSQL;

    const state = EditorState.create({
      doc: code,
      extensions: codeBlockExtensions(dark, sqlDialect),
    });
    const view = new EditorView({ state, parent: containerRef.current });
    viewRef.current = view;

    const observer = new MutationObserver(() => {
      const nowDark = document.documentElement.classList.contains('dark');
      const newState = EditorState.create({
        doc: view.state.doc.toString(),
        extensions: codeBlockExtensions(nowDark, sqlDialect),
      });
      view.setState(newState);
    });
    observer.observe(document.documentElement, { attributes: true, attributeFilter: ['class'] });

    return () => {
      observer.disconnect();
      view.destroy();
      viewRef.current = null;
    };
  }, [code, dialect]);

  return <div ref={containerRef} className="h-full w-full overflow-hidden" />;
}
