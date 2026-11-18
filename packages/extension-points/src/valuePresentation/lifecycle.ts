/**
 * valuePresentation 注册帮助函数 — 接入扩展模块生命周期（issue-32 设计 §3）。
 *
 * 对齐 src/lifecycle.ts 的 registerExtensionPoint：将释放句柄记入
 * ExtensionContext.subscriptions，使 HostExtensionLoader.unload 时按 LIFO
 * 自动清理；activate 抛错时由 load 的失败清理路径释放。
 */

import type { ExtensionContext } from '../lifecycle';
import type { ValuePresentationContribution } from './contracts';
import type { ValuePresentationRegistry } from './registry';
import { valuePresentationRegistry } from './registry';

/**
 * 注册一个字段展示贡献并跟踪卸载句柄。
 *
 * @param context 扩展模块激活上下文（句柄 push 进 subscriptions）
 * @param contribution 展示贡献
 * @param registry 目标注册中心，缺省为全局单例 valuePresentationRegistry
 */
export function registerValuePresentation(
  context: ExtensionContext,
  contribution: ValuePresentationContribution,
  registry: ValuePresentationRegistry = valuePresentationRegistry,
): void {
  const release = registry.register(contribution);
  context.subscriptions.push(release);
}
