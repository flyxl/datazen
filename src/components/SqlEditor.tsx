import { useEffect, useRef } from 'react';
import { EditorView, keymap, placeholder as cmPlaceholder, lineNumbers } from '@codemirror/view';
import { EditorState, Compartment } from '@codemirror/state';
import { defaultKeymap, history, historyKeymap } from '@codemirror/commands';
import { sql, PostgreSQL } from '@codemirror/lang-sql';
import { syntaxHighlighting, HighlightStyle } from '@codemirror/language';
import { tags } from '@lezer/highlight';
import { autocompletion, closeBrackets, acceptCompletion } from '@codemirror/autocomplete';
import { searchKeymap } from '@codemirror/search';
import { useSettingsStore } from '../stores/settingsStore';

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
  { tag: tags.function(tags.variableName), color: '#61afef' },
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
  { tag: tags.function(tags.variableName), color: '#2563eb' },
  { tag: tags.variableName, color: '#dc2626' },
  { tag: tags.name, color: '#0f172a' },
]);

interface ThemeConfig {
  dark: boolean;
  fontSize: number;
  fontFamily: string;
}

function makeEditorTheme({ dark, fontSize, fontFamily }: ThemeConfig) {
  return EditorView.theme(
    {
      '&': {
        height: '100%',
        fontSize: `${fontSize}px`,
        backgroundColor: dark ? '#0f172a' : '#ffffff',
        color: dark ? '#f1f5f9' : '#0f172a',
      },
      '.cm-content': {
        fontFamily: `${fontFamily}, ui-monospace, SFMono-Regular, Menlo, monospace`,
        padding: '12px 0',
        caretColor: dark ? '#f1f5f9' : '#0f172a',
      },
      '.cm-cursor': {
        borderLeftColor: dark ? '#f1f5f9' : '#0f172a',
      },
      '.cm-activeLine': {
        backgroundColor: dark ? 'rgba(30,41,59,0.5)' : 'rgba(241,245,249,0.5)',
      },
      '.cm-selectionBackground, &.cm-focused .cm-selectionBackground': {
        backgroundColor: dark ? 'rgba(59,130,246,0.25)' : 'rgba(59,130,246,0.15)',
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
  return [
    makeEditorTheme(config),
    syntaxHighlighting(config.dark ? darkHighlight : lightHighlight),
  ];
}

/** Table name → column names mapping for autocompletion */
export type SqlSchema = Record<string, string[]>;

interface SqlEditorProps {
  value: string;
  onChange: (value: string) => void;
  onExecute?: () => void;
  placeholder?: string;
  schema?: SqlSchema;
}

export function SqlEditor({ value, onChange, onExecute, placeholder, schema }: SqlEditorProps) {
  const containerRef = useRef<HTMLDivElement>(null);
  const viewRef = useRef<EditorView | null>(null);
  const themeCompartment = useRef(new Compartment());
  const sqlCompartment = useRef(new Compartment());
  const onChangeRef = useRef(onChange);
  const onExecuteRef = useRef(onExecute);

  const editorFontSize = useSettingsStore((s) => s.settings.editorFontSize);
  const editorFontFamily = useSettingsStore((s) => s.settings.editorFontFamily);

  onChangeRef.current = onChange;
  onExecuteRef.current = onExecute;

  function currentThemeConfig(): ThemeConfig {
    const { editorFontSize: fs, editorFontFamily: ff } = useSettingsStore.getState().settings;
    return {
      dark: document.documentElement.classList.contains('dark'),
      fontSize: fs,
      fontFamily: ff,
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
            run: () => { onExecuteRef.current?.(); return true; },
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
            dialect: PostgreSQL,
            upperCaseKeywords: true,
            schema: schema ?? {},
          }),
        ),
        themeCompartment.current.of(themeExtensions(config)),
        EditorView.lineWrapping,
        EditorView.updateListener.of((update) => {
          if (update.docChanged) {
            onChangeRef.current(update.state.doc.toString());
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
          dialect: PostgreSQL,
          upperCaseKeywords: true,
          schema: schema ?? {},
        }),
      ),
    });
  }, [schema]);

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
}
