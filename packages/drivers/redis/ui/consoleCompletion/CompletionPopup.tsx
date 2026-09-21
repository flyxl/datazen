import { AlertTriangle } from 'lucide-react';
import { useI18n } from '../../../../../src/hooks/useI18n';
import { cn } from '../../../../../src/lib/cn';
import { classifyDangerLevel, dangerBadgeColor } from '../redisConsoleDanger';
import type { CompletionItem } from './useCompletion';

const GROUP_LABEL_KEYS: Record<string, string> = {
  string: 'redis.grp.string',
  hash: 'redis.grp.hash',
  list: 'redis.grp.list',
  set: 'redis.grp.set',
  zset: 'redis.grp.zset',
  geo: 'redis.grp.geo',
  stream: 'redis.grp.stream',
  generic: 'redis.grp.generic',
  keyspace: 'redis.grp.keyspace',
  server: 'redis.grp.server',
  pubsub: 'redis.grp.pubsub',
  script: 'redis.grp.script',
  cluster: 'redis.grp.cluster',
  connection: 'redis.grp.connection',
  bitmap: 'redis.grp.bitmap',
};

export interface CompletionPopupProps {
  items: CompletionItem[];
  activeIndex: number;
  loading: boolean;
  onHover: (index: number) => void;
  onAccept: (index: number) => void;
}

export function CompletionPopup({
  items,
  activeIndex,
  loading,
  onHover,
  onAccept,
}: CompletionPopupProps) {
  const { t } = useI18n();
  if (items.length === 0) return null;

  return (
    <div
      className="absolute bottom-1 left-3 z-20 max-h-64 w-[420px] overflow-auto rounded-md border border-edge bg-surface shadow-lg"
      data-testid="redis-completion-popup"
      role="listbox"
    >
      {loading && (
        <div className="px-3 py-1 text-[10px] text-fg-muted">{t('redis.completion.loading')}</div>
      )}
      {items.map((item, index) => {
        const active = index === activeIndex;
        if (item.kind === 'key') {
          return (
            <button
              key={`key-${item.insertText}`}
              type="button"
              role="option"
              aria-selected={active}
              data-completion-index={index}
              data-testid={`redis-completion-item-${index}`}
              onMouseEnter={() => onHover(index)}
              onMouseDown={(e) => {
                e.preventDefault();
                onAccept(index);
              }}
              className={cn(
                'flex w-full items-center gap-2 px-3 py-1.5 text-left text-xs',
                active ? 'bg-accent/15' : 'hover:bg-surface-alt',
              )}
            >
              <span className="min-w-0 flex-1 truncate font-mono text-fg">{item.label}</span>
              <span className="shrink-0 rounded bg-surface-alt px-1.5 py-0.5 text-[10px] text-fg-muted">
                {t('redis.grp.key')}
              </span>
            </button>
          );
        }
        const level = classifyDangerLevel(item.meta.name);
        const isDangerous = level === 'danger' || level === 'ultra-danger';
        return (
          <button
            key={`cmd-${item.meta.name}`}
            type="button"
            role="option"
            aria-selected={active}
            data-completion-index={index}
            data-testid={`redis-completion-item-${index}`}
            onMouseEnter={() => onHover(index)}
            onMouseDown={(e) => {
              e.preventDefault();
              onAccept(index);
            }}
            className={cn(
              'block w-full px-3 py-1.5 text-left',
              active ? 'bg-accent/15' : 'hover:bg-surface-alt',
            )}
          >
            <div className="flex items-center gap-2">
              {isDangerous && <AlertTriangle className="h-3 w-3 shrink-0 text-orange-500" />}
              <span className="min-w-0 flex-1 truncate text-xs font-semibold text-fg">
                {item.meta.name}
              </span>
              {isDangerous ? (
                <span
                  className={cn(
                    'shrink-0 rounded px-1.5 py-0.5 text-[10px] font-medium',
                    dangerBadgeColor(level),
                  )}
                >
                  {level === 'ultra-danger'
                    ? t('redis.console.dangerUltra')
                    : t('redis.console.dangerDanger')}
                </span>
              ) : (
                <span className="shrink-0 rounded bg-surface-alt px-1.5 py-0.5 text-[10px] text-fg-muted">
                  {t(GROUP_LABEL_KEYS[item.meta.group] ?? 'redis.grp.generic')}
                </span>
              )}
            </div>
            <div className="truncate text-[11px] text-fg-secondary">{item.meta.desc}</div>
            <div className="truncate font-mono text-[10px] text-fg-muted">{item.meta.syntax}</div>
          </button>
        );
      })}
    </div>
  );
}
