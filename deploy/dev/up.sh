#!/usr/bin/env bash
# deploy/dev/up.sh —— 开发侧件栈一键入口（server + 同源 web 静态面 + dex OIDC）。
#
# 做什么：
#   1. 构建开发镜像（deploy/dev/Dockerfile.dev，可用 --no-build 跳过）
#   2. docker compose up -d（compose 内建引导：migrate → membership 种子 → serve）
#   3. 等健康探针：dex 发现文档、/healthz、/readyz（router.rs 的探针路径）
#   4. 冒烟：GET /api/v1/auth/login 期待 302（证明 OIDC 装配 + issuer 匹配 + 容器侧发现全通）
#   5. 检查宿主能否解析 id.localhost（浏览器路径），不行则给 hosts 一行提示
#   6. 打印登录入口与测试账号
#
# 幂等：重复执行安全（migrate 空跑成功、种子是 upsert、up 对已起服务是 no-op）。
# 运行手册与常见坑：docs/development/dev-environment.md
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
COMPOSE_FILE="${SCRIPT_DIR}/docker-compose.dev.yml"
ROOT="$(cd "${SCRIPT_DIR}/../.." && pwd)"

SERVER_URL="http://localhost:8080"
DEX_URL="http://localhost:5556"
IDP_ISSUER_HOST="id.localhost"

usage() {
  cat <<'EOF'
用法: deploy/dev/up.sh [选项]

选项:
  --no-build                跳过镜像构建（复用已有 datazen-server:dev）
  --release-parity          server 用仓库 [profile.release] 原样构建
                            （lto=z / codegen-units=1 / opt-level=z，慢；
                            默认是快速档 opt-level=1 无 LTO）
  --npm-registry <url>      web 构建阶段的 pnpm registry（国内网络示例:
                            https://registry.npmmirror.com）
  --cargo-mirror <url>      rust 构建阶段的 crates.io 镜像（sparse 协议示例:
                            sparse+https://mirrors.tuna.tsinghua.edu.cn/crates.io-index/）
  -h, --help                本帮助

停栈: deploy/dev/down.sh（加 -v 清卷，管理库与产物会全部丢失）
EOF
}

NO_BUILD=0
RELEASE_PARITY=0
NPM_REGISTRY=""
CARGO_MIRROR=""
while [ $# -gt 0 ]; do
  case "$1" in
    --no-build) NO_BUILD=1 ;;
    --release-parity) RELEASE_PARITY=1 ;;
    --npm-registry)
      [ $# -ge 2 ] || { echo "错误: --npm-registry 需要一个 URL 参数" >&2; exit 2; }
      NPM_REGISTRY="$2"; shift ;;
    --cargo-mirror)
      [ $# -ge 2 ] || { echo "错误: --cargo-mirror 需要一个 URL 参数" >&2; exit 2; }
      CARGO_MIRROR="$2"; shift ;;
    -h|--help) usage; exit 0 ;;
    *) echo "未知选项: $1（--help 看用法）" >&2; exit 2 ;;
  esac
  shift
done

command -v docker >/dev/null 2>&1 || { echo "错误: 需要 docker（https://docs.docker.com/get-docker/）" >&2; exit 1; }
docker compose version >/dev/null 2>&1 || { echo "错误: 需要 docker compose v2（docker compose 子命令）" >&2; exit 1; }
command -v curl >/dev/null 2>&1 || { echo "错误: 需要 curl（探针与健康检查用）" >&2; exit 1; }

# 空数组在 bash 3.2 + set -u 下展开会炸，用 ${ARR[@]+...} 守卫。
BUILD_ARGS=()
[ "$RELEASE_PARITY" -eq 1 ] && BUILD_ARGS+=(--build-arg RELEASE_PARITY=1)
[ -n "$NPM_REGISTRY" ] && BUILD_ARGS+=(--build-arg "NPM_REGISTRY=${NPM_REGISTRY}")
[ -n "$CARGO_MIRROR" ] && BUILD_ARGS+=(--build-arg "CARGO_MIRROR=${CARGO_MIRROR}")

cd "$ROOT"

compose() { docker compose -f "$COMPOSE_FILE" "$@"; }

