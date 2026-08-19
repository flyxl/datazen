import type { ComponentType } from 'react';
import { DocumentConnectionView } from '../../windows/connection/DocumentConnectionView';
import { getPluginConnectionView } from '../../plugins/generated';
import type { ConnectionViewProps } from './types';

/**
 * Built-in views for non-SQL connection types.
 * SQL panel content is rendered directly by PanelContentRenderer, not via this registry.
 */
const BUILTIN_VIEWS: Record<string, ComponentType<ConnectionViewProps>> = {
  document: DocumentConnectionView,
};

export function getConnectionView(mode: string): ComponentType<ConnectionViewProps> {
  const pluginView = getPluginConnectionView(mode);
  if (pluginView) return pluginView as ComponentType<ConnectionViewProps>;
  return BUILTIN_VIEWS[mode] ?? DocumentConnectionView;
}
