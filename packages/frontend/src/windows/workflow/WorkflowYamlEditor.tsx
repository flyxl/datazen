import { useEffect, useRef } from 'react';
import { EditorView, keymap, lineNumbers } from '@codemirror/view';
import { EditorState } from '@codemirror/state';
import { defaultKeymap, history, historyKeymap } from '@codemirror/commands';
import { lintGutter } from '@codemirror/lint';
import { langYaml } from './yamlMode';
import { yamlParamLint } from './yamlLint';

interface WorkflowYamlEditorProps {
  value: string;
  onChange: (yaml: string) => void;
  readOnly?: boolean;
}

/**
 * Editor chrome for the workflow YAML editor, driven by the app's `--cm-*` tokens
 * (`:root` = light, `.dark` = dark). Because they are CSS variables, the editor
 * re-colors instantly whenever the app/OS theme changes — no JS reconfigure needed.
 */
const yamlEditorTheme = EditorView.theme(
  {
    '&': {
      height: '100%',
      fontSize: '12px',
      color: 'var(--cm-foreground)',
      backgroundColor: 'var(--cm-background)',
    },
    '.cm-scroller': {
      fontFamily: 'ui-monospace, SFMono-Regular, Menlo, monospace',
      overflow: 'auto',
      color: 'var(--cm-foreground)',
    },
    '.cm-content': { caretColor: 'var(--cm-cursor)' },
    '.cm-cursor, .cm-dropCursor': { borderLeftColor: 'var(--cm-cursor)' },
    '.cm-selectionBackground, &.cm-focused .cm-selectionBackground': {
      backgroundColor: 'var(--cm-selection)',
    },
    '.cm-gutters': {
      backgroundColor: 'var(--cm-background)',
      color: 'var(--cm-comment)',
      border: 'none',
      borderRight: '1px solid var(--cm-punctuation)',
    },
    '.cm-activeLine': { backgroundColor: 'transparent' },
    '.cm-activeLineGutter': { backgroundColor: 'transparent' },
  },
  { dark: false },
);

/** Plain CodeMirror editor for workflow YAML dual-mode editing. */
export function WorkflowYamlEditor({ value, onChange, readOnly = false }: WorkflowYamlEditorProps) {
  const hostRef = useRef<HTMLDivElement>(null);
  const viewRef = useRef<EditorView | null>(null);
  const onChangeRef = useRef(onChange);
  onChangeRef.current = onChange;

  useEffect(() => {
    if (!hostRef.current) return;
    const updateListener = EditorView.updateListener.of((update) => {
      if (update.docChanged) {
        onChangeRef.current(update.state.doc.toString());
      }
    });
    const state = EditorState.create({
      doc: value,
      extensions: [
        lineNumbers(),
        lintGutter(),
        history(),
        langYaml(),
        yamlParamLint(),
        keymap.of([...defaultKeymap, ...historyKeymap]),
        yamlEditorTheme,
        EditorView.lineWrapping,
        updateListener,
        EditorState.readOnly.of(readOnly),
      ],
    });
    const view = new EditorView({ state, parent: hostRef.current });
    viewRef.current = view;
    return () => {
      view.destroy();
      viewRef.current = null;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps -- mount once; sync value separately
  }, [readOnly]);

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
      ref={hostRef}
      data-testid="workflow-yaml-editor"
      className="h-full min-h-0 w-full flex-1 overflow-hidden rounded-md border border-edge"
    />
  );
}
