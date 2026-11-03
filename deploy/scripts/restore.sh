#!/usr/bin/env bash
# datazen-server 恢复（WR-G3「备份恢复（不自动重放未知外部写入）」行）。
#
# 语义对齐 team-service-operations §7（灾难恢复）：
#   * **显式人工确认门**：交互式确认 + 双重开关；脚本永不自动启动 server、
#     永不自动重放任何请求/Job/令牌。
#   * 恢复 = 管理库整库替换 + 产物字节按 manifest 复制。旧登录会话、提交
#     令牌、attachment 随旧库一起消失（§7.1 第 3 条），恢复后客户端必须
#     重新登录；**不得**用新 key 自动重放旧请求。
#   * **RPO 期间的未知外部写入**（§7.2）：备份水位后已发生的业务库写入
#     不会被本脚本发现或撤销——恢复后先核验、后开放写受理；无法证明的
#     范围记 unknown，管理员明确重审后才能创建新计划。业务库时间点恢复
#     是独立审批流程，与本脚本无关。
#
# 用法：
#   restore.sh <备份目录> [数据目录]
#   非交互（CI/演练）环境必须同时满足：
#     CONFIRM_RESTORE=yes  且  I_UNDERSTAND_RESTORE_OVERWRITES_STATE=yes
#   演练模式（只校验不落盘）：DRY_RUN=1
set -euo pipefail

backup_dir="${1:?usage: restore.sh <backup-dir> [data-dir] (default data dir: /var/lib/datazen)}"
data_dir="${2:-/var/lib/datazen}"
db_file="${DATAZEN_SERVER_DB:-$data_dir/datazen-admin.sqlite}"
artifact_root="${DATAZEN_SERVER_ARTIFACT_ROOT:-$data_dir/artifacts}"

[ -f "$backup_dir/manifest.json" ] || { echo "restore: manifest.json missing in $backup_dir" >&2; exit 1; }
[ -f "$backup_dir/datazen-admin.sqlite" ] || { echo "restore: admin db snapshot missing" >&2; exit 1; }
[ -f "$backup_dir/SHA256SUMS" ] || { echo "restore: SHA256SUMS missing" >&2; exit 1; }

# ── 1. 显式人工确认门 ──
if [ "${CONFIRM_RESTORE:-}" = "yes" ] && [ "${I_UNDERSTAND_RESTORE_OVERWRITES_STATE:-}" = "yes" ]; then
  echo "restore: non-interactive confirmation granted (CONFIRM_RESTORE + override flag)"
else
  echo "== datazen-server RESTORE =="
  echo " 这将用 $backup_dir 覆盖："
  echo "   管理库：$db_file"
  echo "   产物：  $artifact_root"
  echo " 备份水位之后发生的一切受理记录、产物与密钥版本变更都会丢失。"
  echo " 恢复后：所有会话/令牌失效，未知外部写入不自动重放（先核验后开放写）。"
  printf ' 输入大写 RESTORE 继续（其他任意输入放弃）：'
  read -r answer
  [ "$answer" = "RESTORE" ] || { echo "restore: aborted by operator"; exit 2; }
fi

# ── 2. 完整性校验（先校验后落盘；失败备份不进恢复） ──
work_dir="$(mktemp -d "${TMPDIR:-/tmp}/dz-restore.XXXXXX")"
trap 'rm -rf "$work_dir"' EXIT
rsync -a "$backup_dir"/ "$work_dir"/
( cd "$work_dir" && sha256sum --quiet --check SHA256SUMS ) \
  || { echo "restore: checksum mismatch — refusing to restore a broken backup" >&2; exit 1; }
echo "restore: checksum ok"

if [ "${DRY_RUN:-0}" = "1" ]; then
  echo "restore: DRY_RUN=1 → validation only, nothing written"
  exit 0
fi

# ── 3. 停服前置检查：拒绝在服务运行时热替换（避免 WAL 分叉） ──
if pgrep -f 'datazen-server serve' >/dev/null 2>&1; then
  echo "restore: datazen-server is still running — stop it first (see §5.2 回退/维护窗口)" >&2
  exit 1
fi

# ── 4. 管理库替换（原子：先落临时名再 rename；清掉旧 WAL/SHM） ──
mkdir -p "$data_dir"
install -m 600 "$work_dir/datazen-admin.sqlite" "$data_dir/.datazen-admin.sqlite.restore"
rm -f "$db_file-wal" "$db_file-shm"
mv -f "$data_dir/.datazen-admin.sqlite.restore" "$db_file"
echo "restore: admin db replaced"

# ── 5. 产物字节恢复（不带 --delete：水位后的未知产物只补不删，交人工核验） ──
mkdir -p "$artifact_root"
rsync -a "$work_dir/artifacts"/ "$artifact_root"/
echo "restore: artifacts restored (no --delete: post-watermark files are kept for review)"

# ── 6. 恢复后清单（脚本到此为止；每一步都需要人执行） ──
cat <<'EOF'
restore: done. MANDATORY follow-ups (manual, in order):
  1. 迁移 lock 核验：datazen-server check-schema（落后=3 / 超前=4 → 先处理再启动）。
  2. 轮换 server_secret（DATAZEN_SERVER_SECRET）并重启：旧提交令牌/密钥版本
     随新 key 失效（§7.1 第 3 条：签发使用新 keyVersion，旧请求不得自动重放）。
  3. datazen-server migrate → serve。
  4. 只开放管理员核验与只读路径；对备份水位后的请求按审计与目标证据核验
     （§7.2），unknown 范围不自动重放，管理员重审后才能创建新计划。
  5. 恢复 smoke（§7.3）：重新登录、跨组织拒绝、旧令牌拒绝、产物摘要完整。
EOF
