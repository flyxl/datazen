import { describe, expect, it, beforeEach } from 'vitest';
import {
  capabilitiesForDbSession,
  hasSchemaLevel,
  relationSchemaFor,
  supportsOffset,
} from '../driverCapabilities';
import { useActiveConnectionStore } from '../../stores/activeConnectionStore';
import type { DriverCapabilities } from '../../types';

const SCHEMA_AWARE: DriverCapabilities = {
  supportsCancelQuery: true,
  supportsQueryExecutionCancel: true,
  supportsExplain: true,
  supportsStreamingResults: true,
  supportsOffset: true,
  hasSchemaLevel: true,
};

const SCHEMA_LESS: DriverCapabilities = { ...SCHEMA_AWARE, hasSchemaLevel: false };

describe('driverCapabilities', () => {
  beforeEach(() => {
    useActiveConnectionStore.setState({ connections: {} });
  });

  it('reports unknown for a session whose info has not loaded', () => {
    expect(hasSchemaLevel(undefined)).toBe('unknown');
    expect(hasSchemaLevel(null)).toBe('unknown');
    expect(supportsOffset(undefined)).toBe('unknown');
  });

  it('reads the runtime flags from the driver', () => {
    expect(hasSchemaLevel(SCHEMA_AWARE)).toBe(true);
    expect(hasSchemaLevel(SCHEMA_LESS)).toBe(false);
    // OFFSET is opt-out: only an explicit false turns it off.
    expect(supportsOffset({ ...SCHEMA_AWARE, supportsOffset: false })).toBe(false);
    expect(supportsOffset(SCHEMA_AWARE)).toBe(true);
  });

  it('finds capabilities by db session id, not by connection id', () => {
    useActiveConnectionStore.setState({
      connections: {
        'conn-1': {
          connectionId: 'conn-1',
          dbSessionId: 'sess-1',
          status: 'connected',
          serverInfo: null,
          capabilities: SCHEMA_AWARE,
          currentDatabase: 'app',
          error: null,
        },
      },
    });
    expect(capabilitiesForDbSession('sess-1')).toBe(SCHEMA_AWARE);
    expect(capabilitiesForDbSession('conn-1')).toBeUndefined();
    expect(capabilitiesForDbSession(null)).toBeUndefined();
  });

  describe('relationSchemaFor', () => {
    it('keeps the schema for a schema-aware engine', () => {
      expect(relationSchemaFor(SCHEMA_AWARE, 'public')).toBe('public');
    });

    it('keeps the schema while the capability is unknown', () => {
      // Fail-safe: dropping it here would silently read the wrong namespace on
      // a schema-aware engine.
      expect(relationSchemaFor(undefined, 'sales')).toBe('sales');
    });

    it('drops a namespace label on a schema-less engine', () => {
      // Path-hierarchy trees put catalog names in `TableInfo.schema`; such a
      // driver rejects any schema argument.
      expect(relationSchemaFor(SCHEMA_LESS, 'CATALOG')).toBeNull();
      expect(relationSchemaFor(SCHEMA_LESS, 'public')).toBeNull();
    });

    it('treats blank and missing schemas as absent', () => {
      expect(relationSchemaFor(SCHEMA_AWARE, '   ')).toBeNull();
      expect(relationSchemaFor(SCHEMA_AWARE, null)).toBeNull();
      expect(relationSchemaFor(SCHEMA_AWARE, undefined)).toBeNull();
      expect(relationSchemaFor(SCHEMA_AWARE, '  public  ')).toBe('public');
    });
  });
});
