/**
 * 每个 spec 文件开始前清空**共享夹具库**（`e2e/reset-sync-dbs.sh`）。
 *
 * 为什么放在 service 的 `beforeSuite` 里：串跑模式（`pnpm e2e` 不带
 * `--isolate-specs`）只有一个应用进程、一次 WDIO 会话，config 层没有「一个 spec
 * 文件开始」的钩子；service 的 `beforeSuite` 是唯一按 suite 触发的点。这里再按
 * `suite.file` 去重，于是语义就是「每个 spec 文件一次」，而不是每个 describe 一次。
 *
 * 隔离模式（`--isolate-specs`）下每个文件本来就是独立进程，由 `e2e/run.mjs` 在起
 * 应用之前重置，那边会设 `E2E_SHARED_FIXTURE_RESET=runner`，这里就不重复做。
 *
 * 关掉：`E2E_SKIP_SHARED_FIXTURE_RESET=1`。
 */
import { execFileSync } from 'node:child_process';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const SCRIPT = path.join(path.dirname(fileURLToPath(import.meta.url)), '..', 'reset-sync-dbs.sh');

/** 上一次重置发生在哪个 spec 文件；同文件里的后续 suite 不再重置。 */
let lastResetFile = '';

type SuiteLike = { file?: string; title?: string };

export default class SharedFixtureResetService {
  async beforeSuite(suite: SuiteLike): Promise<void> {
    if (process.env.E2E_SKIP_SHARED_FIXTURE_RESET === '1') return;
    if (process.env.E2E_SHARED_FIXTURE_RESET === 'runner') return;
    const file = suite?.file ?? '';
    if (!file) return;
    if (file === lastResetFile) return;
    lastResetFile = file;
    try {
      execFileSync('bash', [SCRIPT], {
        stdio: 'pipe',
        cwd: path.resolve(path.dirname(SCRIPT), '..'),
        env: { ...process.env, E2E_RESET_SYNC_QUIET: '1' },
      });
    } catch (error) {
      // 重置失败不该把一个 UI spec 判红；记一条日志让整包结论能解释。
      console.warn(
        `[e2e] shared fixture reset failed before ${path.basename(file)}: ${String(error)
          .split('\n')[0]
          .slice(0, 200)}`,
      );
    }
  }
}
