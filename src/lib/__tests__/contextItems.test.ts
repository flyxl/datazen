import { describe, expect, it } from 'vitest';
import { splitContextItems } from '../contextItems';

describe('splitContextItems', () => {
  it('splits tables and files', () => {
    const r = splitContextItems([
      { kind: 'table', id: 'users', name: 'users' },
      { kind: 'file', id: 'a.sql', name: 'a.sql', path: 'a.sql' },
    ]);
    expect(r.contextTables).toEqual(['users']);
    expect(r.contextFiles).toEqual(['a.sql']);
  });
});
