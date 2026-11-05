/**
 * Repo-relative source paths for the specs that assert on **源码文本**
 * （不是页面）。
 *
 * D 重构把前端从仓库根的 `src/` 搬进了 `packages/frontend/src/`；这类断言读的是
 * 文件而不是 DOM，路径写死就会在每次搬家后整族变红（本轮 `docs-online` /
 * `file-connection-fields-theme` / `path-ipc-hardening` 就是这么红的）。集中到
 * 这里，下一次搬家只改这一处。
 */
import path from 'node:path';

export const REPO_ROOT = path.resolve(import.meta.dirname, '../..');

/** 共享前端包的源码根（`@datazen/frontend`）。 */
export const FRONTEND_SRC = path.join(REPO_ROOT, 'packages/frontend/src');

/** 拼一个前端源码文件路径，例如 `frontendSource('lib/docsUrls.ts')`。 */
export function frontendSource(...segments: string[]): string {
  return path.join(FRONTEND_SRC, ...segments);
}

/** Desktop 宿主 crate 源码根（crate `datazen_desktop`，原 `src-tauri/`）。 */
export const DESKTOP_CRATE_SRC = path.join(REPO_ROOT, 'crates/datazen-desktop/src');

/** 拼一个 desktop crate 源码路径，例如 `desktopSource('lib.rs')`。 */
export function desktopSource(...segments: string[]): string {
  return path.join(DESKTOP_CRATE_SRC, ...segments);
}

/** Desktop 壳包源码根（`packages/desktop`：入口、平台适配、shell）。 */
export const DESKTOP_PKG_ROOT = path.join(REPO_ROOT, 'packages/desktop');

/** 拼一个 desktop 壳包内的路径，例如 `desktopPackage('tailwind.config.ts')`。 */
export function desktopPackage(...segments: string[]): string {
  return path.join(DESKTOP_PKG_ROOT, ...segments);
}
