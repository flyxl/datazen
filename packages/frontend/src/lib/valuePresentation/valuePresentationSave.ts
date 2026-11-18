/**
 * valuePresentation 保存的路径化更新（issue-32 T11，设计 §6 / PRD §5/§6
 * 验收 10/12）。
 *
 * 问题：`StoredValuePresentationSettings` 是一份整份重写的对象；T07
 * 之前面板与对话框都从本窗口 store 的本地值拼整份 `next` 再保存。
 * 两窗口同时保存时，后保存的一方用自己读到的旧基底覆盖对方刚写
 * 的字段（A 改全局格式、B 改规则/覆盖 → 只剩后保存一方的字段），
 * 且整份重写还会丢弃未知版本 payload 里本面板不知道的字段
 * （T10 P10(d) 观察项）。
 *
 * 裁决（T11，不新增后端接口——`saveSettings` 是整份写、无修订号/
 * ETag，版本条件更新需要内核 + backend-client + Rust 三层新契
 * 约，超出本任务范围）：
 *
 * 1. 路径化更新：保存调用方不再传整份对象，改为传一个 updater
 *    `(fresh) => next`；settingsStore 在保存前重读一次权威配置，
 *    updater 只替换调用方拥有的路径（时间渲染器一项、规则列表、
 *    单个列覆盖），其余字段一律取 fresh 原文。A 改格式、B 改规则/
 *    覆盖时，两次保存各写各的路径，最终一致且无字段丢失。
 * 2. 规则列表是一条路径：列表整体替换，两窗口同时改规则仍是后写
 *    胜出（列表无单条身份可做逐条合并）；面板在收到他窗更新时若
 *    有未保存的规则编辑会提示、由用户决定是否重新加载（见
 *    ValuePresentationSettingsSection 的外部更新提示）。
 * 3. 残余窗口：重读与保存之间仍有竞态（无后端修订号无法彻底关
 *    闭）；本模块只把窗口从「整个本地基底陈旧」缩到「重读→保存
 *    的毫秒级间隔」，并在汇报里写明。重读失败或权威响应无
 *    valuePresentation 字段（旧后端）时回落本地基底，不阻塞保存。
 * 4. 未知版本：`withValuePresentationRules` 只在 version 1 写
 *    rules；其余 helper 一律 `{ ...fresh }` 展开，fresh 里的未知
 *    字段原样保留，绝不用默认值写回覆盖。
 *
 * 本模块是纯函数，不读 Store / IPC / DOM；store 的保存链路与面板
 * 的调用见 settingsStore.ts / columnOverrides.ts。
 */

import { TIMESTAMP_CONTRIBUTION_ID } from './timestampContribution';
import type { StoredValuePresentationSettings } from './settings';
import type { TimestampOptions } from './timestampConfig';
import type { ValuePresentationRule } from './valuePresentationRules';

/**
 * valuePresentation 保存的输入：整份对象（旧形式：整份替换）或
 * updater（新形式：只改拥有路径，其余取 fresh 原文）。store 接受
 * 两种形式；T07 面板、T09/T10 对话框与 saveColumnOverride 一律
 * 用 updater 形式。
 */
export type ValuePresentationSettingsUpdate =
  | StoredValuePresentationSettings
  | ((fresh: StoredValuePresentationSettings) => StoredValuePresentationSettings);

/** 把一次保存输入解析为下一份存储值（对象原样返回，updater 作用于 fresh）。 */
export function applyValuePresentationUpdate(
  fresh: StoredValuePresentationSettings,
  update: ValuePresentationSettingsUpdate,
): StoredValuePresentationSettings {
  return typeof update === 'function' ? update(fresh) : update;
}

/**
 * 只替换 `datazen.timestamp` 渲染器一项（enabled + options）：
 * 其余 renderer 条目、rules、columnOverrides 与 version 一律取
 * fresh 原文。未知版本也不丢字段（`{ ...fresh }` 展开保留一切）。
 */
export function withTimestampRenderer(
  fresh: StoredValuePresentationSettings,
  enabled: boolean,
  options: TimestampOptions,
): StoredValuePresentationSettings {
  return {
    ...fresh,
    renderers: {
      ...fresh.renderers,
      [TIMESTAMP_CONTRIBUTION_ID]: { enabled, options },
    },
  };
}

/**
 * 整体替换全局规则列表。仅 version 1 写 rules；未知版本原样返回
 * fresh（不按 v1 形状解释其 rules、不写回、不丢字段）。
 */
export function withValuePresentationRules(
  fresh: StoredValuePresentationSettings,
  rules: ReadonlyArray<ValuePresentationRule>,
): StoredValuePresentationSettings {
  if (fresh.version !== 1) return fresh;
  return { ...fresh, rules: [...rules] };
}
