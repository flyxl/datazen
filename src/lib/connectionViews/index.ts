import type { ComponentType } from 'react';
import { RedisConnectionView } from '../../windows/connection/RedisConnectionView';
import { SqlConnectionView } from '../../windows/connection/SqlConnectionView';
import type { ConnectionViewProps } from './types';

export const CONNECTION_VIEWS: Record<string, ComponentType<ConnectionViewProps>> = {
  sql: SqlConnectionView,
  keyvalue: RedisConnectionView,
};

export function getConnectionView(mode: string): ComponentType<ConnectionViewProps> {
  return CONNECTION_VIEWS[mode] ?? SqlConnectionView;
}
