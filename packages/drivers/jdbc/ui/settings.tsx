import { PathInput } from '../../../../src/components/ui/PathInput';
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
        JDBC uses an external Java agent process. Requires <strong>JRE 17+</strong> and a built{' '}
        <code className="text-[11px]">datazen-jdbc-agent.jar</code>. Vendor JDBC drivers are not
        bundled. Paths apply on the next connect (agent restarts if java/jar change).
      </p>
      <div>
        <div className="mb-1 font-medium">Java executable</div>
        <PathInput
          value={v.javaPath ?? ''}
          onChange={(path) => onChange({ ...v, javaPath: path })}
          placeholder="java (or /path/to/java)"
          className="w-full"
        />
        <p className="mt-1 text-[11px] text-fg-muted">
          Leave empty to use JAVA_HOME or PATH. Override with env DATAZEN_JDBC_JAVA.
        </p>
      </div>
      <div>
        <div className="mb-1 font-medium">Agent JAR</div>
        <PathInput
          value={v.agentJarPath ?? 'datazen-jdbc-agent.jar'}
          onChange={(path) => onChange({ ...v, agentJarPath: path })}
          placeholder="datazen-jdbc-agent.jar"
          dialogOptions={{
            filters: [{ name: 'JAR', extensions: ['jar'] }],
          }}
          className="w-full"
        />
        <p className="mt-1 text-[11px] text-fg-muted">
          Build with <code className="text-[11px]">./datazen-jdbc-agent/build.sh</code>. Env
          override: DATAZEN_JDBC_AGENT_JAR.
        </p>
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
      <div className="rounded-md border border-edge/60 bg-surface-2/30 px-3 py-2 text-[11px] text-fg-muted">
        <strong className="text-fg">Probe:</strong> there is no separate health command yet — open a
        JDBC connection or use Test Connection. Failures such as missing jar / wrong JRE surface as
        connection errors with the resolved path.
      </div>
    </div>
  );
}
