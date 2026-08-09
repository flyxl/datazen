export const redisSettingsSchema = {
  type: 'object',
  properties: {
    allowFlush: {
      type: 'boolean',
      title: 'Allow FLUSHDB / FLUSHALL',
      description: 'Dangerous. Off by default.',
      default: false,
    },
  },
} as const;

export function RedisSettingsSection({
  value,
  onChange,
}: {
  value: unknown;
  onChange: (next: unknown) => void;
}) {
  const v = (value && typeof value === 'object' ? value : {}) as { allowFlush?: boolean };
  return (
    <label className="flex items-start gap-2 text-sm">
      <input
        type="checkbox"
        checked={v.allowFlush === true}
        onChange={(e) => onChange({ ...v, allowFlush: e.target.checked })}
      />
      <span>Allow FLUSHDB / FLUSHALL</span>
    </label>
  );
}
