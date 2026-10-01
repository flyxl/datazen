/**
 * 契约层自检：门禁 F-05 / F-07、方法数与 `MethodMap` 的编译期一致性、TS 接线三处。
 *
 * **为什么要有这些测试**：F-07 是 CI 规则 `backend-client-transport-agnostic` 的前置
 * 形式——本包一旦出现 Tauri IPC 导入前缀或浏览器取数 API 字面量，桌面与 Web 就不能再
 * 共用同一套契约。这里的扫描在本地就能把违规挡下；禁用词以拼接方式构造，因为 CI 规则
 * 扫描的是**整个 src 目录**，本文件自己命中同样算违规。
 */

import { existsSync, readFileSync, readdirSync } from 'node:fs';
import { join, resolve } from 'node:path';
import { describe, expect, it } from 'vitest';

import { API_ERROR_CODES } from '@datazen/backend-client';
import { createBackendClient } from '@datazen/backend-client';
import type {
  BackendClient,
  BackendTransport,
  JobFilter,
  MethodMap,
} from '@datazen/backend-client';

// vitest 的 root 就是仓库根；jsdom 环境下 `import.meta.url` 不是 file: URL，
// 因此按 cwd 定位，并在扫描前断言入口确实存在——定位错了要当场失败，
// 不能让扫描静默扫到 0 个文件然后「通过」。
const REPO_ROOT = resolve(process.cwd());
const SRC_DIR = join(REPO_ROOT, 'packages/backend-client/src');
if (!existsSync(join(SRC_DIR, 'index.ts'))) {
  throw new Error(`backend-client 源码定位失败：${SRC_DIR}（vitest cwd=${process.cwd()}）`);
}

// ---------------------------------------------------------------------------
// 编译期一致性：`keyof BackendClient` 必须严格等于 `keyof MethodMap`。
// 加一个门面方法而忘了加 MethodMap（或反过来），这里直接编译失败。
// ---------------------------------------------------------------------------

type Equal<A, B> =
  (<T>() => T extends A ? 1 : 2) extends <T>() => T extends B ? 1 : 2 ? true : false;

/** 21 个方法名一一对应；`true` 只在两侧键集完全相同时可赋值。 */
export type ClientMethodParity = Equal<keyof BackendClient, keyof MethodMap>;

/** `MethodMap` 的键必须全部出现在门面上。 */
export type MethodMapCoveredByClient = Equal<keyof MethodMap, keyof BackendClient>;

/** `listJobs` 的载荷就是「过滤器或 null」，翻页游标不落盘（见 types/job.ts）。 */
export type ListJobsPayloadParity = Equal<
  MethodMap['listJobs']['request'],
  { readonly filter: JobFilter | null }
>;

const CLIENT_METHOD_PARITY: ClientMethodParity = true;
const METHOD_MAP_COVERED: MethodMapCoveredByClient = true;
const LIST_JOBS_PARITY: ListJobsPayloadParity = true;

function sourceFiles(dir: string): string[] {
  return readdirSync(dir, { withFileTypes: true }).flatMap((entry) => {
    const full = join(dir, entry.name);
    if (entry.isDirectory()) return sourceFiles(full);
    return entry.name.endsWith('.ts') ? [full] : [];
  });
}

/** 概要 §6.2 的 21 个方法，按客户端分组。增删任何一项都必须同步改这里。 */
const EXPECTED_METHODS = [
  // ConnectionClient（12）
  'listConnections',
  'openSession',
  'getSession',
  'executeInSession',
  'executeAtTarget',
  'setSessionContext',
  'attachSession',
  'detachSession',
  'closeSession',
  'getExecution',
  'cancelExecution',
  'subscribeEvents',
  // ProfileClient（3）
  'createConnection',
  'updateConnection',
  'disableConnection',
  // JobClient（4）
  'startJob',
  'listJobs',
  'getJob',
  'cancelJob',
  // ArtifactClient（1）
  'readArtifact',
  // SubmissionTokenClient（1）
  'issueSubmissionToken',
] as const;

