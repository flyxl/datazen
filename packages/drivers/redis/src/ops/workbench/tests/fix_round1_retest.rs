// [tester] Bug 循环第 1 轮修复后的复测新增面（HEAD `c844b6804`，2026-09-22）。
//
// 只测不修：本文件不放宽、也不改写任何既有判定，只把两条此前**无人断言**的事实
// 钉成可证伪用例 ——
//
//   1. `is_connection_level_failure` 的错误分类表。Cluster 修复的全部赌注都在这张
//      表上：redis 0.27.6 的 cluster 分发层在**单命令**路径上也会把服务端错误折叠成
//      `Err`（`cluster_async/mod.rs` `try_cmd_request` → `Value::extract_error`），
//      所以"按项降级"能不能成立，取决于 `-ERR` / `-WRONGTYPE` 被判成非连接级、
//      `-MOVED` / `-CLUSTERDOWN` 被判成连接级。既有用例只经 `ClusterFoldingConn`
//      自造的错误对象（`ErrorKind::IoError`）走过一次，分类表本身从未被逐类覆盖。
//      这里用 redis 自己的解析器（`parse_redis_value` + `Value::extract_error`）
//      造真实 `ErrorKind`，不靠手写枚举。
//
//   2. redis 自己**怎么路由**这些探测命令（`cluster_routing::RoutingInfo::for_routable`）。
//      `ClusterFoldingConn` 用"最后一个参数即键"建模路由，这比真实客户端更乐观，
//      因此路由事实必须由 redis 的路由函数自己回答，而不是由 mock 替它声明
//      （首轮 Tester 对"mock 把被测行为 mock 掉"的批评在这一点上同样适用）。
//      下面的用例全部**通过**，它们是 BUG-007 的进程内证据与升级绊线。

use super::*;

use redis::cluster_routing::{
    get_slot, AggregateOp, MultipleNodeRoutingInfo, ResponsePolicy, RoutingInfo,
    SingleNodeRoutingInfo,
};
use redis::{ErrorKind, RedisError};

/// A real `RedisError` as the async client sees it: parse the RESP error line, then
/// fold it exactly like `try_cmd_request` does on the single-command path.
fn server_error(line: &str) -> RedisError {
    let value = redis::parse_redis_value(format!("-{line}\r\n").as_bytes())
        .expect("test fixture produced an invalid error reply");
    value
        .extract_error()
        .expect_err("an error line must fold into an error")
}

fn probe(segments: &[&str], key: &str) -> redis::Cmd {
    let mut cmd = redis::cmd(segments[0]);
    for segment in &segments[1..] {
        cmd.arg(*segment);
    }
    cmd.arg(key);
    cmd
}

// --- 1. 错误分类表：cluster 按项降级的真正依据 -----------------------------------

#[test]
fn test_tester_connection_failure_classifier_matches_real_reply_kinds() {
    // 服务端**答复了**"这个字段问不到"：探测本身跑完了，只能降级单字段，
    // 否则侧栏在默认 noeviction / 非 LFU 实例上就会常红（BUG-001 的症状）。
    let degrades = [
        "ERR this object's access frequency counter is not available",
        "ERR unknown command 'MEMORY'",
        "WRONGTYPE Operation against a key holding the wrong kind of value",
        "NOPERM this user has no permissions to run the 'memory' command",
        "LOADING Redis is loading the dataset in memory",
    ];
    for line in degrades {
        let error = server_error(line);
        assert!(
            !is_connection_level_failure(&error),
            "server answer {line:?} (kind {:?}) must degrade one field, never abort the probe",
            error.kind()
        );
    }

    // 服务端**没有答复**这个键：连接/拓扑级故障，整条上抛。把它读成"该键没有属性"
    // 会把掉线伪装成空态（`cluster_topology::a_transport_failure_on_the_single_command_path_aborts_the_probe`
    // 已断言形状，此处补全错误种类）。
    let aborts = [
        "MOVED 3999 127.0.0.1:30003",
        "ASK 3999 127.0.0.1:30003",
        "TRYAGAIN Multiple keys request during rehashing of slot",
        "CLUSTERDOWN The cluster is down",
        "CROSSSLOT Keys in request don't hash to the same slot",
    ];
    for line in aborts {
        let error = server_error(line);
        assert!(
            is_connection_level_failure(&error),
            "topology-level answer {line:?} (kind {:?}) must abort the probe",
            error.kind()
        );
    }

    // 非 RESP 文本的传输级错误同样上抛。
    assert!(is_connection_level_failure(&RedisError::from((
        ErrorKind::IoError,
        "connection reset"
    ))));
    assert!(is_connection_level_failure(&RedisError::from((
        ErrorKind::ClientError,
        "no such node"
    ))));
}

