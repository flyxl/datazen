import { useEffect, useState } from 'react';
import { cn } from '../lib/cn';
import { getDbIcon, getDriverIconParents } from '../lib/databaseTypes';
import { getActiveIconResolver, type IconResolver } from '../lib/iconResolver';
import type { DatabaseType } from '../types';

export interface DbTypeBadgeProps {
  databaseType: string;
  className?: string;
  /** Pixel width/height; defaults to 24 (compact badges). */
  size?: number;
  resolver?: IconResolver;
}

function badgeFontSize(size: number): number {
  if (size <= 20) return 9;
  if (size <= 24) return 10;
  return 11;
}

function cornerFontSize(size: number): number {
  return Math.min(11, Math.max(8, Math.round(size * 0.38)));
}

export function DbTypeBadge({
  databaseType,
  className,
  size = 24,
  resolver,
}: DbTypeBadgeProps) {
  const [, bump] = useState(0);
  useEffect(() => {
    const onPackChanged = () => bump((n) => n + 1);
    document.addEventListener('datazen:theme-pack-changed', onPackChanged);
    return () => document.removeEventListener('datazen:theme-pack-changed', onPackChanged);
  }, []);

  const activeResolver = resolver ?? getActiveIconResolver();
  const resolved = activeResolver.resolve(`db.${databaseType}`);
  const dimensionStyle = { width: size, height: size };

  if (resolved.kind === 'url') {
    return (
      <img
        src={resolved.href}
        alt=""
        draggable={false}
        style={dimensionStyle}
        className={cn(
          'inline-flex shrink-0 items-center justify-center rounded-lg object-contain shadow-sm',
          className,
        )}
      />
    );
  }

  const parentId = getDriverIconParents()[databaseType];
  const parentResolved = parentId ? activeResolver.resolve(`db.${parentId}`) : null;
  if (parentResolved?.kind === 'url') {
    const { label, bg } = getDbIcon(databaseType as DatabaseType);
    return (
      <span
        style={dimensionStyle}
        className={cn('relative inline-flex shrink-0', className)}
        aria-hidden
      >
        <img
          src={parentResolved.href}
          alt=""
          draggable={false}
          className="h-full w-full rounded-lg object-contain shadow-sm"
        />
        <span
          style={{ fontSize: cornerFontSize(size) }}
          className={cn(
            'absolute -right-0.5 -bottom-0.5 flex items-center justify-center rounded px-0.5 font-bold leading-none text-white shadow',
            bg,
          )}
        >
          {label}
        </span>
      </span>
    );
  }

  const { label, bgClass } =
    resolved.kind === 'placeholder'
      ? resolved
      : (() => {
          const { label: dbLabel, bg } = getDbIcon(databaseType as DatabaseType);
          return { label: dbLabel, bgClass: bg };
        })();

  return (
    <span
      style={{ ...dimensionStyle, fontSize: badgeFontSize(size) }}
      className={cn(
        'inline-flex shrink-0 items-center justify-center rounded-lg font-bold text-white shadow-sm',
        bgClass,
        className,
      )}
      aria-hidden
    >
      {label}
    </span>
  );
}
