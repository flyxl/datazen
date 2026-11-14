# TN H6 WASM 包能编过 验证报告

## 范围

- 分支：`feat/rune-native-ui-migration`
- 被测提交：`66c65eae feat(rune): the datazen-ui wasm package builds (issue-rune H6)`
- 改动面：根 `Cargo.toml`（rune 特性去掉 `webgl`）、`crates/datazen-ui-api`（桌面用例适配器及其 datazen 依赖改为 native 专属：`[target.'cfg(not(target_arch = "wasm32"))'.dependencies]` + 9 处 `#[cfg(not(target_arch = "wasm32"))]`）、`crates/datazen-ui/src/lib.rs`（`UseCaseAdapter` 导入与注入点 cfg 化）、`crates/datazen-ui/README.md`（wasm 构建说明）

## 验证路径

- `rustup target add wasm32-unknown-unknown`（沙箱新装）
- `cargo build -p datazen-ui --target wasm32-unknown-unknown --offline` → **Finished**，产物 `target/wasm32-unknown-unknown/debug/datazen_ui.wasm`（cdylib，debug 42MB；发布用 release + wasm-opt 收缩，属 H7）
- 不挂页面：`packages/frontend`、`packages/web` 零改动（G1–G4 不适用）

## 排障记录（两个拦路虎）

1. **mio 不支持 wasm32**：H3 给 ui-api 引入的 datazen 依赖带着 execution 特性（tokio net → mio）。而 connection 单独编不过（connection 的 DTO 面硬性再导出 execution 词汇，见 `crates/datazen/src/connection/dto.rs`）。**解法**：桌面用例适配器本来就是桌面宿主的——依赖与代码全部收进 `cfg(not(wasm32))`；wasm 宿主经内存端口跑同一套 UI 词汇，将来带自己的适配器。native 测试不受影响（全过）。
2. **wgpu 的 WebGL 后端让 hal 实例 ！Sync**：rune `render.rs` 的进程级实例缓存 `static Mutex<Vec<(Backends, Instance)>>` 要求 Sync；启用 `webgl` 特性后 wasm32 上 `DynInstance` 不满足。rune 自家 web 应用（`vendor/rune/www`）同样不带 webgl 发布（WebGPU-only，其 index.html 直接抛 "WebGPU requires a supported browser"）。**解法**：工作区 rune 特性去掉 `webgl`，注释写明因果。

## 门禁

| 门禁 | 命令 | 结果 |
| --- | --- | --- |
| GR1 | `cargo test -p datazen-ui-api -p datazen-ui --offline --lib -- --test-threads=1` | 通过，exit 0（107 + 54 passed，特性变更后重跑） |
| GR2 | `cargo check -p datazen-ui-api -p datazen-ui --offline` | 通过，exit 0 |
| G1–G4 | vitest / pnpm | 不适用（未改 `packages/frontend`、`packages/web`） |

## 结论与遗留

**放行。** WASM 包编译通过且未挂任何页面。遗留：H7 做单独静态页加载该 WASM（不改 React 的 index 和路由）；真机浏览器（WebGPU）验证留给用户环境；wasm 侧连接列表由端口兜底显示（与桌面相同的演示列表）。
