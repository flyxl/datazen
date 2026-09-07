import { describe, expect, it } from 'vitest';
import { maskSemicolonsInLiterals, scanSql, splitSqlBySemicolon } from '../scanner';
import { SqlTokenKind } from '../tokens';

describe('scanSql', () => {
  it('tokenizes a simple statement with semicolon delimiter', () => {
    const { tokens } = scanSql('SELECT 1;');
    expect(tokens.some((t) => t.kind === SqlTokenKind.Semicolon)).toBe(true);
    expect(tokens.some((t) => t.text === 'SELECT')).toBe(true);
  });

  it('does not treat semicolons inside single quotes as delimiters', () => {
    const { tokens } = scanSql("SELECT 'a;b';");
    const semicolons = tokens.filter((t) => t.kind === SqlTokenKind.Semicolon);
    expect(semicolons).toHaveLength(1);
    expect(tokens.some((t) => t.kind === SqlTokenKind.SingleQuoted && t.text.includes(';'))).toBe(
      true,
    );
  });

  it('handles escaped single quotes', () => {
    const { tokens } = scanSql("SELECT 'it''s;ok'; SELECT 2");
    const semicolons = tokens.filter((t) => t.kind === SqlTokenKind.Semicolon);
    expect(semicolons).toHaveLength(1);
  });

  it('handles double-quoted identifiers with semicolons', () => {
    const { tokens } = scanSql('SELECT "a;b"; SELECT 2');
    expect(tokens.filter((t) => t.kind === SqlTokenKind.Semicolon)).toHaveLength(1);
  });

  it('handles escaped double quotes', () => {
    const { tokens } = scanSql('SELECT "a""b;c"; SELECT 2');
    expect(tokens.filter((t) => t.kind === SqlTokenKind.Semicolon)).toHaveLength(1);
  });

  it('handles backtick-quoted identifiers', () => {
    const { tokens } = scanSql('SELECT `col;name` FROM t;');
    expect(tokens.some((t) => t.kind === SqlTokenKind.BacktickQuoted)).toBe(true);
    expect(tokens.filter((t) => t.kind === SqlTokenKind.Semicolon)).toHaveLength(1);
  });

  it('handles bracket-quoted identifiers with escaped closing bracket', () => {
    const { tokens } = scanSql('SELECT [a]];b] FROM t;');
    expect(tokens.some((t) => t.kind === SqlTokenKind.BracketQuoted)).toBe(true);
    expect(tokens.filter((t) => t.kind === SqlTokenKind.Semicolon)).toHaveLength(1);
  });

  it('tracks parenthesis depth for nested expressions', () => {
    const { tokens, finalParenDepth } = scanSql('SELECT (1 + (2));');
    const open = tokens.filter((t) => t.kind === SqlTokenKind.OpenParen);
    const close = tokens.filter((t) => t.kind === SqlTokenKind.CloseParen);
    expect(open).toHaveLength(2);
    expect(close).toHaveLength(2);
    expect(finalParenDepth).toBe(0);
    const semi = tokens.find((t) => t.kind === SqlTokenKind.Semicolon)!;
    expect(semi.parenDepth).toBe(0);
  });

  it('ignores semicolons inside parentheses', () => {
    const { tokens } = scanSql('SELECT (1;2); SELECT 3');
    const semicolons = tokens.filter((t) => t.kind === SqlTokenKind.Semicolon);
    expect(semicolons).toHaveLength(2);
    expect(semicolons[0]!.parenDepth).toBe(1);
    expect(semicolons[1]!.parenDepth).toBe(0);
  });

  it('tokenizes line comments (-- and #)', () => {
    const dash = scanSql('-- foo; bar\nSELECT 1');
    expect(dash.tokens.some((t) => t.kind === SqlTokenKind.LineComment)).toBe(true);
    const hash = scanSql('# foo; bar\nSELECT 1');
    expect(hash.tokens.some((t) => t.kind === SqlTokenKind.LineComment)).toBe(true);
  });

  it('tokenizes block comments including nested semicolons', () => {
    const { tokens } = scanSql('SELECT 1; /* a; b */ SELECT 2');
    expect(tokens.some((t) => t.kind === SqlTokenKind.BlockComment && t.text.includes(';'))).toBe(
      true,
    );
  });

  it('tolerates unclosed block comments', () => {
    const { tokens } = scanSql('SELECT 1; /* never closed');
    expect(tokens.some((t) => t.kind === SqlTokenKind.BlockComment)).toBe(true);
  });

  it('tokenizes dollar-quoted strings', () => {
    const plain = scanSql('SELECT $$foo;bar$$; SELECT 2');
    expect(plain.tokens.filter((t) => t.kind === SqlTokenKind.Semicolon)).toHaveLength(1);
    const tagged = scanSql('SELECT $tag$foo;bar$tag$; SELECT 2');
    expect(tagged.tokens.filter((t) => t.kind === SqlTokenKind.Semicolon)).toHaveLength(1);
  });

  it('falls through when dollar-quote tag has no closer', () => {
    const { tokens } = scanSql('SELECT $tag$foo; SELECT 2');
    expect(tokens.filter((t) => t.kind === SqlTokenKind.Semicolon)).toHaveLength(1);
  });

  it('leaves positional $1 without treating it as dollar quote', () => {
    const { tokens } = scanSql('SELECT $1; SELECT 2');
    expect(tokens.some((t) => t.kind === SqlTokenKind.DollarQuoted)).toBe(false);
    expect(tokens.filter((t) => t.kind === SqlTokenKind.Semicolon)).toHaveLength(1);
  });
});

describe('maskSemicolonsInLiterals', () => {
  it('masks semicolons inside comments and strings', () => {
    expect(maskSemicolonsInLiterals("SELECT 'a;b'; /* x; y */")).toBe("SELECT 'a b'; /* x  y */");
    expect(maskSemicolonsInLiterals('SELECT 1; # foo; bar\nSELECT 2')).toBe(
      'SELECT 1; # foo  bar\nSELECT 2',
    );
  });
});

describe('splitSqlBySemicolon', () => {
  it('splits on semicolons outside literals', () => {
    const parts = splitSqlBySemicolon("SELECT 'a;b'; SELECT 2");
    expect(parts).toHaveLength(2);
    expect(parts[0]).toBe("SELECT 'a;b';");
    expect(parts[1]).toBe(' SELECT 2');
  });
});