describe('backend-client 契约', () => {
  it('门面方法与 MethodMap 键集完全一致（编译期 + 运行期）', () => {
    expect(CLIENT_METHOD_PARITY).toBe(true);
    expect(METHOD_MAP_COVERED).toBe(true);
    expect(LIST_JOBS_PARITY).toBe(true);

    // 这里只数方法名，响应不会被读取；`null` 是安全的占位响应。
    const transport = {
      call: async <K extends keyof MethodMap>(
        _method: K,
        _payload: MethodMap[K]['request'],
      ): Promise<MethodMap[K]['response']> => null as unknown as MethodMap[K]['response'],
    } satisfies BackendTransport;
    const client = createBackendClient('local', transport);
    expect(Object.keys(client).sort()).toEqual([...EXPECTED_METHODS].sort());
    expect(EXPECTED_METHODS).toHaveLength(21);
  });

  it('ApiErrorCode 枚举与 Rust 侧 ApiErrorCode::ALL 同为 31 项', () => {
    expect(API_ERROR_CODES).toHaveLength(31);
    expect(new Set(API_ERROR_CODES).size).toBe(31);
    expect(API_ERROR_CODES).toContain('configRevisionMismatch');
    expect(API_ERROR_CODES).toContain('outcomeUnknown');
  });

  it('门禁 F-07：本包源码不含 Tauri 传输前缀与浏览器取数 API', () => {
    // 禁用词必须**拼接**出来：本测试文件自身也位于 packages/backend-client/src，
    // 直接写字面量会被 CI 规则 backend-client-transport-agnostic 判为违规。
    const FORBIDDEN = [
      '@tauri' + '-apps/',
      ['fetch', '('].join(''),
      ['XMLHttp', 'Request'].join(''),
    ];
    const files = sourceFiles(SRC_DIR);
    expect(files.length).toBeGreaterThan(10);
    for (const file of files) {
      const text = readFileSync(file, 'utf8');
      for (const needle of FORBIDDEN) {
        expect(text, `${file} 命中禁用词 ${needle}`).not.toContain(needle);
      }
    }
  });

  it('门禁 F-05：本包不依赖 React（useBackendClient 由前端层实现）', () => {
    const REACT_IMPORTS = [/from\s+'react(-dom)?'/];
    for (const file of sourceFiles(SRC_DIR)) {
      const text = readFileSync(file, 'utf8');
      for (const pattern of REACT_IMPORTS) {
        expect(text, `${file} 依赖了 ${String(pattern)}`).not.toMatch(pattern);
      }
    }
  });

  it('TS 别名恰好接在三处：tsconfig paths、tsconfig include、vite alias', () => {
    const tsconfig = JSON.parse(readFileSync(join(REPO_ROOT, 'tsconfig.json'), 'utf8')) as {
      compilerOptions: { paths: Record<string, string[]> };
      include: string[];
    };
    expect(tsconfig.compilerOptions.paths['@datazen/backend-client']).toEqual([
      './packages/backend-client/src/index.ts',
    ]);
    expect(tsconfig.include).toContain('packages/backend-client');

    const viteConfig = readFileSync(join(REPO_ROOT, 'vite.config.ts'), 'utf8').replace(/\s+/g, ' ');
    expect(viteConfig).toContain(
      "'@datazen/backend-client': resolve(__dirname, 'packages/backend-client/src/index.ts'),",
    );
  });

  it('backend-client 不是 Cargo 成员（它只有 TS 契约）', () => {
    const manifest = readFileSync(join(REPO_ROOT, 'Cargo.toml'), 'utf8');
    expect(manifest).not.toContain('packages/backend-client');
    expect(manifest).toContain('"packages/platform-api"');
    expect(manifest).toContain('"packages/application"');
  });
});
