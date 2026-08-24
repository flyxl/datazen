import { describe, expect, it } from 'vitest';
import { isSqlCodeBlock, parseMessageSegments } from '../aiMessageBlocks';

describe('parseMessageSegments', () => {
  it('returns empty for empty string', () => {
    expect(parseMessageSegments('')).toEqual([]);
  });

  it('returns single text segment when no fences', () => {
    expect(parseMessageSegments('Hello world')).toEqual([{ type: 'text', content: 'Hello world' }]);
  });

  it('parses one sql fence with surrounding text', () => {
    const input = 'Try:\n```sql\nSELECT 1\n```\nDone.';
    expect(parseMessageSegments(input)).toEqual([
      { type: 'text', content: 'Try:\n' },
      { type: 'code', language: 'sql', code: 'SELECT 1' },
      { type: 'text', content: '\nDone.' },
    ]);
  });

  it('parses multiple fences', () => {
    const input = 'A\n```sql\nSELECT 1\n```\nB\n```json\n{"a":1}\n```\nC';
    const segments = parseMessageSegments(input);
    expect(segments).toHaveLength(5);
    expect(segments[1]).toEqual({ type: 'code', language: 'sql', code: 'SELECT 1' });
    expect(segments[3]).toEqual({ type: 'code', language: 'json', code: '{"a":1}' });
  });

  it('handles bare fence without language tag', () => {
    const input = '```\nSELECT 2\n```';
    expect(parseMessageSegments(input)).toEqual([{ type: 'code', language: '', code: 'SELECT 2' }]);
  });

  it('preserves trailing whitespace in text but trims code trailing newline', () => {
    const input = 'text\n```sql\nSELECT 1;\n\n```\n';
    const segments = parseMessageSegments(input);
    expect(segments[0]).toEqual({ type: 'text', content: 'text\n' });
    expect(segments[1]).toEqual({ type: 'code', language: 'sql', code: 'SELECT 1;' });
  });
});

describe('isSqlCodeBlock', () => {
  it('detects sql language tag', () => {
    expect(isSqlCodeBlock('sql', 'anything')).toBe(true);
    expect(isSqlCodeBlock('postgresql', 'x')).toBe(true);
  });

  it('detects sql by content when no language tag', () => {
    expect(isSqlCodeBlock('', 'SELECT 1')).toBe(true);
    expect(isSqlCodeBlock('', '{"a":1}')).toBe(false);
  });

  it('rejects non-sql json blocks', () => {
    expect(isSqlCodeBlock('json', '{"a":1}')).toBe(false);
  });
});
