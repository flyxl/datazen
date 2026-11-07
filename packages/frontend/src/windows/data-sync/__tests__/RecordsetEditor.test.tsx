import { cleanup, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { RecordsetEditor } from '../RecordsetEditor';

afterEach(cleanup);

vi.mock('../../../hooks/useI18n', () => ({
  useI18n: () => ({ t: (key: string) => key }),
}));

describe('RecordsetEditor', () => {
  it('creates a primary-key range and keeps text bounds lossless', () => {
    const onChange = vi.fn();
    render(<RecordsetEditor primaryKeys={['id']} onChange={onChange} />);

    fireEvent.click(screen.getByTestId('data-sync-recordset-toggle'));
    expect(onChange).toHaveBeenLastCalledWith({ orderBy: 'id' });

    render(
      <RecordsetEditor
        primaryKeys={['id']}
        recordset={{ orderBy: 'id' }}
        onChange={onChange}
      />,
    );
    fireEvent.change(screen.getByTestId('data-sync-recordset-start'), {
      target: { value: '9223372036854775807' },
    });
    expect(onChange).toHaveBeenLastCalledWith({
      orderBy: 'id',
      start: { value: '9223372036854775807', inclusive: true },
    });
  });

  it('exposes only primary-key order choices and clears empty bounds', () => {
    const onChange = vi.fn();
    render(
      <RecordsetEditor
        primaryKeys={['tenant', 'id']}
        recordset={{ orderBy: 'tenant', start: { value: '10' } }}
        onChange={onChange}
      />,
    );
    fireEvent.click(screen.getByTestId('data-sync-recordset-order'));
    expect(screen.getAllByRole('option').map((option) => option.textContent)).toEqual([
      'tenant✓',
      'id',
    ]);
    fireEvent.change(screen.getByTestId('data-sync-recordset-start'), {
      target: { value: '' },
    });
    expect(onChange).toHaveBeenLastCalledWith({ orderBy: 'tenant' });
  });

  it('rejects zero, decimal, and unsafe limits before IPC', () => {
    const onChange = vi.fn();
    render(
      <RecordsetEditor primaryKeys={['id']} recordset={{ orderBy: 'id' }} onChange={onChange} />,
    );
    const input = screen.getByTestId('data-sync-recordset-limit');
    for (const value of ['0', '1.5', '9007199254740992']) {
      fireEvent.change(input, { target: { value } });
      expect(screen.getByTestId('data-sync-recordset-limit-error')).toBeTruthy();
    }
    expect(onChange).not.toHaveBeenCalled();
    fireEvent.change(input, { target: { value: '10' } });
    expect(onChange).toHaveBeenLastCalledWith({ orderBy: 'id', limit: 10 });
  });

  it('creates complete tuple ranges for composite keys and edits every component', () => {
    const onChange = vi.fn();
    const primaryKeys = ['tenant_id', 'bucket', 'id'];
    render(<RecordsetEditor primaryKeys={primaryKeys} onChange={onChange} />);

    fireEvent.click(screen.getByTestId('data-sync-recordset-toggle'));
    expect(onChange).toHaveBeenLastCalledWith({ tupleRange: { columns: primaryKeys } });

    const editor = render(
      <RecordsetEditor
        primaryKeys={primaryKeys}
        recordset={{ tupleRange: { columns: primaryKeys } }}
        onChange={onChange}
      />,
    );
    fireEvent.change(screen.getByTestId('data-sync-recordset-start-0'), {
      target: { value: '9223372036854775808' },
    });
    expect(onChange).toHaveBeenLastCalledWith({
      tupleRange: {
        columns: primaryKeys,
        start: { values: ['9223372036854775808', '', ''], inclusive: true },
      },
      orderBy: undefined,
      start: undefined,
      end: undefined,
    });
    editor.rerender(
      <RecordsetEditor
        primaryKeys={primaryKeys}
        recordset={{
          tupleRange: {
            columns: primaryKeys,
            start: { values: ['9223372036854775808', '', ''], inclusive: true },
          },
        }}
        onChange={onChange}
      />,
    );
    fireEvent.change(screen.getByTestId('data-sync-recordset-start-1'), {
      target: { value: '7' },
    });
    editor.rerender(
      <RecordsetEditor
        primaryKeys={primaryKeys}
        recordset={{
          tupleRange: {
            columns: primaryKeys,
            start: { values: ['9223372036854775808', '7', ''], inclusive: true },
          },
        }}
        onChange={onChange}
      />,
    );
    fireEvent.change(screen.getByTestId('data-sync-recordset-start-2'), {
      target: { value: '42' },
    });
    expect(onChange).toHaveBeenLastCalledWith({
      tupleRange: {
        columns: primaryKeys,
        start: { values: ['9223372036854775808', '7', '42'], inclusive: true },
      },
      orderBy: undefined,
      start: undefined,
      end: undefined,
    });
  });

  it('clears a tuple endpoint when all component values are cleared', () => {
    const onChange = vi.fn();
    const primaryKeys = ['tenant_id', 'id'];
    const editor = render(
      <RecordsetEditor
        primaryKeys={primaryKeys}
        recordset={{
          tupleRange: {
            columns: primaryKeys,
            start: { values: ['10', '20'], inclusive: false },
          },
        }}
        onChange={onChange}
      />,
    );
    fireEvent.change(screen.getByTestId('data-sync-recordset-start-0'), {
      target: { value: '' },
    });
    expect(onChange).toHaveBeenLastCalledWith({
      tupleRange: {
        columns: primaryKeys,
        start: { values: ['', '20'], inclusive: false },
      },
    });
    editor.rerender(
      <RecordsetEditor
        primaryKeys={primaryKeys}
        recordset={{
          tupleRange: {
            columns: primaryKeys,
            start: { values: ['', '20'], inclusive: false },
          },
        }}
        onChange={onChange}
      />,
    );
    fireEvent.change(screen.getByTestId('data-sync-recordset-start-1'), {
      target: { value: '' },
    });
    expect(onChange).toHaveBeenLastCalledWith({
      tupleRange: { columns: primaryKeys },
    });
  });
});
