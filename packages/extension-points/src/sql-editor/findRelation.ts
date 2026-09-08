import type { EditorMetadataSnapshot, EditorRelationMetadata } from './types';
import type { QualifiedRelationId, SqlDialectAdapter } from './types';
import { relationKey } from './quoteHelper';
import type { SqlSchema } from './contracts';

/**
 * Robust relation metadata resolver from a snapshot with fallback to schema tree.
 * Matches:
 * 1. direct key match
 * 2. dbSessionId::key match
 * 3. case-insensitive table name match
 * 4. fallback synthesized metadata from editor schema tree
 */
export function findRelationMetadata(
  snapshot: EditorMetadataSnapshot | undefined,
  relation: QualifiedRelationId,
  adapter: SqlDialectAdapter,
  schemaTree?: SqlSchema,
): EditorRelationMetadata | undefined {
  const name = relation.name.name;
  const lowerName = name.toLowerCase();

  if (snapshot && snapshot.relations && snapshot.relations.size > 0) {
    const key = relationKey(relation, adapter);
    const direct = snapshot.relations.get(key);
    if (direct) return direct;

    const prefixed = snapshot.relations.get(`${snapshot.dbSessionId}::${key}`);
    if (prefixed) return prefixed;

    for (const [k, rel] of snapshot.relations) {
      const rawKey = k.includes('::') ? k.split('::')[1]! : k;
      if (
        rawKey.toLowerCase() === key.toLowerCase() ||
        rel.identity.name.name.toLowerCase() === lowerName
      ) {
        return rel;
      }
    }
  }

  // Fallback to editor schema tree if available
  if (schemaTree) {
    const cols = extractColumnsFromSchema(schemaTree, name);
    if (cols && cols.length > 0) {
      return {
        key: name,
        identity: relation,
        kind: 'table',
        columns: cols.map((colName, idx) => ({
          name: colName,
          dataType: 'text',
          nullable: true,
          isPrimaryKey: idx === 0 && colName.toLowerCase() === 'id',
        })),
        primaryKey: cols.filter((c) => c.toLowerCase() === 'id'),
        indexes: [],
        foreignKeys: [],
        loadedAt: Date.now(),
      };
    }
  }

  return undefined;
}

export function extractColumnsFromSchema(
  schema: SqlSchema,
  tableName: string,
): readonly string[] | null {
  const lower = tableName.toLowerCase();
  for (const [key, val] of Object.entries(schema)) {
    if (key.toLowerCase() === lower && Array.isArray(val)) {
      return val;
    }
    if (typeof val === 'object' && val !== null && !Array.isArray(val)) {
      const found = extractColumnsFromSchema(val, tableName);
      if (found) return found;
    }
  }
  return null;
}
