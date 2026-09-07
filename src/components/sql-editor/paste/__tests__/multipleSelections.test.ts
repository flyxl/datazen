import { describe, it, expect } from 'vitest';
import { EditorState } from '@codemirror/state';
import { EditorView } from '@codemirror/view';
import { selectNextOccurrence } from '@codemirror/search';
import { createMultipleSelectionsExtension } from '../multipleSelections';

describe('multipleSelections', () => {
  it('selects word on first Mod-D and adds next occurrences on consecutive Mod-D', () => {
    const parent = document.createElement('div');
    document.body.appendChild(parent);

    const doc = 'SELECT name, age, name FROM users WHERE name = 1';
    const state = EditorState.create({
      doc,
      // Place cursor in the middle of the first 'name' (position 9)
      selection: { anchor: 9, head: 9 },
      extensions: createMultipleSelectionsExtension(),
    });

    const view = new EditorView({ state, parent });

    try {
      expect(view.state.selection.ranges).toHaveLength(1);
      expect(view.state.selection.main.empty).toBe(true);

      // First Mod-D: expands empty cursor to the full word 'name'
      const handled1 = selectNextOccurrence(view);
      expect(handled1).toBe(true);
      expect(view.state.selection.ranges).toHaveLength(1);
      expect(
        view.state.sliceDoc(view.state.selection.main.from, view.state.selection.main.to),
      ).toBe('name');
      expect(view.state.selection.main.from).toBe(7);
      expect(view.state.selection.main.to).toBe(11);

      // Second Mod-D: selects second 'name' (ranges has length 2)
      const handled2 = selectNextOccurrence(view);
      expect(handled2).toBe(true);
      expect(view.state.selection.ranges).toHaveLength(2);
      expect(
        view.state.sliceDoc(
          view.state.selection.ranges[0]!.from,
          view.state.selection.ranges[0]!.to,
        ),
      ).toBe('name');
      expect(
        view.state.sliceDoc(
          view.state.selection.ranges[1]!.from,
          view.state.selection.ranges[1]!.to,
        ),
      ).toBe('name');
      expect(view.state.selection.ranges[1]!.from).toBe(18);
      expect(view.state.selection.ranges[1]!.to).toBe(22);

      // Third Mod-D: selects third 'name' (ranges has length 3)
      const handled3 = selectNextOccurrence(view);
      expect(handled3).toBe(true);
      expect(view.state.selection.ranges).toHaveLength(3);
      expect(view.state.selection.ranges[2]!.from).toBe(40);
      expect(view.state.selection.ranges[2]!.to).toBe(44);
    } finally {
      view.destroy();
      parent.remove();
    }
  });

  it('triggers via keydown event on contentDOM with full SqlEditor extensions', () => {
    const parent = document.createElement('div');
    document.body.appendChild(parent);

    const doc = 'SELECT name, age, name FROM users WHERE name = 1';
    const state = EditorState.create({
      doc,
      selection: { anchor: 9, head: 9 },
      extensions: createMultipleSelectionsExtension(),
    });

    const view = new EditorView({ state, parent });

    try {
      // Dispatch Cmd+D (Meta+d on macOS)
      const event1 = new KeyboardEvent('keydown', {
        key: 'd',
        code: 'KeyD',
        metaKey: true,
        bubbles: true,
        cancelable: true,
      });
      view.contentDOM.dispatchEvent(event1);

      console.log(
        'After keydown 1 ranges:',
        view.state.selection.ranges.map((r) => [r.from, r.to]),
      );
      expect(view.state.selection.ranges).toHaveLength(1);
      expect(view.state.selection.main.from).toBe(7);
      expect(view.state.selection.main.to).toBe(11);

      // Dispatch Cmd+D second time
      const event2 = new KeyboardEvent('keydown', {
        key: 'd',
        code: 'KeyD',
        metaKey: true,
        bubbles: true,
        cancelable: true,
      });
      view.contentDOM.dispatchEvent(event2);

      console.log(
        'After keydown 2 ranges:',
        view.state.selection.ranges.map((r) => [r.from, r.to]),
      );
      expect(view.state.selection.ranges).toHaveLength(2);
    } finally {
      view.destroy();
      parent.remove();
    }
  });
});
