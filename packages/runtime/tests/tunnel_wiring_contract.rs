//! `ResourceManager` ⇄ `TunnelLedger` 的**接线面**与 **CM-27 全阶段失败矩阵**。
//!
//! 闭合 `packages/runtime/src/tunnel/mod.rs:52-60` 登记表里的第 **(1)** 格
//! 「实现缺失」：permit / socket / 隧道 / handshake / init / register
//! **每一个阶段失败时，隧道引用都必须正确回滚**。CM-27 把它拆成两半
//! （`src/tunnel/mod.rs:~24`）：**建隧道失败不落账** + **隧道开成后回滚释放**，
//! 本文件两半各带用例。
//!
//! 另两格在别处：
//!
//! * **第 (2)** 格（登记表里缺号的那一格）= CM-27「可确认关闭的资源许可归零」，
//!   主语是**资源许可**、不是隧道引用，已由三处闭合并各自带测试 —— 见
//!   本文件末尾 `cell_two_is_a_resource_permit_cell_not_a_tunnel_cell`。
//! * **第 (3)** 格（隧道引用与物理预算的**归属配对**）在同目录的
//!   `tunnel_budget_pairing.rs`。
//!
//! # 唯一计数器
//!
//! 本文件**不**引入任何计数：断言一律读 `ResourceManager::tunnel_refs`，而它是
//! `TunnelLedger::ref_count` 的直通投影。`last_cell_refuses_to_keep_a_second_tally`
//! 把「不许有第二个计数器」钉成可执行的断言（CM-32 的 `single_counter_audit`）。
//!
//! 那条自证原先是对源码做**字面子串黑名单**，改个名就能把第二本账整套搬进来（实测
//! `local_tunnels: AtomicU64` + `cached_refs: u32` 配 `fetch_add`/`fetch_sub` 全绿通过）。
//! 现在改由 `syn` 解析源码，判据一律取**闭合集合**，实现与判据说明见
//! [`single_tally_audit`]。

mod single_tally_audit;
mod tunnel_arch_support;

use tunnel_arch_support::{request, tunnel_spec, unwired, wired};

use datazen_runtime::resource::LeaseState;

// ============================================================ 接线面

#[test]
fn the_manager_learns_tunnels_exist_only_through_the_wired_port() {
    let (plain, _t, _c) = unwired();
    assert!(
        !plain.has_tunnel_port(),
        "a manager built by `new` must declare that it has no tunnel port"
    );

    let (wired, _t, _tun, _c) = wired();
    assert!(
        wired.has_tunnel_port(),
        "`with_tunnel_transport` is the only way a manager learns tunnels exist"
    );
}

#[test]
fn a_lease_declaring_a_tunnel_without_a_wired_port_is_refused_never_silently_direct() {
    let (mut manager, transport, connection_id) = unwired();
    let wanted = request(&connection_id, "appdb").via_tunnel(tunnel_spec());

    let error = manager
        .acquire(&wanted)
        .expect_err("a tunneled request must not quietly degrade to a direct connection");

    assert_eq!(error.reason(), "resourceTunnelPortNotWired");
    assert_eq!(
        (transport.opened(), transport.closed()),
        (1, 1),
        "the socket stage runs before the tunnel stage, so its resource is compensated"
    );
    assert_eq!(
        manager.lease_count(),
        0,
        "refusal happens inside `acquire`, so no lease row ever becomes visible"
    );
    assert!(manager.live_tunnels() == 0);
}

#[test]
fn a_direct_lease_never_touches_the_tunnel_ledger() {
    let (mut manager, _t, tunnel, connection_id) = wired();
    let lease = manager
        .acquire(&request(&connection_id, "appdb"))
        .expect("a direct lease needs no tunnel");

    assert_eq!(tunnel.opened(), 0, "a direct lease must not open a tunnel");
    assert_eq!(manager.live_tunnels(), 0);
    assert_eq!(manager.tunnel_close_calls(), 0);
    assert_eq!(
        manager.lease(&lease.lease_id).map(|r| r.state),
        Some(LeaseState::InUse)
    );
}

#[test]
fn a_tunneled_lease_holds_exactly_one_tunnel_reference() {
    let (mut manager, _t, tunnel, connection_id) = wired();
    let spec = tunnel_spec();

    manager
        .acquire(&request(&connection_id, "appdb").via_tunnel(spec.clone()))
        .expect("a wired tunnel port accepts the tunneled request");

    assert_eq!(
        manager.tunnel_refs(&spec),
        Some(1),
        "the authoritative count is the ledger's, and it reads exactly one"
    );
    assert_eq!(tunnel.opened(), 1);
}

