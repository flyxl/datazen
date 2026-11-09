# 开发环境侧件栈运行手册（deploy/dev，一键可跑）

> 目标读者：要在本机跑起 **server/web 形态**完整环境（登录可用）的贡献者。
> 桌面形态开发不经过本栈（Tauri 开发流照旧）。
> 入口命令只有一条：`deploy/dev/up.sh`。本手册解释它拉起了什么、
> 怎么验证、坏了怎么查、与生产栈差在哪。

## 1. 前置

| 依赖 | 最低版本 | 用途 |
| --- | --- | --- |
| docker（含 compose v2） | 24.x / compose v2 | 三个服务 + 两个卷的编排 |
| curl | 任意 | 探针与登录面冒烟 |
| 可用磁盘 | ~6 GB | 镜像构建（Rust + pnpm 双工具链层） |

不需要本机 Rust / Node 工具链——构建全部发生在镜像内。
首次构建 10–25 分钟（取决于网络）；之后有层缓存，秒级到分钟级。

国内网络建议带上镜像参数（两个都可选）：

```bash
deploy/dev/up.sh \
  --npm-registry https://registry.npmmirror.com \
  --cargo-mirror sparse+https://mirrors.tuna.tsinghua.edu.cn/crates.io-index/
```

## 2. 一条命令起栈

```bash
deploy/dev/up.sh
```

它按序做六件事（幂等，重复执行安全）：

1. 构建开发镜像 `datazen-server:dev`（`deploy/dev/Dockerfile.dev`；
   前端真构建 + cargo `--locked`，`--release-parity` 可切生产同款优化档）；
2. `docker compose up -d`（编排内建引导链：**migrate → membership 种子 → serve**）；
3. 等三组探针：dex 发现文档、`/healthz`、`/readyz`；
4. 冒烟 `GET /api/v1/auth/login` 期待 **302**（证明 OIDC 装配、issuer
   匹配、容器侧发现全部打通）；
5. 检查宿主能否解析 `id.localhost`（浏览器登录路径），不能则给 hosts 提示；
6. 打印登录入口与测试账号。

成功标志（脚本末尾输出）：

```
✓ 栈已就绪: http://localhost:8080
  测试账号: dev@datazen.local / password
```

浏览器**必须用 `http://localhost:8080`，不要用 `127.0.0.1`**（见 §5 坑 2）。

## 3. 栈里有什么

```
浏览器 ──http://localhost:8080──> datazen-server（同源 web 静态面 + /api/v1 + SSE）
                                      │ 发现/token/JWKS（容器网络别名 id.localhost）
                                      ▼
        浏览器登录跳转 ──http://id.localhost:5556──> dex（OIDC IdP，静态口令）
```

| 服务 | 作用 | 备注 |
| --- | --- | --- |
| `dex` | OIDC IdP 侧件（`dexidp/dex:v2.37.0`，零编译） | 静态口令用户、public 客户端 + 纯 PKCE；只绑宿主回环 5556 |
| `datazen-migrate` | 一次性 expand 迁移 | serve 对 schema 落后 fail-closed，所以它在最前 |
| `datazen-seed` | 一次性 membership 种子（sqlite3 upsert） | server 无「建 membership」子命令且登录映射 fail-closed，种子是开发栈侧补位 |
| `datazen-server` | 本体（单实例） | `user 10001`、SIGTERM 优雅停机、healthcheck 走 `check-schema` |
| 卷 `admin-db` | SQLite 管理库（+WAL/SHM） | `down.sh -v` 才会清 |
| 卷 `artifacts` | 产物字节（受控上传/下载） | 同上 |

env 键 → server 代码位置的逐键对照在
[`deploy/dev/docker-compose.dev.yml`](../../deploy/dev/docker-compose.dev.yml)
文件头（唯一权威是 `crates/datazen-server/src/config.rs` 的 `from_env()`，
本栈不发明任何 server 不认识的键）。

## 4. 登录与测试账号

| 项 | 值 |
| --- | --- |
| 入口 | `http://localhost:8080`（页面上的登录按钮 → `GET /api/v1/auth/login` → 302 dex） |
| 账号 | `dev@datazen.local` |
| 口令 | `password`（bcrypt cost 10，写死在 `deploy/dev/dex/config.yaml`） |
| 身份映射 | dex 静态用户 sub=`devadmin` → 种子映射为 `datazen-local-org` 的 **orgAdmin** |

