import { DB_REGISTRY } from './databaseTypes';
import { isLeaf, type SqlNamespace } from './sqlNamespace';
import type { ConnectionEntry } from '../stores/activeConnectionStore';
import type { ConnectionSchemaState } from '../stores/schemaStore';
import type { ConnectionConfig, DatabaseObject, TableInfo } from '../types';

export type ConnectionMatchReason =
  | 'name'
  | 'host'
  | 'database'
  | 'database-type'
  | 'schema'
  | 'table'
  | 'view'
  | 'object'
  | 'namespace'
  | 'path';

export interface ConnectionMatch {
  /** Lower is a stronger match and is the first ranking key. */
  rank: number;
  reason: ConnectionMatchReason;
  /** The concrete value or path that caused this connection to match. */
  context: string;
}

export interface ConnectionLocatorResult {
  connection: ConnectionConfig;
  match: ConnectionMatch | null;
}

/** Runtime schema/cache inputs used by the connection locator. */
export interface ConnectionLocatorUsageState {
  activeConnections?: Record<string, Pick<ConnectionEntry, 'dbSessionId' | 'status'> | undefined>;
  schemas?: Map<string, ConnectionSchemaState>;
  dbTablesMap?: Record<string, TableInfo[]>;
  dbObjectsMap?: Record<string, DatabaseObject[]>;
}

export const RECENT_CONNECTION_LIMIT = 5;
export const RECENT_GROUP_KEY = '__recent__';
export const PINNED_GROUP_KEY = '__pinned__';

type Candidate = Omit<ConnectionMatch, 'rank'> & { rank: number };

function normalize(value: string | undefined | null): string {
  return value?.trim().toLowerCase() ?? '';
}

function matchCandidate(
  value: string | undefined | null,
  query: string,
  reason: ConnectionMatchReason,
  context = value ?? '',
  fieldOffset: number,
): Candidate | null {
  const normalizedValue = normalize(value);
  if (!normalizedValue || !normalizedValue.includes(query)) return null;

  const rank =
    normalizedValue === query
      ? fieldOffset
      : normalizedValue.startsWith(query)
        ? fieldOffset + 1
        : fieldOffset + 2;
  return { rank, reason, context };
}

function objectReason(item: TableInfo | DatabaseObject): ConnectionMatchReason {
  if ('tableType' in item) {
    return item.tableType === 'view' || item.tableType === 'materializedView' ? 'view' : 'table';
  }
  return 'object';
}

function objectContext(item: TableInfo | DatabaseObject): string {
  const schema = 'schema' in item ? item.schema : undefined;
  return schema ? `${schema}.${item.name}` : item.name;
}

function collectNamespaceCandidates(
  tree: SqlNamespace,
  query: string,
  parentSegments: string[] = [],
): Candidate[] {
  if (isLeaf(tree)) return [];

  const candidates: Candidate[] = [];
  for (const [name, child] of Object.entries(tree)) {
    const segments = [...parentSegments, name];
    const context = segments.join('/');
    const ownMatch = matchCandidate(
      name,
      query,
      isLeaf(child) ? 'table' : 'namespace',
      context,
      60,
    );
    if (ownMatch) candidates.push(ownMatch);
    if (!isLeaf(child)) candidates.push(...collectNamespaceCandidates(child, query, segments));
  }
  return candidates;
}

