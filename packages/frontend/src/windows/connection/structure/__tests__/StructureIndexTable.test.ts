import { describe, expect, it } from 'vitest';
import { suggestedIndexName } from '../StructureIndexTable';

describe('suggestedIndexName', () => {
  it('uses table name when no columns selected', () => {
    expect(suggestedIndexName('demo_sales', [])).toBe('idx_demo_sales');
  });

  it('appends selected columns in order', () => {
    expect(suggestedIndexName('demo_sales', ['region', 'category'])).toBe(
      'idx_demo_sales_region_category',
    );
  });

  it('falls back when table name is empty', () => {
    expect(suggestedIndexName('', ['id'])).toBe('idx_table_id');
  });
});
