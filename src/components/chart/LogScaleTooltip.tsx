import { Tooltip } from 'recharts';
import { formatCompact } from '../../lib/chart/format';

/**
 * Recharts Tooltip for manually-log-transformed data.
 * Reads the original (pre-transform) values from `__orig__${name}` stored by
 * `mapToLogScale`, so users always see real numbers rather than log10 values.
 */
export function LogScaleTooltip() {
  return (
    <Tooltip
      contentStyle={{
        background: 'var(--c-surface-alt, #1e1e2e)',
        border: '1px solid var(--c-edge, #333)',
        borderRadius: 6,
        color: 'var(--c-fg, #eee)',
        fontSize: 12,
      }}
      formatter={(
        _logValue: number,
        name: string,
        props: { payload?: Record<string, unknown> },
      ) => {
        const orig = props.payload?.[`__orig__${name}`];
        if (typeof orig === 'number') {
          return [orig <= 0 ? '0' : formatCompact(orig), name];
        }
        return [formatCompact(Math.pow(10, _logValue)), name];
      }}
    />
  );
}
