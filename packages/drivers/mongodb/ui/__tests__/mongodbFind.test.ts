import { describe, it, expect } from 'vitest';
import {
  buildMongoDeleteCommand,
  buildMongoFindCommand,
  buildMongoInsertCommand,
  buildMongoUpdateCommand,
  cellToDisplay,
  getDocumentId,
  parseMongoDocumentJson,
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

  it('parses document JSON and extracts _id', () => {
    const doc = parseMongoDocumentJson('{"_id":"abc","name":"bob"}');
    expect(getDocumentId(doc)).toBe('abc');
    expect(getDocumentId({})).toBeUndefined();
    expect(getDocumentId({ _id: null })).toBeUndefined();
  });

  it('builds update/insert/delete commands', () => {
    expect(JSON.parse(buildMongoUpdateCommand({
      collection: 'orders',
      database: 'shop',
      filter: { _id: '1' },
      setFields: { status: 'paid' },
    }))).toEqual({
      collection: 'orders',
      database: 'shop',
      update: { filter: { _id: '1' }, update: { $set: { status: 'paid' } } },
    });
    expect(JSON.parse(buildMongoInsertCommand({
      collection: 'orders',
      documents: [{ name: 'new' }],
    }))).toEqual({
      collection: 'orders',
      insert: [{ name: 'new' }],
    });
    expect(JSON.parse(buildMongoDeleteCommand({
      collection: 'orders',
      filter: { _id: '1' },
    }))).toEqual({
      collection: 'orders',
      delete: { filter: { _id: '1' } },
    });
  });
});