# wait_for <名称> <尝试次数> <命令...> —— 每 2 秒一次，命令成功即返回。
wait_for() {
  local what="$1" tries="$2"; shift 2
  local i=0
  while ! "$@" >/dev/null 2>&1; do
    i=$((i + 1))
    if [ "$i" -ge "$tries" ]; then
      echo "超时: ${what} 未在 $((tries * 2)) 秒内就绪。" >&2
      return 1
    fi
    sleep 2
  done
}

if [ "$NO_BUILD" -eq 0 ]; then
  echo "==> 构建开发镜像（首次约 10-30 分钟: pnpm install + cargo release;之后走缓存）"
  compose build ${BUILD_ARGS[@]+"${BUILD_ARGS[@]}"}
else
  echo "==> 跳过构建（--no-build）"
fi

echo "==> 起栈（migrate → 种子 → serve，dex 同步拉起）"
compose up -d

echo "==> 等待探针"
if ! wait_for "dex 发现文档 (${DEX_URL})" 60 curl -fsS -m 5 "${DEX_URL}/.well-known/openid-configuration"; then
  echo "  诊断: docker compose -f deploy/dev/docker-compose.dev.yml logs dex" >&2
  exit 1
fi
if ! wait_for "server /healthz" 60 curl -fsS -m 5 "${SERVER_URL}/healthz"; then
  echo "  诊断: 看 datazen-server 日志（命令见尾部输出）" >&2
  exit 1
fi
if ! wait_for "server /readyz" 60 curl -fsS -m 5 "${SERVER_URL}/readyz"; then
  echo "  诊断: 看 datazen-server 日志（命令见尾部输出）" >&2
  exit 1
fi

echo "==> 冒烟: GET /api/v1/auth/login（期待 302 → dex）"
LOGIN_CODE="$(curl -s -o /dev/null -w '%{http_code}' -m 10 "${SERVER_URL}/api/v1/auth/login" || true)"
case "$LOGIN_CODE" in
  302)
    echo "  OK: 302（OIDC 装配、issuer 精确匹配、容器侧发现全部通过）"
    ;;
  503)
    echo "  警告: 503 —— server 无法完成 OIDC 发现/上游不可达。" >&2
    echo "  最常见原因: issuer 与 dex 配置不一致，或 compose 网络别名 id.localhost 未生效。" >&2
    echo "  诊断: compose logs datazen-server | grep oidc" >&2
    ;;
  *)
    echo "  警告: HTTP ${LOGIN_CODE}（期待 302）。" >&2
    echo "  诊断: 确认 environment 里 OIDC 三件套与 public_origin 成套（config.rs 校验）。" >&2
    ;;
esac

echo "==> 检查宿主侧 id.localhost 解析（浏览器登录页可达性）"
if curl -fsS -m 5 "http://${IDP_ISSUER_HOST}:5556/.well-known/openid-configuration" >/dev/null 2>&1; then
  echo "  OK: 本机可直接解析 ${IDP_ISSUER_HOST}"
else
  cat <<EOF
  提示: 本机解析不了 ${IDP_ISSUER_HOST}（Chrome/Chromium 与多数 Linux 的
  systemd-resolved 原生支持 *.localhost；Firefox/macOS/Windows 视环境而异）。
  浏览器若打不开 dex 登录页，把下面一行加进 hosts 文件后重试:
      127.0.0.1 ${IDP_ISSUER_HOST}
  hosts 文件位置: Linux /etc/hosts；macOS /etc/hosts；Windows C:\\Windows\\System32\\drivers\\etc\\hosts
EOF
fi

echo
echo "==================== 开发栈就绪 ===================="
cat <<EOF
  登录入口（SPA）:      ${SERVER_URL}/
  手动触发 OIDC 登录:  ${SERVER_URL}/api/v1/auth/login
  dex 测试账号:         dev@datazen.local  /  password
                       （sub=devadmin → datazen-local-org 的 orgAdmin）

  浏览器务必用 localhost 访问（别用 127.0.0.1: SSE Origin 与
  public_origin 是字符串等值比较，且 cookie 是 __Host- 前缀）。

  日志:      docker compose -f deploy/dev/docker-compose.dev.yml logs -f datazen-server
  管理库:    docker compose -f deploy/dev/docker-compose.dev.yml exec datazen-server \\
               sqlite3 /var/lib/datazen/datazen-admin.sqlite 'select principal_id, role_id, enabled from memberships;'
  停栈:      deploy/dev/down.sh   （-v 连卷清除）
EOF
