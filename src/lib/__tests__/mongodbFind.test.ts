import { describe, it, expect } from 'vitest';
import {
  buildMongoFindCommand,
  cellToDisplay,
  parseMongoFilterJson,
  rowToDocument,
} from '../mongodbFind';

describe('mongodbFind helpers', () => {
  it('parses empty filter as empty object', () => {
    expect(parseMongoFilterJson('')).toEqual({});
    expect(parseMongoFilterJson('   ')).toEqual({});
  });

  it('rejects non-object filter JSON', () => {
    expect(() => parseMongoFilterJson('[]')).toThrow(/object/);
    expect(() => parseMongoFilterJson('"x"')).toThrow(/object/);
  });

  it('builds find command with database and filter', () => {
    const sql = buildMongoFindCommand({
      collection: 'orders',
      filterText: '{"status":"paid"}',
      limit: 50,
      database: 'shop',
    });
    expect(JSON.parse(sql)).toEqual({
      collection: 'orders',
      filter: { status: 'paid' },
      limit: 50,
      database: 'shop',
    });
  });

  it('formats nested cells as JSON', () => {
    expect(cellToDisplay({ a: 1 })).toBe('{"a":1}');
    expect(cellToDisplay(null)).toBe('NULL');
  });

  it('maps row cells back to a document object', () => {
    expect(rowToDocument(['_id', 'name'], ['1', 'alice'])).toEqual({
      _id: '1',
      name: 'alice',
    });
  });
});
