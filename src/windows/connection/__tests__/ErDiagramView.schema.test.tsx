import { describe, expect, it, beforeEach, vi } from 'vitest';
import { render, waitFor } from '@testing-library/react';

const getErData = vi.fn(async () => []);
vi.mock('../../../commands/database', () => ({
  databaseCommands: { getErData: (...args: unknown[]) => getErData(...args) },
}));

// React Flow needs a measurable container; the diagram's data fetch is what
// this suite pins, so the canvas itself is stubbed out.
vi.mock('@xyflow/react', () => ({
  ReactFlowProvider: ({ children }: { children: React.ReactNode }) => <div>{children}</div>,
  ReactFlow: () => <div data-testid="rf" />,
  Background: () => null,
  Controls: () => null,
  MiniMap: () => null,
  useReactFlow: () => ({ fitView: vi.fn(), zoomIn: vi.fn(), zoomOut: vi.fn() }),
  useNodesState: (initial: unknown[]) => [initial, vi.fn(), vi.fn()],
  useEdgesState: (initial: unknown[]) => [initial, vi.fn(), vi.fn()],
  Handle: () => null,
  Position: { Left: 'left', Right: 'right' },
  MarkerType: { ArrowClosed: 'arrowclosed' },
}));

import { ErDiagramView } from '../ErDiagramView';

describe('ErDiagramView forwards the schema to get_er_data', () => {
  beforeEach(() => {
    getErData.mockClear();
  });

  it('passes the panel schema as the third argument', async () => {
    render(<ErDiagramView dbSessionId="sess-1" database="db_a" schema="reporting" />);
    await waitFor(() => expect(getErData).toHaveBeenCalled());
    expect(getErData).toHaveBeenCalledWith('sess-1', 'db_a', 'reporting');
  });

  it('passes null when no schema is known, letting the host decide', async () => {
    render(<ErDiagramView dbSessionId="sess-2" database="db_b" />);
    await waitFor(() => expect(getErData).toHaveBeenCalled());
    // Never the database name: a schema is a namespace inside the database.
    expect(getErData).toHaveBeenCalledWith('sess-2', 'db_b', null);
  });

  it('re-reads when the schema changes', async () => {
    const { rerender } = render(
      <ErDiagramView dbSessionId="sess-3" database="db_a" schema="reporting" />,
    );
    await waitFor(() => expect(getErData).toHaveBeenCalledTimes(1));

    rerender(<ErDiagramView dbSessionId="sess-3" database="db_a" schema="audit" />);
    await waitFor(() => expect(getErData).toHaveBeenLastCalledWith('sess-3', 'db_a', 'audit'));
  });
});
