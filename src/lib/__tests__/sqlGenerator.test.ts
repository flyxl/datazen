import { describe, it, expect } from 'vitest';
import { generateTableSql, formatTableIdentifier } from '../sqlGenerator';
import type { TableSchema } from '../../types';

describe('sqlGenerator', () => {
  const sampleSchema: TableSchema = {
    tableName: 'users',
    primaryKeys: ['id'],
    columns: [
      {
        name: 'id',
        dataType: 'bigint',
        nullable: false,
        isPrimaryKey: true,
        isAutoIncrement: true,
      },
      { name: 'name', dataType: 'varchar(255)', nullable: false },
      { name: 'email', dataType: 'varchar(255)', nullable: true },
      {
        name: 'created_at',
        dataType: 'timestamp',
        nullable: false,
        defaultValue: 'CURRENT_TIMESTAMP',
      },
    ],
    indexes: [],
    foreignKeys: [],
  };

  describe('formatTableIdentifier', () => {
    it('formats unquoted or quoted table identifier based on db dialect', () => {
      expect(formatTableIdentifier('users', 'postgresql')).toBe('"users"');
      expect(formatTableIdentifier('users', 'mysql')).toBe('`users`');
      expect(formatTableIdentifier('users', 'postgresql', 'public')).toBe('"public"."users"');
      expect(formatTableIdentifier('users', 'mysql', 'mydb')).toBe('`mydb`.`users`');
    });
  });

  describe('SELECT generation', () => {
    it('generates SELECT with columns in ordinal order without SELECT *', () => {
      const sql = generateTableSql(sampleSchema, 'select', 'postgresql');
      expect(sql).toBe('SELECT "id", "name", "email", "created_at"\nFROM "users";');
    });

    it('quotes identifiers with schema prefix when provided', () => {
      const sql = generateTableSql(sampleSchema, 'select', 'postgresql', {
        schemaPrefix: 'public',
      });
      expect(sql).toBe('SELECT "id", "name", "email", "created_at"\nFROM "public"."users";');
    });

    it('generates SELECT * when columns array is empty', () => {
      const emptyColsSchema: TableSchema = { ...sampleSchema, columns: [] };
      const sql = generateTableSql(emptyColsSchema, 'select', 'postgresql');
      expect(sql).toBe('SELECT *\nFROM "users";');
    });

    it('double-prefixes schema when tableName already includes schema prefix', () => {
      // Callers MUST override tableName to bare name before calling generateTableSql
      // to avoid this double-prefix. See handleGenerateTableSql for the fix.
      const qualifiedSchema: TableSchema = {
        ...sampleSchema,
        tableName: 'public.users',
      };
      const sql = generateTableSql(qualifiedSchema, 'select', 'postgresql', {
        schemaPrefix: 'public',
      });
      // This documents the raw formatTableRef behavior — callers must not trigger this.
      expect(sql).toBe('SELECT "id", "name", "email", "created_at"\nFROM "public"."public.users";');
    });

    it('uses mysql backticks for mysql dialect', () => {
      const sql = generateTableSql(sampleSchema, 'select', 'mysql');
      expect(sql).toBe('SELECT `id`, `name`, `email`, `created_at`\nFROM `users`;');
    });
  });

  describe('INSERT generation', () => {
    it('excludes auto-increment and identity columns', () => {
      const sql = generateTableSql(sampleSchema, 'insert', 'mysql');
      expect(sql).toContain('INSERT INTO `users`');
      expect(sql).not.toContain('`id`');
      expect(sql).toContain('`name`');
      expect(sql).toContain('`email`');
      expect(sql).toContain('`created_at`');
      expect(sql).toContain('CURRENT_TIMESTAMP');
    });

    it('generates DEFAULT VALUES when columns array is empty', () => {
      const emptyColsSchema: TableSchema = { ...sampleSchema, columns: [] };
      const sql = generateTableSql(emptyColsSchema, 'insert', 'postgresql');
      expect(sql).toBe('INSERT INTO "users"\nDEFAULT VALUES;');
    });

    it('generates DEFAULT VALUES when all columns are auto-increment', () => {
      const allAutoSchema: TableSchema = {
        ...sampleSchema,
        columns: sampleSchema.columns.map((c) => ({ ...c, isAutoIncrement: true })),
      };
      const sql = generateTableSql(allAutoSchema, 'insert', 'postgresql');
      expect(sql).toBe('INSERT INTO "users"\nDEFAULT VALUES;');
    });
  });

  describe('UPDATE generation', () => {
    it('uses primary keys in WHERE clause', () => {
      const sql = generateTableSql(sampleSchema, 'update', 'postgresql');
      expect(sql).toContain('UPDATE "users"\nSET');
      expect(sql).toContain('"name" = \'\'');
      expect(sql).toContain('"email" = \'\'');
      expect(sql).not.toContain('"id" = \'\'');
      expect(sql).toContain('WHERE "id" = ;');
    });

    it('generates placeholder comment when columns array is empty', () => {
      const emptyColsSchema: TableSchema = { ...sampleSchema, columns: [] };
      const sql = generateTableSql(emptyColsSchema, 'update', 'postgresql');
      expect(sql).toBe('/* No column metadata available for UPDATE "users" */');
    });

    it('delegates to base class when columns array is empty (clickhouse)', () => {
      const emptyColsSchema: TableSchema = { ...sampleSchema, columns: [] };
      const sql = generateTableSql(emptyColsSchema, 'update', 'clickhouse');
      expect(sql).toBe('/* No column metadata available for UPDATE `users` */');
    });

    it('generates clickhouse specific ALTER TABLE UPDATE statement', () => {
      const sql = generateTableSql(sampleSchema, 'update', 'clickhouse');
      expect(sql).toContain('ALTER TABLE `users`\nUPDATE');
      expect(sql).toContain("`name` = ''");
      expect(sql).toContain('WHERE `id` = ;');
    });

    it('generates placeholder warning in WHERE clause if no primary key exists', () => {
      const noPkSchema: TableSchema = {
        ...sampleSchema,
        primaryKeys: [],
        columns: sampleSchema.columns.map((c) => ({ ...c, isPrimaryKey: false })),
      };
      const sql = generateTableSql(noPkSchema, 'update', 'postgresql');
      expect(sql).toContain('WHERE /* WARNING: Primary Key not found. Specify condition */');
    });
  });

  describe('DELETE generation', () => {
    it('uses primary keys in WHERE clause', () => {
      const sql = generateTableSql(sampleSchema, 'delete', 'postgresql');
      expect(sql).toBe('DELETE FROM "users"\nWHERE "id" = ;');
    });

    it('generates placeholder comment when columns array is empty', () => {
      const emptyColsSchema: TableSchema = { ...sampleSchema, columns: [] };
      const sql = generateTableSql(emptyColsSchema, 'delete', 'postgresql');
      expect(sql).toBe('/* No column metadata available for DELETE "users" */');
    });

    it('delegates to base class when columns array is empty (clickhouse)', () => {
      const emptyColsSchema: TableSchema = { ...sampleSchema, columns: [] };
      const sql = generateTableSql(emptyColsSchema, 'delete', 'clickhouse');
      expect(sql).toBe('/* No column metadata available for DELETE `users` */');
    });

    it('generates clickhouse specific ALTER TABLE DELETE statement', () => {
      const sql = generateTableSql(sampleSchema, 'delete', 'clickhouse');
      expect(sql).toBe('ALTER TABLE `users`\nDELETE WHERE `id` = ;');
    });

    it('generates placeholder warning in WHERE clause if no primary key exists', () => {
      const noPkSchema: TableSchema = {
        ...sampleSchema,
        primaryKeys: [],
        columns: sampleSchema.columns.map((c) => ({ ...c, isPrimaryKey: false })),
      };
      const sql = generateTableSql(noPkSchema, 'delete', 'postgresql');
      expect(sql).toBe(
        'DELETE FROM "users"\nWHERE /* WARNING: Primary Key not found. Specify condition */;',
      );
    });
  });

  describe('Dialect strategy extensibility and constraints', () => {
    it('supports SELECT but throws on INSERT/UPDATE/DELETE for Elasticsearch SQL', () => {
      const selectSql = generateTableSql(sampleSchema, 'select', 'elasticsearch');
      expect(selectSql).toContain('SELECT "id", "name", "email", "created_at"\nFROM "users";');

      expect(() => generateTableSql(sampleSchema, 'insert', 'elasticsearch')).toThrowError(
        'Elasticsearch SQL does not support INSERT statements',
      );
      expect(() => generateTableSql(sampleSchema, 'update', 'elasticsearch')).toThrowError(
        'Elasticsearch SQL does not support UPDATE statements',
      );
      expect(() => generateTableSql(sampleSchema, 'delete', 'elasticsearch')).toThrowError(
        'Elasticsearch SQL does not support DELETE statements',
      );
    });

    it('falls back gracefully to standard ANSI generator for unknown or generic dialects', () => {
      const sql = generateTableSql(sampleSchema, 'select', 'custom_db');
      expect(sql).toContain('SELECT "id", "name", "email", "created_at"\nFROM "users";');
    });
  });
});
