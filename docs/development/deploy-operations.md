# datazen-server 部署运维（WR-G3 落点）

> 状态：WR-G3 的可码化落点清单与操作说明，2026-10。语义来源是
> [team-service-operations.md](./team-service-operations.md)（下称 TSO）；本文
> 不复述其验收矩阵，只记录**当前仓库已可码化的形状**与**已登记未实现的边界**。
> 本文描述的 deploy/ 文件均为形状定义：镜像未真实构建过、脚本未在真实部署
> 演练过之前，不得当作已验收的运维流程引用（TSO 卷首同款纪律）。

服务端配置面唯一来源：`crates/datazen-server/src/config.rs` 的 `DATAZEN_SERVER_*` 全键清单
（23 键，含默认值与校验规则）；镜像与 compose 不新增配置来源。进程面是
一个二进制、三个子命令：`serve`（监听）、`migrate`（expand 迁移，落后退出 3）、
`check-schema`（只读版本报告，落后 3 / 超前 4）。

## 1. 发布镜像（TSO §1）

- `deploy/Dockerfile`：三阶段形状。
  1. `frontend`：静态资产构建 → `/webapp/dist`（冻结 pnpm-lock）；
  2. `server`：`cargo build --locked --release`（Cargo.lock 偏移即失败，发布包冻结语义）；
  3. `runtime`：仅二进制 + 静态资产，非 root（uid 10001），`VOLUME /var/lib/datazen`，
     `HEALTHCHECK` 用只读 `check-schema`，`CMD serve`。
- 部署用不可变镜像摘要（TSO §1）：标签只是查找入口。发布附件记录镜像摘要、
  supportedSchemaMin/Max、驱动矩阵与未运行验证。
- 首次部署顺序（TSO §3）：配置管理库/存储/密钥 → 组织与管理员导入 →
  `check-schema` → 迁移锁下 `migrate` → `serve` → `/readyz` 验证 → 放行代理 →
  只读 smoke。
- 探针：live = `/healthz`（纯进程活性，不碰库）；ready = `/readyz`（schema
  区间 + 管理库可达）。编排层放行以 ready 为准，不以 HEALTHCHECK 替代。

## 2. 管理数据库（TSO §2「管理库」行）

- v1 单实例形态：SQLite 管理库（`DATAZEN_SERVER_DB`）+ WAL，与产物同落
  `/var/lib/datazen` 卷（`deploy/docker-compose.yml` 的 `admin-db` 卷）。
  TSO 的目标形态是独立 PostgreSQL 管理库；接入时替换存储依赖与备份脚本，
  不改服务面与配置键（登记项，见 §9）。
- 业务数据库永远不作为管理库（TSO §2）；管理库连接秘密走受控引用（§3）。
- schema 演进：expand-only（先 expand 后 contract，TSO §5.1）；`migrate` 必须
  在独占迁移锁下执行，非事务迁移按对象核验，不靠「文件名已出现」判断成功。
- 探查口径：管理库是普通 SQLite 文件，运维可用 `sqlite3` 只读探查
  （`crates/datazen-server/tests/transfer_submission.rs` 的审计直查即此口径）；不要在
  WAL 活跃期用只读模式抢 shm，读写打开与生产探查同形。

## 3. SecretProvider 与密钥注入（TSO §2「密钥」行）

- 注入面（当前唯一）：`DATAZEN_SERVER_SECRET`（≥32 字符，fail-closed）——
  CSRF 双提交与 `policyIsolationKey` 的 HMAC 主钥；不落库、不进日志、
  不进镜像。compose 经 `env_file` + 必填校验（`${DATAZEN_SERVER_SECRET:?}`
  缺失即拒绝起栈）注入；生产用部署平台的 secret 机制（Docker secrets /
  K8s Secret / 云 KMS 挂载为环境变量），禁止 bake 进镜像或命令行。
- 运行时凭据面：`SecretProvider` 适配器把连接/隧道敏感字段密封进管理库
  `stored_secret` / `stored_secret_version`（v7）两表，**行内只有密文**
  （encrypt-then-MAC，master 即 `DATAZEN_SERVER_SECRET` 经域分离派生）；
  `server-credential:` 形状引用的落库接线随 profiles 切片（登记项）。
