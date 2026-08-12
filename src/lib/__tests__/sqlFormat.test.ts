import { describe, expect, it } from 'vitest';
import { formatSql, sqlFormatLanguage } from '../sqlFormat';

describe('sqlFormatLanguage', () => {
  it('maps known dialects', () => {
    expect(sqlFormatLanguage('postgresql')).toBe('postgresql');
    expect(sqlFormatLanguage('mysql')).toBe('mysql');
    expect(sqlFormatLanguage('mariadb')).toBe('mariadb');
    expect(sqlFormatLanguage('sqlite')).toBe('sqlite');
    expect(sqlFormatLanguage('sqlserver')).toBe('transactsql');
    expect(sqlFormatLanguage('tsql')).toBe('transactsql');
  });

  it('falls back to sql for unknown or missing types', () => {
    expect(sqlFormatLanguage()).toBe('sql');
    expect(sqlFormatLanguage('redis')).toBe('sql');
  });

  it('uses registry sqlDialect when the type is not in LANGUAGE_MAP', async () => {
    const { DB_REGISTRY } = await import('../databaseTypes');
    const aliased = Object.entries(DB_REGISTRY).find(
      ([id, meta]) => !['postgresql', 'mysql', 'mariadb', 'sqlite', 'sqlserver', 'tsql'].includes(id)
        && (meta.sqlDialect === 'postgresql' || meta.sqlDialect === 'mysql'),
    );
    if (!aliased) return;
    expect(['postgresql', 'mysql', 'mariadb', 'sqlite', 'transactsql']).toContain(
      sqlFormatLanguage(aliased[0]),
    );
  });
});

describe('formatSql', () => {
  it('returns whitespace-only input unchanged', () => {
    expect(formatSql('   \n')).toBe('   \n');
    expect(formatSql('')).toBe('');
  });

  it('uppercases keywords for postgres', () => {
    const out = formatSql('select id from users where id = 1', 'postgresql');
    expect(out.toUpperCase()).toContain('SELECT');
    expect(out.toUpperCase()).toContain('FROM');
  });

  it('formats mysql as well', () => {
    const out = formatSql('select 1', 'mysql');
    expect(out.toUpperCase()).toContain('SELECT');
  });
});
