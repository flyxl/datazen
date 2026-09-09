import { useEffect, useRef, type RefObject } from 'react';
import type { ConnectionViewActions } from '../../lib/connectionViews/types';
import type { ConnectionTab } from './connectionPageUtils';
import { GETTING_STARTED_QUERY_TITLE, SAMPLE_GETTING_STARTED_SQL } from './gettingStartedQuery';

interface AutoOpenGettingStartedQueryOptions {
  actionsRef: RefObject<ConnectionViewActions | undefined>;
  onboardingStatus: string;
  sampleConnectionId: string | null;
  activeTab: ConnectionTab | null;
}

export function useAutoOpenGettingStartedQuery({
  actionsRef,
  onboardingStatus,
  sampleConnectionId,
  activeTab,
}: AutoOpenGettingStartedQueryOptions): void {
  const sampleQueryOpenedRef = useRef(false);

  useEffect(() => {
    if (sampleQueryOpenedRef.current) return;
    if (onboardingStatus !== 'active' || !sampleConnectionId) return;
    if (!activeTab?.dbSessionId || activeTab.status !== 'connected') return;
    if (activeTab.connectionId !== sampleConnectionId) return;

    const newQuery = actionsRef.current?.newQuery;
    if (!newQuery) return;

    const opened = newQuery(SAMPLE_GETTING_STARTED_SQL, undefined, GETTING_STARTED_QUERY_TITLE);
    if (opened !== false) {
      sampleQueryOpenedRef.current = true;
    }
  }, [
    actionsRef,
    onboardingStatus,
    sampleConnectionId,
    activeTab?.connectionId,
    activeTab?.dbSessionId,
    activeTab?.status,
  ]);
}
