import type { DatabaseObject } from '../types';

/** Stable identity for objects that can share a display name. */
export function databaseObjectIdentityKey(object: DatabaseObject): string {
  return JSON.stringify([
    object.kind,
    object.schema ?? null,
    object.name,
    object.signature ?? null,
    object.targetSchema ?? null,
    object.targetName ?? null,
  ]);
}
