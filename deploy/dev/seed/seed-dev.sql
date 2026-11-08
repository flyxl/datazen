-- 开发栈 OIDC 登录种子：把 dex 静态用户的 sub（id_token 的 sub =
-- dex/config.yaml 的 staticPasswords[].userID = 'devadmin'）映射成
-- datazen-local-org 组织内的 orgAdmin membership。
--
-- 依据（crates/datazen-server/src/auth/claims.rs 的 resolve_membership）：
-- OIDC 回调校验通过后，server 用 (organization_id, external_subject) 查
-- memberships 表，只认 enabled=1 的行；未命中 = 403 拒绝登录（不在登录时
-- 隐式开户）。因此开发栈必须在首次登录前落这一行——server 二进制没有
-- 「建 membership」的子命令（main.rs 只有 serve | migrate | check-schema），
-- 本文件就是那个缺口在开发栈侧的补位。
--
-- 列形与 CHECK 约束以 storage/migrations.rs 的 V5_AUTHZ_BASELINE 为准：
-- role_id ∈ ('orgAdmin','orgMember','connectionOwner','connectionViewer',
-- 'jobOperator','auditor')；PK = (organization_id, principal_id)。
--
-- 幂等：每次 `up` 收敛这一行为目标状态（upsert），不触碰其他行。
-- created_at 在冲突时保留原值（首次创建时刻），只刷新 role / subject /
-- enabled / updated_at——开发栈不关心审计精度，只要求可重复执行。

INSERT INTO memberships (
  organization_id, principal_id, role_id, external_subject, enabled,
  created_at, updated_at
) VALUES (
  'datazen-local-org', 'dev-admin', 'orgAdmin', 'devadmin', 1,
  strftime('%Y-%m-%dT%H:%M:%fZ', 'now'),
  strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
)
ON CONFLICT (organization_id, principal_id) DO UPDATE SET
  role_id          = excluded.role_id,
  external_subject = excluded.external_subject,
  enabled          = 1,
  updated_at       = excluded.updated_at;
