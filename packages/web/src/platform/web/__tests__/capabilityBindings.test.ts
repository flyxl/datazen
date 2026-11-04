/**
 * WR-E4 能力驱动的 web 装配测试 —— node 环境、全手写假体：
 *
 * - 表行 `updater: false` → 装配**不绑定**（账目 `updater: null`），且
 *   `WebUpdaterOffProof` 在编译期证明表行为 false（表翻真即编译失败）；
 * - 表行驱动的选择器是表的纯函数：`true` → 原实现（同一引用），`false` →
 *   静态 all-throw 端口（`CapabilityUnsupported`，kind `capability`）；
 * - `versionSource: 'build-constant'` → 版本即构建常量；桌面专属
 *   `host-app` 行命中即装配错误（不是探测回退路径）；
 * - web 表节集与桌面专属节（`settingsSections.ts` 的
 *   `DESKTOP_SETTINGS_SECTIONS` 七节）零交集；
 * - 编译期同形纪律：packages/web/src 全树零 `@tauri-apps`、零运行时形态探测
 *   标记（`IS_DESKTOP_BUILD` / `isTauri` / `__TAURI`）。
 */
import { describe, expect, it } from 'vitest';
import { readdir, readFile } from 'node:fs/promises';
import path from 'node:path';
import { ApiError } from '@datazen/backend-client';
import { WEB_CAPABILITIES } from '@datazen/frontend';
import {
  WEB_BUILD_VERSION,
  WEB_CAPABILITY_BINDINGS,
  resolveVersionByTable,
  selectByCapability,
  unsupportedCapabilityPort,
  type WebUpdaterOffProof,
  type WebVersionSourceProof,
} from '../capabilityBindings';

/** 编译期证明落点：表行不是预期值时这两个常量直接编译失败。 */
const UPDATER_OFF: WebUpdaterOffProof = true;
const VERSION_BUILD_CONSTANT: WebVersionSourceProof = true;
void UPDATER_OFF;
void VERSION_BUILD_CONSTANT;

/** `settingsSections.ts` 的 `DESKTOP_SETTINGS_SECTIONS`（宿主供应的七节）。 */
const DESKTOP_ONLY_SECTIONS: readonly string[] = [
  'logging',
  'ai',
  'prompts',
  'mcpServer',
  'mcpClient',
  'tunnels',
  'extensions',
];

describe('WR-E4 capability-driven web assembly', () => {
  it('binds nothing for the updater row: the desktop-only item is structurally absent', () => {
    expect(WEB_CAPABILITIES.updater).toBe(false);
    expect(WEB_CAPABILITY_BINDINGS.updater).toBeNull();
    // 账目如实回放表：capabilities 行就是装配输入的那张表。
    expect(WEB_CAPABILITY_BINDINGS.capabilities).toBe(WEB_CAPABILITIES);
  });

  it('selectByCapability is a pure function of the table row, never a probe', async () => {
    const implementation = { read: () => 'impl' };
    // true 行：同一实现原样绑定（引用相等）。
    expect(selectByCapability(true, implementation, 'updater')).toBe(implementation);
    // false 行：静态 all-throw 替身——显式缺口，绝不静默空答。
    const fallback = selectByCapability(false, implementation, 'updater');
    expect(fallback).not.toBe(implementation);
    expect(() => (fallback as unknown as { read(): string }).read()).toThrowError(ApiError);
    try {
      (fallback as unknown as { read(): string }).read();
      expect.unreachable('the disabled row must throw');
    } catch (error) {
      expect(error).toBeInstanceOf(ApiError);
      const apiError = error as ApiError;
      expect(apiError.code).toBe('CapabilityUnsupported');
      expect((apiError as unknown as { unsupportedKind?: string }).unsupportedKind).toBe(
        'capability',
      );
      expect(apiError.message).toContain('updater');
    }
    // 通用替身：任何形状的端口拿到同一口径的失败（方法调用即抛）。
    const anyPort = unsupportedCapabilityPort<{ ping(): Promise<void> }>('updater');
    expect(() => (anyPort as unknown as { ping(): unknown }).ping()).toThrowError(ApiError);
  });

  it('resolves the version from the build constant row and refuses desktop-only rows', () => {
    expect(WEB_CAPABILITIES.versionSource).toBe('build-constant');
    expect(resolveVersionByTable(WEB_CAPABILITIES)).toBe(WEB_BUILD_VERSION);
    expect(WEB_CAPABILITY_BINDINGS.version).toBe(WEB_BUILD_VERSION);
    // host-app 是桌面专属行：命中即装配错误（编译期决定，无探测回退）。
    expect(() =>
      resolveVersionByTable({ ...WEB_CAPABILITIES, versionSource: 'host-app' }),
    ).toThrowError(/never appears in the web table/);
  });

  it('the web table shares no section with the desktop-only host sections', () => {
    for (const section of DESKTOP_ONLY_SECTIONS) {
      expect(WEB_CAPABILITIES.settingsSections.has(section as never)).toBe(false);
    }
    // 共享契约节仍在（web 表的正面清单）。
    expect(WEB_CAPABILITIES.settingsSections.has('general')).toBe(true);
  });

  it('compile-time discipline: zero @tauri-apps imports and zero form probes in packages/web/src', async () => {
    // import.meta.url 锚定本文件（…/packages/web/src/platform/web/__tests__）。
    const here = path.dirname(new URL(import.meta.url).pathname);
    const root = path.resolve(here, '..', '..', '..', '..'); // = packages/web
    const sources: string[] = [];
    const walk = async (dir: string): Promise<void> => {
      for (const entry of await readdir(dir, { withFileTypes: true })) {
        const full = path.join(dir, entry.name);
        if (entry.isDirectory()) {
          // 测试目录豁免：本测试的文档注释就写着 `@tauri-apps`（审计者
          // 不是被审计的生产面）；断言对象 = packages/web/src 生产源码树。
          if (entry.name === '__tests__') continue;
          await walk(full);
        } else if (/\.(ts|tsx)$/.test(entry.name)) {
          sources.push(full);
        }
      }
    };
    await walk(path.join(root, 'src'));
    expect(sources.length).toBeGreaterThan(0);
    // 断言对象是**代码形态**，不是文档提及：import 语句形态的
    // `@tauri-apps`，与不在反引号内的探测标识符（文档注释里对
    // 「为什么没有探测」的解释用反引号提及，合法且应当保留）。
    for (const file of sources) {
      const text = await readFile(file, 'utf8');
      expect(text, path.relative(root, file)).not.toMatch(
        /from\s+['"]@tauri-apps|import\s+['"]@tauri-apps/,
      );
      expect(text, path.relative(root, file)).not.toMatch(
        /(?<!`)\b(IS_DESKTOP_BUILD|isTauri|__TAURI)\b(?!`)/,
      );
    }
  });
});
