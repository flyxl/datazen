/**
 * 能力驱动的装配降级（WR-E4）。
 *
 * `WEB_CAPABILITIES` 静态表（F5 已建）从这里开始成为 web 装配的**实际输入**：
 * 表里 `false` / 缺席的能力，装配期就降级——要么不绑定（updater：web 装配
 * 没有 updater 端口槽位，桌面专属项结构性缺席），要么绑定「如实抛
 * `CapabilityUnsupported`（kind `capability`）的静态形态」。选择发生在
 * 编译期（模块常量表 → 纯函数选择器），**零运行时形态探测**——本文件里
 * 没有 `isTauri` / `IS_DESKTOP_BUILD` / UA 嗅探，机制刻意沿用
 * `DATAZEN_UPDATER_CHANNEL` 的「plain module constant」先例
 * （packages/frontend/src/platform/capabilities.ts 头注）。
 *
 * 消费者永远分支在表上（`requireCapabilities()`），从不分支在形态上；
 * 本文件只回答装配期的问题：「这张表允许绑定什么」。
 */

import { ApiError } from '@datazen/backend-client';
import { WEB_CAPABILITIES, type PlatformCapabilities } from '@datazen/frontend';

/**
 * 构建期版本常量（`versionSource: 'build-constant'` 的唯一事实源）。
 *
 * 桌面形态的 `versionSource: 'host-app'` 向宿主运行时询问版本；web 没有
 * 宿主，版本来自构建脚本写入（或手写）的模块常量——与生成的更新渠道
 * 常量同一先例，无任何运行时环境嗅探。构建脚本替换此字面量即可发布。
 */
export const WEB_BUILD_VERSION = '0.0.0-web-build';

/**
 * 静态 all-throw 端口：表说该能力不属于此形态时，装配期绑定的替身。
 *
 * 任何方法调用都抛 `CapabilityUnsupported`（kind `capability`，与
 * `unsupportedMethods` 同口径）——**显式缺口，绝不静默空答**（§13 / §7.4）。
 * 形状由调用点的泛型实参静态决定；`Proxy` 只是「每个方法都同错」的机械，
 * 不含任何形态判断。
 */
export function unsupportedCapabilityPort<T>(capability: string): T {
  const reject = (): never => {
    throw new ApiError(
      'CapabilityUnsupported',
      `${capability} is not part of this form's capability table (WR-E4): the static table disabled it at assembly time.`,
      { unsupportedKind: 'capability' },
    );
  };
  // get 陷阱：每个属性都解析成「调用即抛」的同一静态方法——端口形状由
  // 调用点的泛型实参决定；Proxy 只提供「每个方法同一错」的机械，
  // 不含任何形态判断（目标可以是任意冻结对象）。
  return new Proxy(Object.freeze({ __capability: capability }), { get: () => reject }) as T;
}

/**
 * 表行驱动的端口选择器（编译期决定）：`enabled` 直接来自能力表行——
 * `true` 绑定形态实现，`false` 绑定静态 all-throw 端口。同名桌面侧装配
 * 传 `DESKTOP_CAPABILITIES` 的行即得桌面实现；两端装配**同形不同表**。
 */
export function selectByCapability<T>(enabled: boolean, port: T, capability: string): T {
  return enabled ? port : unsupportedCapabilityPort<T>(capability);
}

/**
 * `versionSource` 的装配期解析：表行决定版本事实来自哪里。
 *
 * - `build-constant`（web 表）：本模块常量；
 * - `host-app` / `http`：web 表永不出现的行——编译期穷尽断言守住，命中即
 *   装配错误（不是运行时探测的回退路径）。
 */
export function resolveVersionByTable(table: PlatformCapabilities): string {
  switch (table.versionSource) {
    case 'build-constant':
      return WEB_BUILD_VERSION;
    case 'host-app':
      throw new Error(
        'resolveVersionByTable: "host-app" never appears in the web table — bind the desktop assembly instead (WR-E4).',
      );
    case 'http':
      throw new Error(
        'resolveVersionByTable: "http" has no web route bound yet — register the wiring gap instead of probing (WR-E4).',
      );
    default: {
      const never: never = table.versionSource;
      throw new Error(`resolveVersionByTable: unknown versionSource ${String(never)}`);
    }
  }
}

/**
 * web 装配的能力绑定账目（静态、可断言）：
 *
 * - `updater: false` → **不绑定**（`null`）：`PlatformBindings` 本就没有
 *   updater 端口槽位，桌面专属项在 web 装配结构性缺席；
 * - `versionSource` → 表行解析出的构建常量；
 * - `unsupported` → 若有消费者误触桌面专属面，拿到的是 all-throw 静态
 *   端口（此处预铸 updater 替身，供消费点经 `selectByCapability` 复用）。
 */
export interface WebCapabilityBindings {
  readonly capabilities: PlatformCapabilities;
  /** 表行 `updater` 的装配结果：web 恒为 null（不绑定）。 */
  readonly updater: null;
  /** 表行 `versionSource` 的装配结果：构建常量。 */
  readonly version: string;
  /** 误触桌面专属 updater 面时的静态替身（all-throw）。 */
  readonly updaterUnsupportedPort: never;
}

/** 由 `WEB_CAPABILITIES` 装配出的绑定账目（模块常量，非运行时探测）。 */

/** 编译期证明：web 表的 `updater` 行是 `false` —— 表若翻真，此处编译失败。 */
export type WebUpdaterOffProof = WEB_CAPABILITIES['updater'] extends false ? true : never;

/** 编译期证明：web 表的版本来源是构建常量（桌面专属 `host-app` 永不出现）。 */
export type WebVersionSourceProof = WEB_CAPABILITIES['versionSource'] extends 'build-constant'
  ? true
  : never;

export const WEB_CAPABILITY_BINDINGS: WebCapabilityBindings = {
  capabilities: WEB_CAPABILITIES,
  updater: null,
  version: resolveVersionByTable(WEB_CAPABILITIES),
  updaterUnsupportedPort: unsupportedCapabilityPort<never>('updater'),
};
