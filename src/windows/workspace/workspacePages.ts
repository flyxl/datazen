import { useMemo } from 'react';
import {
  useWorkspaceTabsStore,
  workspaceTabKey,
  type WorkspaceTab,
} from '../../stores/workspaceTabsStore';
import { useExtensionStore } from '../../stores/extensionStore';
import type { ExtensionPageSummary, ExtensionSummary } from '../../types/extension';

/** Mirrors `EXTENSIONS_OPEN_PAGE_EVENT` in `src-tauri/src/extensions/protocol.rs`. */
export const EXTENSIONS_OPEN_PAGE_EVENT = 'wapps:open-page';

/** Payload of the `wapps:open-page` deep-link event (`datazen://…/open?page=…`). */
export interface OpenPageEventPayload {
  wappId?: string;
  pluginId?: string;
  pageId?: string;
  /** Startup params; forwarded to the plugin page by the bridge. Ignored here. */
  params?: Record<string, string>;
}

/** Flattened "enabled plugin/wapp × contributed page" row used across workspace UI. */
export interface WorkspacePageEntry {
  key: string;
  wappId: string;
  /** @deprecated use wappId */
  pluginId: string;
  pageId: string;
  title: string;
  icon?: string;
  version: string;
  author?: string;
  description?: string;
}

function toEntry(plugin: ExtensionSummary, page: ExtensionPageSummary): WorkspacePageEntry {
  return {
    key: workspaceTabKey(plugin.id, page.id),
    wappId: plugin.id,
    pluginId: plugin.id,
    pageId: page.id,
    title: page.title || plugin.name,
    icon: page.icon,
    version: plugin.version,
    author: plugin.author,
    description: plugin.description,
  };
}

/** All pages contributed by enabled plugins, in install order. */
export function deriveWorkspacePages(plugins: ExtensionSummary[]): WorkspacePageEntry[] {
  return plugins
    .filter((p) => p.enabled && p.pages.length > 0)
    .flatMap((p) => p.pages.map((page) => toEntry(p, page)));
}

/** Reactive list of workspace pages (memoized on the plugin list reference). */
export function useWorkspacePages(): WorkspacePageEntry[] {
  const plugins = useExtensionStore((s) => s.extensions);
  return useMemo(() => deriveWorkspacePages(plugins), [plugins]);
}

export function buildWorkspaceTab(
  plugin: ExtensionSummary,
  page: ExtensionPageSummary,
): WorkspaceTab {
  const entry = toEntry(plugin, page);
  return {
    key: entry.key,
    wappId: entry.wappId,
    pluginId: entry.pluginId,
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
  const plugin = useExtensionStore.getState().byId(wappId);
  if (!plugin?.enabled || plugin.pages.length === 0) return false;
  const page = pageId ? plugin.pages.find((p) => p.id === pageId) : plugin.pages[0];
  if (!page) return false;
  useWorkspaceTabsStore.getState().open(buildWorkspaceTab(plugin, page));
  return true;
}

/** Alias of openWappPage for backward compatibility. */
export const openPluginPage = openWappPage;
