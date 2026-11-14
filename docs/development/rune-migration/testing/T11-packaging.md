# T11 打包分发 — 测试报告

- 任务：`.deb`（dpkg-deb，control+图标+桌面项）；GitHub Actions 发布流水线（打 tag → 三平台构建 → 产物上传）；文档更新。
- 日期：2026-11-14
- 提交：见 tasks.md T11 行。

## 结论

通过。`.deb` 在沙箱内构建并解包检查全对；流水线 YAML 与仓库既有 workflow 同构；native-host.md 如实更新。第三阶段（T1–T11）收官。

## 实现面（如实）

- **`packaging/native/`**：
  - `build-deb.sh`——`cargo build --profile <release|dev>` → staging（`/usr/bin/datazen-native` 0755、`/usr/share/applications/datazen-native.desktop`、`/usr/share/icons/hicolor/128x128/apps/datazen-native.png` 复用应用自身图标、`DEBIAN/control`）→ `dpkg-deb --build --root-owner-group`。版本取 `git describe`（tag→净版本；本地 dirty 后缀如实带入并净化为 Debian 合法字符集，尾点修剪）。架构取 `dpkg --print-architecture`（amd64/arm64 自适）。`Depends: libxkbcommon0`——winit 事件层唯一非必然预装的原生库。
  - `datazen-native.desktop`——Type/Name/Exec/Icon/Categories/StartupWMClass。
- **`.github/workflows/release-native.yml`**：tag `v*` 或手动触发；矩阵 ubuntu（.deb）/macos（tar.gz）/windows（zip）；每任务 `contents: write` 直接把产物传到 tag 的草稿 release——沿用 `release.yml` 的全部纪律：`actions/checkout@v5`、`softprops/action-gh-release@v3`（draft、不在矩阵任务里写 notes）、并发组 `release-native-<ref>`、`FORCE_JAVASCRIPT_ACTIONS_TO_NODE24`、无钥匙串 runner 设 `DATAZEN_NATIVE_KEYRING=file`（CI 纪律与 ci.yml 的 `DATAZEN_KEYRING=file` 同款）。
- **文档**：native-host.md 增"打包"节（.deb 用法、流水线说明、与 Tauri 版 release.yml 的分工）。

## 测试（沙箱实测）

- **release 构建**：`cargo build --release -p datazen-rune-host` 完成（39.4 MB 二进制，workspace release 剖面）。
- **`.deb` 构建**：`build-deb.sh release` → `datazen-native_0.2.3.2443.gd87f316da_amd64.deb`（15 MB 压缩）。
- **解包检查**（`dpkg-deb -I`/`-c`/`-f`）：control 字段全对（Package/Version/Architecture/Depends/Maintainer/Description）；内容树全对——`/usr/bin/datazen-native`（0755、39.4 MB）、desktop 项、hicolor 图标；属主 root/root（--root-owner-group）。
- **版本净化**：`v0.2.3-2443-gd87f316da-dirty` → `0.2.3.2443.gd87f316da`（尾点修剪后复核 `-f Version`）。
- **踩坑（环境，如实）**：沙箱磁盘被多套 RUSTFLAGS 指纹的 release 工件占满三次——`prune_cargo_build.py` 扩展到 `target/release/build`（每包保留最新指纹）后完成；脚本本身在正常环境无此依赖。

## 门禁

- 本任务改 `packaging/`、`.github/workflows/`、`docs/`——**无 Rust 代码变更**，GR1–GR4 沿用 T10 的绿色基线（138+55 / 53 / check exit 0）未动。
- CI YAML 的语法与惯例按 `release.yml`/`ci.yml` 逐条对齐（checkout@v5、action-gh-release@v3、concurrency、permissions、env 纪律）。
- G1–G4（前端）：未改 `packages/frontend`、`packages/web`，不适用。

## 遗留（如实）

- 流水线本沙箱不可运行（无 GitHub Actions runner）——YAML 的真跑留给用户环境打 tag 验证；Linux 侧 .deb 的构建与解包已在沙箱实测，macOS/Windows 的打包步骤未实测（交叉编译面留给 CI）。
- `.rpm`/AppImage/Homebrew 未做（Tauri 版有 Homebrew/winget 模板在 `packaging/`，native 版按需增补同一目录）。
- `.deb` 未含 postinst 的 update-desktop-database 触发（hicolor 缓存多数桌面自刷；严格化属后续）。
- 版本号当前来自 git describe；无 tag 的裸构建是 `0.0.0` 兜底。
