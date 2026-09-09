import { Input } from '@datazen/ui';

export const jdbcSettingsSchema = {
  type: 'object',
  properties: {
    javaPath: {
      type: 'string',
      title: 'Java executable',
      description: 'Override java binary (empty = JAVA_HOME / PATH).',
      default: '',
    },
    agentJarPath: {
      type: 'string',
      title: 'Agent JAR path',
      description: 'Path to datazen-jdbc-agent.jar',
      default: 'datazen-jdbc-agent.jar',
    },
    idleTimeoutSecs: {
      type: 'number',
      title: 'Agent idle timeout (seconds)',
      description: 'Shut down the shared agent after this many idle seconds.',
      default: 600,
    },
  },
} as const;

type JdbcPluginSettings = {
  javaPath?: string;
  agentJarPath?: string;
  idleTimeoutSecs?: number;
};

export function JdbcSettingsSection({
  value,
  onChange,
}: {
  value: unknown;
  onChange: (next: unknown) => void;
}) {
  const v = (value && typeof value === 'object' ? value : {}) as JdbcPluginSettings;

  return (
    <div className="space-y-3 text-sm">
      <p className="text-xs text-fg-muted">
        JDBC uses an external Java agent process. Requires JRE 17+ and a built agent jar. Vendor
        JDBC drivers are not bundled.
      </p>
      <div>
        <div className="mb-1 font-medium">Java executable</div>
        <Input
          value={v.javaPath ?? ''}
          onChange={(e) => onChange({ ...v, javaPath: e.target.value })}
          placeholder="java (or /path/to/java)"
          className="h-8 w-full text-xs"
        />
      </div>
      <div>
        <div className="mb-1 font-medium">Agent JAR</div>
        <Input
          value={v.agentJarPath ?? 'datazen-jdbc-agent.jar'}
          onChange={(e) => onChange({ ...v, agentJarPath: e.target.value })}
          placeholder="datazen-jdbc-agent.jar"
          className="h-8 w-full text-xs"
        />
      </div>
      <div>
        <div className="mb-1 font-medium">Idle timeout (seconds)</div>
        <Input
          type="number"
          min={60}
          value={String(v.idleTimeoutSecs ?? 600)}
          onChange={(e) =>
            onChange({
              ...v,
              idleTimeoutSecs: Math.max(60, Number(e.target.value) || 600),
            })
          }
          className="h-8 w-full text-xs"
        />
      </div>
    </div>
  );
}