- 轮换：轮换 `DATAZEN_SERVER_SECRET` = 轮换 CSRF/令牌签发密钥版本（旧提交
  令牌立即失效）+ 密封材料 master；恢复流程里这是强制步骤
  （`deploy/scripts/restore.sh` 尾部清单第 2 条）。密钥恢复资料走独立受控
  渠道（TSO §6），不进备份包。

## 4. 网络可达性（TSO §2「网络」行）

- 容器只监听 `127.0.0.1:8080`（compose 端口映射），默认拒绝一切直暴；
  worker 可达的目标/代理/隧道路由按部署显式配置，未配置路线默认拒绝。
- `Forwarded / X-Forwarded-*` 只信任明确代理网段；直连客户端不能伪造
  HTTPS、host 或 principal（TSO §2 原文；server 侧 SSE 的 Origin 校验已由
  `crates/datazen-server/tests/sse_events.rs` 钉住：无 origin 证明拒绝、跨源拒绝、同源
  `Sec-Fetch-Site` 放行）。
- SSE/流式：反代必须禁用对 `text/event-stream` 的无限缓冲，透传
  `x-request-id`（骨架钉子见 `crates/datazen-server/tests/skeleton.rs`）。
- OIDC 回调、SSE、Wapp 跨 origin 分别验证（TSO §2）；`public_origin` 配置
  OIDC 时必填且必须是裸 origin（config.rs 校验）。

## 5. 备份与恢复（TSO §6/§7）

- `deploy/scripts/backup.sh`：
  1. 管理库一致性快照 —— sqlite `VACUUM INTO`（WAL 下等价一致性读，独立
     紧凑单文件）；v1 建议在停写维护窗口执行，是否停写如实记入 manifest；
  2. 产物字节 `rsync` 到 staging，校验通过才出库（原对象保留到校验结束）；
  3. `manifest.json` + `SHA256SUMS`：备份 ID、时间、水位说明、schema 报告、
     全部文件摘要；**不含 SQL、不含秘密**（TSO §6 第 4 条）。
  禁止备份项（session/attachment/cursor/ResourceLease/目录/runtimeEpoch）
  由恢复侧语义兜底：这些不是可恢复状态，恢复后一律失效。
- `deploy/scripts/restore.sh`：
  - **显式人工确认门**：交互式输入 `RESTORE`，非交互需 `CONFIRM_RESTORE=yes`
    **且** `I_UNDERSTAND_RESTORE_OVERWRITES_STATE=yes` 双开关；`DRY_RUN=1`
    只校验不落盘。
  - 校验（SHA256）→ 停服检查（拒绝热替换运行中的库）→ 管理库原子替换
    （清旧 WAL/SHM）→ 产物恢复**不带 `--delete`**（水位后的未知产物只补
    不删，交人工核验）。
  - **不自动重放未知外部写入**（TSO §7.2 字面执行）：脚本永不启动 server、
    永不重放请求/Job/令牌；恢复后旧会话/提交令牌全部失效，客户端重新登录；
    备份水位后的外部写入按独立审计与目标证据核验，无法证明的范围记
    unknown，管理员明确重审后才能创建新计划。业务库时间点恢复是独立审批
    流程，不由本脚本顺带执行。
  - 恢复验收清单（TSO §7.3）在脚本尾部输出，逐条人工执行。
- 保留期限按组织策略与恢复窗口配置（TSO §6 不写统一天数）；清理先标记、
  确认无受保护备份引用再删字节。

## 6. TLS（TSO §2「公网身份」行）

- **目标形态**：TLS 在反向代理单点终止，反代 ↔ server 走内网明文
  （`DATAZEN_SERVER_LISTEN=0.0.0.0:8080`）；`8443` 直连 TLS 是后续切片的
  形态（lib.rs 登记项），当前骨架只承诺回环明文 + 反代单源。
- 公网身份 = HTTPS base URL + 允许 Origin + OIDC issuer/audience + 回调白名单；
  `DATAZEN_SERVER_PUBLIC_ORIGIN` 必须与反代对外 origin 逐字等值（SSE Origin
  校验与 OIDC redirect 构造共用它）。
