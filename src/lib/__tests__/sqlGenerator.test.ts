import { describe, it, expect } from 'vitest';
import { generateTableSql, formatTableIdentifier } from '../sqlGenerator';
import type { TableSchema } from '../../types';

describe('sqlGenerator', () => {
  const sampleSchema: TableSchema = {
    tableName: 'users',
    primaryKeys: ['id'],
    columns: [
      { name: 'id', dataType: 'bigint', nullable: false, isPrimaryKey: true, isAutoIncrement: true },
      { name: 'name', dataType: 'varchar(255)', nullable: false },
      { name: 'email', dataType: 'varchar(255)', nullable: true },
      { name: 'created_at', dataType: 'timestamp', nullable: false, defaultValue: 'CURRENT_TIMESTAMP' },
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
      const sql = generateTableSql(sampleSchema, 'select', 'postgresql', { schemaPrefix: 'public' });
      expect(sql).toBe('SELECT "id", "name", "email", "created_at"\nFROM "public"."users";');
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

    it('generates placeholder warning in WHERE clause if no primary key exists', () => {
      const noPkSchema: TableSchema = {
        ...sampleSchema,
        primaryKeys: [],
        columns: sampleSchema.columns.map((c) => ({ ...c, isPrimaryKey: false })),
      };
      const sql = generateTableSql(noPkSchema, 'delete', 'postgresql');
      expect(sql).toBe('DELETE FROM "users"\nWHERE /* WARNING: Primary Key not found. Specify condition */;');
    });
  });
});
