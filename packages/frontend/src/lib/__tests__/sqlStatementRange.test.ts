import { describe, it, expect } from 'vitest';
import { getStatementAtCursor } from '../sqlStatementRange';

describe('getStatementAtCursor', () => {
  it('returns empty string for empty SQL', () => {
    expect(getStatementAtCursor('', 0)).toBe('');
    expect(getStatementAtCursor('   \n  ', 2)).toBe('');
  });

  it('returns the single statement when only one statement exists', () => {
    const sql = 'SELECT * FROM users;';
    expect(getStatementAtCursor(sql, 5)).toBe('SELECT * FROM users;');
    expect(getStatementAtCursor(sql, 0)).toBe('SELECT * FROM users;');
    expect(getStatementAtCursor(sql, sql.length)).toBe('SELECT * FROM users;');
  });

  it('returns statement without trailing semicolon', () => {
    const sql = 'SELECT 1';
    expect(getStatementAtCursor(sql, 3)).toBe('SELECT 1');
  });

  it('selects correct statement among multiple statements based on cursor position', () => {
    const sql = 'SELECT id FROM users;\n\nSELECT name FROM orders;\n\nDELETE FROM logs;';
    // Position within first statement
    expect(getStatementAtCursor(sql, 5)).toBe('SELECT id FROM users;');
    // Position within second statement
    const secondPos = sql.indexOf('orders');
    expect(getStatementAtCursor(sql, secondPos)).toBe('SELECT name FROM orders;');
    // Position within third statement
    const thirdPos = sql.indexOf('logs');
    expect(getStatementAtCursor(sql, thirdPos)).toBe('DELETE FROM logs;');
  });

  it('ignores semicolons inside single or double quoted strings', () => {
    const sql = "SELECT 'hello; world' AS greeting;\nSELECT \"col;name\" FROM tbl;";
    expect(getStatementAtCursor(sql, 10)).toBe("SELECT 'hello; world' AS greeting;");
    const secondPos = sql.indexOf('col;name');
    expect(getStatementAtCursor(sql, secondPos)).toBe('SELECT "col;name" FROM tbl;');
  });

  it('ignores semicolons inside line and block comments', () => {
    const sql = '-- comment ; with semicolon\nSELECT 1;\n/* block ; comment */\nSELECT 2;';
    expect(getStatementAtCursor(sql, 5)).toBe('-- comment ; with semicolon\nSELECT 1;');
    const secondPos = sql.indexOf('SELECT 2');
    expect(getStatementAtCursor(sql, secondPos)).toBe('/* block ; comment */\nSELECT 2;');
  });
});