- Wapp 独立真实 origin + `frame-ancestors` 白名单，不共用主站 API/cookie
  （TSO §2「Wapp」行）。
- 证书续期、HSTS、代理超时上跟上限（上传/下载/流式各一档）是反代配置面，
  属部署方职责；server 侧对应的请求体 8 MiB 闸已有（`skeleton.rs` 钉住）。

## 7. 资源额度（TSO §2「预算」行）

- 应用层账（config.rs 全键）：`DATAZEN_SERVER_BUDGET_NODE_LIMIT` /
  `BUDGET_ORGANIZATION_LIMIT`（许可账，CM-60 单实例断言钉在
  `crates/datazen-server/tests/operations_gate_and_revocation.rs`：admit/deny/release +
  drain 拒绝）、L5 速率五键（默认档/严格档/键表上界）、SSE 背压
  （`SSE_MAX_BACKLOG`）、产物 TTL 清扫周期、WR-F7 出站白名单
  （`EGRESS_ALLOWED_HOSTS`，逗号分隔；空表全拒——驱动/隧道出口的唯一
  放行面）、WR-E5 同源静态根（`WEB_ROOT`）。额度是**部署可调值**，
  不是协议常量；改额度走配置变更，不走代码。
- 容器层账（compose）：`mem_limit` / `cpus` / `pids_limit` / `tmpfs /tmp`
  ——两层账各自独立，cgroups 不替代预算协调器，反之亦然。
- 额度耗尽的语义：拒绝 + 审计 + 429 词汇，不静默排队到无限
  （`quota_denied` 审计钉在同上测试文件）。

## 8. Drain 与回退（TSO §5.2 / §6 第 1 条）

- **停止信号（WR-G3 已接线）**：server 同时接 `SIGTERM` 与 `SIGINT`
  （`bootstrap::shutdown_signal`，先到先得）；`docker stop` 的默认
  SIGTERM 即优雅停机路径，compose 不再折算信号，`stop_grace_period: 30s`
  是部署方承诺值，不是 server 保证值。在途请求由
  `with_graceful_shutdown` 排空；后台清扫任务的协调停机随 drain 切片。
- 停写 → 备份 → 升级（TSO §6 停写一致性 + §5.1）：P7 首版允许维护窗口
  停旧起新，**不宣称零停机**；升级前比对镜像 supportedSchemaMin/Max 与
  计划 migrationHead 兼容性，长 Job 先完成或结束为核验态。
- 回退（TSO §5.2 字面执行）：关新写入口 → drain → 运行 schema 仍兼容的
  旧镜像；没有反向 SQL 自动降级，旧镜像区间不覆盖 migrationHead 时不能
  启动（`check-schema` 退出码 4，fail-closed），应修复或从备份另建环境
  恢复。**镜像回退不撤销业务库已发生的写入，也不恢复活动 session；
  未知任务保持核验态。**

## 9. 已登记未实现（本轮边界，不得当作已验收）

| 项 | 现状 | 归属 |
| --- | --- | --- |
| ~~SIGTERM 停机信号~~ | **已实现（WR-G3）**：SIGTERM+SIGINT 双信号，compose 折算行已删 | ~~src 改动~~ |
| 容器内 TLS / `:8443` | 未接线；反代单源是当前唯一承诺形态 | WR-D3 同源 HTTPS 切片 |
| 容器内 TLS / `:8443` | 未接线；反代单源是当前唯一承诺形态 | WR-D3 同源 HTTPS 切片 |
| PostgreSQL 管理库 | compose/备份脚本按 SQLite v1 落 | 管理库独立切片 |
| `readyz` 语义完整度 | ready 校验面随域路由逐域点亮 | WR-D2+ 各切片 |
| SecretProvider 生产化（KMS/AES） | v1 为 HMAC 密封盒 + 管理库密文表 | 生产化切片（只动 seal/open 两函数） |
| `server-credential:` 引用落库 | 适配器对其回答 NotFound（已定级） | profiles 存储切片 |
| 镜像真实构建与演练 | deploy/ 是形状定义，未构建未演练 | 发布流水线切片 |
| 备份演练（TSO §8 清单） | 脚本就位，演练未执行 | 部署方 + 发布证据 |
