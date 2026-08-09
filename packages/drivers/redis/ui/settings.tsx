import { Select } from '@datazen/plugin-sdk';

export const redisSettingsSchema = {
  type: 'object',
  properties: {
    allowFlush: {
      type: 'boolean',
      title: 'Allow FLUSHDB / FLUSHALL',
      description: 'Dangerous. Off by default.',
      default: false,
    },
    clusterRouting: {
      type: 'string',
      title: 'Cluster routing',
      description: 'How Redis Cluster connections route commands.',
      enum: ['auto', 'pinnedNode'],
      default: 'auto',
    },
  },
} as const;

type RedisPluginSettings = {
  allowFlush?: boolean;
  clusterRouting?: 'auto' | 'pinnedNode';
};

export function RedisSettingsSection({
  value,
  onChange,
}: {
  value: unknown;
  onChange: (next: unknown) => void;
}) {
  const v = (value && typeof value === 'object' ? value : {}) as RedisPluginSettings;
  const clusterRouting = v.clusterRouting === 'pinnedNode' ? 'pinnedNode' : 'auto';

  return (
    <div className="space-y-3 text-sm">
      <label className="flex items-start gap-2">
        <input
          type="checkbox"
          checked={v.allowFlush === true}
          onChange={(e) => onChange({ ...v, allowFlush: e.target.checked })}
        />
        <span>Allow FLUSHDB / FLUSHALL</span>
      </label>

      <div>
        <div className="mb-1 font-medium">Cluster routing</div>
        <Select
          value={clusterRouting}
          onChange={(val) =>
            onChange({
              ...v,
              clusterRouting: val === 'pinnedNode' ? 'pinnedNode' : 'auto',
            })
          }
          className="h-8 w-full text-xs"
          options={[
            { value: 'auto', label: 'Auto — follow MOVED / ASK' },
            { value: 'pinnedNode', label: 'Pinned node — target one cluster node' },
          ]}
        />
      </div>
    </div>
  );
}
