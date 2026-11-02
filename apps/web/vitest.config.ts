// apps/web 独立测试程序（P2b/E2）——只服务 web 形态内核（platform/web）的
// 单元/契约表测试。node 环境、零插件：被测的 backendTransport / httpRoutes /
// eventStreamSource / webUnsupportedMethods 全部不依赖 DOM 渲染层（DOM 侧
// 的 document.cookie 等宿主点在测试里一律注入替代）。
//
// 注意：根仓库的 vitest 配置不覆盖本目录；`pnpm --filter datazen-web test`
// 用本文件。include 只收 platform/web 测试目录下的 .test.ts，避免把
// 共享 frontend 树的测试拉进本程序。
import { defineConfig } from 'vitest/config';

export default defineConfig({
  test: {
    environment: 'node',
    include: ['src/platform/web/__tests__/**/*.test.ts'],
  },
});
