import { describe, it, expect } from 'vitest';
import {
  parseDelimitedValues,
  formatInClause,
  toSqlLiteral,
  escapeSqlString,
  MAX_SOURCE_BYTES,
  MAX_VALUE_COUNT,
} from '../parseDelimitedValues';

describe('parseDelimitedValues', () => {
  describe('basic delimiter detection', () => {
    it('detects comma-separated values', () => {
      const result = parseDelimitedValues('a,b,c');
      expect(result.ok).toBe(true);
      if (result.ok) {
        expect(result.values).toEqual(['a', 'b', 'c']);
        expect(result.delimiter).toBe(',');
      }
    });

    it('detects tab-separated values', () => {
      const result = parseDelimitedValues('a\tb\tc');
      expect(result.ok).toBe(true);
      if (result.ok) {
        expect(result.values).toEqual(['a', 'b', 'c']);
        expect(result.delimiter).toBe('\t');
      }
    });

    it('detects newline-separated values', () => {
      const result = parseDelimitedValues('a\nb\nc');
      expect(result.ok).toBe(true);
      if (result.ok) {
        expect(result.values).toEqual(['a', 'b', 'c']);
        expect(result.delimiter).toBe('\n');
      }
    });

    it('normalizes CRLF to LF', () => {
      const result = parseDelimitedValues('a\r\nb\r\nc');
      expect(result.ok).toBe(true);
      if (result.ok) {
        expect(result.values).toEqual(['a', 'b', 'c']);
      }
    });

    it('normalizes lone CR to LF', () => {
      const result = parseDelimitedValues('a\rb\rc');
      expect(result.ok).toBe(true);
      if (result.ok) {
        expect(result.values).toEqual(['a', 'b', 'c']);
      }
    });

    it('detects semicolon-separated values', () => {
      const result = parseDelimitedValues('a;b;c');
      expect(result.ok).toBe(true);
      if (result.ok) {
        expect(result.values).toEqual(['a', 'b', 'c']);
        expect(result.delimiter).toBe(';');
      }
    });
  });

  describe('explicit delimiter option', () => {
    it('uses explicit comma delimiter', () => {
      const result = parseDelimitedValues('a,b,c', { delimiter: ',' });
      expect(result.ok).toBe(true);
      if (result.ok) {
        expect(result.values).toEqual(['a', 'b', 'c']);
        expect(result.delimiter).toBe(',');
      }
    });

    it('uses explicit pipe delimiter', () => {
      const result = parseDelimitedValues('a|b|c', { delimiter: '|' });
      expect(result.ok).toBe(true);
      if (result.ok) {
        expect(result.values).toEqual(['a', 'b', 'c']);
      }
    });
  });

  describe('trimming', () => {
    it('trims whitespace by default', () => {
      const result = parseDelimitedValues('  a  ,  b  ,  c  ');
      expect(result.ok).toBe(true);
      if (result.ok) {
        expect(result.values).toEqual(['a', 'b', 'c']);
      }
    });

    it('preserves whitespace when trim is false', () => {
      const result = parseDelimitedValues('  a  ,  b  ,  c  ', { trim: false });
      expect(result.ok).toBe(true);
      if (result.ok) {
        expect(result.values).toEqual(['  a  ', '  b  ', '  c  ']);
      }
    });
  });

  describe('empty items', () => {
    it('skips empty items by default', () => {
      const result = parseDelimitedValues('a,,b,,c');
      expect(result.ok).toBe(true);
      if (result.ok) {
        expect(result.values).toEqual(['a', 'b', 'c']);
      }
    });

    it('keeps empty items when configured', () => {
      const result = parseDelimitedValues('a,,b,,c', { emptyItems: 'keep-empty' });
      expect(result.ok).toBe(true);
      if (result.ok) {
        expect(result.values).toEqual(['a', '', 'b', '', 'c']);
      }
    });

    it('skips trailing empty items', () => {
      const result = parseDelimitedValues('a,b,');
      expect(result.ok).toBe(true);
      if (result.ok) {
        expect(result.values).toEqual(['a', 'b']);
      }
    });

    it('skips leading empty items', () => {
      const result = parseDelimitedValues(',a,b');
      expect(result.ok).toBe(true);
      if (result.ok) {
        expect(result.values).toEqual(['a', 'b']);
      }
    });
  });

  describe('NULL handling', () => {
    it('preserves NULL as a value', () => {
      const result = parseDelimitedValues('a,NULL,b');
      expect(result.ok).toBe(true);
      if (result.ok) {
        expect(result.values).toEqual(['a', 'NULL', 'b']);
      }
    });

    it('preserves null (lowercase) as a value', () => {
      const result = parseDelimitedValues('a,null,b');
      expect(result.ok).toBe(true);
      if (result.ok) {
        expect(result.values).toEqual(['a', 'null', 'b']);
      }
    });
  });

  describe('numeric values', () => {
    it('preserves integer strings', () => {
      const result = parseDelimitedValues('1,2,3');
      expect(result.ok).toBe(true);
      if (result.ok) {
        expect(result.values).toEqual(['1', '2', '3']);
      }
    });

    it('preserves float strings', () => {
      const result = parseDelimitedValues('1.5,2.7,3.14');
      expect(result.ok).toBe(true);
      if (result.ok) {
        expect(result.values).toEqual(['1.5', '2.7', '3.14']);
      }
    });

    it('preserves negative numbers', () => {
      const result = parseDelimitedValues('-1,-2.5,3');
      expect(result.ok).toBe(true);
      if (result.ok) {
        expect(result.values).toEqual(['-1', '-2.5', '3']);
      }
    });
  });

  describe('limits', () => {
    it('rejects source exceeding 1 MiB', () => {
      const largeSource = 'a'.repeat(MAX_SOURCE_BYTES + 1);
      const result = parseDelimitedValues(largeSource);
      expect('ok' in result).toBe(false);
      if (!('ok' in result)) {
        expect(result.kind).toBe('too-large');
        expect(result.maxBytes).toBe(MAX_SOURCE_BYTES);
      }
    });

    it('accepts source at exactly 1 MiB', () => {
      // Create a source that's exactly 1 MiB with few values (using long strings)
      const padding = 'x'.repeat(MAX_SOURCE_BYTES - 4); // "ab,x...x" = exactly 1 MiB
      const source = `ab,${padding}`;
      const result = parseDelimitedValues(source);
      expect('ok' in result).toBe(true);
    });

    it('rejects more than 10,000 values', () => {
      const manyValues = Array.from({ length: MAX_VALUE_COUNT + 1 }, (_, i) => `${i}`).join(',');
      const result = parseDelimitedValues(manyValues);
      expect('ok' in result).toBe(false);
      if (!('ok' in result)) {
        expect(result.kind).toBe('too-many-values');
        expect(result.maxCount).toBe(MAX_VALUE_COUNT);
      }
    });

    it('accepts exactly 10,000 values', () => {
      const exactValues = Array.from({ length: MAX_VALUE_COUNT }, (_, i) => `${i}`).join(',');
      const result = parseDelimitedValues(exactValues);
      expect('ok' in result).toBe(true);
      if ('ok' in result) {
        expect(result.values).toHaveLength(MAX_VALUE_COUNT);
      }
    });
  });

  describe('no values', () => {
    it('returns no-values for empty string', () => {
      const result = parseDelimitedValues('');
      expect('ok' in result).toBe(false);
      if (!('ok' in result)) {
        expect(result.kind).toBe('no-values');
      }
    });

    it('returns no-values for whitespace-only', () => {
      const result = parseDelimitedValues('   ');
      expect('ok' in result).toBe(false);
      if (!('ok' in result)) {
        expect(result.kind).toBe('no-values');
      }
    });
  });

  describe('single value', () => {
    it('parses a single value', () => {
      const result = parseDelimitedValues('hello');
      expect(result.ok).toBe(true);
      if (result.ok) {
        expect(result.values).toEqual(['hello']);
      }
    });
  });

  describe('quoted strings with delimiters', () => {
    it('preserves quotes in values', () => {
      const result = parseDelimitedValues("'hello', 'world'");
      expect(result.ok).toBe(true);
      if (result.ok) {
        expect(result.values).toEqual(["'hello'", "'world'"]);
      }
    });
  });
});

