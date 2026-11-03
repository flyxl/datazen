/**
 * WR-F4: wapp 资产服务的**双形态解析**（`datazen://` → 真实 origin 静态服务）。
 *
 * 桌面形态沿用自定义 scheme（`datazen://<id>/<path>`，Tauri URI scheme 协议
 * 处理，见 apps/desktop `wapps/protocol.rs`）；web 形态把资产服务换成
 * **同源静态路径约定**：`{origin}/wapps/{wapp id}/{package-relative path}`。
 * 仅资产服务协议随形态变化 —— 路径段、查询串、wapp id 文法与路径安全规则
 * 与桌面 `parse_datazen_uri` 逐字同构（shared kernel：modules/src/wapps/protocol.rs）。
 *
 * 形态选择走**装配期注入**（与 `configurePlatform` 同一登记风格），不是
 * 运行时探测：共享代码调 `buildWappAssetSrc`，默认绑定桌面解析器（桌面
 * 产物逐字节不变），web 装配根（apps/web `platform/web/assembly.ts`）绑
 * `WEB_WAPP_ASSET_RESOLVER`。编译期同形纪律：零 `@tauri-apps`、零
 * `IS_DESKTOP_BUILD` 运行时分支外泄到调用点。
 */

/** web 静态服务的路径前缀（server 静态 origin 约定，相对 host origin）。 */
export const WAPP_ASSET_PATH_PREFIX = '/wapps/';

/**
 * web 形态的 wapp 资产 CSP（WR-F4 单源化）。
 *
 * 桌面双源（`'self' datazen:`）的存在理由是 WebKit 把自定义 scheme 文档
 * 当 opaque origin、必须点名 `datazen:` 才放行自身子资源（BUG-F9-04）。
 * web 形态资产与宿主同源 —— 一个真实 origin，`'self'` 一处覆盖全部指令，
 * `datazen:` 源随之删除；`connect-src 'none'`（插件离线）与 `data:`
 * 仅限图片（内联 SVG）语义保持不变。与桌面 `ASSET_CSP` 逐指令对照：
 * 只有源列表收窄，指令集合不变。
 */
export const WAPP_ASSET_CSP =
  "default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; " +
  "img-src 'self' data:; font-src 'self'; connect-src 'none'";

/**
 * wapp id 文法（与 modules `WAPP_ID_RE` 同构）：
 * `<publisher>.<name>`，全小写，`[a-z0-9][a-z0-9-]{0,30}\.[a-z][a-z0-9-]{1,31}`。
 */
const WAPP_ID_RE = /^[a-z0-9][a-z0-9-]{0,30}\.[a-z][a-z0-9-]{1,31}$/;

/** Is `id` a syntactically valid wapp id (publisher-dot grammar)? */
export function isValidWappId(id: string): boolean {
  return WAPP_ID_RE.test(id);
}

/**
 * Lenient percent-decoding — 与桌面 `percent_decode` 同语义：非法转义保留
 * 原文，非 UTF-8 序列有损替换。校验发生在解码**之后**，编码穿越无法绕过。
 */
function percentDecode(input: string): string {
  if (!input.includes('%')) return input;
  const bytes: number[] = [];
  for (let i = 0; i < input.length; i += 1) {
    if (input[i] === '%' && i + 2 < input.length) {
      const hi = Number.parseInt(input[i + 1]!, 16);
      const lo = Number.parseInt(input[i + 2]!, 16);
      if (!Number.isNaN(hi) && !Number.isNaN(lo)) {
        bytes.push(hi * 16 + lo);
        i += 2;
        continue;
      }
    }
    bytes.push(input.charCodeAt(i));
  }
  return new TextDecoder('utf-8', { fatal: false }).decode(new Uint8Array(bytes));
}

/**
 * 路径安全（与桌面 `safe_relative_path` 同语义）：每个 `/` 分量非空、不以
 * `.` 开头（拒绝 `.storage.json` / `.enabled` / `..` / 任何隐藏分量），整串
 * 不含反斜杠与 NUL。
 */
export function isSafeWappAssetPath(path: string): boolean {
  if (path.length === 0 || path.includes('\\') || path.includes('\0')) return false;
  return path.split('/').every((component) => component.length > 0 && !component.startsWith('.'));
}

/** Decoded query parameters keyed by name（last wins，`+` 为空格）。 */
export type WappAssetQueryMap = Record<string, string>;

function parseQuery(raw: string | null): WappAssetQueryMap {
  const query: WappAssetQueryMap = {};
  if (!raw) return query;
  for (const pair of raw.split('&')) {
    if (pair.length === 0) continue;
    const eq = pair.indexOf('=');
    const key = eq === -1 ? pair : pair.slice(0, eq);
    const value = eq === -1 ? '' : pair.slice(eq + 1);
    query[percentDecode(key.replace(/\+/g, ' '))] = percentDecode(value.replace(/\+/g, ' '));
  }
  return query;
}

