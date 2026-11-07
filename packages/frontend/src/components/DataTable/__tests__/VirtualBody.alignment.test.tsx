import { describe, expect, it } from 'vitest';
import { render } from '@testing-library/react';
import { CellRenderer } from '../CellRenderer';

const NOOP = () => {};

describe('CellRenderer text alignment', () => {
  it('renders numeric value with right alignment', () => {
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
    expect(span.className).toContain('text-right');
  });

  it('renders integer value with right alignment', () => {
    const { container } = render(
      <CellRenderer
        columnName="id"
        dataType="integer"
        value={42}
        isEditing={false}
        onCommit={NOOP}
        onCancel={NOOP}
      />,
    );
    const span = container.querySelector('span')!;
    expect(span.className).toContain('text-right');
  });

  it('renders text value with left alignment', () => {
    const { container } = render(
      <CellRenderer
        columnName="name"
        dataType="varchar"
        value="Alice"
        isEditing={false}
        onCommit={NOOP}
        onCancel={NOOP}
      />,
    );
    const span = container.querySelector('span')!;
    expect(span.className).not.toContain('text-right');
  });

  it('renders boolean value with left alignment', () => {
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
    expect(span.className).not.toContain('text-right');
  });

  it('renders NULL with left alignment', () => {
    const { container } = render(
      <CellRenderer
        columnName="x"
        dataType="text"
        value={null}
        isEditing={false}
        onCommit={NOOP}
        onCancel={NOOP}
      />,
    );
    const span = container.querySelector('span')!;
    expect(span.className).not.toContain('text-right');
  });

  it('renders timestamp with left alignment', () => {
    const { container } = render(
      <CellRenderer
        columnName="created_at"
        dataType="timestamp"
        value="2024-01-01T00:00:00Z"
        isEditing={false}
        onCommit={NOOP}
        onCancel={NOOP}
      />,
    );
    const span = container.querySelector('span')!;
    expect(span.className).not.toContain('text-right');
  });

  it('renders JSON with left alignment', () => {
    const { container } = render(
      <CellRenderer
        columnName="meta"
        dataType="jsonb"
        value={{ key: 'val' }}
        isEditing={false}
        onCommit={NOOP}
        onCancel={NOOP}
      />,
    );
    const span = container.querySelector('span')!;
    expect(span.className).not.toContain('text-right');
  });
});
