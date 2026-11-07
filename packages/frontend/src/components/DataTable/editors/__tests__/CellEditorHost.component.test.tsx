/**
 * The component tests of the editor host (DB-05).
 *
 * They cover the two halves of the editor: the **controls** (what a typed
 * value means) and the **state machine** (when the value is written).
 *
 * The assertions use the contract's data attributes and the historical
 * `table-edit-input` locator — never the visible wording, which belongs to
 * the translation (12-testing §1.1 D7).
 */
import { afterEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render } from '@testing-library/react';
import { CellEditorHost, type CellEditorProps } from '../CellEditorHost';
import { resolveCellEditorKind } from '../../../../lib/cellEditorKind';
import { resolveCellType } from '../../../../lib/cellTypes';

vi.mock('../../../../hooks/useI18n', () => ({
  useI18n: () => ({ t: (key: string) => key }),
}));

afterEach(cleanup);

function renderEditor(overrides: Partial<CellEditorProps> = {}) {
  const props: CellEditorProps = {
    columnName: 'name',
    dataType: 'varchar',
    cellType: 'text',
    kind: 'text',
    originalValue: 'hello',
    nullable: true,
    onCommit: vi.fn(),
    onCancel: vi.fn(),
    ...overrides,
  };
  const view = render(<CellEditorHost {...props} />);
  return { ...view, props };
}

function inputOf(container: HTMLElement): HTMLInputElement {
  const found = container.querySelector('input');
  if (!(found instanceof HTMLInputElement)) throw new Error('no input rendered');
  return found;
}

function textareaOf(container: HTMLElement): HTMLTextAreaElement {
  const found = container.querySelector('textarea');
  if (!(found instanceof HTMLTextAreaElement)) throw new Error('no textarea rendered');
  return found;
}

