import { useMemo } from 'react';
import {
  useWorkspaceTabsStore,
  workspaceTabKey,
  type WorkspaceTab,
} from '../../stores/workspaceTabsStore';
import { usePluginStore } from '../../stores/pluginStore';
import type { PluginPageSummary, PluginSummary } from '../../types/plugin';

/** Mirrors `PLUGINS_OPEN_PAGE_EVENT` in `src-tauri/src/plugins/protocol.rs`. */
export const PLUGINS_OPEN_PAGE_EVENT = 'plugins:open-page';

/** Payload of the `plugins:open-page` deep-link event (`datazen://…/open?page=…`). */
export interface OpenPageEventPayload {
  pluginId?: string;
  pageId?: string;
  /** Startup params; forwarded to the plugin page by the F6 bridge. Ignored here. */
  params?: Record<string, string>;
}

/** Flattened "enabled plugin × contributed page" row used across workspace UI. */
export interface WorkspacePageEntry {
  key: string;
  pluginId: string;
  pageId: string;
  title: string;
  icon?: string;
  version: string;
  author?: string;
  description?: string;
}

function toEntry(plugin: PluginSummary, page: PluginPageSummary): WorkspacePageEntry {
  return {
    key: workspaceTabKey(plugin.id, page.id),
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
export function deriveWorkspacePages(plugins: PluginSummary[]): WorkspacePageEntry[] {
  return plugins
    .filter((p) => p.enabled && p.pages.length > 0)
    .flatMap((p) => p.pages.map((page) => toEntry(p, page)));
}

/** Reactive list of workspace pages (memoized on the plugin list reference). */
export function useWorkspacePages(): WorkspacePageEntry[] {
  const plugins = usePluginStore((s) => s.plugins);
  return useMemo(() => deriveWorkspacePages(plugins), [plugins]);
}

export function buildWorkspaceTab(plugin: PluginSummary, page: PluginPageSummary): WorkspaceTab {
  const entry = toEntry(plugin, page);
  return {
    key: entry.key,
    pluginId: entry.pluginId,
    pageId: entry.pageId,
    title: entry.title,
    icon: entry.icon,
    version: entry.version,
  };
}

/**
 * Resolve + open a plugin page tab (activates it). Returns false when the
 * plugin is missing/disabled or has no such page — callers decide whether to
 * surface that to the user.
 */
export function openPluginPage(pluginId: string, pageId?: string): boolean {
  const plugin = usePluginStore.getState().byId(pluginId);
  if (!plugin?.enabled || plugin.pages.length === 0) return false;
  const page = pageId ? plugin.pages.find((p) => p.id === pageId) : plugin.pages[0];
  if (!page) return false;
  useWorkspaceTabsStore.getState().open(buildWorkspaceTab(plugin, page));
  return true;
}