function connectionCandidates(
  connection: ConnectionConfig,
  query: string,
  usageState: ConnectionLocatorUsageState,
): Candidate[] {
  const candidates: Candidate[] = [];
  const push = (
    value: string | undefined | null,
    reason: ConnectionMatchReason,
    context: string | undefined,
    offset: number,
  ) => {
    const candidate = matchCandidate(value, query, reason, context, offset);
    if (candidate) candidates.push(candidate);
  };

  push(connection.name, 'name', connection.name, 0);
  push(connection.host, 'host', connection.host, 10);
  push(connection.database, 'database', connection.database, 20);
  push(connection.databaseType, 'database-type', connection.databaseType, 30);
  push(
    DB_REGISTRY[connection.databaseType]?.label,
    'database-type',
    DB_REGISTRY[connection.databaseType]?.label,
    30,
  );

  const active = usageState.activeConnections?.[connection.id];
  if (!active || active.status !== 'connected' || !active.dbSessionId) return candidates;

  const schemaData = usageState.schemas?.get(active.dbSessionId);
  if (schemaData) {
    for (const database of schemaData.databases) push(database, 'database', database, 20);
    for (const schema of schemaData.schemaNames) push(schema, 'schema', schema, 40);
    for (const item of [...schemaData.tables, ...schemaData.views]) {
      push(item.name, objectReason(item), objectContext(item), 50);
      if (item.schema) push(item.schema, 'schema', item.schema, 40);
    }
    candidates.push(...collectNamespaceCandidates(schemaData.namespaceTree, query));
    for (const items of Object.values(schemaData.pathItems)) {
      for (const item of items) {
        push(item.name, 'path', objectContext(item), 70);
        if (item.schema) push(item.schema, 'path', item.schema, 70);
      }
    }
  }

  const tablePrefix = `${active.dbSessionId}::`;
  for (const [key, items] of Object.entries(usageState.dbTablesMap ?? {})) {
    if (!key.startsWith(tablePrefix)) continue;
    const database = key.slice(tablePrefix.length);
    push(database, 'database', database, 20);
    for (const item of items) {
      push(item.name, objectReason(item), objectContext(item), 50);
      if (item.schema) push(item.schema, 'schema', item.schema, 40);
    }
  }

  const objectPrefix = `${connection.id}::`;
  for (const [key, items] of Object.entries(usageState.dbObjectsMap ?? {})) {
    if (!key.startsWith(objectPrefix)) continue;
    const contextPrefix = key
      .slice(objectPrefix.length)
      .replace(/::(tables|views|functions|procedures|triggers|sequences|types)$/, '');
    for (const item of items) {
      const context = contextPrefix
        ? `${contextPrefix}.${objectContext(item)}`
        : objectContext(item);
      push(item.name, objectReason(item), context, 50);
      if (item.schema) push(item.schema, 'schema', item.schema, 40);
    }
  }

  return candidates;
}

function bestCandidate(candidates: Candidate[]): ConnectionMatch | null {
  return (
    [...candidates].sort((a, b) => {
      if (a.rank !== b.rank) return a.rank - b.rank;
      if (a.reason !== b.reason) return a.reason.localeCompare(b.reason);
      return a.context.localeCompare(b.context);
    })[0] ?? null
  );
}

function timestamp(value: string | undefined): number | null {
  if (!value) return null;
  const parsed = Date.parse(value);
  return Number.isFinite(parsed) ? parsed : null;
}

function compareStrings(a: string | undefined, b: string | undefined): number {
  return (a ?? '').localeCompare(b ?? '', undefined, { sensitivity: 'base' });
}

function compareResults(a: ConnectionLocatorResult, b: ConnectionLocatorResult): number {
  const aMatchRank = a.match?.rank ?? Number.POSITIVE_INFINITY;
  const bMatchRank = b.match?.rank ?? Number.POSITIVE_INFINITY;
  if (aMatchRank !== bMatchRank) return aMatchRank - bMatchRank;

  const aPinned = a.connection.pinned === true ? 1 : 0;
  const bPinned = b.connection.pinned === true ? 1 : 0;
  if (aPinned !== bPinned) return bPinned - aPinned;

  const aTime = timestamp(a.connection.lastConnectedAt);
  const bTime = timestamp(b.connection.lastConnectedAt);
  if (aTime !== bTime) {
    if (aTime === null) return 1;
    if (bTime === null) return -1;
    return bTime - aTime;
  }

  const groupOrder = compareStrings(a.connection.group, b.connection.group);
  if (groupOrder !== 0) return groupOrder;
  const nameOrder = compareStrings(a.connection.name, b.connection.name);
  if (nameOrder !== 0) return nameOrder;
  return a.connection.id.localeCompare(b.connection.id);
}

