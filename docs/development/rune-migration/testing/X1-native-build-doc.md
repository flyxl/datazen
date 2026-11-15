# X1 纯 native 包编译指南 — 测试报告

- 任务：写文档说明怎么编译纯 native 包——新建 `docs/development/rune-migration/building-native.md`；同步瘦身 native-host.md 的构建节（指向新指南）、修正 README 的过时驱动声明（"sqlite wired end to end today" → basic SKU 四驱动已链接、sqlite 全验、三家远程待真机冒烟）。
- 日期：2026-11-15
- 提交：见 tasks.md X1 行。
- 性质：**纯文档任务**，无 Rust 代码变更（T11 同款先例：门禁基线沿用 + 新鲜复跑关键门禁佐证）。

## 结论

通过。指南的每一条命令都在本沙箱真跑过：GR3/GR4 复跑双绿，release 构建、无头冒烟、`.deb` 构建与解包核验全部完成；文档中的每个数字（体积、内存峰值、磁盘占用、版本号形态）来自本沙箱实测而非转抄。过程中撞出的三类环境问题（fat LTO 内存、libxkbcommon dev 链接、registry 缓存）全部沉淀为指南第 8 节的排障条目——排障表不是编的，是本沙箱踩过的坑。

## 实现面（如实）

- **`docs/development/rune-migration/building-native.md`（新，主交付物）**：九节——前置条件表（Rust stable、cc 是唯一编译期原生依赖、libxkbcommon、明确"不需要"清单：node/pnpm/libpq/libmysqlclient/系统 sqlite）；一条命令编译（`-p` 必带、bin 名≠包名、release 剖面四参数）；驱动链接语义（basic SKU 四家静态准入、链接≠支持、增删接缝）；运行与冒烟（无显示环境预期失败、托盘降级、两个环境变量）；体积与资源表（fat LTO sqlite-only 39.4 MB / ThinLTO 四驱动 50.5 MB 两档实测 + CI 为准）；打包（`build-deb.sh` 四步、版本取 git describe 含 dirty 语义、三平台流水线）；验证清单表（GR 门禁 + 二进制冒烟 + `.deb` 核验）；排障表（六个真实案例）；相关文档。
- **`native-host.md`**：构建与运行节收敛为两行命令 + 指向新指南；打包节保留一句话摘要 + 指南第 6 节。避免两处漂移。
- **`README.md`**：293 行的 native 段落更新——删掉过时的"sqlite wired end to end today"，改为 basic SKU 四驱动如实分级；补 building-native.md 链接。

## 测试（沙箱实测，环境：x86_64 双核、4 GB 内存、Rust 1.100.0、根分区 9.9G）

- **GR3 复跑**：`cargo test -p datazen-rune-host --lib` → **62 passed / 0 failed**（与 E7 基线数字一致；E7 时点的规范命令带 `--offline`，本次测试执行期无网络行为，等价）。沙箱磁盘受限，编译段用 `CARGO_PROFILE_DEV_DEBUG=0`（免调试信息，不改测试语义，如实记录）。
- **GR4 复跑**：`cargo check -p datazen-rune-host` → exit 0（2m26s）。
- **release 构建**：`cargo build --release -p datazen-rune-host` 裸跑**三次 OOM**（最终链接 `signal: 9`——fat LTO + `codegen-units=1` 的链接内存峰值超 4 GB；`-Wl,--no-keep-memory` 亦不救，证死点是 rustc/链接本体而非 lld 缓存策略）。按指南第 8 节逃生口 `CARGO_PROFILE_RELEASE_LTO=thin` 完成构建 → `target/release/datazen-native` **53,004,568 字节（50.5 MB）**，stripped ELF。fat LTO 在 E4 变大后的链接图超本沙箱内存上限，**其体积以 CI 产物为准**，指南体积表如实分档。
- **二进制冒烟**：`file` 确认 stripped ELF；无头运行实测两步行为——托盘缺失降级日志（"the window interface continues without it"）→ winit 无显示环境报错退出，与 native-host.md 声明一致（图形应用预期行为）。
- **`.deb` 构建**：`packaging/native/build-deb.sh release` → `datazen-native_0.2.3.2460.g6312cea8b.dirty_amd64.deb`（17,504,808 字节，16.7 MB）；`dpkg-deb -I`/`-c` 核验 control 七字段（含 `Depends: libxkbcommon0`）与内容树三件套（/usr/bin、desktop 项、hicolor 图标）全对；版本号如实携带 dirty 后缀（工作区未提交改动）。
- **环境障碍（如实记录，已沉淀为指南排障表）**：①沙箱回收后 `libxkbcommon.so` dev 链接丢失 → 无 root 环境用 `~/dev-libs` 符号链接 + `LIBRARY_PATH` 等价解决；②误删 `~/.cargo/registry/cache` 后 `--offline` 断链（缺 accesskit）→ `cargo fetch` 补齐；③9.9G 根分区被构建产物写满两次 → 按档清理 target/。

## 门禁

| 门禁 | 结果 |
|---|---|
| GR3 | **复跑通过**：62 passed / 0 failed（本任务实测） |
| GR4 | **复跑通过**：exit 0（本任务实测） |
| GR1 / GR2 | 沿用 E7 绿色基线（55+146 passed / exit 0，提交 a623e8a 时点）——本任务零 Rust 变更，ui-api/ui 依赖图未动，T11 先例 |
| GR5 | 未涉及（未改驱动 crate），沿用 E6 时点基线 |
| G1–G4（前端） | 不适用（未改 `packages/frontend`、`packages/web`） |

## 遗留（如实）

- fat LTO 官方剖面的 release 体积在 4 GB 内存沙箱**不可测**（链接 OOM），指南第 5 节体积表以"CI 产物为准"如实标注；有 8 GB+ 内存的本地环境可直跑官方剖面补测。
- 发布流水线本沙箱不可运行（无 GitHub Actions runner），沿用 T11 的"留给打 tag 验证"结论。
- `.deb` 的 postinst update-desktop-database 触发、`.rpm`/AppImage 等分发形态：T11 遗留项，未在本任务范围。
- 排障表第六行（版本 dirty）与 `.deb` 实测互证；其余五条均本沙箱亲测。
