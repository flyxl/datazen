import { describe, expect, it } from 'vitest';
import { render } from '@testing-library/react';
import { CellRenderer } from '../CellRenderer';

const NOOP = () => {};

describe('CellRenderer branches', () => {
  it('renders EditableCell when editing', () => {
    const { container } = render(
      <CellRenderer
        columnName="name"
        dataType="varchar"
        value="x"
        isEditing
        onCommit={NOOP}
        onCancel={NOOP}
      />,
    );
    expect(container.querySelector('input')).toBeInTheDocument();
  });

  it('truncates long json text', () => {
    const longObj = { data: 'x'.repeat(200) };
    const { container } = render(
      <CellRenderer
        columnName="meta"
        dataType="jsonb"
        value={longObj}
        isEditing={false}
        onCommit={NOOP}
        onCancel={NOOP}
      />,
    );
    const span = container.querySelector('span')!;
    expect(span.textContent).toContain('…');
  });

  it('renders long plain text with ellipsis', () => {
    const { container } = render(
      <CellRenderer
        columnName="notes"
        dataType="text"
        value={'a'.repeat(150)}
        isEditing={false}
        onCommit={NOOP}
        onCancel={NOOP}
      />,
    );
    const span = container.querySelector('span')!;
    expect(span.textContent).toContain('…');
  });

  it('stringifies object values for generic columns', () => {
    const { container } = render(
      <CellRenderer
        columnName="payload"
        dataType="unknown"
        value={{ k: 1 }}
        isEditing={false}
        onCommit={NOOP}
        onCancel={NOOP}
      />,
    );
    expect(container.textContent).toContain('{"k":1}');
  });
});
