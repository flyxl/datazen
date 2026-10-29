/** Pairings covered by live end-to-end migration runs for this release. */
const verifiedTypes = new Set(['postgresql', 'mysql']);

export function isVerifiedMigrationPair(sourceType: string, targetType: string): boolean {
  return verifiedTypes.has(sourceType.toLowerCase()) && verifiedTypes.has(targetType.toLowerCase());
}
