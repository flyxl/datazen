import { describe, expect, it } from 'vitest';
import {
  isSchemaChangingStatement,
  isSchemaMutatingStatement,
  sqlContainsSchemaChangingDdl,
  sqlMayMutateSchema,
  isDataModifyingStatement,
  sqlContainsDataModifying,
} from '../schemaChangingSql';

describe('schemaChangingSql', () => {
  it('detects create/drop database, schema, and table', () => {
    expect(isSchemaChangingStatement('CREATE DATABASE foo')).toBe(true);
    expect(isSchemaChangingStatement('DROP DATABASE foo')).toBe(true);
    expect(isSchemaChangingStatement('CREATE SCHEMA audit')).toBe(true);
    expect(isSchemaChangingStatement('DROP SCHEMA audit CASCADE')).toBe(true);
    expect(isSchemaChangingStatement('CREATE TABLE users (id int)')).toBe(true);
    expect(isSchemaChangingStatement('DROP TABLE users')).toBe(true);
  });

  it('supports common DDL variants', () => {
    expect(isSchemaChangingStatement('CREATE DATABASE IF NOT EXISTS app')).toBe(true);
    expect(isSchemaChangingStatement('CREATE TEMPORARY TABLE staging (id int)')).toBe(true);
    expect(isSchemaChangingStatement('CREATE OR REPLACE TABLE t (id int)')).toBe(true);
  });

  it('ignores non-schema-changing statements', () => {
    expect(isSchemaChangingStatement('SELECT 1')).toBe(false);
    expect(isSchemaChangingStatement('INSERT INTO users VALUES (1)')).toBe(false);
    expect(isSchemaChangingStatement('CREATE INDEX idx ON users (id)')).toBe(false);
    expect(isSchemaChangingStatement('CREATE VIEW v AS SELECT 1')).toBe(false);
    expect(isSchemaChangingStatement('ALTER TABLE users ADD COLUMN x int')).toBe(false);
  });

  it('checks any statement in a script', () => {
    expect(sqlContainsSchemaChangingDdl('SELECT 1; CREATE DATABASE app;')).toBe(true);
    expect(sqlContainsSchemaChangingDdl("SELECT 'DROP TABLE x';")).toBe(false);
    expect(sqlContainsSchemaChangingDdl('-- DROP DATABASE x\nSELECT 1')).toBe(false);
  });

  it('detects schema mutating statements (including ALTER, TRUNCATE, RENAME)', () => {
    expect(isSchemaMutatingStatement('ALTER TABLE users ADD COLUMN x int')).toBe(true);
    expect(isSchemaMutatingStatement('TRUNCATE TABLE users')).toBe(true);
    expect(isSchemaMutatingStatement('RENAME TABLE a TO b')).toBe(true);
    expect(isSchemaMutatingStatement("COMMENT ON TABLE users IS 'app users'")).toBe(true);
    expect(isSchemaMutatingStatement('SELECT 1')).toBe(false);
    expect(sqlMayMutateSchema('SELECT 1; ALTER TABLE users ADD COLUMN y text;')).toBe(true);
  });

  it('detects data-modifying statements (DML + TRUNCATE)', () => {
    expect(isDataModifyingStatement("UPDATE users SET name = 'x' WHERE id = 1")).toBe(true);
    expect(isDataModifyingStatement('INSERT INTO users VALUES (1)')).toBe(true);
    expect(isDataModifyingStatement('DELETE FROM users WHERE id = 1')).toBe(true);
    expect(isDataModifyingStatement('REPLACE INTO users VALUES (1)')).toBe(true);
    expect(isDataModifyingStatement('MERGE INTO users USING s ON 1=1')).toBe(true);
    expect(isDataModifyingStatement('TRUNCATE TABLE users')).toBe(true);
    expect(isDataModifyingStatement('SELECT * FROM users')).toBe(false);
    expect(isDataModifyingStatement('CREATE TABLE users (id int)')).toBe(false);
    expect(sqlContainsDataModifying("SELECT 1; UPDATE users SET name = 'x';")).toBe(true);
    expect(sqlContainsDataModifying("SELECT 'UPDATE x'; SELECT 1")).toBe(false);
    expect(sqlContainsDataModifying('SELECT 1')).toBe(false);
  });
});
