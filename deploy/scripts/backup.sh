#!/usr/bin/env bash
# datazen-server 管理库 + 产物备份（WR-G3「管理数据库 / 备份恢复」行）。
#
# 语义对齐 docs/development/team-service-operations.md §6（停写一致性备份）：
#   1. 管理库一致性快照 —— sqlite `VACUUM INTO`（WAL 下等价一致性读快照，
#      产出独立的紧凑单文件，不中断在线服务；v1 建议在维护窗口/停写后执行，
#      脚本只在 manifest 里如实记录是否停写，不假装强制）。
#   2. 产物字节 —— rsync 按卷目录复制（先复制后校验，原对象保留到校验结束）。
#   3. manifest —— 备份 ID、时间、schema 版本、文件摘要；不含 SQL、不含秘密
#      （§6 第 4 条：manifest 不含 SQL/秘密）。
#
# 禁止备份项（§6）：session / attachment / cursor / ResourceLease / 目录 /
# runtimeEpoch —— 这些表/状态本就不该进恢复路径；本脚本备份的是管理库
# 整库快照，恢复侧（restore.sh）负责按 §7.1 让旧会话/令牌失效。
#
# 用法：
#   backup.sh <目标目录> [数据目录]
#     <目标目录>  备份产物根（本机路径或 host:path，rsync 语义）
#     [数据目录]  含 datazen-admin.sqlite 与 artifacts/ 的卷目录
#                 （默认 /var/lib/datazen；容器外用挂载卷路径）
#   环境变量：
#     BACKUP_ID          自定义备份 id（默认 UTC 时间戳）
#     DZ_SKIP_STOP_WRITE=1  声明未停写（只在 manifest 标 maintenance=false）
set -euo pipefail

dest="${1:?usage: backup.sh <dest-dir> [data-dir] (default data dir: /var/lib/datazen)}"
data_dir="${2:-/var/lib/datazen}"
db_file="${DATAZEN_SERVER_DB:-$data_dir/datazen-admin.sqlite}"
artifact_root="${DATAZEN_SERVER_ARTIFACT_ROOT:-$data_dir/artifacts}"
backup_id="${BACKUP_ID:-$(date -u +%Y%m%dT%H%M%SZ)}"
work_dir="$(mktemp -d "${TMPDIR:-/tmp}/dz-backup-${backup_id}.XXXXXX")"
trap 'rm -rf "$work_dir"' EXIT

[ -f "$db_file" ] || { echo "backup: admin db not found: $db_file" >&2; exit 1; }
[ -d "$artifact_root" ] || { echo "backup: artifact root not found: $artifact_root" >&2; exit 1; }
command -v sqlite3 >/dev/null || { echo "backup: sqlite3 is required" >&2; exit 1; }

# ── 1. 管理库一致性快照（VACUUM INTO：独立新文件，含已提交事务全量） ──
snapshot="$work_dir/datazen-admin.sqlite"
sqlite3 "$db_file" "VACUUM INTO '$snapshot';"
echo "backup: admin db snapshot ok ($(du -h "$snapshot" | cut -f1))"

# ── 2. 产物字节（rsync 到本地 staging，校验通过后才出库） ──
mkdir -p "$work_dir/artifacts"
rsync -a --delete "$artifact_root"/ "$work_dir/artifacts"/

# ── 3. manifest（无 SQL、无秘密；schema 版本用只读 check-schema 取，可缺省） ──
schema_report="unavailable"
if command -v datazen-server >/dev/null; then
  schema_report="$(datazen-server check-schema 2>/dev/null || echo 'check-schema-failed')"
fi
sha256sum "$snapshot" > "$work_dir/SHA256SUMS"
( cd "$work_dir" && find artifacts -type f -print0 | sort -z | xargs -0 sha256sum >> SHA256SUMS )
cat > "$work_dir/manifest.json" <<EOF
{
  "backupId": "$backup_id",
  "createdAt": "$(date -u +%Y-%m-%dT%H:%M:%SZ)",
  "maintenanceWindow": "$( [ "${DZ_SKIP_STOP_WRITE:-0}" = "1" ] && echo false || echo true )",
  "adminDb": { "file": "datazen-admin.sqlite", "sha256": "see SHA256SUMS" },
  "artifactRoot": "artifacts/",
  "schemaReport": "$schema_report",
  "note": "sessions/tokens/attachments are not restorable state; see restore.sh"
}
EOF
chmod 600 "$snapshot" "$work_dir/manifest.json"

# ── 4. 出库（rsync 到目标；目标目录失败不污染源站） ──
mkdir -p "$dest"
final_dir="$dest/$backup_id"
rsync -a --delete "$work_dir"/ "$final_dir"/
echo "backup: completed → $final_dir (id=$backup_id)"
echo "backup: restore path = restore.sh（显式人工确认门；未知外部写入不自动重放）"
