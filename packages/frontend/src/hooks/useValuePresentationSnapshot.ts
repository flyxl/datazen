/**
 * useValuePresentationSnapshot — 从 settingsStore 提供展示快照的 hook
 * （issue-32 T06，设计 §6/§7）。
 *
 * 面板层共享 hook：快照由本 hook 产出，再由提供方（PanelContentRenderer /
 * ContentView / ResultTableView）经 props 注入 DataTable 与 DetailPanel；
 * 组件本身不直接读 Store 或 IPC（快照之外的读取）。
 *
 * - 存储值读 `useSettingsStore(s => s.settings.valuePresentation)`，修订
 *   号读 `s.valuePresentationRevision`（store 在 valuePresentation 的
 *   JSON 变化时 +1，不变时不动——O-5 revision 不变量由该计数器保证）；
 * - registry revision 经 useSyncExternalStore 订阅（仿 T04 hook 的写法），
 *   注册 / 卸载触发重建；
 * - environment = resolvePresentationEnvironment(settings.language)：
 *   language 是规范化后的 UI 语言（'en' / 'zh-CN' 等），缺省时
 *   resolvePresentationEnvironment 自身回退到 navigator.language；
 * - 不读 DOM / IPC / 文件（除 resolvePresentationEnvironment 内部的
 *   navigator / Intl 回退）。
 */

import { useCallback, useMemo, useSyncExternalStore } from 'react';
import {
  valuePresentationRegistry,
  type ValuePresentationRegistry,
} from '@datazen/extension-points';
import { useSettingsStore } from '../stores/settingsStore';
import {
  resolvePresentationEnvironment,
  type ValuePresentationSnapshot,
} from '../lib/valuePresentation/presentationPlan';
import {
  buildValuePresentationSnapshot,
  parseStoredValuePresentationSettings,
} from '../lib/valuePresentation/settings';

export function useValuePresentationSnapshot(
  registry: ValuePresentationRegistry = valuePresentationRegistry,
): ValuePresentationSnapshot {
  const storedRaw = useSettingsStore((s) => s.settings.valuePresentation);
  const settingsRevision = useSettingsStore((s) => s.valuePresentationRevision) ?? 0;
  const language = useSettingsStore((s) => s.settings.language);

  // Defensive: mocked / pre-T06 store states may lack the field entirely —
  // parse coerces unknown input to the default instead of throwing.
  // Memoized on the raw identity so the snapshot below stays referentially
  // stable across renders.
  const stored = useMemo(() => parseStoredValuePresentationSettings(storedRaw), [storedRaw]);

  const subscribe = useCallback(
    (onStoreChange: () => void): (() => void) => registry.subscribe(onStoreChange),
    [registry],
  );
  const getSnapshot = useCallback((): number => registry.revision, [registry]);
  const registryRevision = useSyncExternalStore(subscribe, getSnapshot, getSnapshot);

  const environment = useMemo(() => resolvePresentationEnvironment(language), [language]);

  // 依赖只取 registryRevision：注册中心 identity 稳定（缺省全局单例），
  // 其变化由 revision 携带；settingsRevision 携带存储值的变化。
  // eslint-disable-next-line react-hooks/exhaustive-deps
  return useMemo(
    () => buildValuePresentationSnapshot(stored, environment, registry, settingsRevision),
    [stored, environment, registryRevision, settingsRevision],
  );
}
