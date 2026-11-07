import type { I18nKey } from '../../../locales';
import type { ConnectionEntry } from '../../../stores/activeConnectionStore';

type T = (key: I18nKey) => string;

/**
 * The three-dot connection state, rendered next to a connection row's name.
 *
 * `idle` draws nothing: a row that has never been opened should not carry a
 * status pip, and a layout that reserved the space anyway made every idle row
 * look like it was reporting something.
 */
export function renderStatusDot(
  connectionId: string,
  activeConnections: Record<string, ConnectionEntry | undefined>,
  t: T,
) {
  const status = activeConnections[connectionId]?.status ?? 'idle';
  if (status === 'connecting') {
    return (
      <span
        className="h-2 w-2 shrink-0 animate-pulse rounded-full bg-yellow-400"
        title={t('conn.connecting')}
      />
    );
  }
  if (status === 'connected') {
    return (
      <span className="h-2 w-2 shrink-0 rounded-full bg-green-500" title={t('conn.connected')} />
    );
  }
  if (status === 'error') {
    return <span className="h-2 w-2 shrink-0 rounded-full bg-red-500" title={t('conn.failed')} />;
  }
  return null;
}
