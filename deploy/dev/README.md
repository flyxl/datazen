# deploy/dev —— 开发侧件栈（一键可跑）

一条命令拉起 server/web 形态的完整本地环境：datazen-server（含同源 web
静态面与 `/api/v1`）+ **dex**（OIDC IdP，登录侧件）+ 管理库/产物卷 + 首次
引导（迁移 + 登录种子）。

```bash
deploy/dev/up.sh        # 构建 + 起栈 + 探针 + 冒烟 + 打印登录入口/测试账号
deploy/dev/down.sh      # 停栈（-v 连卷清除）
```

前置只有 docker（含 compose v2）与 curl。测试账号 `dev@datazen.local` /
`password`。浏览器用 `http://localhost:8080` 访问（别用 127.0.0.1）。

**运行手册（含网络拓扑、常见坑、与生产栈差异表、已知限制）**：
[docs/development/dev-environment.md](../../docs/development/dev-environment.md)

文件清单：

| 文件 | 作用 |
| --- | --- |
| `docker-compose.dev.yml` | 开发栈编排（dex + migrate/seed 引导 + server）；文件头有 env 键 → server 代码位置对照 |
| `Dockerfile.dev` | 开发镜像（前端真构建 + cargo `--locked` + 与生产同款的 runtime 形状） |
| `Dockerfile.dev.dockerignore` | 该 Dockerfile 专属的构建上下文裁剪（BuildKit per-Dockerfile ignore） |
| `dex/config.yaml` | dex 静态配置（静态口令用户 / public 客户端 + PKCE / 回调白名单） |
| `seed/seed-dev.sql` | membership 种子（dex 用户的 sub → orgAdmin；幂等 upsert） |
| `up.sh` / `down.sh` | 一键入口 / 停栈 |

本目录一切内容**仅限本地开发**：明文测试口令、开发默认密钥、非 TLS 的
localhost 公网身份。生产形态见上一级 `deploy/`（反代单源 HTTPS + 受控
secret 注入），两栈差异见文档 §7。