describe('escapeSqlString', () => {
  it('escapes single quotes', () => {
    expect(escapeSqlString("it's")).toBe("it''s");
  });

  it('escapes multiple single quotes', () => {
    expect(escapeSqlString("a''b")).toBe("a''''b");
  });

  it('does not modify strings without quotes', () => {
    expect(escapeSqlString('hello')).toBe('hello');
  });

  it('handles empty string', () => {
    expect(escapeSqlString('')).toBe('');
  });
});

describe('toSqlLiteral', () => {
  describe('auto-type mode', () => {
    it('quotes regular strings', () => {
      expect(toSqlLiteral('hello', 'auto-type')).toBe("'hello'");
    });

    it('escapes single quotes in strings', () => {
      expect(toSqlLiteral("it's", 'auto-type')).toBe("'it''s'");
    });

    it('preserves integers as numeric', () => {
      expect(toSqlLiteral('42', 'auto-type')).toBe('42');
    });

    it('preserves negative integers', () => {
      expect(toSqlLiteral('-7', 'auto-type')).toBe('-7');
    });

    it('preserves floats as numeric', () => {
      expect(toSqlLiteral('3.14', 'auto-type')).toBe('3.14');
    });

    it('preserves negative floats', () => {
      expect(toSqlLiteral('-2.5', 'auto-type')).toBe('-2.5');
    });

    it('preserves NULL as SQL NULL', () => {
      expect(toSqlLiteral('NULL', 'auto-type')).toBe('NULL');
    });

    it('preserves null as SQL NULL', () => {
      expect(toSqlLiteral('null', 'auto-type')).toBe('NULL');
    });

    it('quotes strings that look like numbers but have leading zeros', () => {
      expect(toSqlLiteral('007', 'auto-type')).toBe("'007'");
    });

    it('quotes strings with special characters', () => {
      expect(toSqlLiteral('hello world', 'auto-type')).toBe("'hello world'");
    });
  });

  describe('all-strings mode', () => {
    it('quotes everything as string', () => {
      expect(toSqlLiteral('42', 'all-strings')).toBe("'42'");
    });

    it('quotes NULL as string', () => {
      expect(toSqlLiteral('NULL', 'all-strings')).toBe("'NULL'");
    });

    it('quotes floats as string', () => {
      expect(toSqlLiteral('3.14', 'all-strings')).toBe("'3.14'");
    });

    it('escapes single quotes', () => {
      expect(toSqlLiteral("it's", 'all-strings')).toBe("'it''s'");
    });
  });
});

describe('formatInClause', () => {
  it('formats comma-separated SQL literals', () => {
    expect(formatInClause(['a', 'b', 'c'], 'auto-type')).toBe("'a', 'b', 'c'");
  });

  it('handles mixed types in auto-type', () => {
    expect(formatInClause(['hello', '42', 'NULL'], 'auto-type')).toBe("'hello', 42, NULL");
  });

  it('formats all as strings', () => {
    expect(formatInClause(['a', '42', 'NULL'], 'all-strings')).toBe("'a', '42', 'NULL'");
  });

  it('handles single value', () => {
    expect(formatInClause(['hello'], 'auto-type')).toBe("'hello'");
  });

  it('escapes quotes in values', () => {
    expect(formatInClause(["it's", "he's"], 'auto-type')).toBe("'it''s', 'he''s'");
  });
});
