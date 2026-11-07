import { useCallback, useMemo } from 'react';
import type {
  QueryBuilderCatalog,
  QueryBuilderContribution,
  QueryBuilderPanelProps,
  QueryBuilderRelationCandidate,
  TableSchema,
} from '@datazen/extension-points';
import type { TableSchema as HostTableSchema } from '../../../types';
import { DB_REGISTRY } from '../../../lib/databaseTypes';
import { formatSql } from '../../../lib/sqlFormat';
import { getCachedTableSchema } from '../../../lib/schemaCache';
import { toPredictionTablesFromSchemas } from '../../../lib/relationPrediction/fromTableSchema';
import { predictRelations } from '../../../lib/relationPrediction/predictRelations';
import { useSchemaStore } from '../../../stores/schemaStore';
import { useSettingsStore } from '../../../stores/settingsStore';

export interface QueryBuilderHostAdapterProps {
  contribution: QueryBuilderContribution;
  panelId: string;
  connectionId: string;
  dbSessionId: string;
  databaseType: string;
  database: string;
  schema: string | null;
  currentSql: string;
  onCommit: QueryBuilderPanelProps['onCommit'];
  onCancel: () => void;
}

/** Supplies the active query tab's schema and host-owned services to Pro. */
export function QueryBuilderHostAdapter({
  contribution,
  panelId,
  connectionId,
  dbSessionId,
  databaseType,
  database,
  schema,
  currentSql,
  onCommit,
  onCancel,
}: QueryBuilderHostAdapterProps) {
  const sessionSchema = useSchemaStore((state) => state.schemas.get(dbSessionId));
  const sqlFormatOptions = useSettingsStore((state) => state.settings.sqlFormatOptions);
  const enableFkPrediction = useSettingsStore(
    (state) => state.settings.enableFkPrediction ?? false,
  );

  const catalog = useMemo<QueryBuilderCatalog>(() => {
    const catalogItems =
      sessionSchema?.tableCatalogs[database] ??
      (sessionSchema?.currentDatabase === database
        ? [...sessionSchema.tables, ...sessionSchema.views]
        : []);
    const relations = [
      ...catalogItems,
      ...Object.values(
        sessionSchema?.currentDatabase === database ? sessionSchema.pathItems : {},
      ).flat(),
    ].filter((relation) => !schema || relation.schema === schema);
    const unique = new Map<string, (typeof relations)[number]>();
    for (const relation of relations) {
      unique.set(JSON.stringify([relation.schema ?? null, relation.name]), relation);
    }
    const counts = new Map<string, number>();
    for (const relation of unique.values())
      counts.set(relation.name, (counts.get(relation.name) ?? 0) + 1);
    const columnMap: Record<string, string[]> = {};
    const typedColumnMap: Record<string, Record<string, string>> = {};
    for (const value of Object.values(sessionSchema?.relationColumns ?? {})) {
      if (
        value.ref.database !== database ||
        (schema && value.ref.schema !== schema) ||
        counts.get(value.ref.name) !== 1 ||
        !unique.has(JSON.stringify([value.ref.schema, value.ref.name]))
      )
        continue;
      columnMap[value.ref.name] = value.columns.map((column) => column.name);
      typedColumnMap[value.ref.name] = Object.fromEntries(
        value.columns.map((column) => [column.name, column.dataType]),
      );
    }
    return {
      tables: [...unique.values()]
        .filter((relation) => counts.get(relation.name) === 1)
        .map((relation) => ({ name: relation.name, schema: relation.schema ?? null })),
      columnMap,
      typedColumnMap,
    };
  }, [sessionSchema, schema, database]);

  const ensureColumns = useCallback(
    async (names: readonly string[]) => {
      if (!dbSessionId || !database.trim() || names.length === 0) return;
      await useSchemaStore.getState().ensureColumns([...names], dbSessionId, database, {
        ...(schema ? { schema } : {}),
      });
    },
    [dbSessionId, database, schema],
  );

  const loadTableSchema = useCallback(
    async (name: string): Promise<TableSchema | null> => {
      if (!dbSessionId || !database.trim() || !name.trim()) return null;
      try {
        const relation = catalog.tables.find((item) => item.name === name);
        if (!relation) return null;
        return await getCachedTableSchema(dbSessionId, relation.name, database, relation.schema);
      } catch {
        return null;
      }
    },
    [dbSessionId, database, catalog.tables],
  );

  const predict = useCallback(
    (schemas: readonly TableSchema[]): readonly QueryBuilderRelationCandidate[] => {
      const hostSchemas: HostTableSchema[] = schemas.map((item) => ({
        ...item,
        columns: item.columns.map((column) => ({
          ...column,
          comment: column.comment ?? undefined,
        })),
      }));
      return predictRelations(toPredictionTablesFromSchemas(hostSchemas)).map((candidate) => ({
        id: candidate.id,
        fromTable: candidate.fromTable,
        toTable: candidate.toTable,
        columnPairs: candidate.columnPairs,
        tier: candidate.tier,
        ambiguous: candidate.ambiguous,
      }));
    },
    [],
  );

  const format = useCallback(
    (sql: string) => formatSql(sql, databaseType, sqlFormatOptions),
    [databaseType, sqlFormatOptions],
  );

  const Panel = contribution.Panel;
  return (
    <Panel
      panelId={panelId}
      connectionId={connectionId}
      dbSessionId={dbSessionId}
      databaseType={databaseType}
      database={database}
      schema={schema}
      currentSql={currentSql}
      catalog={catalog}
      dialectFamily={
        DB_REGISTRY[databaseType as keyof typeof DB_REGISTRY]?.sqlDialect ?? databaseType
      }
      enableFkPrediction={enableFkPrediction}
      ensureColumns={ensureColumns}
      loadTableSchema={loadTableSchema}
      predictRelations={predict}
      formatSql={format}
      onCommit={onCommit}
      onCancel={onCancel}
    />
  );
}
