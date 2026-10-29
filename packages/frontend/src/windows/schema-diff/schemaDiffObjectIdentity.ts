import type { SchemaDiffObjectIdentity } from '../../commands/schemaDiff';

/** Stable identity key that preserves overloads and trigger attachment targets. */
export function schemaDiffObjectIdentityKey(object: SchemaDiffObjectIdentity): string {
  return JSON.stringify([
    object.kind,
    object.schema ?? null,
    object.name,
    object.signature ?? null,
    object.targetSchema ?? null,
    object.targetName ?? null,
  ]);
}
