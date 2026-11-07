/**
 * Data Transfer pairing (mirrors `src-tauri/src/transfer/pairing.rs`).
 * Allows SQL Direct + IR; rejects cross-category pairs (SQL↔Redis etc.).
 */

import { normalizeSyncFamily, syncCategory } from './syncPairing';
import { normalizeDriverId } from './syncTaxonomy';

export type TransferPath = 'direct' | 'ir' | 'unsupported';

export interface TransferPairingResult {
  path: TransferPath;
  supported: boolean;
  family?: string;
  reason?: string;
}

export function resolveTransferPairing(
  sourceType: string,
  targetType: string,
): TransferPairingResult {
  // Optional drivers may be absent from generated UI metadata in a basic build.
  // Keep their known taxonomy stable in that case.
  const transferCategory = (databaseType: string) => {
    const id = normalizeDriverId(databaseType);
    if (id === 'redis') return 'kv';
    if (id === 'mongodb') return 'document';
    return syncCategory(databaseType);
  };
  const srcCat = transferCategory(sourceType);
  const tgtCat = transferCategory(targetType);

  if (srcCat !== tgtCat) {
    return {
      path: 'unsupported',
      supported: false,
      reason: `Transfer between ${sourceType} (${srcCat}) and ${targetType} (${tgtCat}) is not supported`,
    };
  }

  if (srcCat === 'other') {
    return {
      path: 'unsupported',
      supported: false,
      reason: `Transfer is not supported for database type '${sourceType}'`,
    };
  }

  // Redis currently has no Data Transfer row adapters. Its Data Sync pairing
  // policy must not imply an executable Data Transfer path.
  if (normalizeDriverId(sourceType) === 'redis' || normalizeDriverId(targetType) === 'redis') {
    return {
      path: 'unsupported',
      supported: false,
      reason: 'Data Transfer has no Redis source and target adapters',
    };
  }

  const srcFamily = normalizeSyncFamily(sourceType);
  const tgtFamily = normalizeSyncFamily(targetType);

  if (srcFamily === tgtFamily) {
    return { path: 'direct', supported: true, family: srcFamily };
  }

  if (srcCat === 'sql') {
    return { path: 'ir', supported: true };
  }

  return {
    path: 'unsupported',
    supported: false,
    reason: `Transfer between ${sourceType} and ${targetType} is not supported`,
  };
}

export function isTransferTargetSupported(sourceType: string, targetType: string): boolean {
  return resolveTransferPairing(sourceType, targetType).supported;
}
