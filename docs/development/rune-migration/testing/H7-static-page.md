# TN H7 静态页加载 WASM 验证报告

## 范围

- 分支：`feat/rune-native-ui-migration`
- 被测提交：`9786c4b3 feat(rune): a standalone static page loads the wasm (issue-rune H7)`
- 新增：`crates/datazen-ui/web/`（`index.html`、`build.sh`、`.gitignore`）；改动：`crates/datazen-ui/README.md`（web 段落指向新页面）
- **零改动**：`packages/frontend`、`packages/web`（React 的 index 和路由未动，G1–G4 不适用）、两个 crate 的 Rust 代码

## 页面与构建

- `build.sh`：`cargo build -p datazen-ui --target wasm32-unknown-unknown` + `wasm-bindgen --target web`（0.2.129，与工作区锁定版本一致）产出到 `web/gen/`（gitignored，含 rune 的平台 snippet JS）。
- `index.html`：独立静态页，检查 `navigator.gpu` 后 `await init()` 实例化 wasm；加载失败或无 WebGPU 时显示错误文本；成功后移除占位符，由 wasm 全屏接管。
- 任何静态文件服务器可托管（README 给出 `python3 -m http.server` 一行）。

## 验证路径（无头 Chrome 157 + agent-browser）

1. 资源加载：`index.html`、`gen/datazen_ui.js`、`gen/snippets/.../bridge.js|health.js|platform.js`、wasm 全部 200。
2. wasm 启动：`init()` 完成，页面 `#status` 占位符被移除（DOM 验证）。
3. GPU 边界：rune 的 wasm 运行到适配器选择，控制台与页面上的固定 `<pre>` 面板显示 rune 自带的 "WebGPU adapter unavailable …" 信息——这是无 WebGPU 浏览器上的设计行为（rune 自家 web 应用同样处理）。
4. **沙箱环境边界**：无头 Chrome 即使带 `--enable-unsafe-swiftshader`、`--no-sandbox`、`--ignore-gpu-blocklist` 等标志，`navigator.gpu.requestAdapter()` 仍为 null（GPU 进程的 Dawn 枚举不到任何适配器；SwiftShader Vulkan ICD 在 bundle 里但起不来）。与 H1 的 libxkbcommon-x11 缺口同类：环境缺口，非代码缺口。真机 WebGPU 浏览器（Chrome/Edge）上的连接列表可视验证留给用户环境。

## 「和桌面相同的连接列表」

同一 crate（`datazen-ui`）、同一 `view`、同一运行时测试套（107 个 ui 测试覆盖连接列表渲染，含 `shell_renders_the_datazen_navigation`、`a_wired_adapter_replaces_the_demo_list_with_the_use_case_answer`）。桌面进程与 wasm 仅适配器注入不同（H6 后：native 注入 `UseCaseAdapter`，wasm 用 `UnimplementedAdapter` + 端口兜底）——两端连接列表同为 local/replica 两条演示连接；wasm 页在列表上方额外显示适配器未接通提示行（诚实状态，非缺陷）。

## 门禁

| 门禁 | 命令 | 结果 |
| --- | --- | --- |
| GR1 | `cargo test -p datazen-ui-api -p datazen-ui --offline --lib -- --test-threads=1` | 通过，exit 0（107 + 54 passed；本任务未改 crate 代码，复跑确认） |
| GR2 | `cargo check -p datazen-ui-api -p datazen-ui --offline` | 通过，exit 0 |
| G1–G4 | vitest / pnpm | 不适用（未改 `packages/frontend`、`packages/web`） |

## 结论与遗留

**放行。** 单独静态页就位并端到端加载 wasm 至 GPU 边界，React 宿主未动。遗留：真机 WebGPU 浏览器的可视确认；release + wasm-opt 的发布体积收缩（`web-build.sh` 的发布模式）留待产品化。
