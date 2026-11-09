import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render, waitFor } from '@testing-library/react';

const mockExportElementAsPng = vi.fn().mockResolvedValue(undefined);
const mockExportElementAsSvg = vi.fn().mockResolvedValue(undefined);

vi.mock('../../../lib/chart/export', () => ({
  exportElementAsPng: (...args: unknown[]) => mockExportElementAsPng(...args),
  exportElementAsSvg: (...args: unknown[]) => mockExportElementAsSvg(...args),
}));

const getErData = vi.fn(async () => [
  { tableName: 'users', columns: [], primaryKeys: [], indexes: [], foreignKeys: [] },
]);
vi.mock('../../../commands/database', () => ({
  databaseCommands: {
    getErData: (...args: Parameters<typeof getErData>) => getErData(...args),
  },
}));

// React Flow needs a measurable container; the export buttons are what this
// suite pins, so the canvas renders its children and the viewport is a plain
// div the test plants in the document, exactly what the handler queries for.
vi.mock('@xyflow/react', () => ({
  ReactFlowProvider: ({ children }: { children: React.ReactNode }) => <div>{children}</div>,
  ReactFlow: ({ children }: { children: React.ReactNode }) => (
    <div data-testid="rf">{children}</div>
  ),
  Background: () => null,
  BackgroundVariant: { Dots: 'dots' },
  Controls: () => null,
  ControlButton: ({ children }: { children: React.ReactNode }) => <>{children}</>,
  MiniMap: () => null,
  Panel: ({ children }: { children: React.ReactNode }) => <div>{children}</div>,
  useReactFlow: () => ({ fitView: vi.fn(), zoomIn: vi.fn(), zoomOut: vi.fn() }),
  useNodesState: (initial: unknown[]) => [initial, vi.fn(), vi.fn()],
  useEdgesState: (initial: unknown[]) => [initial, vi.fn(), vi.fn()],
  Handle: () => null,
  Position: { Left: 'left', Right: 'right' },
  MarkerType: { ArrowClosed: 'arrowclosed' },
}));

import { ErDiagramView } from '../ErDiagramView';

describe('ErDiagramView export calls the shared DOM image export (item 4)', () => {
  let viewport: HTMLDivElement;

  beforeEach(() => {
    vi.clearAllMocks();
    viewport = document.createElement('div');
    viewport.className = 'react-flow__viewport';
    document.body.appendChild(viewport);
  });

  afterEach(() => {
    cleanup();
    viewport.remove();
  });

  it('exports PNG through the shared function with the ER backdrop and quality', async () => {
    const { getByTestId } = render(<ErDiagramView dbSessionId="sess-1" database="db_a" />);
    await waitFor(() =>
      expect(getByTestId('er-diagram-export-png')).toBeTruthy(),
    );
    fireEvent.click(getByTestId('er-diagram-export-png'));
    await waitFor(() =>
      expect(mockExportElementAsPng).toHaveBeenCalledWith(viewport, 'er-diagram-db_a', {
        backgroundColor: '#1a1a2e',
        quality: 1,
      }),
    );
    expect(mockExportElementAsSvg).not.toHaveBeenCalled();
  });

  it('exports SVG through the shared function with the ER backdrop', async () => {
    const { getByTestId } = render(<ErDiagramView dbSessionId="sess-1" database="db_a" />);
    await waitFor(() =>
      expect(getByTestId('er-diagram-export-svg')).toBeTruthy(),
    );
    fireEvent.click(getByTestId('er-diagram-export-svg'));
    await waitFor(() =>
      expect(mockExportElementAsSvg).toHaveBeenCalledWith(viewport, 'er-diagram-db_a', {
        backgroundColor: '#1a1a2e',
      }),
    );
    expect(mockExportElementAsPng).not.toHaveBeenCalled();
  });

  it('does nothing when the viewport element is absent', async () => {
    viewport.remove();
    const { getByTestId } = render(<ErDiagramView dbSessionId="sess-1" database="db_a" />);
    await waitFor(() =>
      expect(getByTestId('er-diagram-export-png')).toBeTruthy(),
    );
    fireEvent.click(getByTestId('er-diagram-export-png'));
    fireEvent.click(getByTestId('er-diagram-export-svg'));
    await new Promise((resolve) => setTimeout(resolve, 0));
    expect(mockExportElementAsPng).not.toHaveBeenCalled();
    expect(mockExportElementAsSvg).not.toHaveBeenCalled();
  });

  it('swallows an export failure the way the inline handler did', async () => {
    const errorSpy = vi.spyOn(console, 'error').mockImplementation(() => {});
    mockExportElementAsPng.mockRejectedValueOnce(new Error('boom'));
    const { getByTestId } = render(<ErDiagramView dbSessionId="sess-1" database="db_a" />);
    await waitFor(() =>
      expect(getByTestId('er-diagram-export-png')).toBeTruthy(),
    );
    fireEvent.click(getByTestId('er-diagram-export-png'));
    await waitFor(() =>
      expect(errorSpy).toHaveBeenCalledWith('Export failed:', expect.any(Error)),
    );
    errorSpy.mockRestore();
  });

  it('swallows an SVG export failure too', async () => {
    const errorSpy = vi.spyOn(console, 'error').mockImplementation(() => {});
    mockExportElementAsSvg.mockRejectedValueOnce(new Error('boom'));
    const { getByTestId } = render(<ErDiagramView dbSessionId="sess-1" database="db_a" />);
    await waitFor(() =>
      expect(getByTestId('er-diagram-export-svg')).toBeTruthy(),
    );
    fireEvent.click(getByTestId('er-diagram-export-svg'));
    await waitFor(() =>
      expect(errorSpy).toHaveBeenCalledWith('Export failed:', expect.any(Error)),
    );
    errorSpy.mockRestore();
  });
});
