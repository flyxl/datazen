import type { ComponentType } from 'react';
import { DocumentConnectionView } from '../../windows/connection/DocumentConnectionView';
import { RedisConnectionView } from '../../windows/connection/RedisConnectionView';
import { SqlConnectionView } from '../../windows/connection/SqlConnectionView';
import type { ConnectionViewProps } from './types';

export const CONNECTION_VIEWS: Record<string, ComponentType<ConnectionViewProps>> = {
  sql: SqlConnectionView,
  keyvalue: RedisConnectionView,
  document: DocumentConnectionView,
};

export function getConnectionView(mode: string): ComponentType<ConnectionViewProps> {
  return CONNECTION_VIEWS[mode] ?? SqlConnectionView;
}