describe('CellEditorHost', () => {
  it('renders a compact grid-cell editor instead of the generic form input', () => {
    const { container } = renderEditor();
    const input = inputOf(container);
    const classes: string = input.getAttribute('class') ?? '';
    expect(classes).toContain('h-7');
    expect(classes).toContain('font-mono');
    // The generic <Input> widget carries px-3 + rounded-md + focus:ring-2; a
    // grid cell editor should stay minimal and edge-aligned.
    expect(classes).not.toContain('px-3');
    expect(classes).not.toContain('focus:ring-2');
  });

  it('commits the typed value on Enter', () => {
    const onCommit = vi.fn();
    const onCancel = vi.fn();
    const { container } = renderEditor({ onCommit, onCancel });
    const input = inputOf(container);
    fireEvent.change(input, { target: { value: 'world' } });
    fireEvent.keyDown(input, { key: 'Enter' });
    expect(onCommit).toHaveBeenCalledWith('world');
    expect(onCancel).not.toHaveBeenCalled();
  });

  it('cancels on Escape', () => {
    const onCommit = vi.fn();
    const onCancel = vi.fn();
    const { container } = renderEditor({ onCommit, onCancel });
    const input = inputOf(container);
    fireEvent.keyDown(input, { key: 'Escape' });
    expect(onCancel).toHaveBeenCalledTimes(1);
    expect(onCommit).not.toHaveBeenCalled();
  });

  it('cancels on blur when the value is unchanged', () => {
    const onCommit = vi.fn();
    const onCancel = vi.fn();
    const { container } = renderEditor({ onCommit, onCancel });
    fireEvent.blur(inputOf(container));
    expect(onCancel).toHaveBeenCalledTimes(1);
    expect(onCommit).not.toHaveBeenCalled();
  });

  it('commits once on blur', () => {
    const onCommit = vi.fn();
    const onCancel = vi.fn();
    const { container } = renderEditor({ onCommit, onCancel });
    const input = inputOf(container);
    fireEvent.change(input, { target: { value: 'world' } });
    fireEvent.blur(input);
    expect(onCommit).toHaveBeenCalledTimes(1);
    expect(onCommit).toHaveBeenCalledWith('world');
    expect(onCancel).not.toHaveBeenCalled();
  });

  it('coerces an integer column', () => {
    const onCommit = vi.fn();
    const { container } = renderEditor({
      originalValue: 1,
      dataType: 'integer',
      cellType: 'number',
      kind: 'number',
      onCommit,
    });
    const input = inputOf(container);
    fireEvent.change(input, { target: { value: '99' } });
    fireEvent.keyDown(input, { key: 'Enter' });
    expect(onCommit).toHaveBeenCalledWith(99);
  });

  it('coerces a numeric column', () => {
    const onCommit = vi.fn();
    const { container } = renderEditor({
      originalValue: 1.5,
      dataType: 'numeric',
      cellType: 'number',
      kind: 'number',
      onCommit,
    });
    const input = inputOf(container);
    fireEvent.change(input, { target: { value: '3.14' } });
    fireEvent.keyDown(input, { key: 'Enter' });
    expect(onCommit).toHaveBeenCalledWith(3.14);
  });

  it('coerces a json column and refuses a text that does not parse', () => {
    const onCommit = vi.fn();
    const onInvalid = vi.fn();
    const { container } = renderEditor({
      originalValue: { a: 1 },
      dataType: 'jsonb',
      cellType: 'json',
      kind: 'json',
      onCommit,
      onInvalid,
    });
    const area = textareaOf(container);
    fireEvent.change(area, { target: { value: '{"x":1}' } });
    fireEvent.keyDown(area, { key: 'Enter' });
    expect(onCommit).toHaveBeenCalledWith({ x: 1 });
  });

  it('keeps the editor open and reports the reason for an invalid value', () => {
    const onCommit = vi.fn();
    const onInvalid = vi.fn();
    const { container } = renderEditor({
      originalValue: { a: 1 },
      dataType: 'jsonb',
      cellType: 'json',
      kind: 'json',
      onCommit,
      onInvalid,
    });
    const area = textareaOf(container);
    fireEvent.change(area, { target: { value: '{not json' } });
    fireEvent.keyDown(area, { key: 'Enter' });
    expect(onCommit).not.toHaveBeenCalled();
    expect(onInvalid).toHaveBeenCalledWith('invalidJson');
    // The editor stays open and says why.
    expect(container.querySelector('[data-dt-editor-error="invalidJson"]')).not.toBeNull();
  });

  it('commits null for an empty input when the value was not null', () => {
    const onCommit = vi.fn();
    const { container } = renderEditor({ originalValue: 'text', onCommit });
    const input = inputOf(container);
    fireEvent.change(input, { target: { value: '' } });
    fireEvent.blur(input);
    expect(onCommit).toHaveBeenCalledWith(null);
  });

  it('cancels when the input is empty and the original was null', () => {
    const onCancel = vi.fn();
    const onCommit = vi.fn();
    const { container } = renderEditor({ originalValue: null, onCancel, onCommit });
    fireEvent.blur(inputOf(container));
    expect(onCancel).toHaveBeenCalled();
    expect(onCommit).not.toHaveBeenCalled();
  });

  it('refuses an empty input on a NOT NULL column', () => {
    const onCommit = vi.fn();
    const onInvalid = vi.fn();
    const { container } = renderEditor({ originalValue: 'text', nullable: false, onCommit, onInvalid });
    const input = inputOf(container);
    fireEvent.change(input, { target: { value: '' } });
    fireEvent.keyDown(input, { key: 'Enter' });
    expect(onCommit).not.toHaveBeenCalled();
    expect(onInvalid).toHaveBeenCalledWith('emptyNotNull');
  });
});

