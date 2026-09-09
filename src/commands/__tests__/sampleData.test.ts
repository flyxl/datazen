import { describe, it, expect, vi, beforeEach } from 'vitest';
import { sampleDataCommands } from '../sampleData';

const mockInvoke = vi.fn();
vi.mock('@tauri-apps/api/core', () => ({
  invoke: (...args: unknown[]) => mockInvoke(...args),
}));

describe('sampleDataCommands', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('calls init_sample_database and returns created connection config', async () => {
    const mockConfig = {
      id: 'sample_sqlite',
      name: 'Sample E-Commerce (SQLite)',
      databaseType: 'sqlite',
      database: '/tmp/sample_ecommerce.sqlite',
      group: 'Samples',
    };
    mockInvoke.mockResolvedValueOnce(mockConfig);

    const result = await sampleDataCommands.initSampleDatabase();
    expect(mockInvoke).toHaveBeenCalledWith('init_sample_database');
    expect(result.id).toBe('sample_sqlite');
    expect(result.databaseType).toBe('sqlite');
  });
});
