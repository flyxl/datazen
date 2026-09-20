import { useCallback, useEffect, useRef } from 'react';
import { cn } from '@datazen/ui';
import { useI18n } from '../../../hooks/useI18n';
import type { QbJoinType } from '../types';
import type { RelationShape } from './fkGeometry';

const JOIN_TYPES: readonly QbJoinType[] = ['INNER', 'LEFT', 'RIGHT', 'FULL'] as const;

export interface JoinPopoverProps {
  shape: RelationShape;
  /** Anchor in **screen** coordinates relative to the canvas viewport. */
  at: { x: number; y: number };
  onSetType: (type: QbJoinType) => void;
  /** Promote every pair of the constraint into the SQL. */
  onConfirm: () => void;
  /** Remove every pair of the constraint from the SQL. */
  onRemove: () => void;
  onClose: () => void;
}

/**
 * Transient actions for one relation.
 *
 * The canvas itself carries no text: confirming, re-typing and removing a
 * relation all live here, shown at the click point and dismissed on outside
 * click or Escape.
 */
export function JoinPopover({
  shape,
  at,
  onSetType,
  onConfirm,
  onRemove,
  onClose,
}: JoinPopoverProps) {
  const { t } = useI18n();
  const ref = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const onKeyDown = (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
        e.stopPropagation();
        onClose();
      }
    };
    const onPointerDown = (e: PointerEvent) => {
      if (ref.current && !ref.current.contains(e.target as Node)) onClose();
    };
    document.addEventListener('keydown', onKeyDown, true);
    document.addEventListener('pointerdown', onPointerDown, true);
    return () => {
      document.removeEventListener('keydown', onKeyDown, true);
      document.removeEventListener('pointerdown', onPointerDown, true);
    };
  }, [onClose]);

  const handleSetType = useCallback((type: QbJoinType) => onSetType(type), [onSetType]);

  const title = shape.constraint ?? t('query.visualBuilder.manualJoin');
  const isConfirmed = shape.state === 'confirmed';

  return (
    <div
      ref={ref}
      className={cn(
        'absolute z-30 w-[220px] rounded-lg border border-edge bg-surface-alt p-2 shadow-xl',
        'text-[11px] text-fg',
      )}
      style={{ left: at.x + 8, top: at.y + 8 }}
      data-testid="qb-join-popover"
      onPointerDown={(e) => e.stopPropagation()}
    >
      <div className="mb-1.5 flex items-center gap-1.5">
        <span className="truncate font-medium" title={title}>
          {title}
        </span>
        {shape.pairCount > 1 && (
          <span
            className="shrink-0 rounded bg-surface-inset px-1 text-[10px] text-fg-muted"
            data-testid="qb-join-popover-pairs"
          >
            {shape.pairCount} {t('query.visualBuilder.columns')}
          </span>
        )}
        <button
          type="button"
          onClick={onClose}
          className="ml-auto shrink-0 rounded px-1 text-fg-muted hover:bg-surface-inset hover:text-fg"
          data-testid="qb-join-popover-close"
          aria-label={t('common.close')}
        >
          ×
        </button>
      </div>

      <div className="mb-2 flex items-center gap-1" role="radiogroup">
        {JOIN_TYPES.map((type) => (
          <button
            key={type}
            type="button"
            role="radio"
            aria-checked={shape.type === type}
            onClick={() => handleSetType(type)}
            className={cn(
              'flex-1 rounded px-1 py-0.5 text-[10px] font-medium transition-colors',
              shape.type === type
                ? 'bg-accent text-on-accent'
                : 'text-fg-secondary hover:bg-surface-raised hover:text-fg',
            )}
            data-testid={`qb-join-type-${type}`}
          >
            {type}
          </button>
        ))}
      </div>

      <button
        type="button"
        onClick={isConfirmed ? onRemove : onConfirm}
        className={cn(
          'w-full rounded px-2 py-1 text-[11px] font-medium transition-colors',
          isConfirmed
            ? 'text-danger hover:bg-danger/10'
            : 'bg-accent text-on-accent hover:bg-accent-2',
        )}
        data-testid={isConfirmed ? 'qb-join-remove' : 'qb-join-confirm'}
      >
        {isConfirmed ? t('query.visualBuilder.removeJoin') : t('query.visualBuilder.confirmJoin')}
      </button>
    </div>
  );
}
