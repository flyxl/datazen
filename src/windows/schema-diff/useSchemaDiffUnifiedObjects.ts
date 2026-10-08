import { useCallback, useMemo, useRef, useState } from 'react';
import { databaseCommands } from '../../commands/database';
import type { SchemaDiffObjectIdentity, SchemaDiffObjectKind } from '../../commands/schemaDiff';
import type { SchemaDiffObjectLoadErrors } from './SchemaDiffUnifiedObjectsPicker';
import { schemaDiffObjectIdentityKey } from './schemaDiffObjectIdentity';
import type { DatabaseObject } from '../../types';

export interface SchemaDiffObjectCatalog {
  sourceObjects: SchemaDiffObjectIdentity[];
  targetObjects: SchemaDiffObjectIdentity[];
}

export interface SchemaDiffObjectSelectionRestore {
  missingSource: number;
  missingTarget: number;
}

const OBJECT_KINDS: SchemaDiffObjectKind[] = [
  'view',
  'type',
  'sequence',
  'function',
  'procedure',
  'trigger',
];

function normalizeObject(
  object: DatabaseObject,
  kind: SchemaDiffObjectKind,
): SchemaDiffObjectIdentity {
  return {
    kind,
    schema: object.schema ?? null,
    name: object.name,
    signature: object.signature ?? null,
    targetSchema: object.targetSchema ?? null,
    targetName: object.targetName ?? null,
  };
}

function filterSchema(
  objects: SchemaDiffObjectIdentity[],
  schema: string,
): SchemaDiffObjectIdentity[] {
  if (!schema) return objects;
  return objects.filter((object) => !object.schema || object.schema === schema);
}

async function listObjects(
  dbSessionId: string,
  database: string,
  schema: string,
): Promise<{ objects: SchemaDiffObjectIdentity[]; errors: SchemaDiffObjectLoadErrors['source'] }> {
  const objects: SchemaDiffObjectIdentity[] = [];
  const errors: SchemaDiffObjectLoadErrors['source'] = {};
  // Keep one catalog query in flight per session; some drivers share a single
  // connection for catalog reads and do not permit overlapping commands.
  for (const kind of OBJECT_KINDS) {
    try {
      const rows = await databaseCommands.getDatabaseObjects(dbSessionId, kind, database);
      objects.push(...rows.map((object) => normalizeObject(object, kind)));
    } catch (error) {
      errors[kind] = error instanceof Error ? error.message : String(error);
    }
  }
  return { objects: filterSchema(objects, schema), errors };
}

export function useSchemaDiffUnifiedObjects() {
  const [sourceObjects, setSourceObjects] = useState<SchemaDiffObjectIdentity[]>([]);
  const [targetObjects, setTargetObjects] = useState<SchemaDiffObjectIdentity[]>([]);
  const [selectedSourceKeys, setSelectedSourceKeys] = useState<string[]>([]);
  const [selectedTargetKeys, setSelectedTargetKeys] = useState<string[]>([]);
  const [errors, setErrors] = useState<SchemaDiffObjectLoadErrors>({ source: {}, target: {} });
  const requestIdRef = useRef(0);

  const selectedSourceSet = useMemo(() => new Set(selectedSourceKeys), [selectedSourceKeys]);
  const selectedTargetSet = useMemo(() => new Set(selectedTargetKeys), [selectedTargetKeys]);
  const selectedSourceObjects = useMemo(
    () =>
      sourceObjects.filter((object) => selectedSourceSet.has(schemaDiffObjectIdentityKey(object))),
    [selectedSourceSet, sourceObjects],
  );
  const selectedTargetObjects = useMemo(
    () =>
      targetObjects.filter((object) => selectedTargetSet.has(schemaDiffObjectIdentityKey(object))),
    [selectedTargetSet, targetObjects],
  );

  const clear = useCallback(() => {
    requestIdRef.current += 1;
    setSourceObjects([]);
    setTargetObjects([]);
    setSelectedSourceKeys([]);
    setSelectedTargetKeys([]);
    setErrors({ source: {}, target: {} });
  }, []);

  const load = useCallback(
    async (
      sourceDbSessionId: string,
      targetDbSessionId: string,
      sourceDatabase: string,
      targetDatabase: string,
      sourceSchema: string,
      targetSchema: string,
    ): Promise<SchemaDiffObjectCatalog | null> => {
      const requestId = ++requestIdRef.current;
      const [source, target] = await Promise.all([
        listObjects(sourceDbSessionId, sourceDatabase, sourceSchema),
        listObjects(targetDbSessionId, targetDatabase, targetSchema),
      ]);
      if (requestId !== requestIdRef.current) return null;

      setSourceObjects(source.objects);
      setTargetObjects(target.objects);
      // Newly discovered schema objects are opt-in. This preserves the
      // existing table-only migration scope and prevents surprise DDL.
      setSelectedSourceKeys([]);
      setSelectedTargetKeys([]);
      setErrors({ source: source.errors, target: target.errors });
      return { sourceObjects: source.objects, targetObjects: target.objects };
    },
    [],
  );

  const restoreSelection = useCallback(
    (
      catalog: SchemaDiffObjectCatalog,
      desiredSourceObjects: SchemaDiffObjectIdentity[],
      desiredTargetObjects: SchemaDiffObjectIdentity[],
    ): SchemaDiffObjectSelectionRestore => {
      const sourceKeys = new Set(catalog.sourceObjects.map(schemaDiffObjectIdentityKey));
      const targetKeys = new Set(catalog.targetObjects.map(schemaDiffObjectIdentityKey));
      const selectedSourceKeys = desiredSourceObjects
        .map(schemaDiffObjectIdentityKey)
        .filter((key) => sourceKeys.has(key));
      const selectedTargetKeys = desiredTargetObjects
        .map(schemaDiffObjectIdentityKey)
        .filter((key) => targetKeys.has(key));
      setSelectedSourceKeys([...new Set(selectedSourceKeys)]);
      setSelectedTargetKeys([...new Set(selectedTargetKeys)]);
      return {
        missingSource: desiredSourceObjects.length - selectedSourceKeys.length,
        missingTarget: desiredTargetObjects.length - selectedTargetKeys.length,
      };
    },
    [],
  );

  const toggle = useCallback((side: 'source' | 'target', object: SchemaDiffObjectIdentity) => {
    const key = schemaDiffObjectIdentityKey(object);
    const update = side === 'source' ? setSelectedSourceKeys : setSelectedTargetKeys;
    update((previous) => {
      const next = new Set(previous);
      if (next.has(key)) next.delete(key);
      else next.add(key);
      return [...next];
    });
  }, []);

  const selectAll = useCallback(
    (side: 'source' | 'target', selected: boolean) => {
      const objects = side === 'source' ? sourceObjects : targetObjects;
      const update = side === 'source' ? setSelectedSourceKeys : setSelectedTargetKeys;
      update(selected ? objects.map(schemaDiffObjectIdentityKey) : []);
    },
    [sourceObjects, targetObjects],
  );

  return {
    sourceObjects,
    targetObjects,
    selectedSourceObjects,
    selectedTargetObjects,
    selectedSourceKeys,
    selectedTargetKeys,
    errors,
    clear,
    load,
    restoreSelection,
    toggle,
    selectAll,
  };
}
