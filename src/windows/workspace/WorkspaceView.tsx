import { useEffect } from 'react';
import { listen } from '@tauri-apps/api/event';
import { usePluginStore } from '../../stores/pluginStore';
import { useWorkspaceTabsStore } from '../../stores/workspaceTabsStore';
import { PluginPageShell } from './PluginPageShell';
import { WorkspaceDefaultCards } from './WorkspaceDefaultCards';
import { WorkspaceNavigator } from './WorkspaceNavigator';
import { WorkspaceTabBar } from './WorkspaceTabBar';
import {
  PLUGINS_OPEN_PAGE_EVENT,
  openPluginPage,
  useWorkspacePages,
  type OpenPageEventPayload,
} from './workspacePages';

export interface WorkspaceViewProps {
  /** Empty-state / navigator shortcut to the plugin management page. */
  onOpenPlugins?: () => void;
}

/**
 * Workspace mode layout: plugin navigator on the left, independent tab strip +
 * panels (or the default card grid) on the right.
 *
 * Also hosts the `plugins:open-page` deep-link listener (`datazen://…/open`).
 */
export function WorkspaceView({ onOpenPlugins }: WorkspaceViewProps) {
  const pages = useWorkspacePages();
  const plugins = usePluginStore((s) => s.plugins);
  const pluginsLoaded = usePluginStore((s) => s.loaded);
  const tabs = useWorkspaceTabsStore((s) => s.tabs);
  const activeKey = useWorkspaceTabsStore((s) => s.activeKey);

  // Fire-and-forget initial load; refreshed via `plugins:changed` by the store.
  useEffect(() => {
    if (!usePluginStore.getState().loaded) void usePluginStore.getState().fetch();
  }, []);

  // BUG-F4-01: a `plugins:changed` refresh triggered outside this window
  // (another window disabling/uninstalling a plugin) must also close that
  // plugin's workspace tabs — the management page only covers its own actions.
  // The diff only runs once the store has loaded, so the initial (possibly
  // empty) plugin list can never close pre-existing tabs.
  useEffect(() => {
    if (!pluginsLoaded) return;
    const { tabs: openTabs, closeByPlugin } = useWorkspaceTabsStore.getState();
    const visited = new Set<string>();
    for (const tab of openTabs) {
      if (visited.has(tab.pluginId)) continue;
      visited.add(tab.pluginId);
      const plugin = plugins.find((p) => p.id === tab.pluginId);
      if (!plugin || !plugin.enabled) closeByPlugin(tab.pluginId);
    }
  }, [plugins, pluginsLoaded]);

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    let disposed = false;
    void listen<OpenPageEventPayload>(PLUGINS_OPEN_PAGE_EVENT, (event) => {
      const payload = event.payload;
      if (!payload?.pluginId || !payload?.pageId) return;
      const plugin = usePluginStore.getState().byId(payload.pluginId);
      if (!plugin || !plugin.enabled) return;
      if (!plugin.pages.some((p) => p.id === payload.pageId)) return;
      openPluginPage(payload.pluginId, payload.pageId);
      // `params` is stored with the tab by the bridge consumer in F6.
    }).then((fn) => {
      if (disposed) fn();
      else unlisten = fn;
    });
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, []);

  return (
    <div className="flex h-full min-h-0 flex-1">
      <WorkspaceNavigator onOpenPlugins={onOpenPlugins} />
      <div className="flex min-h-0 min-w-0 flex-1 flex-col">
        <WorkspaceTabBar />
        {tabs.length === 0 ? (
          <WorkspaceDefaultCards
            pages={pages}
            onOpen={(page) => openPluginPage(page.pluginId, page.pageId)}
            onOpenPlugins={onOpenPlugins}
          />
        ) : (
          <div className="relative min-h-0 flex-1">
            {tabs.map((tab) => (
              <PluginPageShell key={tab.key} tab={tab} active={tab.key === activeKey} />
            ))}
          </div>
        )}
      </div>
    </div>
  );
}
