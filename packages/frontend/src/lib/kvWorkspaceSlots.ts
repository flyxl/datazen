/**
 * KV workspace slot resolution.
 *
 * The host may never import a concrete driver component; driver components reach
 * the host only through the codegen registry written by
 * `scripts/resolve-drivers.mjs`. This module is the single place that combines
 * the two gates the host requires before it renders a KV slot:
 *
 *   1. the driver declared the capability (`DatabaseTypeMeta.kvWorkspace`), and
 *   2. this build actually registered a component for that slot.
 *
 * Either gate failing resolves to `undefined`, which the callers treat as
 * "render today's default UI" — never as an error.
 */
import type { ComponentType } from 'react';
import type { KvSlotName } from '@datazen/driver-sdk';
import { getDriverKvSlot } from '../extensions/generated';
import type { DatabaseType } from '../types';
import { DB_REGISTRY } from './databaseTypes';
import { hasKvSlotCapability } from './kvWorkspaceCapabilities';

/**
 * The driver-contributed component for {@link slot}, or `undefined` when the
 * driver lacks the capability or contributed no component in this build.
 */
export function getKvSlotComponent<T extends object>(
  databaseType: DatabaseType | undefined,
  slot: KvSlotName,
): ComponentType<T> | undefined {
  if (!databaseType) return undefined;
  const meta = DB_REGISTRY[databaseType];
  if (!hasKvSlotCapability(meta, slot)) return undefined;
  return getDriverKvSlot(databaseType, slot) as ComponentType<T> | undefined;
}
