import { describe, expect, it, vi } from 'vitest';
import {
  buildSchemaObjectPayload,
  buildLegacyTablePayload,
  setDragPayload,
  SCHEMA_OBJECT_MIME,
  LEGACY_TABLE_MIME,
  type DragPayloadOptions,
} from '../schemaTreeDrag';

const baseOpts: DragPayloadOptions = {
  kind: 'table',
  database: 'testdb',
  schema: 'public',
  table: 'users',
  connectionId: 'conn-1',
  databaseType: 'postgresql',
};

describe('schemaTreeDrag', () => {
  describe('buildSchemaObjectPayload', () => {
    it('creates a V1 payload with all fields', () => {
      const payload = buildSchemaObjectPayload(baseOpts);
      expect(payload).toEqual({
        version: 1,
        kind: 'table',
        namespace: {
          database: 'testdb',
          schema: 'public',
          table: 'users',
        },
        connectionId: 'conn-1',
        databaseType: 'postgresql',
      });
    });

    it('includes dbSessionId when provided', () => {
      const payload = buildSchemaObjectPayload({
        ...baseOpts,
        dbSessionId: 'session-42',
      });
      expect(payload.dbSessionId).toBe('session-42');
    });

    it('omits dbSessionId when not provided', () => {
      const payload = buildSchemaObjectPayload(baseOpts);
      expect(payload.dbSessionId).toBeUndefined();
    });

    it('handles view kind', () => {
      const payload = buildSchemaObjectPayload({
        ...baseOpts,
        kind: 'view',
      });
      expect(payload.kind).toBe('view');
    });

    it('handles undefined schema', () => {
      const payload = buildSchemaObjectPayload({
        ...baseOpts,
        schema: undefined,
      });
      expect(payload.namespace.schema).toBeUndefined();
    });
  });

  describe('buildLegacyTablePayload', () => {
    it('creates legacy payload compatible with DroppedTablePayload', () => {
      const payload = buildLegacyTablePayload(baseOpts);
      expect(payload).toEqual({
        tables: [{ tableName: 'users', schema: 'public' }],
        connectionId: 'conn-1',
        databaseType: 'postgresql',
      });
    });

    it('handles undefined schema', () => {
      const payload = buildLegacyTablePayload({
        ...baseOpts,
        schema: undefined,
      });
      expect(payload.tables[0].schema).toBeUndefined();
    });
  });

  describe('setDragPayload', () => {
    it('sets both MIME types on dataTransfer', () => {
      const dataTransfer = {
        setData: vi.fn(),
        effectAllowed: '' as string,
      } as unknown as DataTransfer;

      setDragPayload(dataTransfer, baseOpts);

      expect(dataTransfer.setData).toHaveBeenCalledTimes(3);
      expect(dataTransfer.setData).toHaveBeenCalledWith(SCHEMA_OBJECT_MIME, expect.any(String));
      expect(dataTransfer.setData).toHaveBeenCalledWith(LEGACY_TABLE_MIME, expect.any(String));
      expect(dataTransfer.setData).toHaveBeenCalledWith('text/plain', 'users');
      expect(dataTransfer.effectAllowed).toBe('copy');
    });

    it('serializes valid JSON for both MIME types', () => {
      const captured = new Map<string, string>();
      const dataTransfer = {
        setData: (type: string, value: string) => {
          captured.set(type, value);
        },
        effectAllowed: '' as string,
      } as unknown as DataTransfer;

      setDragPayload(dataTransfer, baseOpts);

      const v1 = JSON.parse(captured.get(SCHEMA_OBJECT_MIME)!);
      expect(v1.version).toBe(1);
      expect(v1.kind).toBe('table');
      expect(v1.namespace.table).toBe('users');

      const legacy = JSON.parse(captured.get(LEGACY_TABLE_MIME)!);
      expect(legacy.tables[0].tableName).toBe('users');
      expect(legacy.connectionId).toBe('conn-1');
    });
  });
});