换口令 / 加用户：改 `deploy/dev/dex/config.yaml` 的 `staticPasswords`
（hash 用 bcrypt 重新生成），`docker compose -f deploy/dev/docker-compose.dev.yml
restart dex` 即可；新用户要登录还得在 `deploy/dev/seed/seed-dev.sql`
补一行 membership（未命中 sub 是 403，不隐式开户——这是产品行为不是坑）。

## 5. 常见坑（按遇到频率排序）

1. **浏览器打不开 `id.localhost:5556`**：`*.localhost` 的通配解析不是
   所有平台都有。Chrome/Chromium 原生支持；多数 Linux（systemd-resolved）
   支持；其余环境在 hosts 加一行 `127.0.0.1 id.localhost`。`up.sh` 会
   自动检测并打印这行提示。
2. **用 `127.0.0.1:8080` 访问一切正常但登录后接口 401/403**：会话三
   cookie 是 `__Host-` 前缀 + Secure，只在字面 `localhost` 可信 origin
   下被浏览器接受；且 SSE Origin / `public_origin` 校验是字符串等值。
   **一律用 `localhost`**。
3. **改了前端代码想看效果**：默认 web 静态面来自镜像内构建。改完
   `pnpm --filter datazen-web build` 后，把 compose 里注释掉的
   `../../packages/web/dist:/webapp/dist:ro` 挂载解开再 `up -d`；
   或者干脆重跑 `up.sh`（层缓存会跳过没变的 Rust 层）。
4. **dex 重启后第一次登录慢半秒**：dex 的签名钥存 `/tmp`（有意不挂卷），
   重启换新 kid，server 侧触发一次 JWKS 刷新（`oidc.rs decoding_key`
   的兜底路径）——不损失登录能力，正好是这条路径的真实演练。
5. **`down.sh -v` 后再 `up.sh` 登录 403**：卷清了，种子随引导链重放，
   一般不会发生；若手动删过 `seed/` 目录再 up，migrate 之外的种子不会
   重跑——`docker compose -f deploy/dev/docker-compose.dev.yml up datazen-seed`
   单独补一次。

## 6. 停栈 / 清栈

```bash
deploy/dev/down.sh      # 停四个服务（卷保留）
deploy/dev/down.sh -v   # 停 + 清卷（管理库与产物全部丢失，不可恢复）
```

## 7. 开发栈 vs 生产栈差异表

| 维度 | deploy/dev（本栈） | deploy/（生产） |
| --- | --- | --- |
| TLS | 无（localhost 明文） | 反代单源 HTTPS（前端终止） |
| IdP | dex 侧件（静态口令，明文开发口令） | 部署方自备（经 `.env` 注入 issuer/client） |
| client secret | 无（public 客户端纯 PKCE） | 部署方决定 |
| server secret | 开发默认值（可 env 覆盖） | `${DATAZEN_SERVER_SECRET:?}` fail-closed |
| 资源限额 | 无 | mem/cpus/pids 限额齐 |
| 引导 | compose 内建 migrate + 种子 | 运维手册流程（deploy-operations.md） |
| 暴露面 | 全部只绑宿主回环 | 反代统一入口，容器不直暴 |
| 构建档 | 快速档（opt-level=1 无 LTO） | 仓库 `[profile.release]`（`--release-parity` 可对齐验证） |

## 8. 已知限制（沙箱未验证项）

本栈的编排/配置/脚本在交付时**未在真实 docker 环境端到端跑过**
（交付沙箱无 docker）。以下事项首跑时留意：

- `Dockerfile.dev` 的构建层顺序与缓存命中（pnpm store 挂载层）——
  若 layer cache 不理想，优先检查 `Dockerfile.dev.dockerignore` 是否
  把 `target/`、`node_modules/` 排干净；
- dex `v2.37.0` 的 sqlite3 storage 在非 root 用户下的 `/tmp` 写入
  （配置有意选了 `/tmp` 回避卷权限面，理论可行）；
- `up.sh` 的探针超时上限（各 60 次 × 2s）在慢机上首次构建后是否充裕——
  不够就手动 `curl http://localhost:8080/readyz` 确认。

遇到除此之外的失败，先看 `docker compose -f deploy/dev/docker-compose.dev.yml
logs datazen-server`（server 侧）与 `logs dex`（登录侧）。

## 9. 相关文档

- 部署运维（生产）：[deploy-operations.md](./deploy-operations.md)
- 契约：[web-http-api.md](../architecture/platform/web-http-api.md)
- 团队认证：[web-server-host.md](../architecture/platform/web-server-host.md)
