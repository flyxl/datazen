import { useEffect, useState } from 'react';
import { cn } from '../lib/cn';
import { getDbIcon } from '../lib/databaseTypes';
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

  const resolved = (resolver ?? getActiveIconResolver()).resolve(`db.${databaseType}`);
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
