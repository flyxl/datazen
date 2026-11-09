/**
 * The Quick Look dialog itself (DBR-05) — the viewer both the detail drawer
 * and the grid's quick entries (middle-click, `Cmd/Ctrl+Shift+L`) open.
 *
 * The regression this file guards is the one the extraction fixed: an
 * *editable* blob column used to open a textarea of the wire's number array —
 * an editor no user can meaningfully use — instead of the binary viewer. The
 * PRD's rule is the other way around: binary is **viewed and saved**, its
 * write entry is the file loader, not hand-typed text.
 */
import { describe, expect, it, vi, afterEach } from 'vitest';
import { cleanup, fireEvent, render, screen } from '@testing-library/react';
import { QuickLookDialog, isQuickLookableValue } from '../QuickLookDialog';

// The keys themselves are the labels: the dialog's wording is not under test,
// and a raw-key fallback keeps the assertions stable across copy changes.
vi.mock('../../../hooks/useI18n', () => ({
  useI18n: () => ({ t: (key: string) => key }),
}));

afterEach(cleanup);

const LONG_TEXT = 'a'.repeat(200);

describe('QuickLookDialog (DBR-05)', () => {
  it('shows the binary viewer for an editable blob column, not a textarea', () => {
    render(
      <QuickLookDialog
        open
        column={{ id: 'avatar', name: 'avatar', type: 'bytea' }}
        value={[0x89, 0x50, 0x4e, 0x47]}
        editable
        onCommit={vi.fn()}
        onClose={vi.fn()}
      />,
    );

    // The viewer owns the body — before the fix the editable branch won and
    // rendered a textarea of the number array.
    expect(screen.getByTestId('detail-quick-look-binary')).toBeInTheDocument();
    expect(screen.queryByRole('textbox')).toBeNull();
    // And the hand-typed save exit is gone with it: binary is not text.
    expect(screen.queryByRole('button', { name: 'detail.save' })).toBeNull();
    // The viewer's own save-as-file exit is the one that stays.
    expect(screen.getByTestId('detail-quick-look-binary-save')).toBeInTheDocument();
  });

  it('keeps the binary viewer and its save exit on a read-only panel', () => {
    const onCommit = vi.fn();
    render(
      <QuickLookDialog
        open
        column={{ id: 'avatar', name: 'avatar', type: 'blob' }}
        value={[1, 2, 3]}
        editable={false}
        onCommit={onCommit}
        onClose={vi.fn()}
      />,
    );

    expect(screen.getByTestId('detail-quick-look-binary')).toBeInTheDocument();
    expect(screen.getByTestId('detail-quick-look-binary-save')).toBeInTheDocument();
    expect(onCommit).not.toHaveBeenCalled();
  });

  it('edits a long text through the textarea and stages the typed value', () => {
    const onCommit = vi.fn();
    const onClose = vi.fn();
    render(
      <QuickLookDialog
        open
        column={{ id: 'bio', name: 'bio', type: 'text' }}
        value={LONG_TEXT}
        editable
        onCommit={onCommit}
        onClose={onClose}
      />,
    );

    const editor = screen.getByRole('textbox') as HTMLTextAreaElement;
    expect(editor.value).toBe(LONG_TEXT);
    fireEvent.change(editor, { target: { value: 'typed in the quick look' } });
    fireEvent.click(screen.getByRole('button', { name: 'detail.save' }));

    expect(onCommit).toHaveBeenCalledTimes(1);
    expect(onCommit).toHaveBeenCalledWith('typed in the quick look');
    expect(onClose).toHaveBeenCalled();
  });
});

describe('isQuickLookableValue (the entries gate, DBR-05)', () => {
  it('opens for binary columns, structured JSON and long text', () => {
    expect(isQuickLookableValue([1, 2, 3], 'bytea')).toBe(true);
    expect(isQuickLookableValue('{"a":1}', 'json')).toBe(true);
    expect(isQuickLookableValue(LONG_TEXT, 'text')).toBe(true);
  });

  it('stays closed for NULL and short scalar text', () => {
    expect(isQuickLookableValue(null, 'bytea')).toBe(false);
    expect(isQuickLookableValue(undefined, 'text')).toBe(false);
    expect(isQuickLookableValue('short', 'text')).toBe(false);
    expect(isQuickLookableValue(42, 'integer')).toBe(false);
  });
});
