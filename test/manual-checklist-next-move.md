# next-move 黑盒手工测试清单（TS-BB-01~10）

> 执行环境：macOS 本机构建的 Debug 应用（pnpm tauri:dev --drivers=basic 或安装包）；
> 需本地 PostgreSQL 14+ / MySQL 8+ / SQLite 文件。每项记录 PASS/FAIL + 截图或错误文案；
> FAIL 登记到 docs/reviews/test-loop-next-move-20260821.md Bug 表（ID 前缀 BUG-BB）。

## BB-01 完整同步故事（P0 主线）
1. 准备同族 PG 两库：dev 库改 3 行（1 插入 1 更新 1 删除）
2. 导航树 dev 库右键 →「比较数据」→ 选 prod 为目标 → 比较
3. 逐表核对行差异计数与实际 SQL 差异一致；开启删除走两道确认
4. 执行后再次比较：选定范围差异为 0
5. 中途取消一次 Compare：立即停止、UI 无卡死、可重新比较
验收：全程无需刷新；文案准确；再 Compare=0

## BB-02 异构迁移故事
SQLite 文件（含引号/换行/emoji/NULL/日期列）→ PG 新库：向导 8 步全走，选 Drop+Create 需勾确认词；结果行数逐表与源一致，特殊字符无损

## BB-03 原生对话框路径（自动化盲区）
连接右键「执行 SQL 文件」：真实文件选择器选 .sql → 确认弹窗 → 日志滚动 → 成功提示；另验证备份另存为对话框

## BB-04 中断恢复
Compare 进行中 kill 数据库进程 / 断 VPN：错误可复制、不崩溃；恢复网络后重试成功；同步任务中断后出现「继续同步任务」断点弹窗，两种策略均可续跑

## BB-05 多窗口并行
Sync 窗口 Compare 结果展示时，主窗口编辑同一表数据 → Sync 内刷新预览反映新差异；两窗口互不阻塞

## BB-06 i18n
切 en / zh-CN：查询历史侧栏（当前库/全部按钮、未记录库）、Transfer 向导 8 步、Server Status/Process List 面板、SQL 文件对话框、大 DDL 警告框——无缺译 key 名裸露、无布局溢出

## BB-07 键盘可达性
历史侧栏 Tab 顺序合理；对话框 Esc 关闭、Enter 触发主按钮；作用域切换按钮 aria-pressed 正确

## BB-08 大表体验
对 ≥100 万行表发起 ALTER COLUMN TYPE：警告框出现且行数估计量级正确；取消不执行。Compare 千行级表体感 < 数秒；batchSize 调小观察分页推进

## BB-09 只读账号权限故事
用只读账号连 PG：可比较、执行禁用且有提示；进程列表打开正常但 Kill 报权限错误且文案友好；服务器状态只读指标正常

## BB-10 升级兼容
用旧版本（v0.0.x）产生的 appData 启动新版：连接/历史旧记录（database 为空的 legacy 行）归入「未记录库」组不丢失、不报错；objectFilter/pinned 默认值正常
