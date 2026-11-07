# syntax=docker/dockerfile:1
# datazen-server 发布镜像（WR-G3「发布镜像」行的形状定义）。
#
# 三阶段（team-service-operations §1：Web UI 与 server contracts 同版本发布）：
#   1. frontend  —— 前端静态资产构建（CI 冻结 pnpm-lock；产物 → /webapp/dist）
#   2. server    —— Rust release 构建（cargo --locked，冻结 Cargo.lock）
#   3. runtime   —— 仅二进制 + 静态资产；非 root；无构建工具链
#
# 本文件随 WR-G3 落的是**形状**：目标命令以 docs/development/team-service-operations.md
# 的「不使用假命令声称已可部署」为准，首次真实构建在发布流水线里验收并记录镜像摘要。

# ── 1/3 前端静态资产 ─────────────────────────────────────────────────────
FROM node:22-slim AS frontend
WORKDIR /build
# 形状占位：真实构建用仓库冻结的 pnpm-lock.yaml + 既有构建脚本。
# COPY package.json pnpm-lock.yaml ./
# RUN corepack enable && pnpm install --frozen-lockfile
# COPY site ./site
# COPY packages ./packages
# RUN pnpm build
RUN mkdir -p /webapp/dist

# ── 2/3 server 二进制 ────────────────────────────────────────────────────
FROM rust:1-slim AS server
WORKDIR /build
# 依赖层缓存：先 manifests 后源码；--locked = Cargo.lock 偏移即失败（发布包冻结语义，§1）。
COPY Cargo.lock Cargo.toml ./
COPY server ./server
COPY modules ./modules
COPY packages ./packages
RUN cargo build --locked --release --manifest-path server/Cargo.toml \
 && cp /build/target/release/datazen-server /out-datazen-server

# ── 3/3 运行时 ───────────────────────────────────────────────────────────
FROM debian:bookworm-slim AS runtime
# 仅运行必需：ca-certificates（OIDC/HTTPS 出网）、sqlite3（运维探查，可裁剪）。
RUN apt-get update \
 && apt-get install -y --no-install-recommends ca-certificates sqlite3 \
 && rm -rf /var/lib/apt/lists/* \
 && useradd --system --uid 10001 --home /var/lib/datazen --shell /usr/sbin/nologin dz \
 && mkdir -p /var/lib/datazen/artifacts /webapp \
 && chown -R dz:dz /var/lib/datazen /webapp
COPY --from=server /out-datazen-server /usr/local/bin/datazen-server
COPY --from=frontend /webapp/dist /webapp/dist

# 配置面全部走环境变量（crates/datazen-server/src/config.rs 的 DATAZEN_SERVER_* 全键清单，
# 见 deploy/config/env.example）。这里只落「目录形态」默认值，不落任何秘密。
ENV DATAZEN_SERVER_DB=/var/lib/datazen/datazen-admin.sqlite \
    DATAZEN_SERVER_ARTIFACT_ROOT=/var/lib/datazen/artifacts \
    DATAZEN_SERVER_LISTEN=0.0.0.0:8080
# TLS 不在容器内终止：目标形态是反代单源 HTTPS（docs/development/deploy-operations.md §6）。
EXPOSE 8080
USER 10001:10001
VOLUME ["/var/lib/datazen"]
WORKDIR /var/lib/datazen
ENTRYPOINT ["/usr/local/bin/datazen-server"]
CMD ["serve"]
# 探针（team-service-operations §3）：live=/healthz、ready=/readyz；
# 容器 HEALTHCHECK 用只读的 check-schema 子命令（落后退出 3 / 超前退出 4，
# fail-closed），放行流量以编排层探 /readyz 为准。
HEALTHCHECK --interval=30s --timeout=10s --start-period=15s --retries=3 \
  CMD ["/usr/local/bin/datazen-server", "check-schema"]
