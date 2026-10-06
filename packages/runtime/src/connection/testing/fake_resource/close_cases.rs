//! `closeResource` 的定点用例（fake-runtime-fixtures.md §4.1 F11、§5.3 规则 2/3/6、
//! connection-management.md §6.5 / §9.4）。
//!
//! 从 `tests.rs` 拆出来的理由不是「凑文件数」，而是 §9.4(b) 与 F11 这两项缺陷的判据
//! 要求**成对**的用例：每个「拒绝」都必须有同构造的「放行」对照，否则「夹具永远拒绝归池」
//! 与「判据真的在判」在断言上长得一模一样（`cm73_threads.rs` 的对照用例是同一条纪律）。
//! 这一来本主题的用例数量会超过 `tests.rs` 的规模红线，故独立成文件，共享的帮助函数在
//! [`super`]（归属理由见 `mod.rs` 末尾那段注释）。
//!
//! 与既有 CM-74 杀手用例的分工：
//! - `harness::cm74_release_order` 钉**释放顺序**（`handle closed` → `Closed` → `permit -1`）
//!   与「句柄非空时不得归池」；
//! - 本文件钉**判据的输入来源**（实测 vs 宿主声明）与 **F11 的完整后果**。
//!
//! 全程**不 sleep**；断言一律打在台账与 `assert_no_leak` 的返回值上，不看内存布局。

use super::{acquire, close_and_release, owner, provider};
use crate::connection::error::ProviderError;
use crate::connection::port::{CloseResourceRequest, ResourceHandle};
use crate::connection::types::{Counter, ResourceId};

// ---------------------------------------------------------------------------
// §9.4(b)：未知 resourceId 必须在写台账之前被拒 —— 那段 `None` 分支过去是死代码
// ---------------------------------------------------------------------------

/// §9.4(b)：那段 `None` 分支**过去是可证明的死代码**，现在是真实的、被覆盖的分支。
///
/// 死代码的论证（自行核实过）：顶层 `resources` IndexMap 只在 `create_resource` 的
/// `insert` 出现、全目录没有任何 `remove` / `retain` / `clear` 作用在它身上，
/// 而旧实现在同一次调用里先用 `resolve()` 查过一遍这张表，所以后面的 `get_mut` 恒为 `Some`。
/// 重构之后**没有第二次查表**：[`super::close`] 的 `evaluate_close` 就在 `get_mut`
/// 拿到的那个 slot 上跑，于是 `None` 这一支变成「未知 resourceId 的关闭」的唯一入口，
/// 本用例就是它的覆盖。
///
/// 它过去返回的那组值（`request.protocol_drained` + `resource.registered_handles()`
/// + 空注销清单）是**静默说谎**的形状：一条来自不存在资源的 `Closed`、一条凭宿主自述
/// 算出来的归池、以及一次 `-1` permit —— 而本用例钉住的是它必须先变成 `Err`。
///
/// 它必须**在写任何台账之前**失败：若判据先算了再说，就会留下一条来自不存在资源的
/// `Closed` 或 `-1`，守恒式（规则 4）当场失真。
#[test]
fn closing_an_unknown_resource_id_is_rejected_before_any_journal_write() {
    let provider = provider();
    let known = acquire(&provider).expect("acquire 必须成功");
    let seq_before = provider.journal().entries().len();

    // 造一个「从未签发过」的资源 id：§3.1 的 `ResourceHandle` 由提供方签发，
    // 这里用同样的构造口径伪造一个 id，模拟宿主拿着一张不属于本 provider 的凭证。
    let ghost = ResourceId::new("res_w1_9999");
    let forged = ResourceHandle::issue(&ghost, &Counter::new(1), &owner());
    let err = provider
        .close_resource(&CloseResourceRequest {
            handle: forged,
            registered_handles: 0,
            protocol_drained: true,
        })
        .expect_err("未知资源必须被拒：这是**可到达**的输入，不是夹具内部不变量");
    assert!(
        matches!(err, ProviderError::SessionLost(_)),
        "未知资源应报 SessionLost（§3.1 凭证校验），实际 {err:?}"
    );
    assert_eq!(
        provider.journal().entries().len(),
        seq_before,
        "拒绝必须发生在**任何**台账写入之前，否则规则 4 的守恒式会被一条幽灵事件打歪"
    );
    // 已知资源不受影响（这条断言排除「整个 provider 被这一笔搞坏」的误判）。
    assert_eq!(provider.journal().permit_balance(), 1);
    close_and_release(&provider, &known).expect("已知资源仍能正常关闭");
    assert!(
        provider
            .journal()
            .assert()
            .leak_invariant_violations()
            .is_empty(),
        "收尾 I1–I8 必须收口"
    );
}
