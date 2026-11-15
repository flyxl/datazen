# datazen-native 编译指南

`datazen-native` 是纯原生（Rust UI）宿主的二进制：`crates/datazen-rune-host` 的 `[[bin]]` 目标，不带 Tauri、不带 React、**不需要任何前端工具链**（无 node/pnpm）。一条 `cargo build` 从源码到可运行的桌面应用。运行面（配置目录、驱动矩阵、功能差距）见 [native-host.md](native-host.md)，本文件只讲怎么把它编译出来、打包、以及每个环节怎么验证。

> 沙箱实测口径：本指南所有命令与数字（体积、内存、磁盘）都在 x86_64 Linux、Rust 1.100.0、4 GB 内存的环境里真跑过（X1 报告），非转抄。

## 1. 前置条件

| 项 | 要求 | 说明 |
|---|---|---|
| Rust 工具链 | stable（无版本钉死，仓库不携带 rust-toolchain.toml，CI 同款） | `rustup show` 确认 |
| C 编译器 | `cc`（gcc 或 clang） | 唯一的编译期原生依赖：sqlite 经 `libsqlite3-sys` 的 bundled 特性从源码编译 |
| Linux 链接/运行库 | `libxkbcommon` | winit 事件层。链接期要 `libxkbcommon.so`（发行版的 `-dev` 包）；运行期 `libxkbcommon.so.0`（`.deb` 已声明 `Depends: libxkbcommon0`） |
| 磁盘 | 空闲 ≥ 3 GB（release）；全量 debug 构建实测另占约 5 GB | LTO 中间产物大，见第 8 节 |
| 不需要 | node、pnpm、libpq、libmysqlclient、系统 sqlite | postgres/mysql 走 sqlx + rustls（纯 Rust）；sqlite 源码内嵌 |

macOS / Windows：CI 矩阵（`release-native.yml`）在这两个平台也是裸 `cargo build --release -p datazen-rune-host`、无额外系统步骤——这是流水线文件的声明；三平台真跑以打 tag 的首次运行为准（T11 起未验，如实）。

## 2. 一条命令编译

```bash
cargo build --release -p datazen-rune-host   # 产物：target/release/datazen-native
```

- `-p datazen-rune-host` 必带：workspace 多 crate，裸 `cargo build --release` 会编全仓库。
- 产物名是 `datazen-native`（bin 目标名），不是包名。
- 开发迭代用 debug 档更快：`cargo build -p datazen-rune-host` → `target/debug/datazen-native`。

release 剖面（根 `Cargo.toml` `[profile.release]`）：`strip = true` + `lto = true` + `codegen-units = 1` + `opt-level = "z"`——为体积优化，代价是最终链接慢且吃内存（见第 5、8 节）。

## 3. 编进去的是什么（驱动链接语义）

宿主静态链接 **basic SKU 全集四驱动**——sqlite、postgres、mysql、redis，`crates/datazen-rune-host/Cargo.toml` 直接依赖四个驱动 crate，`live.rs` 的 `host_registry()` 用 `DriverRegistry::with_factories([...])` 静态准入十一家工厂 id（与 `scripts/resolve-drivers.mjs` 的 basic 预设同口径）。没有 feature 开关：编出来的每个 `datazen-native` 都带全部四家。

两条纪律（与 [native-host.md](native-host.md) 驱动矩阵一致）：

1. **链接 ≠ 支持**。sqlite 离线全功能验证过；postgres/mysql/redis 是"已链接"（工厂准入、表单可建连接、不可达有诚实错误），真服务器端到端冒烟未验，矩阵如实分级。
2. 增删驱动 = 一个 crate 依赖 + `host_registry()` 里一行工厂函数，接缝固定；其余 11 个 path 驱动未链接（桌面版那边的 optional-feature 形态不适用于此宿主）。

## 4. 运行与冒烟

```bash
./target/release/datazen-native
```

- 首启在 `${XDG_DATA_HOME:-$HOME/.local/share}/datazen-native/` 下建 `connections.json` 等文件（明细见 native-host.md 配置目录表）。
- **无显示环境（无 X/Wayland）时窗口初始化会失败**——`winit` 报 `neither WAYLAND_DISPLAY nor WAYLAND_SOCKET nor DISPLAY is set` 后 panic 退出。这是图形应用的预期行为，不是构建问题；CI/容器里验证编译用第 7 节的门禁，不用真开窗口。
- 系统托盘在无 D-Bus 总线的桌面降级为一条日志（窗口界面继续）。
- 两个环境变量：
  - `DATAZEN_NATIVE_KEYRING=file`——无 OS 钥匙串的环境（CI runner、容器）用文件存主密钥；
  - `DATAZEN_NATIVE_CONNECTIONS=<路径>`——整体指定连接存储位置，其余数据文件落其同级目录。

## 5. 体积与资源（实测）

| 档位 | 条件 | 体积 | 备注 |
|---|---|---|---|
| release，fat LTO，sqlite-only | T11 时点（链接集变更前） | 39.4 MB | T11 报告实测 |
| release，**ThinLTO**，四驱动（E4 后） | 本指南沙箱实测 | **50.5 MB**（53,004,568 字节） | 沙箱 4 GB 内存跑不动 fat LTO 的最终链接（第 8 节），ThinLTO 产物略大于 fat LTO |
| release，fat LTO，四驱动 | CI 产物为准 | — | 官方剖面，介于上两行之间的预期 |

