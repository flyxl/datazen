//! React rendering of a `RenderedView` (R9). Keeps hex/binary output virtualised
//! enough (row-capped in `views.ts`) so large values never freeze the panel.

import { useI18n } from '@datazen/ui';
import type { RenderedView } from './views';

export interface ValueViewOutputProps {
  rendered: RenderedView | null;
  busy: boolean;
  wrap: boolean;
}

export function ValueViewOutput({ rendered, busy, wrap }: ValueViewOutputProps) {
  const { t } = useI18n();

  if (busy) {
    return <div className="px-3 py-6 text-center text-xs text-fg-muted">{t('common.loading')}</div>;
  }
  if (!rendered) return null;

  if (rendered.kind === 'error') {
    return (
      <div className="rounded-md border border-danger/20 bg-danger/10 px-2 py-1.5 text-danger">
        {rendered.message}
      </div>
    );
  }

  if (rendered.kind === 'hex') {
    return (
      <pre
        className="max-h-80 overflow-auto rounded-md border border-edge bg-surface-alt p-2 font-mono text-[11px] leading-relaxed text-fg-secondary"
        data-testid="redis-value-hex"
      >
        {rendered.rows.map((row) => (
          <div key={row.offset} className="whitespace-pre">
            <span className="text-fg-muted">{row.offset.toString(16).padStart(8, '0')} </span>
            <span>{row.hex.padEnd(48, ' ')}</span>
            <span className="text-fg-muted"> |{row.ascii}|</span>
          </div>
        ))}
        {rendered.rows.length * 16 < rendered.totalBytes && (
          <div className="text-fg-muted">
            … {rendered.totalBytes - rendered.rows.length * 16} more byte(s)
          </div>
        )}
      </pre>
    );
  }

  return (
    <div className="space-y-1">
      {rendered.invalidUtf8 && (
        <div className="rounded-md border border-warning/20 bg-warning/10 px-2 py-1 text-[11px] text-warning">
          {t('redis.view.invalidUtf8')}
        </div>
      )}
      <pre
        className={
          'max-h-80 overflow-auto rounded-md border border-edge bg-surface-alt p-2 font-mono text-xs text-fg-secondary ' +
          (wrap ? 'whitespace-pre-wrap break-all' : 'whitespace-pre')
        }
        data-testid="redis-value-text"
      >
        {rendered.text || <span className="text-fg-muted">{t('redis.view.empty')}</span>}
      </pre>
    </div>
  );
}
