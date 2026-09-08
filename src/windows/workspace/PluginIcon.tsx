import { useEffect, useState } from 'react';
import { Puzzle } from 'lucide-react';
import { cn } from '../../lib/cn';

export interface PluginIconProps {
  wappId?: string;
  /** @deprecated use wappId */
  pluginId?: string;
  /** Package-relative icon path from a manifest contribution. */
  icon?: string;
  className?: string;
}

/**
 * Renders a workspace app / plugin contributed icon through the `datazen://` asset protocol,
 * falling back to a puzzle glyph when absent or broken.
 */
export function PluginIcon({ wappId, pluginId, icon, className }: PluginIconProps) {
  const [broken, setBroken] = useState(false);
  const targetId = wappId || pluginId || '';

  useEffect(() => {
    setBroken(false);
  }, [icon]);

  if (!icon || broken) {
    return <Puzzle className={cn('shrink-0 text-fg-muted', className)} aria-hidden />;
  }
  return (
    <img
      src={`datazen://${targetId}/${icon.replace(/^\.\//, '')}`}
      alt=""
      draggable={false}
      className={cn('inline-block shrink-0 object-contain', className)}
      onError={() => setBroken(true)}
    />
  );
}