/** `parseWappAssetTarget` 的产物：wapp id + 包相对路径 + 解码查询。 */
export interface WappAssetTarget {
  wappId: string;
  /** 包相对路径或 deep-link 命令（首段 `open` 由服务端路由裁决）。 */
  path: string;
  query: WappAssetQueryMap;
}

/**
 * 解析 web 静态资产 URL（origin 相对路径形）为 `(wapp id, path, query)`。
 *
 * 与桌面 `parse_datazen_uri` 同一套拒绝语义：未知前缀、非法 wapp id、
 * 空路径、解码后不安全的路径分量一律抛错 —— 解析失败即 404，绝不静默。
 * 输入可以是完整 URL（`https://host/wapps/…`）或 origin 相对路径。
 */
export function parseWappAssetTarget(url: string): WappAssetTarget {
  const withoutOrigin = /^[a-z][a-z0-9+.-]*:\/\//i.test(url)
    ? new URL(url).pathname + new URL(url).search
    : url;
  const [pathAndFile, rawQuery] = withoutOrigin.split('?') as [string, string | undefined];
  if (!pathAndFile.startsWith(WAPP_ASSET_PATH_PREFIX)) {
    throw new Error(`unsupported wapp asset path: ${pathAndFile}`);
  }
  const rest = pathAndFile.slice(WAPP_ASSET_PATH_PREFIX.length);
  const slash = rest.indexOf('/');
  const wappId = slash === -1 ? rest : rest.slice(0, slash);
  const rawPath = slash === -1 ? '' : rest.slice(slash + 1);
  if (!isValidWappId(wappId)) {
    throw new Error(`invalid wapp host: \`${wappId}\``);
  }
  const path = percentDecode(rawPath);
  if (!isSafeWappAssetPath(path)) {
    throw new Error(`unsafe path component in \`${path}\``);
  }
  return { wappId, path, query: parseQuery(rawQuery ?? null) };
}

// ---------------------------------------------------------------------------
// Form binding: the resolver registry (assembly-time injection, desktop default)
// ---------------------------------------------------------------------------

/** 一条形态的资产 URL 解析面：页面入口与贡献图标两类产物。 */
export interface WappAssetResolver {
  /** Page entry URL for an iframe src (carries `?v=` cache buster). */
  buildSrc(targetId: string, entry: string, version: string): string;
  /** Contributed icon URL (`WappIcon`). */
  buildIconSrc(targetId: string, icon: string): string;
}

function stripLeadingDotSlash(path: string): string {
  return path.replace(/^\.\//, '');
}

function encodeVersion(version: string): string {
  return encodeURIComponent(version);
}

/** 桌面解析器：`datazen://<id>/<path>`（Tauri 自定义 scheme，路径逐字节不变）。 */
export const DESKTOP_WAPP_ASSET_RESOLVER: WappAssetResolver = {
  buildSrc(targetId, entry, version) {
    return `datazen://${targetId}/${stripLeadingDotSlash(entry)}?v=${encodeVersion(version)}`;
  },
  buildIconSrc(targetId, icon) {
    return `datazen://${targetId}/${stripLeadingDotSlash(icon)}`;
  },
};

/**
 * web 解析器：`/wapps/<id>/<path>`（真实 origin 静态服务，同源相对路径）。
 * 查询串与桌面形一致（`?v=<version>` 作缓存破坏符）。
 */
export const WEB_WAPP_ASSET_RESOLVER: WappAssetResolver = {
  buildSrc(targetId, entry, version) {
    return `${WAPP_ASSET_PATH_PREFIX}${targetId}/${stripLeadingDotSlash(entry)}?v=${encodeVersion(version)}`;
  },
  buildIconSrc(targetId, icon) {
    return `${WAPP_ASSET_PATH_PREFIX}${targetId}/${stripLeadingDotSlash(icon)}`;
  },
};

let resolver: WappAssetResolver = DESKTOP_WAPP_ASSET_RESOLVER;

/**
 * 装配根在首帧前绑定形态解析器。重复调用覆盖旧绑定（测试可重绑）。
 */
export function configureWappAssetResolver(next: WappAssetResolver): void {
  resolver = next;
}

/** 恢复桌面默认绑定（测试缝）。 */
export function resetWappAssetResolver(): void {
  resolver = DESKTOP_WAPP_ASSET_RESOLVER;
}

/** wapp 页面入口 URL（`WappPageShell.buildSrc` 的唯一定点）。 */
export function buildWappAssetSrc(targetId: string, entry: string, version: string): string {
  return resolver.buildSrc(targetId, entry, version);
}

/** wapp 贡献图标 URL（`WappIcon` 的唯一定点）。 */
export function buildWappIconSrc(targetId: string, icon: string): string {
  return resolver.buildIconSrc(targetId, icon);
}
