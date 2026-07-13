import { useEffect, useRef } from 'react';
import { EditorView, lineNumbers } from '@codemirror/view';
import { EditorState } from '@codemirror/state';
import { sql, PostgreSQL, MySQL } from '@codemirror/lang-sql';
import { syntaxHighlighting, HighlightStyle } from '@codemirror/language';
import { tags } from '@lezer/highlight';

const darkHighlight = HighlightStyle.define([
  { tag: tags.keyword, color: '#c678dd' },
  { tag: tags.operatorKeyword, color: '#c678dd' },
  { tag: tags.typeName, color: '#e5c07b' },
  { tag: tags.string, color: '#98c379' },
  { tag: tags.number, color: '#d19a66' },
  { tag: tags.bool, color: '#d19a66' },
  { tag: tags.null, color: '#d19a66' },
  { tag: tags.comment, color: '#5c6370', fontStyle: 'italic' },
  { tag: tags.punctuation, color: '#abb2bf' },
  { tag: tags.bracket, color: '#abb2bf' },
  { tag: tags.operator, color: '#56b6c2' },
  { tag: tags.propertyName, color: '#61afef' },
  { tag: tags.variableName, color: '#e06c75' },
  { tag: tags.name, color: '#abb2bf' },
]);

const lightHighlight = HighlightStyle.define([
  { tag: tags.keyword, color: '#7c3aed' },
  { tag: tags.operatorKeyword, color: '#7c3aed' },
  { tag: tags.typeName, color: '#b45309' },
  { tag: tags.string, color: '#16a34a' },
  { tag: tags.number, color: '#c2410c' },
  { tag: tags.bool, color: '#c2410c' },
  { tag: tags.null, color: '#c2410c' },
  { tag: tags.comment, color: '#94a3b8', fontStyle: 'italic' },
  { tag: tags.punctuation, color: '#475569' },
  { tag: tags.bracket, color: '#475569' },
  { tag: tags.operator, color: '#0891b2' },
  { tag: tags.propertyName, color: '#2563eb' },
  { tag: tags.variableName, color: '#dc2626' },
  { tag: tags.name, color: '#0f172a' },
]);

function makeTheme(dark: boolean) {
  return EditorView.theme(
    {
      '&': {
        height: '100%',
        fontSize: '13px',
        backgroundColor: 'transparent',
        color: dark ? '#f1f5f9' : '#0f172a',
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
        backgroundColor: dark ? 'rgba(59,130,246,0.25)' : 'rgba(59,130,246,0.15)',
      },
    },
    { dark },
  );
}

interface SqlCodeBlockProps {
  code: string;
  dialect?: 'postgresql' | 'mysql' | 'sqlite' | 'trino';
}

export function SqlCodeBlock({ code, dialect = 'postgresql' }: SqlCodeBlockProps) {
  const containerRef = useRef<HTMLDivElement>(null);
  const viewRef = useRef<EditorView | null>(null);

  useEffect(() => {
    if (!containerRef.current) return;
    const dark = document.documentElement.classList.contains('dark');
    const sqlDialect = dialect === 'mysql' ? MySQL : PostgreSQL;

    const state = EditorState.create({
      doc: code,
      extensions: [
        EditorState.readOnly.of(true),
        EditorView.editable.of(false),
        lineNumbers(),
        sql({ dialect: sqlDialect }),
        syntaxHighlighting(dark ? darkHighlight : lightHighlight),
        makeTheme(dark),
      ],
    });
    const view = new EditorView({ state, parent: containerRef.current });
    viewRef.current = view;

    const observer = new MutationObserver(() => {
      const nowDark = document.documentElement.classList.contains('dark');
      const newState = EditorState.create({
        doc: view.state.doc.toString(),
        extensions: [
          EditorState.readOnly.of(true),
          EditorView.editable.of(false),
          lineNumbers(),
          sql({ dialect: sqlDialect }),
          syntaxHighlighting(nowDark ? darkHighlight : lightHighlight),
          makeTheme(nowDark),
        ],
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
