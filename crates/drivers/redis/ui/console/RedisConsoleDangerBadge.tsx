/**
 * Danger badge for the Redis Console toolbar (track `redis-console-safety`).
 *
 * Split out of `RedisConsole.tsx` so the level → copy/color mapping is one
 * small, testable unit. Copy is resolved through i18n keys only; the badge also
 * publishes its state as `data-*` attributes because the test suite must assert
 * on the enum value, never on the English label (AGENTS.md §0.4 ②).
 */

import { cn } from '@datazen/ui';
import { dangerBadgeColor, type CommandAssessment, type DangerLevel } from './redisConsoleDanger';

function dangerLevelLabel(
  level: DangerLevel,
  unknown: boolean,
  t: (key: string) => string,
): string {
  if (level === 'ultra-danger' && unknown) return t('redis.consoleSafety.badgeUnknown');
  switch (level) {
    case 'ultra-danger':
      return t('redis.console.dangerUltra');
    case 'danger':
      return t('redis.console.dangerDanger');
    case 'write':
      return t('redis.console.dangerWrite');
    default:
      return t('redis.console.dangerSafe');
  }
}

export function dangerBorderClass(assessment: CommandAssessment): string {
  if (assessment.level === 'ultra-danger') {
    return assessment.unknown ? 'border-l-red-800' : 'border-l-red-500';
  }
  return assessment.level === 'danger' ? 'border-l-orange-500' : 'border-l-transparent';
}

export interface RedisConsoleDangerBadgeProps {
  assessment: CommandAssessment;
  t: (key: string) => string;
}

export function RedisConsoleDangerBadge({ assessment, t }: RedisConsoleDangerBadgeProps) {
  return (
    <span
      className={cn(
        'shrink-0 rounded px-1.5 py-0.5 text-[10px] font-medium',
        dangerBadgeColor(assessment.level, assessment.unknown),
      )}
      data-testid="redis-console-danger-badge"
      data-danger-level={assessment.level}
      data-danger-unknown={assessment.unknown ? 'true' : 'false'}
    >
      {dangerLevelLabel(assessment.level, assessment.unknown, t)}
    </span>
  );
}
