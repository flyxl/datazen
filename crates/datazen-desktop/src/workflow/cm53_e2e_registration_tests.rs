//! CM-53（AI/MCP/Wapp 归属）F 层 e2e 的**自动化实现点登记**（WR-A4「CM-53
//! e2e 回写」）。
//!
//! # 登记什么
//!
//! CM-53 的定义在
//! `docs/architecture/platform/connection-management.md` §9（授权后同队列
//! 可见、未授权拒绝、断开只清理自己资源）；它的 F 层旅程自动化点 =
//! **`e2e/specs/ai-grant-lifecycle-journey.ts`**（真实 IPC 面、不依赖 AI
//! key、随 `pnpm e2e` 的 specs 发现机制 `./specs/**/*.ts` 实测执行）。
//!
//! 本模块把这条登记**钉成可执行断言**：旅程用例搬家、改名、断言半边丢失、
//! 或被移出 wdio 发现范围，这里先红——登记不能悄悄烂掉。
//!
//! # 阶段归属（不因本登记而变更）
//!
//! CM-53 的 **W1 断言**按
//! `docs/archive/development/platform-development-plan.md`（#227 归档，内容冻结）
//! §17/§11 仍归 P7/P8 退出门槛（「CM-05～07、38～40、53～56、59、60 单实例部分…的 W1 断言」）；
//! 本登记只回写 **F 层旅程**的自动化落点，不把 W1 计入本切片门槛。H 层已
//! 交付（`crates/datazen/src/ai/application/ownership.rs` +
//! `src-tauri/src/commands/ai/grant.rs`），见
//! `docs/architecture/platform/consumer-adapters.md` 的旅程表。

use std::path::PathBuf;

/// 仓库根（`crates/datazen-desktop/` 上溯三级，与 `doc_reference_tests` 同形）。
fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("crates/datazen-desktop 位于 crates/ 下，上溯两级即仓库根")
        .to_path_buf()
}

fn read(relative: &str) -> String {
    std::fs::read_to_string(repo_root().join(relative))
        .unwrap_or_else(|error| panic!("{relative} 应当可读：{error}"))
}

/// 行为钉子：CM-53 F 层旅程的自动化实现点存在且以 CM-53 命名。
#[test]
fn the_cm53_frontend_journey_has_a_named_automation_point() {
    let spec = read("e2e/specs/ai-grant-lifecycle-journey.ts");
    assert!(
        spec.contains("describe('AI editor-session grant lifecycle (CM-53)'"),
        "e2e 旅程用例必须以 CM-53 命名，登记才能指向它"
    );
    assert!(
        spec.contains("ai_grant_editor_session"),
        "旅程必须走真实授权 IPC 面（grant）"
    );
}

/// 行为钉子：旅程覆盖 CM-53 的三条腿——授权绑定当前编辑器 session、
/// 未授权/冒名拒绝、断开只清理授权自有资源（授权随 session 关闭失效）。
#[test]
fn the_cm53_journey_covers_the_three_contract_legs() {
    let spec = read("e2e/specs/ai-grant-lifecycle-journey.ts");
    // 授权后同队列可见（grant + touch 保活，同 session 句柄）。
    assert!(
        spec.contains("ai_touch_editor_grant"),
        "授权绑定 + 保活（同队列可见半边）必须在旅程里",
    );
    // 未授权拒绝：冒名 client 与他人 pane 两条拒绝路径都要有。
    assert!(
        spec.contains("aiGrantClientMismatch|PermissionDenied|ClientMismatch"),
        "冒名 clientInstanceId 的拒绝断言必须在旅程里"
    );
    assert!(
        spec.contains("aiGrantSessionGone|SessionGone"),
        "非本 pane 的拒绝断言必须在旅程里"
    );
    // 断开只清理自己资源：session 关闭后授权失效，编辑器资源不再可用。
    assert!(
        spec.contains("close_session") && spec.contains("mode: 'rollbackAndClose'"),
        "session 关闭触发授权失效的断言必须在旅程里"
    );
}

/// 行为钉子：旅程在 `pnpm e2e` 的实测范围内（wdio specs 发现不含排除项）。
#[test]
fn the_cm53_journey_rides_the_standard_pnpm_e2e_discovery() {
    let conf = read("e2e/wdio.conf.ts");
    assert!(
        conf.contains("specs: ['./specs/**/*.ts']"),
        "CM-53 旅程靠 wdio 的 specs 发现机制实测；发现范围被收窄时必须显式重新登记"
    );
}

/// 行为钉子：合规矩阵的 CM-53 锚点仍在（定义文档 + 旅程归属表 + W1 阶段
/// 门槛）。文档重组导致锚点漂移时，登记随之更新而不是失效。
#[test]
fn the_compliance_matrix_anchors_for_cm53_are_intact() {
    let connection_management = read("docs/architecture/platform/connection-management.md");
    assert!(
        connection_management.contains("CM-53"),
        "CM-53 的定义文档锚点（connection-management §9）必须存在"
    );
    let consumer_adapters = read("docs/architecture/platform/consumer-adapters.md");
    assert!(
        consumer_adapters.contains("CM-53")
            && consumer_adapters.contains("ai_grant_editor_session"),
        "consumer-adapters 旅程表必须把 AI 授权旅程归属到 CM-53"
    );
    // #227 已将该计划归档至 docs/archive/（内容冻结，「53～56」锚点仍在）。
    let plan = read("docs/archive/development/platform-development-plan.md");
    assert!(
        plan.contains("53～56"),
        "P7 退出门槛必须仍包含 CM-53 的 W1 断言（53～56）；提前收口时同步改写本登记"
    );
}
