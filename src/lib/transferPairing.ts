/**
 * Data Transfer pairing (mirrors `src-tauri/src/transfer/pairing.rs`).
 * Allows SQL Direct + IR; rejects cross-category pairs (SQL↔Redis etc.).
 */

import { normalizeSyncFamily, syncCategory } from './syncPairing';

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
  const srcCat = syncCategory(sourceType);
  const tgtCat = syncCategory(targetType);

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