// ============================================================ 格 (1)：全阶段失败矩阵

#[test]
fn stage_permit_failure_takes_no_tunnel_reference_at_all() {
    let (mut manager, _t, tunnel, connection_id) = wired();
    let spec = tunnel_spec();

    // permit 阶段：归属被禁用 ⇒ 申请在拿到任何东西之前就被拒。
    manager
        .disable(&connection_id)
        .expect("disabling an unused connection is itself clean");
    let error = manager
        .acquire(&request(&connection_id, "appdb").via_tunnel(spec.clone()))
        .expect_err("a disabled connection refuses the permit stage");

    assert_eq!(error.reason(), "resourceOwnerDisabled");
    assert_eq!(
        manager.tunnel_refs(&spec),
        None,
        "CM-27「建隧道失败不落账」：permit 阶段失败时压根没落过账"
    );
    assert_eq!(tunnel.opened(), 0);
}

#[test]
fn stage_socket_failure_takes_no_tunnel_reference_at_all() {
    let (mut manager, transport, tunnel, connection_id) = wired();
    let spec = tunnel_spec();
    transport.fail_open();

    let error = manager
        .acquire(&request(&connection_id, "appdb").via_tunnel(spec.clone()))
        .expect_err("the socket stage refused the connection");

    assert_eq!(error.reason(), "resourceTransportRefused");
    assert_eq!(
        manager.tunnel_refs(&spec),
        None,
        "the tunnel stage never ran, so the ledger never saw a reference"
    );
    assert_eq!(tunnel.opened(), 0);
}

#[test]
fn stage_tunnel_failure_lands_nothing_in_the_ledger_and_still_closes_the_socket() {
    let (mut manager, transport, tunnel, connection_id) = wired();
    let spec = tunnel_spec();
    tunnel.fail_open();

    let error = manager
        .acquire(&request(&connection_id, "appdb").via_tunnel(spec.clone()))
        .expect_err("the tunnel stage refused to open");

    assert_eq!(error.reason(), "resourceTunnelRefused");
    assert_eq!(
        manager.tunnel_refs(&spec),
        None,
        "CM-27「隧道引用正确」的前半句：建隧道失败**不落账**"
    );
    assert_eq!(
        (transport.opened(), transport.closed()),
        (1, 1),
        "the socket opened before the tunnel stage, so it must be closed again"
    );
    assert_eq!(
        manager.lease_count(),
        0,
        "a fully rolled-back acquire leaves no lease row behind"
    );
}

#[test]
fn stage_tunnel_failure_with_an_unconfirmed_compensating_close_retains_a_quarantined_row() {
    let (mut manager, transport, tunnel, connection_id) = wired();
    let spec = tunnel_spec();
    tunnel.fail_open();
    transport.fail_close();

    let error = manager
        .acquire(&request(&connection_id, "appdb").via_tunnel(spec.clone()))
        .expect_err("the tunnel stage refused to open");

    assert_eq!(error.reason(), "resourceTunnelRefused");
    assert_eq!(
        manager.tunnel_refs(&spec),
        None,
        "no reference was ever recorded, so there is nothing to retain"
    );
    assert_eq!(
        manager.lease_count(),
        1,
        "an unconfirmed close must leave a row so the close obligation survives"
    );
    let row = manager
        .leases()
        .into_iter()
        .next()
        .expect("the retained row is quarantined, not forgotten");
    assert_eq!(
        row.state,
        LeaseState::Quarantined,
        "the physical budget stays held until the close is confirmed"
    );
}

#[test]
fn stage_handshake_init_register_failure_releases_the_tunnel_reference_it_had_taken() {
    let (mut manager, transport, tunnel, connection_id) = wired();
    let spec = tunnel_spec();
    let lease = manager
        .acquire(&request(&connection_id, "appdb").via_tunnel(spec.clone()))
        .expect("the tunnel stage succeeded");
    assert_eq!(manager.tunnel_refs(&spec), Some(1));

    // 握手 / 初始化 / 注册三个阶段任一失败，走的是同一个补偿入口。
    manager
        .roll_back_unpublished(&lease.lease_id)
        .expect("rollback runs on a lease the table still holds");

    assert_eq!(
        manager.tunnel_refs(&spec),
        None,
        "CM-27「隧道引用正确」的后半句：隧道开成后回滚释放，且与预算同拍"
    );
    assert_eq!(manager.live_tunnels(), 0);
    assert_eq!(
        (transport.opened(), transport.closed()),
        (1, 1),
        "rollback closes the physical resource too"
    );
    assert_eq!(
        tunnel.closed(),
        1,
        "and the tunnel reference reaching zero is the only thing that closes the tunnel"
    );
}

