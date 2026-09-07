/**
 * Schema tree drag-and-drop payload contracts.
 *
 * The tree only produces drag payloads; it never generates SQL or quotes identifiers.
 * Drop handling (caret placement, smart quoting) is the editor's responsibility (S5-A / S6-D).
 */

/** MIME types used for dataTransfer */
export const SCHEMA_OBJECT_MIME = 'application/datazen-schema-object' as const;
export const LEGACY_TABLE_MIME = 'application/datazen-table' as const;

/**
 * Versioned drag payload for schema objects (tables, views).
 *
 * Future versions bump the `version` field and add optional properties.
 * Consumers MUST ignore unknown fields for forward-compatibility.
 */
export interface SchemaObjectDragPayloadV1 {
  version: 1;
  /** The kind of schema object being dragged. */
  kind: 'table' | 'view';
  /** Fully-qualified namespace of the object. */
  namespace: {
    database: string;
    schema?: string;
    table: string;
  };
  /** Connection that owns this object. */
  connectionId: string;
  /** Runtime database session id (present only when a session is active). */
  dbSessionId?: string;
  /** Database engine type (e.g. "postgresql", "mysql"). */
  databaseType: string;
}

/**
 * Legacy payload shape expected by the editor drop handler (`DroppedTablePayload`).
 * Kept for backward-compatibility until the editor migrates to the versioned MIME.
 */
export interface LegacyTableDragPayload {
  tables: Array<{ tableName: string; schema?: string }>;
  connectionId: string;
  databaseType: string;
}

export interface DragPayloadOptions {
  kind: 'table' | 'view';
  database: string;
  schema?: string;
  table: string;
  connectionId: string;
  dbSessionId?: string;
  databaseType: string;
}

/**
 * Build the versioned SchemaObjectDragPayloadV1.
 */
export function buildSchemaObjectPayload(opts: DragPayloadOptions): SchemaObjectDragPayloadV1 {
  return {
    version: 1,
    kind: opts.kind,
    namespace: {
      database: opts.database,
      schema: opts.schema,
      table: opts.table,
    },
    connectionId: opts.connectionId,
    dbSessionId: opts.dbSessionId,
    databaseType: opts.databaseType,
  };
}

/**
 * Build the legacy `application/datazen-table` payload that the editor already knows how to
 * consume. This is always emitted alongside the versioned MIME so existing drop handlers
 * continue to work without modification.
 */
export function buildLegacyTablePayload(opts: DragPayloadOptions): LegacyTableDragPayload {
  return {
    tables: [{ tableName: opts.table, schema: opts.schema }],
    connectionId: opts.connectionId,
    databaseType: opts.databaseType,
  };
}

/**
 * Attach both the versioned and legacy MIME types to a DragEvent.
 * Call this inside an `onDragStart` handler.
 */
export function setDragPayload(dataTransfer: DataTransfer, opts: DragPayloadOptions): void {
  const sel = window.getSelection();
  if (sel && !sel.isCollapsed) sel.removeAllRanges();

  const v1 = buildSchemaObjectPayload(opts);
  const legacy = buildLegacyTablePayload(opts);

  dataTransfer.setData(SCHEMA_OBJECT_MIME, JSON.stringify(v1));
  dataTransfer.setData(LEGACY_TABLE_MIME, JSON.stringify(legacy));
  dataTransfer.setData('text/plain', opts.table);
  dataTransfer.effectAllowed = 'copy';
}