#[test]
fn test_tester_a_moved_redirect_degrades_nothing_but_costs_a_round_trip() {
    // 分类表的另一面：MOVED 在客户端内部会被重定向重试（`request.rs`
    // `RetryMethod::MovedRedirect` → `Retry::Immediately` + `RebuildSlots`），
    // 只有**重试耗尽**后才以 `ErrorKind::Moved` 抵达这里 ⇒ 上抛。
    // 这条断言的意义是把"重定向属正常成本"和"掉线属失败"从判定上分开：
    // 若有人为了少一次报错而把 Moved 折成 Nil（=按项降级），本用例即红。
    let moved = server_error("MOVED 401 127.0.0.1:30001");
    assert_eq!(moved.kind(), ErrorKind::Moved);
    assert!(is_connection_level_failure(&moved));
    // 而真正的按项错误（freq 计数器不可用）永远不该被当成 Moved。
    assert_ne!(server_error("ERR not supported").kind(), ErrorKind::Moved);
}

// --- 2. redis 的真实 cluster 路由：BUG-007 的进程内证据 --------------------------

/// 采样/侧栏用到的两个键，必须落在不同 slot，否则下面的对照臂毫无意义。
const KEY_A: &str = "kvbar:user:1001";
const KEY_B: &str = "kvbar:session:abcd";

#[test]
fn test_tester_cluster_routing_follows_the_key_for_keyed_probes() {
    // 控制臂：单词命令 `TYPE` / `PTTL` 的路由确实跟着键走。
    assert_ne!(
        get_slot(KEY_A.as_bytes()),
        get_slot(KEY_B.as_bytes()),
        "test fixture must straddle two slots"
    );
    for name in ["TYPE", "PTTL"] {
        let a = RoutingInfo::for_routable(&probe(&[name], KEY_A));
        let b = RoutingInfo::for_routable(&probe(&[name], KEY_B));
        assert!(
            matches!(
                a,
                Some(RoutingInfo::SingleNode(
                    SingleNodeRoutingInfo::SpecificNode(_)
                ))
            ),
            "{name} must route to a specific slot, got {a:?}"
        );
        assert_ne!(
            a, b,
            "{name} must be routed by its key, otherwise a cluster sample collapses onto one node"
        );
    }
}

#[test]
fn test_tester_cluster_routing_ignores_the_key_for_two_word_probes() {
    // 真实 redis 0.27.6 的 `RoutingInfo::for_routable` 只认识它自己列出的两词命令，
    // 其余一律 `_ => arg_idx(1)` —— 于是 `MEMORY USAGE <key>` / `OBJECT ENCODING <key>`
    // 的"键"是**子命令 token**（USAGE / ENCODING / …），而不是那个键。
    // 后果：Cluster 上这 4 条探测命令必然先被投到错误的分片、吃一次 `-MOVED`、
    // 再重定向重试（并触发一次 slots 重建）。⇒ "Cluster 侧栏恰 6 次单命令往返 /
    // 命令级 7 次"这一硬口径不成立。
    //
    // 本用例是**绊线**：若将来 redis 修正了路由（或我们改成显式按地址路由），
    // 这两条 assert_eq 会失败，提醒我们同时回收口径与 R 项 9a 的 MONITOR 核对表。
    //
    // [修复轮 3 补注] 本用例只问 **redis 自己的路由表**，表未变 ⇒ 仍然成立，断言一字未松。
    // 变的是驱动侧：探测命令不再交给这张表，而是按 `get_slot(key)` 显式寻址
    // （`SlotRoutedConnection`），所以"必吃 -MOVED + 槽位刷新"现在是**假如不寻址**的后果，
    // 由 `cluster_topology::an_unaddressed_two_word_probe_is_moved_and_rebuilds_the_slot_map`
    // 作为绊线守住（替身自此按 `for_routable` 路由，不再假设"按键走"）。
    for segments in [
        ["MEMORY", "USAGE"],
        ["OBJECT", "ENCODING"],
        ["OBJECT", "IDLETIME"],
        ["OBJECT", "FREQ"],
    ] {
        assert_ne!(
            get_slot(KEY_A.as_bytes()),
            get_slot(KEY_B.as_bytes()),
            "test fixture must straddle two slots, otherwise this comparison proves nothing"
        );
        let by_a = RoutingInfo::for_routable(&probe(&segments, KEY_A));
        let by_b = RoutingInfo::for_routable(&probe(&segments, KEY_B));
        assert_eq!(
            by_a, by_b,
            "{} {} routed by the key? then the subcommand-token routing claim is stale",
            segments[0], segments[1]
        );
        assert!(
            matches!(
                by_a,
                Some(RoutingInfo::SingleNode(
                    SingleNodeRoutingInfo::SpecificNode(_)
                ))
            ),
            "{} {} still goes to one specific slot, got {by_a:?}",
            segments[0],
            segments[1]
        );
    }
}

