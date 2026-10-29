import { describe, expect, it, vi, afterEach } from 'vitest';
import { render, fireEvent, cleanup, waitFor, screen } from '@testing-library/react';
import { DetailPanel } from '../DetailPanel';
import type { ColumnDef } from '../TableHeader';

vi.mock('../../../hooks/useI18n', () => ({
  useI18n: () => ({ t: (key: string) => key }),
}));

afterEach(cleanup);

const COLS: ColumnDef[] = [
  { id: 'id', name: 'id', type: 'integer' },
  { id: 'name', name: 'name', type: 'varchar' },
  { id: 'meta', name: 'meta', type: 'jsonb' },
  { id: 'active', name: 'active', type: 'boolean' },
];

/** Field editors sit below the column-search input. */
function fieldInputs(container: HTMLElement): HTMLInputElement[] {
  return Array.from(container.querySelectorAll('input')).slice(1) as HTMLInputElement[];
}

describe('DetailPanel', () => {
  it('returns null when closed', () => {
    const { container } = render(
      <DetailPanel open={false} columns={COLS} row={null} rowIndex={null} />,
    );
    expect(container.firstChild).toBeNull();
  });

  it('shows no selection message when row is null', () => {
    const { getByText } = render(<DetailPanel open columns={COLS} row={null} rowIndex={null} />);
    expect(getByText('detail.noSelection')).toBeInTheDocument();
    expect(getByText('detail.title')).toBeInTheDocument();
  });

  it('renders field values in read-only mode', () => {
    const { getByText } = render(
      <DetailPanel
        open
        columns={COLS}
        row={{ id: 1, name: 'Alice', meta: null, active: true }}
        rowIndex={0}
      />,
    );
    expect(getByText('Alice')).toBeInTheDocument();
    expect(getByText('1')).toBeInTheDocument();
    expect(getByText('NULL')).toBeInTheDocument();
    expect(getByText('true')).toBeInTheDocument();
  });

  it('commits edited integer on blur', () => {
    const onFieldEdit = vi.fn();
    const { container } = render(
      <DetailPanel
        open
        editable
        columns={COLS}
        row={{ id: 1, name: 'Alice', meta: null, active: true }}
        rowIndex={2}
        onFieldEdit={onFieldEdit}
      />,
    );
    const idInput = fieldInputs(container)[0];
    fireEvent.focus(idInput);
    fireEvent.change(idInput, { target: { value: '42' } });
    fireEvent.blur(idInput);
    expect(onFieldEdit).toHaveBeenCalledWith(2, 'id', 42);
  });

  it('commits boolean and json edits', () => {
    const onFieldEdit = vi.fn();
    const { container } = render(
      <DetailPanel
        open
        editable
        columns={COLS}
        row={{ id: 1, name: 'Alice', meta: { a: 1 }, active: false }}
        rowIndex={0}
        onFieldEdit={onFieldEdit}
      />,
    );
    const textareas = container.querySelectorAll('textarea');
    fireEvent.focus(textareas[0]);
    fireEvent.change(textareas[0], { target: { value: '{"b":2}' } });
    fireEvent.blur(textareas[0]);
    expect(onFieldEdit).toHaveBeenCalledWith(0, 'meta', { b: 2 });

    // editable fields: id, name, active (meta is textarea)
    const activeInput = fieldInputs(container)[2];
    fireEvent.focus(activeInput);
    fireEvent.change(activeInput, { target: { value: 'true' } });
    fireEvent.blur(activeInput);
    expect(onFieldEdit).toHaveBeenCalledWith(0, 'active', true);
  });

  it('cancels edit on Escape without committing unchanged value', async () => {
    const onFieldEdit = vi.fn();
    const { container } = render(
      <DetailPanel
        open
        editable
        columns={[{ id: 'name', name: 'name', type: 'varchar' }]}
        row={{ name: 'Bob' }}
        rowIndex={0}
        onFieldEdit={onFieldEdit}
      />,
    );
    const input = fieldInputs(container)[0];
    fireEvent.focus(input);
    fireEvent.change(input, { target: { value: 'temp' } });
    fireEvent.keyDown(input, { key: 'Escape' });
    expect(onFieldEdit).not.toHaveBeenCalled();
    await waitFor(() => expect(input).toHaveValue('Bob'));
  });

  it('commits null when text field cleared', () => {
    const onFieldEdit = vi.fn();
    const { container } = render(
      <DetailPanel
        open
        editable
        columns={[{ id: 'name', name: 'name', type: 'varchar' }]}
        row={{ name: 'Bob' }}
        rowIndex={1}
        onFieldEdit={onFieldEdit}
      />,
    );
    const input = fieldInputs(container)[0];
    fireEvent.focus(input);
    fireEvent.change(input, { target: { value: '' } });
    fireEvent.blur(input);
    expect(onFieldEdit).toHaveBeenCalledWith(1, 'name', null);
  });

  // DB-13: the shared viewers behind the quick-look of a structured value.
  describe('quick-look viewers (DB-13)', () => {
    it('opens a JSON document in the shared tree viewer, read-only', () => {
      const { getByTitle } = render(
        <DetailPanel
          open
          columns={[{ id: 'meta', name: 'meta', type: 'jsonb' }]}
          row={{ meta: '{"user":"ada"}' }}
          rowIndex={0}
        />,
      );

      fireEvent.click(getByTitle('detail.quickLook'));

      // The tree mode is the default; the mode bar names the four views.
      expect(screen.getByTestId('detail-quick-look-json-tree')).toHaveTextContent('user');
      expect(screen.getByTestId('detail-quick-look-json-mode-tree')).toBeInTheDocument();
      expect(screen.getByTestId('detail-quick-look-json-mode-minify')).toBeInTheDocument();
    });

    it('opens binary bytes in the shared hex viewer with a save exit', () => {
      const { getByTitle } = render(
        <DetailPanel
          open
          columns={[{ id: 'avatar', name: 'avatar', type: 'bytea' }]}
          row={{ avatar: [0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a] }}
          rowIndex={0}
        />,
      );

      fireEvent.click(getByTitle('detail.quickLook'));

      // A PNG payload: the sniffed kind, the image preview and the hex dump.
      expect(screen.getByTestId('detail-quick-look-binary-kind')).toHaveTextContent('image/png');
      expect(screen.getByTestId('detail-quick-look-binary-image')).toBeInTheDocument();
      expect(screen.getByTestId('detail-quick-look-binary-hex-bytes')).toHaveTextContent(
        '89 50 4e 47',
      );
      expect(screen.getByTestId('detail-quick-look-binary-save')).toBeInTheDocument();
    });
  });
});