/**
 * Search and rank connection configs with one deterministic ordering contract.
 * A non-empty query returns only matching connections; an empty query returns
 * every connection with a null match payload.
 */
export function rankConnections(
  connections: ConnectionConfig[],
  query: string,
  usageState: ConnectionLocatorUsageState = {},
): ConnectionLocatorResult[] {
  const normalizedQuery = normalize(query);
  const uniqueConnections = new Map<string, ConnectionConfig>();
  for (const connection of connections) {
    if (!uniqueConnections.has(connection.id)) uniqueConnections.set(connection.id, connection);
  }
  const results = [...uniqueConnections.values()].map((connection) => {
    const match = normalizedQuery
      ? bestCandidate(connectionCandidates(connection, normalizedQuery, usageState))
      : null;
    return { connection, match };
  });

  return results.filter((result) => !normalizedQuery || result.match !== null).sort(compareResults);
}

/** Preserve the persisted navigator order while keeping pinned items first. */
export function orderConnectionsForDisplay(connections: ConnectionConfig[]): ConnectionConfig[] {
  const pinned = connections.filter((connection) => connection.pinned === true);
  if (pinned.length === 0) return connections;
  return [...pinned, ...connections.filter((connection) => connection.pinned !== true)];
}

/** Group connections while keeping the navigator's non-search hierarchy. */
export function groupConnectionsWithRecentSections(
  connections: ConnectionConfig[],
  groups: string[],
): { group: string; connections: ConnectionConfig[] }[] {
  const grouped = new Map<string, ConnectionConfig[]>();
  for (const group of groups) grouped.set(group, []);
  for (const connection of connections) {
    const group = connection.group ?? '';
    if (!grouped.has(group)) grouped.set(group, []);
    grouped.get(group)?.push(connection);
  }

  const pinned = connections.filter((connection) => connection.pinned === true);
  const pinnedIds = new Set(pinned.map((connection) => connection.id));
  const recent = connections
    .filter(
      (connection) =>
        !pinnedIds.has(connection.id) && timestamp(connection.lastConnectedAt) !== null,
    )
    .sort((a, b) => {
      const aTime = timestamp(a.lastConnectedAt) ?? 0;
      const bTime = timestamp(b.lastConnectedAt) ?? 0;
      if (aTime !== bTime) return bTime - aTime;
      return compareStrings(a.name, b.name) || a.id.localeCompare(b.id);
    })
    .slice(0, RECENT_CONNECTION_LIMIT);
  const recentIds = new Set(recent.map((connection) => connection.id));

  const hasPromotedSections = pinned.length > 0 || recent.length > 0;
  if (!hasPromotedSections) {
    return [...grouped].map(([group, section]) => ({
      group,
      connections: orderConnectionsForDisplay(section),
    }));
  }

  const sections: { group: string; connections: ConnectionConfig[] }[] = [];
  if (pinned.length > 0) {
    sections.push({
      group: PINNED_GROUP_KEY,
      connections: orderConnectionsForDisplay(pinned),
    });
  }
  if (recent.length > 0) sections.push({ group: RECENT_GROUP_KEY, connections: recent });

  for (const group of groups) {
    const section = grouped.get(group) ?? [];
    const visible = section.filter(
      (connection) => !pinnedIds.has(connection.id) && !recentIds.has(connection.id),
    );
    if (visible.length > 0) {
      sections.push({
        group,
        connections: orderConnectionsForDisplay(visible),
      });
    }
  }

  for (const [group, section] of grouped) {
    if (groups.includes(group)) continue;
    const visible = section.filter(
      (connection) => !pinnedIds.has(connection.id) && !recentIds.has(connection.id),
    );
    if (visible.length > 0) {
      sections.push({
        group,
        connections: orderConnectionsForDisplay(visible),
      });
    }
  }
  return sections;
}
