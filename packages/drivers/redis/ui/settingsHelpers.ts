export type ClusterRouting = 'auto' | 'pinnedNode';

/** Read Redis plugin settings.clusterRouting (default auto). */
export function readClusterRouting(raw: unknown): ClusterRouting {
  if (!raw || typeof raw !== 'object') return 'auto';
  const value = (raw as { clusterRouting?: unknown }).clusterRouting;
  return value === 'pinnedNode' ? 'pinnedNode' : 'auto';
}

export function resolvePinnedNodeAddr(
  clusterRouting: ClusterRouting,
  pinnedNodeAddr?: string,
): string | null {
  if (clusterRouting !== 'pinnedNode') return null;
  const addr = pinnedNodeAddr?.trim();
  return addr ? addr : null;
}
