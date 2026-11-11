/**
 * valuePresentation 存储逻辑（issue-32 T06，设计 §5/§6）。
 *
 * 持久化形状（版本化、按 rendererId 命名空间；表列覆盖不在 T06，T10 再加）：
 *
 * ```ts
 * StoredValuePresentationSettings = { version: number; renderers: Record<string, StoredRendererSettings> }
 * StoredRendererSettings = { enabled: boolean; options: TimestampOptions }
 * ```
 *
 * - 当前版本 version = 1；`rules` 不在本版形状里（T08 再扩展，届时版本
 *   或加字段处理）。
 * - 本模块只做存储形状的宽容解析与快照构建，不做设置 UI、不做规则、
 *   不做列配置面板、不做表列覆盖持久化（T10）、不做跨窗口同步的形式化
 *   （T11；valuePresentation 随 AppSettings 全量走既有的
 *   `datazen:settings-changed` 事件）。
 * - 分层：前端存储类型自写，与 backend-client 的
 *   `SettingsValuePresentationSettings` 镜面（AppSettings 侧的类型在
 *   frontend types 里、backend-client 侧在 backend-client 里，两边互指）；
 *   `options` 字段直接用 T02 的 TimestampOptions（同一包内可导入）。
 * - Rust 侧只做存储透传（`ValuePresentationSettings`，renderers 值为
 *   `serde_json::Value`），解释与验证在前端宿主边界做。
 */

import type {
  PresentationEnvironment,
  ValuePresentationContribution,
  ValuePresentationRegistry,
  VersionedPresentationConfig,
} from '@datazen/extension-points';
import { valuePresentationRegistry } from '@datazen/extension-points';
import type { TimestampOptions } from './timestampConfig';
import type { ValuePresentationSnapshot } from './presentationPlan';

/** 一个渲染器的存储设置：全局开关 + 其选项（T02 TimestampOptions JSON 形状）。 */
export interface StoredRendererSettings {
  enabled: boolean;
  options: TimestampOptions;
}

/** 版本化的 valuePresentation 存储形状（T06 v1：无 rules、无表列覆盖）。 */
export interface StoredValuePresentationSettings {
  version: number;
  renderers: Record<string, StoredRendererSettings>;
}

/** 当前版本 v1 的默认存储值：关闭一切（历史配置缺字段默认关闭，设计 §6）。 */
export const DEFAULT_VALUE_PRESENTATION_SETTINGS: StoredValuePresentationSettings = {
  version: 1,
  renderers: {},
};

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

/**
 * 宽容解析未知输入为存储形状（宿主边界：旧后端 payload / 损坏的本地值
 * 都能读，不抛异常）：
 *
 * - 非对象 → 默认值；
 * - version 非数字 → 1；
 * - renderers 非对象 → {}；
 * - renderer 条目只保留纯对象条目，非对象条目丢弃；条目内容原样保留，
 *   不做 options 深验证——深验证在快照构建时按贡献做；
 * - 未知顶层 version（如 99）原样保留 version 值，不丢弃、不改写
 *   （设计 §6：未知版本保留原文，不用默认值写回覆盖）。
 *
 * 每次返回全新对象，不别名 DEFAULT，避免调用方误改共享默认值。
 */
export function parseStoredValuePresentationSettings(
  input: unknown,
): StoredValuePresentationSettings {
  if (!isRecord(input)) {
    return { version: 1, renderers: {} };
  }
  const version = typeof input.version === 'number' ? input.version : 1;
  const renderers: Record<string, StoredRendererSettings> = {};
  if (isRecord(input.renderers)) {
    for (const [id, entry] of Object.entries(input.renderers)) {
      // 只保留纯对象条目；条目内容原样保留（enabled / options 不在这里
      // 解释），深验证在 buildValuePresentationSnapshot 按贡献做。
      if (isRecord(entry)) {
        renderers[id] = entry as unknown as StoredRendererSettings;
      }
    }
  }
  return { version, renderers };
}

/**
 * 从存储值构建内存内展示快照（T04 的快照契约，T06 首次填充真实内容）：
 *
 * - 顶层 version !== 1 → 返回 configs {}、enabledRendererIds []、
 *   overrides {} 的空快照（原文保留在 store，不用默认值写回）；
 * - 逐 renderer 条目：registry.get(id) 缺失 → 跳过；entry.enabled !== true
 *   → 不进 enabledRendererIds、不进 configs；用贡献的 validateConfig
 *   校验 entry.options：ok → configs[id] =
 *   { configVersion, config: 校验后 config } 且 id 进 enabledRendererIds，
 *   fail → 跳过（原文保留在 store）；
 * - enabledRendererIds = enabled === true 且贡献存在且 options 有效的 id
 *   ——与 T04 计划构建器的 missing-config 区分配合：enabled 但无效的不进
 *   configs，计划构建时因 configs 缺失报 missing-config；
 * - overrides 恒为 {}（T10 再填）。
 */
export function buildValuePresentationSnapshot(
  stored: StoredValuePresentationSettings,
  environment: PresentationEnvironment,
  registry: ValuePresentationRegistry = valuePresentationRegistry,
  revision: number,
): ValuePresentationSnapshot {
  // 未知版本：原文保留在 store，这里只给空快照，绝不写回默认值。
  if (stored.version !== 1) {
    return {
      revision,
      environment,
      configs: {},
      enabledRendererIds: [],
      overrides: {},
    };
  }
  const configs: Record<string, VersionedPresentationConfig> = {};
  const enabledRendererIds: string[] = [];
  for (const [id, entry] of Object.entries(stored.renderers)) {
    const contribution = registry.get(id);
    if (contribution === undefined) continue;
    if (!isRecord(entry) || entry.enabled !== true) continue;
    let validated: ReturnType<ValuePresentationContribution['validateConfig']>;
    try {
      validated = contribution.validateConfig(entry.options);
    } catch {
      // validateConfig 契约要求不抛；防御性兜底视为无效配置（原文保留）。
      continue;
    }
    if (!validated.ok) continue;
    configs[id] = {
      configVersion: contribution.configVersion,
      config: validated.config,
    };
    enabledRendererIds.push(id);
  }
  return {
    revision,
    environment,
    configs,
    enabledRendererIds,
    overrides: {},
  };
}
