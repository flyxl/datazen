# 团队服务部署、升级与恢复操作设计

> 状态：P7/P8/P9 目标运维流程，2026-10-02。当前仓库尚未提供本文描述的完整服务部署形态；目标命令、探针和配置契约须随对应阶段实现后改写为可直接执行的操作说明。本文不使用假命令声称服务已可部署。

认证、HTTP 与进程边界以 [团队服务设计](../architecture/platform/team-server-and-auth.md) 为准；schema 与备份数据以 [持久化模型](../architecture/platform/persistence-model.md) 为准；集群关闭证明以 [多 worker 设计](../architecture/platform/multi-worker-coordination.md) 为准。

## 1. 发布包与支持范围

每次发布冻结 host commit、edition、driver registry 摘要、所有 Git driver 的精确 commit、Driver/AI 协议版本、handler/plan/checkpoint 版本、管理库 schema 支持区间、原生工具版本与镜像摘要。标签只是查找入口，部署用不可变镜像摘要。

镜像只包含选定驱动与获许可的扩展。Community/Pro 的构建边界沿用既有 resolve 流程，不因服务形态默认加载所有私有驱动。Web UI 与 server contracts 同版本发布，团队桌面客户端另按兼容矩阵声明最低/最高协议。

发布附件给出支持的操作与驱动矩阵、未运行验证及限制；缺原生工具、无固定资源能力或无恢复证明的操作显式 unsupported。服务支持不能从桌面构建成功推导。

## 2. 部署拓扑与配置

P7 单实例部署为 HTTPS 反向代理 + server + 独立 PostgreSQL 管理库 + Artifact 存储 + SecretProvider/IdP。P9 在此基础上分离 API、worker 与单权威内存目录协调器；管理库保持单一主写端。业务数据库不作为管理库。

| 配置类别 | 必须冻结或校验的内容 |
| --- | --- |
| 公网身份 | HTTPS base URL、允许 Origin、OIDC issuer/audience、回调白名单 |
| Wapp | 独立真实 origin、允许 frame-ancestors；不共用主站 API/cookie |
| 管理库 | 受控连接秘密引用、TLS 校验、连接上限、迁移锁期限 |
| Artifact | 存储位置、加密、组织隔离、TTL、总量/单产物/下载配额 |
| 密钥 | SecretProvider、签名 keyVersion、轮换与恢复策略；无明文默认秘密 |
| 组织 | 允许组织映射、初始管理员主体、角色与 ACL 基线 |
| 网络 | worker 可访问的目标/代理/隧道路由；默认拒绝未配置路线 |
| 预算 | 组织/用户/物理服务上限、类别保留、队列与超时 |
| 工具 | 允许的原生程序及版本、临时目录与输出限额 |
| 集群 | 认证服务身份、目录地址、claim/permit 期限与 drain 截止时间 |

以上是配置语义，不是当前存在的环境变量名。实现时提供版本化 schema，未知键和缺失安全配置启动失败；秘密只用受控引用，不进入命令行、镜像、浏览器或日志。不得以读取本地 `.env` 内容作为运维核验步骤。

反向代理传递可信来源与请求 ID，禁用对流式响应的无限缓冲，设置上传/下载与超时上限。Forwarded/X-Forwarded-* 只信任明确代理网段；直连客户端不能伪造 HTTPS、host 或 principal。OIDC 回调、SSE、Wapp 跨 origin 和桌面 PKCE 流程分别验证。

## 3. 初始化与健康检查

目标 server 命令族 `check-schema`、`migrate`、`serve` 按团队服务设计实现；发布时给出真实 CLI help、配置 schema 和最小配置文件样例，禁止用仓库未有的脚本代替验收。

首次部署顺序：配置管理库/存储/密钥 → 受控导入组织与管理员 → check-schema → 独占迁移锁执行 migrate → 启动 server → readiness 验证 → 放行代理 → 浏览器登录与只读 smoke。管理员导入可重试但不能重置既有 ACL 或覆盖成员角色。

