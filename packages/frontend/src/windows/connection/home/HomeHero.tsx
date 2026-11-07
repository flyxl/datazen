import { DbTypeBadge } from '../../../components/DbTypeBadge';
import { useI18n } from '../../../hooks/useI18n';
import { getDbLabel } from '../../../lib/databaseTypes';
import type { DatabaseType } from '../../../types';

interface HomeHeroProps {
  connectionCount: number;
  groupCount: number;
  /** Distinct database types across saved connections (drives the badge cluster). */
  dbTypes: DatabaseType[];
}

/**
 * Landing hero for the no-active-connection state.
 * Replaces the former static hint + three vanity metric cards with a single
 * dynamic summary line backed by live store data.
 */
export function HomeHero({ connectionCount, groupCount, dbTypes }: HomeHeroProps) {
  const { t } = useI18n();

  return (
    <div className="flex items-end justify-between gap-4">
      <div className="min-w-0">
        <h2 className="text-lg font-bold tracking-tight text-fg sm:text-xl">
          {t('connWin.home.selectConnectionTitle')}
        </h2>
        <p className="mt-1 text-sm text-fg-muted" data-testid="home-hero-subtitle">
          <b className="font-semibold text-fg">{connectionCount}</b>{' '}
          {t('connWin.home.hero.subtitleConnections', { count: connectionCount })}
          <span className="mx-2 text-fg-muted/60">·</span>
          <b className="font-semibold text-fg">{groupCount}</b>{' '}
          {t('connWin.home.hero.subtitleGroups', { groups: groupCount })}
          <span className="mx-2 text-fg-muted/60">·</span>
          <b className="font-semibold text-fg">{dbTypes.length}</b>{' '}
          {t('connWin.home.hero.subtitleTypes', { types: dbTypes.length })}
        </p>
      </div>

      {dbTypes.length > 0 && (
        <div className="flex shrink-0 items-center -space-x-1.5 overflow-hidden pb-1">
          {dbTypes.slice(0, 4).map((dbType) => (
            <div
              key={dbType}
              className="rounded-full ring-2 ring-surface"
              title={getDbLabel(dbType)}
            >
              <DbTypeBadge databaseType={dbType} size={22} />
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
