import { describe, it, expect } from 'vitest';
import { extractSqlFromResponse, extractSqlStreaming } from '../extractSql';

describe('extractSqlFromResponse', () => {
  it('returns empty for empty input', () => {
    expect(extractSqlFromResponse('')).toBe('');
    expect(extractSqlFromResponse('  ')).toBe('');
  });

  it('returns pure SQL as-is', () => {
    const sql = 'SELECT id, name FROM users WHERE active = true ORDER BY name';
    expect(extractSqlFromResponse(sql)).toBe(sql);
  });

  it('extracts SQL from ```sql fence', () => {
    const input = `这是一些分析内容...

\`\`\`sql
SELECT u.id, u.name, o.total
FROM users u
JOIN orders o ON o.user_id = u.id
WHERE u.active = true
ORDER BY o.total DESC
\`\`\`

这是解释说明...`;
    expect(extractSqlFromResponse(input)).toBe(
      'SELECT u.id, u.name, o.total\nFROM users u\nJOIN orders o ON o.user_id = u.id\nWHERE u.active = true\nORDER BY o.total DESC',
    );
  });

  it('extracts SQL from bare ``` fence', () => {
    const input = `让我分析一下表结构，然后生成查询：

\`\`\`
SELECT * FROM products WHERE price > 100
\`\`\``;
    expect(extractSqlFromResponse(input)).toBe('SELECT * FROM products WHERE price > 100');
  });

  it('handles reasoning text before SQL', () => {
    const input = `根据你的描述，我需要查询所有活跃用户的订单总额。考虑到表结构，最合适的 SQL 是：

SELECT u.name, SUM(o.amount) AS total
FROM users u
JOIN orders o ON o.user_id = u.id
WHERE u.active = true
GROUP BY u.name
ORDER BY total DESC`;
    const result = extractSqlFromResponse(input);
    expect(result).toContain('SELECT');
    expect(result).toContain('GROUP BY');
    expect(result).not.toContain('根据你的描述');
  });

  it('handles multi-statement SQL', () => {
    const sql = `SELECT * FROM users;
SELECT * FROM orders;`;
    expect(extractSqlFromResponse(sql)).toBe(sql);
  });

  it('handles WITH (CTE) queries', () => {
    const sql = `WITH active_users AS (
  SELECT id, name FROM users WHERE active = true
)
SELECT * FROM active_users`;
    expect(extractSqlFromResponse(sql)).toBe(sql);
  });

  it('handles CREATE TABLE statements', () => {
    const sql = `CREATE TABLE IF NOT EXISTS test (
  id SERIAL PRIMARY KEY,
  name VARCHAR(255) NOT NULL
)`;
    expect(extractSqlFromResponse(sql)).toBe(sql);
  });

  it('handles case-insensitive SQL fence tags', () => {
    const input = `\`\`\`SQL
SELECT 1
\`\`\``;
    expect(extractSqlFromResponse(input)).toBe('SELECT 1');
  });

  it('handles postgresql fence tag', () => {
    const input = `\`\`\`postgresql
SELECT version()
\`\`\``;
    expect(extractSqlFromResponse(input)).toBe('SELECT version()');
  });

  it('falls back to full text when no SQL is detected', () => {
    const input = 'This is just some random text without any SQL.';
    expect(extractSqlFromResponse(input)).toBe(input);
  });
});

describe('extractSqlStreaming', () => {
  it('returns empty for empty input', () => {
    expect(extractSqlStreaming('')).toBe('');
  });

  it('extracts SQL from closed ```sql fence', () => {
    const input = 'thinking...\n```sql\nSELECT 1\n```';
    // Streaming mode preserves trailing newline — no trim
    expect(extractSqlStreaming(input)).toBe('SELECT 1\n');
  });

  it('extracts SQL from unclosed ```sql fence', () => {
    const input = 'thinking...\n```sql\nSELECT u.id,\n  u.name\nFROM users u';
    expect(extractSqlStreaming(input)).toBe('SELECT u.id,\n  u.name\nFROM users u');
  });

  it('extracts SQL from unclosed bare fence', () => {
    const input = 'Here is the query:\n```\nSELECT * FROM orders';
    expect(extractSqlStreaming(input)).toBe('SELECT * FROM orders');
  });

  it('handles mixed text with SQL lines (no fence)', () => {
    const input = 'Based on the schema:\nSELECT u.id, u.name\nFROM users u\nWHERE u.active = true';
    const result = extractSqlStreaming(input);
    expect(result).toContain('SELECT');
    expect(result).toContain('FROM');
    expect(result).toContain('WHERE');
  });

  it('does not trim output (preserves streaming state)', () => {
    const input = 'SELECT ';
    expect(extractSqlStreaming(input)).toBe('SELECT ');
  });

  it('handles empty lines inside SQL fence', () => {
    const input = '```sql\nSELECT\n\n  id\nFROM\n  users\n```';
    // Streaming mode preserves trailing content as-is — no trim
    expect(extractSqlStreaming(input)).toBe('SELECT\n\n  id\nFROM\n  users\n');
  });

  it('handles multiple fence attempts (takes first)', () => {
    const input = '```sql\nSELECT 1\n```\n\nSome text\n```sql\nSELECT 2\n```';
    // Streaming mode preserves trailing content — no trim
    expect(extractSqlStreaming(input)).toBe('SELECT 1\n');
  });
});
