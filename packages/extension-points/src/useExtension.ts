/**
 * React Hooks for Host Extension Points — 特权扩展点消费 Hooks
 *
 * 通过 React 18 的 useSyncExternalStore 安全订阅扩展点的注册状态，
 * 保证在插件加载、卸载或重载时组件无撕裂更新。
 */

import { useSyncExternalStore, useCallback } from 'react';
import { extensionRegistry, type ExtensionPoint } from './extensionPoints';

/**
 * 订阅并获取特权扩展点的当前实现。
 * 未注册增强插件时自动返回扩展点的 getDefault() 基础实现。
 */
export function useExtension<T>(point: ExtensionPoint<T>): T {
  const subscribe = useCallback(
    (onStoreChange: () => void) => extensionRegistry.subscribe(point.id, onStoreChange),
    [point.id],
  );

  const getSnapshot = useCallback(() => extensionRegistry.get(point), [point]);

  const getServerSnapshot = useCallback(() => extensionRegistry.get(point), [point]);

  return useSyncExternalStore(subscribe, getSnapshot, getServerSnapshot);
}

/**
 * 检查当前扩展点是否已被增强实现激活
 */
export function useIsExtensionEnhanced(point: ExtensionPoint<unknown>): boolean {
  const subscribe = useCallback(
    (onStoreChange: () => void) => extensionRegistry.subscribe(point.id, onStoreChange),
    [point.id],
  );

  const getSnapshot = useCallback(() => extensionRegistry.isEnhanced(point), [point]);

  const getServerSnapshot = useCallback(() => false, []);

  return useSyncExternalStore(subscribe, getSnapshot, getServerSnapshot);
}
