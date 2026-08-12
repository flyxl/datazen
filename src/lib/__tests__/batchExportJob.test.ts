import { describe, expect, it, vi, beforeEach } from 'vitest';
import { unzipSync } from 'fflate';
import { runBatchExportJob, uint8ToBase64, zipBatchExportFiles } from '../batchExportJob';
import type { BatchExportTableInput } from '../batchExport';

function bytesFromBase64(encoded: string): Uint8Array {
  const binary = atob(encoded.trim());
  const bytes = new Uint8Array(binary.length);
  for (let i = 0; i < binary.length; i += 1) {
    bytes[i] = binary.charCodeAt(i);
  }
  return bytes;
}

const users: BatchExportTableInput = {
  tableName: 'users',
  ddl: 'CREATE TABLE users (id INT);',
  columns: [{ name: 'id' }, { name: 'name' }],
  rows: [{ id: 1, name: 'Alice' }],
};

const orders: BatchExportTableInput = {
  tableName: 'orders',
  ddl: 'CREATE TABLE orders (id INT);',
  columns: [{ name: 'id' }],
  rows: [{ id: 10 }],
};

describe('uint8ToBase64 / zipBatchExportFiles', () => {
  it('round-trips zip contents', () => {
    const files = [
      { filename: 'a.sql', content: 'CREATE TABLE a;' },
      { filename: 'b.csv', content: 'id\n1' },
    ];
    const b64 = zipBatchExportFiles(files);
    const unzipped = unzipSync(bytesFromBase64(b64));
    expect(new TextDecoder().decode(unzipped['a.sql'])).toBe('CREATE TABLE a;');
    expect(new TextDecoder().decode(unzipped['b.csv'])).toBe('id\n1');
  });

  it('encodes empty bytes', () => {
    expect(uint8ToBase64(new Uint8Array())).toBe('');
  });
});

describe('runBatchExportJob', () => {
  const saveText = vi.fn();
  const saveBase64 = vi.fn();
  const loadTableExportData = vi.fn();

  beforeEach(() => {
    vi.clearAllMocks();
    saveText.mockResolvedValue(true);
    saveBase64.mockResolvedValue(true);
    loadTableExportData.mockImplementation(async (name: string) => {
      if (name === 'users') return users;
      if (name === 'orders') return orders;
      throw new Error(`unknown table ${name}`);
    });
  });

  it('throws when no tables selected', async () => {
    await expect(
      runBatchExportJob({
        tableNames: [],
        mode: 'structure_only',
        dataFormat: 'csv',
        outputMode: 'single',
        loadTableExportData,
        saveText,
        saveBase64,
      }),
    ).rejects.toThrow('no_tables_selected');
    expect(loadTableExportData).not.toHaveBeenCalled();
  });

  it('loads tables in order and saves a combined single file', async () => {
    const onProgress = vi.fn();
    const result = await runBatchExportJob({
      tableNames: ['users', 'orders'],
      mode: 'structure_only',
      dataFormat: 'csv',
      outputMode: 'single',
      loadTableExportData,
      onProgress,
      saveText,
      saveBase64,
    });

    expect(result).toEqual({ status: 'saved' });
    expect(loadTableExportData).toHaveBeenCalledTimes(2);
    expect(loadTableExportData.mock.calls.map((c) => c[0])).toEqual(['users', 'orders']);
    expect(onProgress).toHaveBeenCalledWith({ current: 1, total: 2, tableName: 'users' });
    expect(onProgress).toHaveBeenCalledWith({ current: 2, total: 2, tableName: 'orders' });
    expect(saveText).toHaveBeenCalledTimes(1);
    expect(saveBase64).not.toHaveBeenCalled();

    const [content, filename, filterName, extensions] = saveText.mock.calls[0]!;
    expect(filename).toMatch(/^export_structure_.*\.sql$/);
    expect(filterName).toBe('SQL');
    expect(extensions).toEqual(['sql', 'txt']);
    expect(content).toContain('CREATE TABLE users');
    expect(content).toContain('CREATE TABLE orders');
    expect(content).toContain('-- ===== users.sql =====');
  });

  it('builds a zip and saves via saveBase64', async () => {
    const result = await runBatchExportJob({
      tableNames: ['users'],
      mode: 'data_only',
      dataFormat: 'csv',
      outputMode: 'zip',
      loadTableExportData,
      saveText,
      saveBase64,
    });

    expect(result).toEqual({ status: 'saved' });
    expect(saveBase64).toHaveBeenCalledTimes(1);
    expect(saveText).not.toHaveBeenCalled();

    const [dataBase64, filename, filterName, extensions] = saveBase64.mock.calls[0]!;
    expect(filename).toMatch(/^export_data_.*\.zip$/);
    expect(filterName).toBe('ZIP');
    expect(extensions).toEqual(['zip']);

    const unzipped = unzipSync(bytesFromBase64(dataBase64 as string));
    expect(Object.keys(unzipped)).toContain('users.csv');
    expect(new TextDecoder().decode(unzipped['users.csv']!)).toContain('Alice');
  });

  it('returns cancelled when save dialog is dismissed', async () => {
    saveText.mockResolvedValue(false);
    const result = await runBatchExportJob({
      tableNames: ['users'],
      mode: 'structure_only',
      dataFormat: 'csv',
      outputMode: 'single',
      loadTableExportData,
      saveText,
      saveBase64,
    });
    expect(result).toEqual({ status: 'cancelled' });
  });

  it('propagates load errors', async () => {
    loadTableExportData.mockRejectedValue(new Error('ddl failed'));
    await expect(
      runBatchExportJob({
        tableNames: ['users'],
        mode: 'structure_only',
        dataFormat: 'csv',
        outputMode: 'single',
        loadTableExportData,
        saveText,
        saveBase64,
      }),
    ).rejects.toThrow('ddl failed');
  });
});
