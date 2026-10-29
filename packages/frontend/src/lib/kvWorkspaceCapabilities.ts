/**
 * KV workspace capability reader.
 *
 * Pure metadata lookup — no registry, no codegen import — so it stays testable
 * without a driver build. The capability is the *only* thing the host reads to
 * decide whether a KV surface exists; the component itself is resolved separately
 * by `kvWorkspaceSlots.ts`, and both must succeed before the host renders a slot.
 */
import type { DatabaseTypeMeta, KvWorkspaceCapabilities } from '@datazen/extension-points';
import type { KvSlotName } from '@datazen/driver-sdk';

/** Every KV slot name, in render order. Iterated by tests and the generator docs. */
export const KV_SLOT_NAMES: readonly KvSlotName[] = [
  'contextBar',
  'statusBar',
  'keyPropsSidebar',
  'connectionHome',
];

/**
 * Map a slot to its capability flag. `connectionHome` is driven by `home`,
 * matching the wording of the PRD's 屏 A ("connection home")裁定.
 */
function capabilityKeyForSlot(slot: KvSlotName): keyof KvWorkspaceCapabilities {
  return slot === 'connectionHome' ? 'home' : slot;
}

/**
 * Whether {@link meta}'s driver claims it can fill {@link slot}.
 *
 * Missing meta (unknown / not-yet-registered driver type) and missing
 * `kvWorkspace` both answer `false`, which is what keeps every non-KV driver
 * (mysql, postgresql, mongodb, …) on the pre-track rendering path.
 */
export function hasKvSlotCapability(meta: DatabaseTypeMeta | undefined, slot: KvSlotName): boolean {
  const capabilities = meta?.kvWorkspace;
  if (!capabilities) return false;
  return capabilities[capabilityKeyForSlot(slot)] === true;
}
