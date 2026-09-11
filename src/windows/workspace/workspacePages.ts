import { useMemo } from 'react';
import {
  useWorkspaceTabsStore,
  workspaceTabKey,
  type WorkspaceTab,
} from '../../stores/workspaceTabsStore';
import { useWappStore } from '../../stores/wappStore';
import type { WappPageSummary, WappSummary } from '../../types/wapp';

/** Mirrors `WAPPS_OPEN_PAGE_EVENT` in `src-tauri/src/wapps/protocol.rs`. */
export const WAPPS_OPEN_PAGE_EVENT = 'wapps:open-page';

/** Payload of the `wapps:open-page` deep-link event (`datazen://…/open?page=…`). */
export interface OpenPageEventPayload {
  wappId?: string;
  pageId?: string;
  /** Startup params; forwarded to the wapp page by the bridge. Ignored here. */
  params?: Record<string, string>;
}

/** Flattened "enabled wapp × contributed page" row used across workspace UI. */
export interface WorkspacePageEntry {
  key: string;
  wappId: string;
  pageId: string;
  title: string;
  icon?: string;
  version: string;
  author?: string;
  description?: string;
}

function toEntry(wapp: WappSummary, page: WappPageSummary): WorkspacePageEntry {
  return {
    key: workspaceTabKey(wapp.id, page.id),
    wappId: wapp.id,
    pageId: page.id,
    title: page.title || wapp.name,
    icon: page.icon,
    version: wapp.version,
    author: wapp.author,
    description: wapp.description,
  };
}

/** All pages contributed by enabled wapps, in install order. */
export function deriveWorkspacePages(wapps: WappSummary[]): WorkspacePageEntry[] {
  return wapps
    .filter((p) => p.enabled && p.pages.length > 0)
    .flatMap((p) => p.pages.map((page) => toEntry(p, page)));
}

/** Reactive list of workspace pages (memoized on the wapp list reference). */
export function useWorkspacePages(): WorkspacePageEntry[] {
  const wapps = useWappStore((s) => s.wapps);
  return useMemo(() => deriveWorkspacePages(wapps), [wapps]);
}

export function buildWorkspaceTab(wapp: WappSummary, page: WappPageSummary): WorkspaceTab {
  const entry = toEntry(wapp, page);
  return {
    key: entry.key,
    wappId: entry.wappId,
    pageId: entry.pageId,
    title: entry.title,
    icon: entry.icon,
    version: entry.version,
  };
}

/**
 * Resolve + open a workspace app page tab (activates it). Returns false when the
 * wapp is missing/disabled or has no such page — callers decide whether to
 * surface that to the user.
 */
export function openWappPage(wappId: string, pageId?: string): boolean {
  const wapp = useWappStore.getState().byId(wappId);
  if (!wapp?.enabled || wapp.pages.length === 0) return false;
  const page = pageId ? wapp.pages.find((p) => p.id === pageId) : wapp.pages[0];
  if (!page) return false;
  useWorkspaceTabsStore.getState().open(buildWorkspaceTab(wapp, page));
  return true;
}