P7 目标探针为 `/health/live` 与 `/health/ready`：

- live 只判断进程可响应，不访问业务库，不暴露配置与秘密。
- ready 校验 schema 支持区间及 checksum、管理库、必要 SecretProvider/Artifact 读写，以及本形态的 worker/协调服务可用性。
- 用户业务数据库暂时离线只影响该连接，不能让整个 server 永久不 ready。

ready 失败不接受新副作用请求。管理库依赖故障时保留进程诊断与受控取消能力，不以反复 liveness 重启代替处理未知提交。清理失败保留指标和告警。

## 4. 发布前验收

验收需完成浏览器登录/登出、组织隔离、ACL 撤销、CSRF 拒绝、查询/SSE 断线恢复、Artifact ACL、幂等写请求、Job 取消及重启核验。P8 增加桌面系统浏览器登录、PKCE、登录失效、backend 切换与旧事件隔离；P9 增加两 API/worker 的分区、暂停恢复和 drain。

演练必须使用发布镜像及所选真实 driver，不用 fake 结果替代。记录输入版本、层、计数、退出码和限制；不把 SQL、数据内容或秘密放进公开发布附件。驱动专属测试留在对应驱动仓库。

## 5. 升级与回退

### 5.1 Expand 发布

1. 比对新旧镜像 supportedSchemaMin/Max，确认计划 migrationHead 在双方支持区间，旧 checksum 均一致。
2. 对长 Job 检查 handler/plan/checkpoint 兼容；不兼容任务先完成或结束为核验态，不让新 worker盲读旧计划。
3. 取得可恢复备份并验证 manifest。阻止新写受理或进入维护窗口，drain 受影响 worker；活动事务不迁移。
4. 独占迁移锁执行 expand。DDL 与 schema_migration_history 原子提交；非事务迁移按定义核验对象和有效性，不靠“文件名已出现”判断成功。
5. 启动新镜像并通过 ready/smoke，逐步放量。观察 unknown、拒绝、队列、产物和预算 held 指标。
6. 仅在读写新字段稳定且旧消费者已退役后，下一次独立发布执行 contract。

P7 首版可选择维护窗口停旧启动新，不能宣称零停机。P9 worker 分批 drain，但目录切换遵守单权威隔离与全 session 失效。UI、API 与桌面协议不兼容时在业务操作前明确拒绝。

### 5.2 回退

新镜像故障时关闭新写入口、drain/隔离新 worker，再运行 schema 仍兼容的旧镜像。没有反向 SQL 自动降级；旧镜像区间不覆盖 migrationHead 时不能启动，应修复或从备份另建环境恢复。

镜像回退不撤销用户数据库已经发生的写入，也不恢复活动 session。未知任务保持核验态。contract 已执行后，旧镜像回退通常不可用，发布者必须提前说明这个边界。

## 6. 备份一致性与保留

备份包括管理库全部业务表与迁移历史、被引用的完整 Artifact 字节及元数据、组织/ACL、计划/checkpoint、幂等记录、审计，以及外部 KMS/SecretProvider 的引用和密钥版本可用性证明。用户业务库备份是另一项独立操作；管理库备份不能撤销或重建它。

session、attachment、cursor、ResourceLease、目录和 runtimeEpoch 禁止备份。Secrets 不打包成明文；密钥恢复资料走独立受控渠道。备份及审计按敏感数据访问管理。

首版采用停写一致性备份：

1. 暂停新的副作用请求与 schedule；drain Job，等待在途写入核验或标 unknown，并固定受理水位。
2. 阻止 Artifact 新发布与元数据变更；完成现有 writer 或标 truncated，固定 Artifact manifest。
3. 取得管理库一致性 snapshot，按 snapshot 中的 Artifact 引用复制字节；保留原对象/版本直到复制与校验结束。
4. manifest 记录备份 ID、时间、水位、schema/checksum、镜像/驱动版本、对象摘要/大小/完整性、密钥版本和未确定任务清单。
5. 验证全部引用可读与摘要一致后解除维护；manifest 不含 SQL/秘密。失败备份不标 completed。

