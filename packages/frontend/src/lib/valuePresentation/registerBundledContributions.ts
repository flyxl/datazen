/**
 * registerBundledContributions — 核心内置展示贡献的启动注册（issue-32 T06）。
 *
 * T05 遗留的生产缺口：至今没有任何生产代码注册 timestampContribution，
 * 所以默认快照永远是空的、时间列永远走默认展示。本模块补上启动注册点：
 * 核心内置贡献随应用启动激活、永不卸载。
 *
 * 设计 §6「EP 激活只注册能力」：激活只注册能力，不读取 Store、文件或
 * 接口；配置的加载与持久化由宿主（settingsStore / snapshot hook）负责。
 */

import {
  valuePresentationRegistry,
  type ValuePresentationRegistry,
} from '@datazen/extension-points';
import { TIMESTAMP_CONTRIBUTION_ID, timestampContribution } from './timestampContribution';

/**
 * 注册全部核心内置展示贡献（当前只有 `datazen.timestamp`）。
 *
 * 幂等：`get(TIMESTAMP_CONTRIBUTION_ID)` 已存在则不重复注册（注册中心
 * 对重复 ID 显式抛错，启动路径重复调用必须安全）。缺省注册到全局单例
 * `valuePresentationRegistry`；测试传入全新注册中心。
 */
export function registerBundledValuePresentationContributions(
  registry: ValuePresentationRegistry = valuePresentationRegistry,
): void {
  if (registry.get(TIMESTAMP_CONTRIBUTION_ID) !== undefined) return;
  registry.register(timestampContribution);
}
