# T10 加密钥匙串 — 测试报告

- 任务：连接密码经 `keyring` v4 存 OS 钥匙串（service `datazen-native`，语义对齐桌面 `key_backend`），`connections.json` 不再存明文密码；钥匙串不可用时连接页如实报错。
- 日期：2026-11-14
- 提交：见 tasks.md T10 行。

## 结论

通过。GR1 138+55、GR3 53 全绿。沙箱无会话总线：**file 后端全链实测**（设/取往返），**keyring 后端的缺席路径实测**（fail-closed + 逃生口）——真钥匙串往返留给用户环境（同桌面 `DATAZEN_TEST_KEYRING` 纪律）。

## 实现面（如实）

- **`secrets.rs`**（桌面 `key_backend`/`key_store` 阶梯的宿主形态）：
  - 后端一次性解析：`DATAZEN_NATIVE_KEYRING=file` → `.key` 文件（0600）；缺省/`keyring` → OS 钥匙串（`keyring` v4 `v1` API，service `datazen-native`、account `app-encryption-key`——桌面同 crate 同 API 面）。
  - keyring 阶梯（逐条对齐桌面）：读到密钥→删遗留 `.key`；NoEntry→遗留 `.key` 存在则尽力迁入钥匙串（迁不动保留文件）；全新→生成 32 字节存钥匙串（写不动落文件）；钥匙串不可达→遗留 `.key` 救援；两者皆无→**fail-closed**（新密钥会让既有密文不可读），错误句点名逃生口 `DATAZEN_NATIVE_KEYRING=file`。
- **store.rs 集成**：主密钥**惰性加载**（仓储构造零钥匙串依赖；无密码流程永不触达钥匙串——无总线环境的既有功能零回归）。`save`：非空密码先 `encrypt_field`（AES-256-GCM，共享层同一实现）再落盘——`connections.json` 从此无明文密码。`get`：密码在则解密（解不开=迁移期遗留明文，照常工作）；钥匙串不可达→诚实 `BackendUnavailable("the password of 'X' is unavailable (…逃生口)")`——连接路径如实报错、不回退明文。`list`：尽力解密，解不开为 None（清单页只显名字；密文不外漏）。
- **共享层一行**：`connection_import` 再导出 `decrypt_field`（`encrypt_field` 的逆，原先未导出）。
- **依赖**：宿主加 `keyring` v4（v1）、`rand` 0.8、`base64` 0.22——全在锁文件，离线解析。

## 测试（GR3 新增 5 例）

- `the_backend_mapping_is_the_desktops_ladder`：file/keyring/缺省/乱值的纯映射。
- `the_file_backend_round_trips_and_restricts_the_file`：密钥 32 字节、重载稳定、文件 0600。
- `a_keyring_that_cannot_be_reachhed_fails_closed_without_a_legacy_key`：本沙箱无总线——keyring 后端 fail-closed（错误句含逃生口、无 `.key` 出现）；写一个 `.key` 后 keyring 后端救援成功。
- `passwords_seal_on_save_and_reveal_on_get`（store）：存 "hunter2" → 磁盘 JSON 无明文有密文 → 重开仓储 get/list 均还原明文。
- `an_encrypted_password_without_a_key_is_an_honest_unavailability`（store）：file 后端写密文 → 只移 store 不移密钥 → keyring 后端（无总线）get 诚实报"password of 'sealed' is unavailable"+逃生口；list 密码为 None 不漏密文。
- **既有测试适配**：store 测试夹具统一 `DATAZEN_NATIVE_KEYRING=file`（桌面的 CI 纪律同款；夹具带密码的既有用例原会在无总线环境 fail-closed）。

## 门禁

- GR1 `cargo test -p datazen-ui-api -p datazen-ui --offline --lib -- --test-threads=1`：138 + 55 passed。
- GR2 `cargo check -p datazen-ui-api -p datazen-ui -p datazen-rune-host --offline`：exit 0。
- GR3 `cargo test -p datazen-rune-host --offline --lib -- --test-threads=1`：53 passed。
- GR4 `cargo check -p datazen-rune-host --offline`：exit 0。
- G1–G4（前端）：未改 `packages/frontend`、`packages/web`，不适用。

## 遗留（如实）

- 真 OS 钥匙串的往返（有 secret service 的桌面）未在本环境验证——keyring 调用形态与桌面逐字对齐（同 crate 同 API 同阶梯），沙箱只证缺席路径；真机验证留给用户环境。
- 遗留明文（T10 前的 `connections.json`）读路径兼容（迁移期）；下次 save 自动封存。无批量重写迁移命令。
- SSH 隧道密码字段未封存（宿主未接隧道域，字段恒 None——接隧道时同一 `seal_password` 补一行）。
- 修改密码/轮换密钥的管理面未做（桌面亦无）。