#[test]
fn test_tester_dbsize_on_cluster_is_an_all_master_sum_not_a_shard_view() {
    // 「契约偏离 · Cluster 分布不是普查」与协调者裁定都把 `dbsize` 说成
    // "只是被路由到的那一个分片的 DBSIZE"。redis 自己不同意：DBSIZE 属于
    // `MultiNode(AllMasters) + Aggregate(Sum)` ⇒ 返回的是**整个集群**所有主节点的
    // 键数之和（并且任一分片失败即整条命令失败）。
    // 结论不是"代码算错了数"，而是**给 Wave 2 与 R 项 9b-补/9d 的口径写反了方向**：
    // M 偏大 ⇒ `truncated` 只会更保守（不会谎称普查），但 9d 的"空分片 ⇒ dbsize 0
    // 且 truncated false"在多分片非空集群上永远不成立，会被误判成回归。
    assert_eq!(
        RoutingInfo::for_routable(&redis::cmd("DBSIZE")),
        Some(RoutingInfo::MultiNode((
            MultipleNodeRoutingInfo::AllMasters,
            Some(ResponsePolicy::Aggregate(AggregateOp::Sum)),
        ))),
        "DBSIZE's cluster routing is what the 口径 hinges on"
    );
}

#[test]
fn test_tester_scan_has_no_cluster_route_so_each_round_may_change_shard() {
    // `SCAN` 在 redis 的路由表里显式返回 `None`（`cluster_routing.rs`
    // `b"SCAN" | … => None`），而 `ClusterConnection::req_packed_command` 对
    // `None` 的处理是 `unwrap_or(SingleNode(Random))` ⇒ `get_random_connection`。
    // ⇒ Cluster 上 `collect_sample` 的每一轮 SCAN 可能落到**不同分片**，
    // 上一轮的游标对下一轮的分片毫无意义（某一轮随机命中已扫完的分片即游标归零、
    // 提前结束）。所以文档里的"分片视图 / 该分片的 DBSIZE"这个说法本身不成立：
    // 它是"随机多分片混合视图"。契约字段与 `sampled == Σcounts` 不变式不受影响，
    // 故本条只作为 BUG-007 的证据用例（不是失败断言）。
    let mut scan = redis::cmd("SCAN");
    scan.arg(0u64).arg("COUNT").arg(TYPE_SCAN_COUNT);
    assert_eq!(
        RoutingInfo::for_routable(&scan),
        None,
        "if redis ever routes SCAN deterministically, the 分片视图 wording can be restored"
    );
}

// --- 3. 补齐 sample_window_for 的第三条臂（Sentinel 不得被误并进 Cluster 预算）----

#[test]
fn test_tester_sample_window_honours_every_topology_arm() {
    // `sample_window_for` 有三个臂，`fix_round1` / `cluster_topology` 只覆盖了
    // Cluster 与 Standalone；Sentinel 走的是单节点预算（与 standalone 同窗），
    // 一旦有人把 Sentinel 当成"也是多分片"就会静默把 1000 钳到 200。
    assert_eq!(
        sample_window_for(None, Topology::Sentinel),
        DEFAULT_TYPE_SAMPLE_LIMIT
    );
    assert_eq!(
        sample_window_for(Some(0), Topology::Sentinel),
        DEFAULT_TYPE_SAMPLE_LIMIT
    );
    assert_eq!(
        sample_window_for(Some(6_000), Topology::Sentinel),
        MAX_TYPE_SAMPLE_LIMIT,
        "sentinel keeps the single-node ceiling, not the cluster one"
    );
    assert_eq!(
        sample_window_for(Some(6_000), Topology::Cluster),
        CLUSTER_TYPE_SAMPLE_LIMIT
    );
    // Cluster 上的小窗口不被抬到 200：钳制取 min，不放大。
    assert_eq!(sample_window_for(Some(50), Topology::Cluster), 50);
    assert_eq!(
        sample_window_for(Some(0), Topology::Cluster),
        CLUSTER_TYPE_SAMPLE_LIMIT,
        "the 0 ⇒ default fallback must happen before the cluster clamp"
    );
}