describe('CellEditorHost and the IME', () => {
  it('does not commit while an IME composition is active', () => {
    const onCommit = vi.fn();
    const onCancel = vi.fn();
    const { container } = renderEditor({ onCommit, onCancel });
    const input = inputOf(container);
    fireEvent.compositionStart(input);
    fireEvent.change(input, { target: { value: '日本語' } });
    // The keydown still carries the composition flag.
    fireEvent.keyDown(input, { key: 'Enter', isComposing: true });
    expect(onCommit).not.toHaveBeenCalled();

    // The composition ends: now Enter commits — exactly once.
    fireEvent.compositionEnd(input);
    fireEvent.keyDown(input, { key: 'Enter' });
    expect(onCommit).toHaveBeenCalledTimes(1);
    expect(onCommit).toHaveBeenCalledWith('日本語');
  });

  it('does not cancel the editor when Escape ends a composition', () => {
    const onCommit = vi.fn();
    const onCancel = vi.fn();
    const { container } = renderEditor({ onCommit, onCancel });
    const input = inputOf(container);
    fireEvent.compositionStart(input);
    fireEvent.change(input, { target: { value: 'にほんご' } });
    fireEvent.keyDown(input, { key: 'Escape', isComposing: true });
    expect(onCancel).not.toHaveBeenCalled();
    expect(onCommit).not.toHaveBeenCalled();

    // After the composition ends, Escape cancels.
    fireEvent.compositionEnd(input);
    fireEvent.keyDown(input, { key: 'Escape' });
    expect(onCancel).toHaveBeenCalledTimes(1);
  });

  it('does not commit when the composition is still running and the focus leaves', () => {
    const onCommit = vi.fn();
    const { container } = renderEditor({ onCommit });
    const input = inputOf(container);
    fireEvent.compositionStart(input);
    fireEvent.change(input, { target: { value: '日本語' } });
    fireEvent.blur(input);
    expect(onCommit).not.toHaveBeenCalled();
  });
});

describe('CellEditorHost and the surfaces it owns', () => {
  it('does not commit when focus moves into an editor-owned portal', () => {
    const onCommit = vi.fn();
    const { container } = renderEditor({ onCommit });
    const input = inputOf(container);
    fireEvent.change(input, { target: { value: 'world' } });

    // The calendar pop-up of the temporal picker lives in a portal on
    // `document.body` — it is not inside the editor's own element.
    const portal = document.createElement('div');
    portal.setAttribute('data-testid', 'cell-editor-temporal-popover');
    document.body.appendChild(portal);
    const inside = document.createElement('button');
    portal.appendChild(inside);

    fireEvent.blur(input, { relatedTarget: inside });
    expect(onCommit).not.toHaveBeenCalled();

    document.body.removeChild(portal);
  });

  it('does not commit when the context menu steals the focus', () => {
    const onCommit = vi.fn();
    const { container } = renderEditor({ onCommit });
    const input = inputOf(container);
    fireEvent.change(input, { target: { value: 'world' } });

    const menu = document.createElement('div');
    menu.setAttribute('role', 'menu');
    document.body.appendChild(menu);
    const item = document.createElement('button');
    menu.appendChild(item);

    fireEvent.blur(input, { relatedTarget: item });
    expect(onCommit).not.toHaveBeenCalled();

    document.body.removeChild(menu);
  });

  it('does not commit when a right-click opens the context menu', () => {
    const onCommit = vi.fn();
    const { container } = renderEditor({ onCommit });
    const input = inputOf(container);
    fireEvent.change(input, { target: { value: 'world' } });

    // The right-click is detected on `mousedown`, because the menu itself
    // appears after the focus change it causes.
    fireEvent.mouseDown(input, { button: 2 });
    fireEvent.blur(input);
    expect(onCommit).not.toHaveBeenCalled();
  });
});

describe('the editor kinds', () => {
  it('routes a zoned datetime to the text editor', () => {
    const kind = resolveCellEditorKind({
      cellType: 'datetime',
      zoned: true,
      hasCandidates: false,
      valueCodePoints: 0,
    });
    expect(kind).toBe('text');
  });

  it('derives the kind of a column from its normalised type', () => {
    expect(resolveCellType('jsonb')).toBe('json');
    expect(resolveCellType('boolean')).toBe('bool');
    expect(resolveCellType('bytea')).toBe('bytes');
  });
});