#[test]
fn stage_handshake_init_register_failure_with_an_unconfirmed_close_keeps_the_tunnel_reference() {
    let (mut manager, transport, tunnel, connection_id) = wired();
    let spec = tunnel_spec();
    let lease = manager
        .acquire(&request(&connection_id, "appdb").via_tunnel(spec.clone()))
        .expect("the tunnel stage succeeded");
    transport.fail_close();

    manager
        .roll_back_unpublished(&lease.lease_id)
        .expect("rollback still reports, it just quarantines");

    assert_eq!(
        manager.tunnel_refs(&spec),
        Some(1),
        "an unconfirmed close releases no budget, so it may release no reference either"
    );
    assert_eq!(tunnel.closed(), 0);
}

// ============================================================ 缺号的那一格

/// 登记表只列了 (1) 与 (3)，缺 (2)。核对结论：**缺号的那一格是 CM-27 的
/// 「可确认关闭的资源许可归零」，主语是资源许可，不是隧道引用**，而且它**已经闭合**。
///
/// 三处闭合点互不知情、也都不需要知道隧道存在 —— 这本身就是「隧道不该进这一格」的证据：
///
/// * `src/budget/ledger.rs:397` —— INV-10 幂等核销，名额按原槽退回；
/// * `src/budget/coordinator.rs:428` —— 端口级幂等核销，把 `Unknown` 报成 `NotFound`；
/// * 宿主侧第三处（登记表自述）。
///
/// 本用例把「隧道不进这一格」钉成断言：确认关闭后**资源许可**归零，
/// 而隧道引用是否归零**另由 (3) 判**（此处因为走的是 `retire` 的确认关闭，也是 0，
/// 但两者的依据不同 —— 一个是许可账本，一个是隧道台账）。
#[test]
fn cell_two_is_a_resource_permit_cell_not_a_tunnel_cell() {
    let (mut manager, _t, _tunnel, connection_id) = wired();
    let spec = tunnel_spec();
    let lease = manager
        .acquire(&request(&connection_id, "appdb").via_tunnel(spec.clone()))
        .expect("the tunnel stage succeeded");

    manager
        .retire(&lease.lease_id)
        .expect("the confirmed close releases the permit");

    assert_eq!(
        manager.idle_lease_count(),
        0,
        "CM-27 的这一格判据是资源许可，不读隧道台账"
    );
    assert_eq!(
        manager.tunnel_refs(&spec),
        None,
        "引用归零是 (3) 的判据：预算核销了，引用才同拍归还"
    );
}

// ============================================================ 唯一计数器自证

/// 把「不许有第二个计数器」钉成可执行的断言（CM-32 的 `single_counter_audit`）。
///
/// 判据不是字面黑名单而是**闭合集合**，理由与全部细则在 [`single_tally_audit`]：
///
/// 1. `ResourceManager` 的字段名集合必须**恰好**等于闭合名单 `MANAGER_FIELDS`，
///    `tunnels` 的类型必须恰为 `Option<TunnelLedger>`；
/// 2. `tunnel_wiring.rs` 里凡取用 `self.tunnels` 的方法只准碰 `self.tunnels`；
/// 3. `tunnel_refs()` 把取数委托给台账、只经 `TunnelLedger::ref_count`、不碰别的
///    `self` 字段、没有整数字面量；
/// 4. `src/resource/*.rs` 里一个 `static` 都没有。
///
/// 只读结构、不读字符串：改名换型（`AtomicU64` → `u64`，`cached_refs` → `spare`）
/// 一样会被判红，而 `idle_ttl_seconds: u64` 这类合法整数字段不受牵连。
#[test]
fn last_cell_refuses_to_keep_a_second_tally() {
    let resource_dir =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src").join("resource");
    let findings = single_tally_audit::audit(&resource_dir);
    assert!(
        findings.is_empty(),
        "{}",
        single_tally_audit::describe(&findings)
    );
}
