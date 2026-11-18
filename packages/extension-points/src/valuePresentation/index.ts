/**
 * @datazen/extension-points/valuePresentation — 字段展示扩展（issue-32）。
 *
 * 贡献契约（§4）+ 集合注册中心（§3）+ 生命周期接线。包 exports 只有
 * "." 入口，本目录经 src/index.ts re-export 对外。
 */

export type {
  PresentationValue,
  ColumnSource,
  PresentationColumnContext,
  PresentationEnvironment,
  PresentationResult,
  PreparedPresentation,
  PresentationConfigValue,
  ConfigValidationProblem,
  ConfigValidationResult,
  VersionedPresentationConfig,
  ColumnPresentationOverride,
  PresentationPrepareInput,
  ValuePresentationContribution,
} from './contracts';
export type { ValuePresentationRegistry } from './registry';
export { ValuePresentationRegistryImpl, valuePresentationRegistry } from './registry';
export { registerValuePresentation } from './lifecycle';
