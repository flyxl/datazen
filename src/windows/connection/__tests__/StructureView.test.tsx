import { describe, expect, it, vi, beforeEach } from 'vitest';
import { render, screen, waitFor } from '@testing-library/react';
import { StructureView } from '../StructureView';
import * as schemaCache from '../../../lib/schemaCache';

vi.mock('../../../lib/schemaCache', () => ({
  getCachedTableSchema: vi.fn(),
}));

describe('StructureView', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('renders columns and scrolls/highlights targetColumn', async () => {
    vi.mocked(schemaCache.getCachedTableSchema).mockResolvedValue({
      tableName: 'er_customers',
      columns: [
        { name: 'id', dataType: 'integer', nullable: false, defaultValue: null, comment: null },
        {
          name: 'order_no',
          dataType: 'varchar(64)',
          nullable: false,
          defaultValue: null,
          comment: null,
        },
        { name: 'city', dataType: 'text', nullable: true, defaultValue: null, comment: null },
      ],
      primaryKeys: ['id'],
      indexes: [],
      foreignKeys: [],
    });

    const scrollIntoViewMock = vi.fn();
    window.HTMLElement.prototype.scrollIntoView = scrollIntoViewMock;

    render(<StructureView dbSessionId="sess-1" tableName="er_customers" targetColumn="order_no" />);

    await waitFor(() => {
      expect(screen.getByText('er_customers')).toBeInTheDocument();
      expect(screen.getByText('order_no')).toBeInTheDocument();
    });

    await waitFor(() => {
      expect(scrollIntoViewMock).toHaveBeenCalled();
    });

    const row = document.querySelector('[data-struct-col="order_no"]');
    expect(row).toBeInTheDocument();
    expect(row?.className).toContain('ring-accent');
  });

  it('passes database to getCachedTableSchema when provided', async () => {
    vi.mocked(schemaCache.getCachedTableSchema).mockResolvedValue({
      tableName: 'users',
      columns: [
        { name: 'id', dataType: 'integer', nullable: false, defaultValue: null, comment: null },
      ],
      primaryKeys: ['id'],
      indexes: [],
      foreignKeys: [],
    });

    render(<StructureView dbSessionId="sess-1" tableName="users" database="catalog_db" />);

    await waitFor(() => {
      expect(schemaCache.getCachedTableSchema).toHaveBeenCalledWith(
        'sess-1',
        'users',
        'catalog_db',
      );
    });
  });
});
