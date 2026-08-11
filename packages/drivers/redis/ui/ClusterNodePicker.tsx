import { useCallback, useEffect, useMemo, useState } from 'react';
import { Input } from '../../../../src/components/ui/Input';
import { Select } from '../../../../src/components/ui/Select';
import { useI18n } from '../../../../src/hooks/useI18n';
import { useConnectionStore } from '../../../../src/stores/connectionStore';
import { useSettingsStore } from '../../../../src/stores/settingsStore';
import { redisCommandInvoke } from './redisInvoke';
import { readRedisOptions } from './connectionOptions';
import { readClusterRouting } from './settingsHelpers';

const SESSION_PREFIX = 'datazen:redis:pinned-node:';

interface ClusterNode {
  id?: string;
  addr: string;
  role?: string;
}

interface ClusterNodesResponse {
  nodes: ClusterNode[];
}

export interface ClusterNodePickerProps {
  connectionId: string;
  compact?: boolean;
  value?: string;
  onChange?: (addr: string) => void;
}

export function ClusterNodePicker({
  connectionId,
  compact = false,
  value,
  onChange,
}: ClusterNodePickerProps) {
  const { t } = useI18n();
  const connection = useConnectionStore((s) =>
    s.connections.find((c) => c.id === connectionId),
  );
  const pluginSettings = useSettingsStore((s) => s.settings.pluginSettings);
  const clusterRouting = readClusterRouting(pluginSettings?.redis);

  const topology = readRedisOptions(connection?.options as Record<string, unknown> | undefined)
    .topology;

  const [nodes, setNodes] = useState<ClusterNode[]>([]);
  const [loading, setLoading] = useState(false);
  const [internalPinned, setInternalPinned] = useState(() =>
    readSessionPinnedNode(connectionId, connection?.options),
  );
  const pinnedNodeAddr = value ?? internalPinned;

  const showPicker = clusterRouting === 'pinnedNode' && topology === 'cluster';

  useEffect(() => {
    if (value !== undefined) return;
    setInternalPinned(readSessionPinnedNode(connectionId, connection?.options));
  }, [connection?.options, connectionId, value]);

  useEffect(() => {
    if (!showPicker) return;

    let cancelled = false;
    setLoading(true);
    void redisCommandInvoke<ClusterNodesResponse>('redis', 'cluster_nodes', { connectionId })
      .then((response) => {
        if (cancelled) return;
        setNodes(response.nodes ?? []);
      })
      .catch(() => {
        if (!cancelled) setNodes([]);
      })
      .finally(() => {
        if (!cancelled) setLoading(false);
      });

    return () => {
      cancelled = true;
    };
  }, [connectionId, showPicker]);

  const nodeOptions = useMemo(
    () =>
      nodes.map((node) => ({
        value: node.addr,
        label: node.role ? `${node.addr} (${node.role})` : node.addr,
      })),
    [nodes],
  );

  const persistPinnedNode = useCallback(
    (addr: string) => {
      if (value === undefined) {
        setInternalPinned(addr);
      }
      onChange?.(addr);
      try {
        sessionStorage.setItem(`${SESSION_PREFIX}${connectionId}`, addr);
      } catch {
        // ignore quota / private mode
      }
    },
    [connectionId, onChange, value],
  );

  if (!showPicker) return null;

  return (
    <div className={`flex items-center gap-2 ${compact ? '' : 'border-b border-edge bg-surface-alt px-3 py-1.5'}`}>
      <span className="text-[11px] text-fg-muted">{t('redis.clusterNode')}</span>
      {nodeOptions.length > 0 ? (
        <Select
          value={pinnedNodeAddr}
          options={[
            { value: '', label: t('redis.clusterNodePlaceholder') },
            ...nodeOptions,
          ]}
          onChange={persistPinnedNode}
          className="h-7 min-w-[180px] text-xs"
        />
      ) : (
        <Input
          value={pinnedNodeAddr}
          onChange={(e) => persistPinnedNode(e.target.value)}
          placeholder={loading ? t('common.loading') : '10.0.0.1:7000'}
          className="h-7 w-[180px] font-mono text-xs"
        />
      )}
    </div>
  );
}

function readSessionPinnedNode(
  connectionId: string,
  options: Record<string, unknown> | undefined,
): string {
  try {
    const fromSession = sessionStorage.getItem(`${SESSION_PREFIX}${connectionId}`);
    if (fromSession) return fromSession;
  } catch {
    // ignore
  }
  return readRedisOptions(options).pinnedNodeAddr ?? '';
}

export function readPinnedNodeAddr(connectionId: string): string {
  try {
    return sessionStorage.getItem(`${SESSION_PREFIX}${connectionId}`) ?? '';
  } catch {
    return '';
  }
}
