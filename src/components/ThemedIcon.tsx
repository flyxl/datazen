import { useEffect, useState } from 'react';
import type { LucideIcon } from 'lucide-react';
import {
  Bot,
  Code2,
  Database,
  DatabaseBackup,
  Download,
  FileText,
  Globe,
  LayoutDashboard,
  MessageSquareText,
  Monitor,
  Moon,
  MousePointerClick,
  Play,
  Plug,
  Plus,
  RefreshCcw,
  Server,
  Settings,
  Square,
  Sun,
  Table2,
  Workflow,
} from 'lucide-react';
import { cn } from '../lib/cn';
import { getActiveIconResolver, type IconResolver } from '../lib/iconResolver';

/** Lucide components referenced by v1 semantic icon catalog defaults. */
const LUCIDE_MAP: Record<string, LucideIcon> = {
  Bot,
  Code2,
  Database,
  DatabaseBackup,
  Download,
  FileText,
  Globe,
  LayoutDashboard,
  MessageSquareText,
  Monitor,
  Moon,
  MousePointerClick,
  Play,
  Plug,
  Plus,
  RefreshCcw,
  Server,
  Settings,
  Square,
  Sun,
  Table2,
  Workflow,
};

export interface ThemedIconProps {
  id: string;
  className?: string;
  resolver?: IconResolver;
}

export function ThemedIcon({ id, className, resolver }: ThemedIconProps) {
  const [, bump] = useState(0);
  useEffect(() => {
    const onPackChanged = () => bump((n) => n + 1);
    document.addEventListener('datazen:theme-pack-changed', onPackChanged);
    return () => document.removeEventListener('datazen:theme-pack-changed', onPackChanged);
  }, []);

  const resolved = (resolver ?? getActiveIconResolver()).resolve(id);

  if (resolved.kind === 'url') {
    return (
      <img
        src={resolved.href}
        alt=""
        className={cn('inline-block shrink-0 object-contain', className)}
        draggable={false}
      />
    );
  }

  if (resolved.kind === 'lucide') {
    const Icon = LUCIDE_MAP[resolved.name];
    if (Icon) {
      return <Icon className={cn('shrink-0', className)} aria-hidden />;
    }
    return (
      <span
        className={cn(
          'inline-flex shrink-0 items-center justify-center rounded bg-slate-600 text-[10px] font-semibold text-white',
          className,
        )}
        aria-hidden
      >
        ?
      </span>
    );
  }

  return (
    <span
      className={cn(
        'inline-flex shrink-0 items-center justify-center rounded text-[10px] font-semibold text-white',
        resolved.bgClass,
        className,
      )}
      aria-hidden
    >
      {resolved.label}
    </span>
  );
}