保留期限以组织策略与恢复窗口配置，不在文档写一个虚假的统一天数。清理先标记、后确认无受保护备份引用，再删除字节；备份过程有保留锁，TTL 清理不能删除正在复制的对象。

## 7. 灾难恢复

### 7.1 隔离后恢复

1. 隔离旧 API、worker、协调器与调度入口，确认其无法继续访问业务库。无法证明旧连接关闭的预算仍 held；不允许同时启动可写恢复环境。
2. 在隔离环境恢复匹配 manifest 的管理库与 Artifact，核验 schema/checksum、引用摘要、密钥可用性与驱动/handler 版本。
3. 旧登录、attachment、提交令牌和目录全部失效。登录会话作撤销迁移，签发使用新 keyVersion；客户端必须重新登录，旧请求不能以新 key 自动重放。
4. queued/running 及未核验副作用任务统一进入恢复核验流程；加载已确认边界，不恢复物理资源或活事务。预算分配账按关闭证据核销，不自动清零。
5. 只开放管理员核验和只读路径，核验外部数据库效果与源一致性。安全证明完整后逐项开放任务继续或人工创建新的计划。
6. 浏览器/桌面执行恢复 smoke，管理员确认 scope 后开放写受理和 schedule；首次调度默认跳过维护期间漏触发点。

“进入核验”是恢复子流程，不新增 JobState 枚举：保留原 state、恢复说明和已确认边界，完成核验后按既有终态/恢复策略转换。

### 7.2 备份点之后的外部写入

RPO 期间管理库记录可能丢失，而业务数据库写入仍存在。恢复后找不到 receipt 不等于从未执行；UI/MCP/调度不能重建原请求并自动执行。先使旧提交令牌版本失效，再对备份水位后的请求按独立审计与目标证据核验；无法证明的范围记 unknown，管理员明确重新审阅后才能创建新计划。

没有业务库协同恢复或目标幂等证据时，只能承诺恢复元数据和产物，不能承诺自动恢复全部外部任务的 exactly-once 效果。业务库时间点恢复也必须作为独立审批与演练流程，不能由管理库 restore 顺带执行。

### 7.3 恢复验收

必须检查登录被重新要求、跨组织访问拒绝、旧 session/令牌拒绝、产物摘要完整、计划过期/版本不兼容拒绝、unknown 任务不自动重放、旧 worker 不可访问业务库。RPO/RTO 由部署方设定并实测；报告实际恢复耗时与丢失水位，不用配置值冒充达成结果。

## 8. 故障处理与演练

| 故障 | 操作与禁止的捷径 |
| --- | --- |
| 管理库不可用 | 停新受理/认领/续租，保留在途效果；不从只读副本抢 claim |
| Artifact 字节缺失 | 标不可读、阻止依赖计划应用、从同 manifest 恢复；不返回空结果冒充完整 |
| worker 失联 | 目录失效、claim 核验、预算 held；不按 TTL 自动重新分配 |
| OIDC/SecretProvider 不可用 | 拒绝需新认证/解析秘密的操作；不退回默认账号或旧秘密 |
| schema 迁移中断 | 独占锁核验事务/非事务结果，按 checksum 续作；不手工伪造 history |
| 产物额度耗尽 | 停止新增/标 truncated，保留已发布数据；不绕过组织额度 |
| 非事务 DDL 或恢复工具中断 | 核验目标真实变化，记录部分/unknown；不靠镜像回退宣称撤销 |

发布前至少演练：迁移中断、提交/checkpoint 故障、备份后写入再恢复、密钥不可用、Artifact 丢失、权限撤销、worker 暂停后恢复和目录重启。演练结果进入对应正式测试与发布证据，不新增过程台账文档。
