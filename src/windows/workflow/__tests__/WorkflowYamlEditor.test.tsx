import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import { WorkflowYamlEditor } from '../WorkflowYamlEditor';

vi.mock('@codemirror/view', () => {
  class EditorView {
    state = { doc: { toString: () => 'id: a\nname: A\nsteps: []\n' } };
    dispatch = vi.fn();
    destroy = vi.fn();
    static updateListener = { of: () => ({}) };
    static lineWrapping = {};
    static theme = () => ({});
    constructor(opts: { parent: HTMLElement }) {
      opts.parent.setAttribute('data-cm', '1');
    }
  }
  return {
    EditorView,
    keymap: { of: () => ({}) },
    lineNumbers: () => ({}),
  };
});

vi.mock('@codemirror/state', () => ({
  EditorState: {
    create: () => ({}),
    readOnly: { of: () => ({}) },
  },
}));

vi.mock('@codemirror/commands', () => ({
  defaultKeymap: [],
  history: () => ({}),
  historyKeymap: [],
}));

vi.mock('@codemirror/theme-one-dark', () => ({ oneDark: {} }));

describe('WorkflowYamlEditor', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('renders yaml editor host', () => {
    render(<WorkflowYamlEditor value="id: x\n" onChange={() => {}} />);
    expect(screen.getByTestId('workflow-yaml-editor')).toBeInTheDocument();
  });
});
