import type { ComponentType } from 'react';
import { DocumentConnectionView } from '../../windows/connection/DocumentConnectionView';
import { SqlConnectionView } from '../../windows/connection/SqlConnectionView';
import { getPluginConnectionView } from '../../plugins/generated';
import type { ConnectionViewProps } from './types';

/** Built-in views that are not driver-plugin contributions. */
const BUILTIN_VIEWS: Record<string, ComponentType<ConnectionViewProps>> = {
  sql: SqlConnectionView,
  document: DocumentConnectionView,
};

export function getConnectionView(mode: string): ComponentType<ConnectionViewProps> {
  const pluginView = getPluginConnectionView(mode);
  if (pluginView) return pluginView as ComponentType<ConnectionViewProps>;
  return BUILTIN_VIEWS[mode] ?? SqlConnectionView;
}
