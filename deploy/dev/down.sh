#!/usr/bin/env bash
# deploy/dev/down.sh —— 停开发侧件栈（up.sh 的对偶）。
#
# 默认: down --remove-orphans（停并移除容器/网络；保留镜像与数据卷，
#       下次 up 秒级回来，管理库/产物不丢）。
# -v:   连数据卷一起清除（管理库 + 产物字节全丢；下次 up 是全新首启）。
#
# 注意: 镜像默认保留（datazen-server:dev 与 dex 镜像）；要连镜像清掉用
#       docker compose -f deploy/dev/docker-compose.dev.yml down --rmi all。
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
COMPOSE_FILE="${SCRIPT_DIR}/docker-compose.dev.yml"

usage() {
  cat <<'EOF'
用法: deploy/dev/down.sh [选项]

选项:
  -v, --volumes   连数据卷清除（admin-db 与 artifacts 全丢，管理库/登录
                  会话/种子行/产物字节不可恢复）
  -h, --help      本帮助
EOF
}

VOLUMES=0
while [ $# -gt 0 ]; do
  case "$1" in
    -v|--volumes) VOLUMES=1 ;;
    -h|--help) usage; exit 0 ;;
    *) echo "未知选项: $1（--help 看用法）" >&2; exit 2 ;;
  esac
  shift
done

ARGS=(down --remove-orphans)
[ "$VOLUMES" -eq 1 ] && ARGS+=(-v)

if [ "$VOLUMES" -eq 1 ]; then
  echo "==> 停栈并清除数据卷（管理库与产物将全部丢失）"
else
  echo "==> 停栈（保留数据卷；-v 可清卷）"
fi

docker compose -f "$COMPOSE_FILE" "${ARGS[@]}"

if [ "$VOLUMES" -eq 1 ]; then
  echo "完成: 容器与卷已清除。下次 deploy/dev/up.sh 是全新首启（重新 migrate + 种子）。"
else
  echo "完成: 容器已停。数据仍在卷里，deploy/dev/up.sh 可原样恢复现场。"
fi
