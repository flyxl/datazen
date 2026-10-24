/**
 * The by-name width half of `useColumnResize` (DB-12).
 *
 * A width belongs to a **column**, not to a position: the acceptance is that
 * a resized column keeps its width across a reorder and across restarts, so
 * the hook has to file every dragged width under the column's name and be
 * seedable from the persisted map.
 */
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { act, renderHook } from '@testing-library/react';
import { useColumnResize } from '../useColumnResize';

const COLUMNS = [
  { name: 'id', type: 'integer' },
  { name: 'name', type: 'text' },
  { name: 'age', type: 'integer' },
];

function firePointer(clientX: number): PointerEvent {
  return new PointerEvent('pointermove', { clientX });
}

describe('useColumnResize by-name widths (DB-12)', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    document.body.style.cursor = '';
    document.body.style.userSelect = '';
  });

  it('seeds the widths from the persisted map, by name', () => {
    const { result } = renderHook(() =>
      useColumnResize({
        count: 3,
        columns: COLUMNS,
        rows: [],
        persistedWidths: { name: 222, age: 333 },
      }),
    );
    const [id, name, age] = result.current.columnWidths;
    expect(name).toBe(222);
    expect(age).toBe(333);
    // A column without a stored width falls back to the computed one.
    expect(id).toBeLessThanOrEqual(400);
  });

  it('a drag files the width under the dragged column and reports the whole map', () => {
    const onWidthsCommit = vi.fn();
    const { result } = renderHook(() =>
      useColumnResize({ count: 3, columns: COLUMNS, rows: [], onWidthsCommit }),
    );

    act(() => {
      result.current.onResizeStart(1, 500);
    });
    act(() => {
      document.dispatchEvent(firePointer(560));
    });
    act(() => {
      document.dispatchEvent(new PointerEvent('pointerup'));
    });

    expect(result.current.columnWidths[1]).toBeGreaterThan(150);
    // The commit carries every column's effective width, by name.
    expect(onWidthsCommit).toHaveBeenCalledOnce();
    const committed = onWidthsCommit.mock.calls[0][0] as Record<string, number>;
    expect(Object.keys(committed).sort()).toEqual(['age', 'id', 'name']);
    expect(committed.name).toBe(result.current.columnWidths[1]);
  });

  it('a reordered column keeps its dragged width', () => {
    const onWidthsCommit = vi.fn();
    const { result, rerender } = renderHook(
      ({ columns }) =>
        useColumnResize({ count: columns.length, columns, rows: [], onWidthsCommit }),
      { initialProps: { columns: COLUMNS } },
    );

    act(() => {
      result.current.onResizeStart(1, 500);
    });
    act(() => {
      document.dispatchEvent(firePointer(620));
    });
    act(() => {
      document.dispatchEvent(new PointerEvent('pointerup'));
    });
    const draggedWidth = result.current.columnWidths[1];

    // `name` moves to the front: its width follows the name, not the slot.
    rerender({
      columns: [COLUMNS[1], COLUMNS[0], COLUMNS[2]],
    });
    expect(result.current.columnWidths[0]).toBe(draggedWidth);
    expect(result.current.columnWidths[1]).not.toBe(draggedWidth);
  });

  it('a persisted width survives a rerender after a reorder', () => {
    const { result, rerender } = renderHook(
      ({ columns }) =>
        useColumnResize({
          count: columns.length,
          columns,
          rows: [],
          persistedWidths: { age: 333 },
        }),
      { initialProps: { columns: COLUMNS } },
    );
    rerender({ columns: [COLUMNS[2], COLUMNS[1], COLUMNS[0]] });
    // `age` is now first, and its stored width is with it.
    expect(result.current.columnWidths[0]).toBe(333);
  });
});
