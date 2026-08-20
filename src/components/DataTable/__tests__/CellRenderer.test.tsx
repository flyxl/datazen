import { describe, expect, it } from 'vitest';
import { render } from '@testing-library/react';
import { CellRenderer } from '../CellRenderer';

const NOOP = () => {};

describe('CellRenderer no-wrap', () => {
  it('JSON cell should not wrap text', () => {
    const { container } = render(
      <CellRenderer
        columnName="metadata"
        dataType="jsonb"
        value={{ brand: 'Apple', color: 'Gray' }}
        isEditing={false}
        onCommit={NOOP}
        onCancel={NOOP}
      />,
    );
    const span = container.querySelector('span')!;
    expect(span.className).toContain('truncate');
    expect(span.className).toContain('text-dt-json');
  });

  it('timestamp cell should not wrap text', () => {
    const { container } = render(
      <CellRenderer
        columnName="created_at"
        dataType="timestamp with time zone"
        value="2026-04-14T09:35:36.554Z"
        isEditing={false}
        onCommit={NOOP}
        onCancel={NOOP}
      />,
    );
    const span = container.querySelector('span')!;
    expect(span.className).toContain('truncate');
    expect(span.className).toContain('text-dt-datetime');
  });

  it('boolean cell should not wrap text', () => {
    const { container } = render(
      <CellRenderer
        columnName="active"
        dataType="boolean"
        value={true}
        isEditing={false}
        onCommit={NOOP}
        onCancel={NOOP}
      />,
    );
    const span = container.querySelector('span')!;
    expect(span.className).toContain('truncate');
    expect(span.className).toContain('text-dt-bool');
  });

  it('numeric cell should not wrap text', () => {
    const { container } = render(
      <CellRenderer
        columnName="price"
        dataType="numeric"
        value={14999}
        isEditing={false}
        onCommit={NOOP}
        onCancel={NOOP}
      />,
    );
    const span = container.querySelector('span')!;
    expect(span.className).toContain('truncate');
    expect(span.className).toContain('text-dt-number');
  });

  it('NULL cell should not wrap text', () => {
    const { container } = render(
      <CellRenderer
        columnName="notes"
        dataType="text"
        value={null}
        isEditing={false}
        onCommit={NOOP}
        onCancel={NOOP}
      />,
    );
    const span = container.querySelector('span')!;
    expect(span.className).toContain('truncate');
    expect(span.className).toContain('text-dt-null');
  });
});
