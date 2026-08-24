# next-move-20260821 完整测试计划

| 项目 | 内容 |
|------|------|
| 测试对象 | next-move-20260821 全部已交付功能（Data Sync V1 / Data Transfer V1 / 查询历史 / SQL 文件执行 / 管理命令 / 5.4 运维） |
| 测试层次 | Rust 单测/集成 → Vitest 单测 → WebdriverIO E2E → 契约矩阵 → 手工黑盒（test/） |
| 明确排除 | macOS 公证执行（无证书）、pt-osc/gh-ost（明确不做）、Dashboard 小部件（deferred） |
| 注 | 原 docs/progress/ 因文档重构移除，本计划迁移至 docs/reviews/；执行状态见同目录 test-loop-next-move-20260821.md |

## 1. 缺口清单（GAP，本轮要新增的自动化）

GAP-E1 查询历史深路径 E2E；GAP-E2 执行 SQL 文件 E2E；GAP-E3 Server Status/Process List 面板 E2E；GAP-E4 大 DDL 警告 E2E；GAP-E5 Sync Schema 选择器；GAP-E6 keyset 多页 compare；GAP-E7 INCOMPATIBLE→Transfer CTA；GAP-E8 Transfer Drop+Create 门闸；GAP-E9 Transfer 跨方言灌数；GAP-E10 备份右键预填断言；GAP-B1 黑盒计划过旧

已销账：E1/E2/E3 部分 spec 已编写（query-history.ts、execute-sql-file.ts、ops-server-status-processes.ts），待运行验证

## 2. 测试用例编号

### Data Sync
- TS-SYNC-E01 keyset 多页（夹具 2500 行、batchSize=1000）
- TS-SYNC-E02 Schema 选择器（sourceSchema/targetSchema 资格名）
- TS-SYNC-E03 INCOMPATIBLE→「数据传输」CTA 打开并预填
- TS-SYNC-E04 再 Compare=0（复核 SYNC-REAL-009）
- TS-SYNC-E05 Delete 双确认流
- TS-SYNC-E06 Swap 与自同步禁止
- TS-SYNC-E07 SYNC-CROSS 复核（PG→MySQL 被拒且文案指向 Transfer）
- TS-SYNC-E08 行级 Diff 工作区（新旧值/搜索/筛选/分页）
- Rust 回归：门闸矩阵 / keyset 边界 / ChangeSet 过滤 / read_only / 事务回滚

### 查询历史（含本次新增分组功能）
- TS-QH-E01 执行即记录；E02 失败也记录；E03 搜索过滤；E04 应用回填编辑器（黑盒：原生菜单）；E05 清空；E06 重启持久化
- 分组专项（新）：当前库默认作用域、全部分组头、未知库归组、去重按 (config,database,schema) 不跨库合并

### 执行 SQL 文件
- TS-SF-E01 有效文件全链路（走 execute_sql_file 路径 IPC）；SF-E02 失败语句 reject；SF-E03 大文件流式可取消；SF-E04 入口一致性；SF-E05 扩展名白名单（黑盒）

### 管理 DDL
- TS-DDL-E01 树刷新闭环；E02 capability 显隐；E03 权限不足错误可复制

### Data Transfer
- TS-XFER-E01 跨方言 SQLite→PG 类型映射与特殊字符；E02 Drop+Create 门闸（未勾禁用/勾选执行 DROP IF EXISTS+CREATE+INSERT）；E03 CREATE 建表对照 IR；E04 手工列映射生效；E05 无 PK 表；Rust 回归 table_to_ir/ddl/format_literal

### 5.4 运维
- TS-OPS-E01 Server Status 链路；E02 Process List 链路（Kill 仅确认框+取消）；E03 大 DDL 警告框出现/取消不执行；E04 备份右键预填 configId/database；E05 对象过滤器 OPS-FILTER-001~005 回归；E06 Pin OPS-PIN-001~004 回归 + 持久化；驱动侧 process/server_status/admin_commands 回归

### 黑盒（test/ 目录登记）
TS-BB-01 完整同步故事；BB-02 异构迁移故事；BB-03 原生对话框路径；BB-04 中断恢复；BB-05 多窗口并行；BB-06 i18n zh/en 全部新界面；BB-07 键盘可达性；BB-08 大表体验与取消即时性；BB-09 只读账号权限故事；BB-10 升级兼容默认值

## 3. 执行顺序

Phase 0 环境（setup-e2e-env.sh / setup-sync-dbs.sh）→ Phase 1 单测基线（vitest + cargo lib via with-plugin-inject + 驱动 crate）→ Phase 2 webdriver 构建（with-plugin-inject → e2e-tauri-build.mjs）→ Phase 3 新 specs 单跑再全量 + contract matrix → Phase 4 黑盒 → Phase 5 coverage 销账（docs/development/e2e-coverage.md）+ 进度回写

## 4. 通过标准

1) 单测 0 fail；2) 新增 E2E 全绿 + 存量全套回归绿 + 契约矩阵三驱动绿；3) 三条硬验收成立（再 Compare=0 / RO 可比禁执 / keyset 生效）；4) GAP 全销账或登记例外；5) 黑盒完成且无未登记 P0/P1 bug