`.deb` 压缩后 16.7 MB（amd64）。构建资源（x86_64 双核实测）：release 全量冷编约 15 分钟（LTO 长尾占一半）；fat LTO + `codegen-units=1` 的**最终链接内存峰值超 4 GB**——E4 链入三套远程驱动栈后，4 GB 机器三次 OOM（SIGKILL），瘦内存机器见第 8 节的 ThinLTO 逃生口。

## 6. 打包

### 6.1 `.deb`（Linux）

```bash
packaging/native/build-deb.sh [release|dev]   # 默认 release
```

脚本自己做四件事：`cargo build --profile <profile> -p datazen-rune-host` → staging（`/usr/bin/datazen-native` 0755、`/usr/share/applications/datazen-native.desktop`、128×128 hicolor 图标复用应用自身图标）→ 写 `DEBIAN/control`（`Depends: libxkbcommon0`）→ `dpkg-deb --build --root-owner-group`。产物在 `target/deb/datazen-native_<版本>_<架构>.deb`。

- 版本取 `git describe --tags --always --dirty`：有 tag 是净版本，本地未提交改动会带 `dirty` 后缀（Debian 版本字符集净化，尾点修剪）。要干净版本号，先提交再打包。
- 只需 dpkg-deb 存在，任何 Debian 系可跑；`dev` 档用于快速验证打包本身。

### 6.2 发布流水线（三平台）

`.github/workflows/release-native.yml`：打 `v*` tag（或手动 `workflow_dispatch`）→ ubuntu/macos/windows 矩阵各跑 `cargo build --release -p datazen-rune-host` → Linux 额外产出 `.deb`，macOS 打 tar.gz，Windows 打 zip → 全部上传到该 tag 的**草稿** release。runner 上无 OS 钥匙串，流水线设 `DATAZEN_NATIVE_KEYRING=file`。与 Tauri 桌面的 `release.yml` 是两条独立流水线。

## 7. 验证清单（怎么知道编对了）

| 检查 | 命令 | 期望 |
|---|---|---|
| 宿主单测（GR3） | `cargo test -p datazen-rune-host --offline --lib` | 62 passed（HEAD 实测） |
| 宿主 check（GR4） | `cargo check -p datazen-rune-host --offline` | exit 0 |
| UI 契约层（GR1/GR2，改动涉及 `datazen-ui`/`datazen-ui-api` 时必跑） | `cargo test -p datazen-ui-api -p datazen-ui --offline --lib -- --test-threads=1` | 55 + 146 passed |
| 二进制冒烟 | `file target/release/datazen-native`；有显示环境时启动 | stripped ELF；窗口起、托盘在无 D-Bus 时降级 |
| `.deb` 核验 | `dpkg-deb -I` / `dpkg-deb -c` | control 字段与内容树三件套齐全 |

离线纪律：`--offline` 依赖热 registry（`cargo fetch` 一次即可暖起来）；CI runner 无此约束。

## 8. 排障（真实案例）

| 症状 | 原因 | 解 |
|---|---|---|
| 最终链接被 SIGKILL（`signal: 9`） | fat LTO + `codegen-units=1` 的链接内存峰值超机器上限（4 GB 实测装不下 E4 后的图） | 逃生口：`CARGO_PROFILE_RELEASE_LTO=thin cargo build --release -p datazen-rune-host`（ThinLTO，体积略增，本指南沙箱即此法）；8 GB+ 内存机器无此问题 |
| `unable to find library -lxkbcommon` | 系统只有运行库 `libxkbcommon.so.0`，无 dev 链接 `libxkbcommon.so` | 正规解：装发行版 `libxkbcommon-dev`；无 root 时的等价解：`ln -s …/libxkbcommon.so.0 ~/dev-libs/libxkbcommon.so` 并 `LIBRARY_PATH=~/dev-libs` |
| `attempting to make an HTTP request, but --offline was specified` | registry 缓存缺包（如清理过 `~/.cargo/registry`） | 先在线 `cargo fetch` 补齐再离线；或去掉 `--offline` |
| `No space left on device` | target/ 膨胀：全量 debug 约 5 GB、release 约 2 GB，多套指纹并存时翻倍 | `cargo clean` 或删 `target/debug`；沙箱类受限盘先算好预算 |
| 启动即 panic 提 `WAYLAND_DISPLAY`/`DISPLAY` | 无显示环境的正常行为（第 4 节） | 不是构建问题；要真开窗口需 X/Wayland 会话 |
| 编出的 `.deb` 版本带 `dirty` | 工作区有未提交改动 | 提交后再 `build-deb.sh` |

## 9. 相关文档

- [native-host.md](native-host.md)——宿主运行面：配置目录、驱动矩阵（验证程度分级）、功能差距
- [testing/T11-packaging.md](testing/T11-packaging.md)——打包与发布流水线的实现报告
- [testing/X1-native-build-doc.md](testing/X1-native-build-doc.md)——本指南的验证报告（沙箱实测口径）
- [plan-native-3.md](plan-native-3.md) / [plan-native-4.md](plan-native-4.md)——宿主接线任务线（T 系列、E 系列）
