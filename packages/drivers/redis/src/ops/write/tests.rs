//! Tests for the string write path's TTL policy: `SET … KEEPTTL` as the default,
//! the `PTTL` + `PX` rescue when the server rejects the keyword, and the
//! `keepTtlFallback` bit both the UI and these tests assert on.
//!
//! Rejection texts are sentinels, never prose: an assertion here must not break
//! when Redis rewords an error.

use super::*;
use futures_util::FutureExt;
use redis::aio::ConnectionLike;
use redis::{RedisFuture, Value as RValue};
use std::collections::{HashMap, VecDeque};

const KEEP_TTL_REJECTED: &str = "sentinel-keepttl-rejected";
const PLAIN_REJECTED: &str = "sentinel-set-rejected";

/// A RESP error line parsed into a `Value` — `ServerError` is not nameable in
/// redis 0.27, and the parser is how a real server reply arrives.
fn err_reply(detail: &str) -> RValue {
    let line = format!("-ERR {detail}\r\n");
    redis::parse_redis_value(line.as_bytes())
        .unwrap_or_else(|e| panic!("test fixture produced an invalid error reply: {e}"))
}

fn args_of(cmd: &redis::Cmd) -> Vec<String> {
    cmd.args_iter()
        .map(|arg| match arg {
            redis::Arg::Simple(bytes) => String::from_utf8_lossy(bytes).into_owned(),
            redis::Arg::Cursor => "@cursor".into(),
        })
        .collect()
}

/// Scripted stand-in for a `redis::aio::ConnectionLike`: replies are queued per
/// command name (FIFO) and every request is journaled verbatim, so a test can
/// assert both the round-trip shape and the keyword set the server saw.
struct Scripted {
    queues: HashMap<String, VecDeque<RValue>>,
    journal: Vec<Vec<String>>,
}

impl Scripted {
    fn new() -> Self {
        Self {
            queues: HashMap::new(),
            journal: Vec::new(),
        }
    }

    fn push(&mut self, name: &str, value: RValue) -> &mut Self {
        self.queues
            .entry(name.to_ascii_uppercase())
            .or_default()
            .push_back(value);
        self
    }

    fn push_ok(&mut self, name: &str) -> &mut Self {
        self.push(name, RValue::Okay)
    }

    fn push_err(&mut self, name: &str, text: &str) -> &mut Self {
        self.push(name, err_reply(text))
    }

    fn push_int(&mut self, name: &str, n: i64) -> &mut Self {
        self.push(name, RValue::Int(n))
    }

    fn take_reply(&mut self, name: &str) -> RValue {
        self.queues
            .get_mut(&name.to_ascii_uppercase())
            .and_then(VecDeque::pop_front)
            .unwrap_or_else(|| panic!("no scripted reply left for {name}"))
    }

    /// Commands issued, each as its argument list, in round-trip order.
    fn journal(&self) -> &[Vec<String>] {
        &self.journal
    }
}

impl ConnectionLike for Scripted {
    fn req_packed_command<'a>(&'a mut self, cmd: &'a redis::Cmd) -> RedisFuture<'a, RValue> {
        let args = args_of(cmd);
        let name = args.first().cloned().unwrap_or_default();
        (async move {
            let reply = self.take_reply(&name);
            self.journal.push(args);
            Ok(reply)
        })
        .boxed()
    }

    fn req_packed_commands<'a>(
        &'a mut self,
        pipe: &'a redis::Pipeline,
        _offset: usize,
        _count: usize,
    ) -> RedisFuture<'a, Vec<RValue>> {
        let batch: Vec<Vec<String>> = pipe.cmd_iter().map(args_of).collect();
        (async move {
            let mut replies = Vec::with_capacity(batch.len());
            for args in batch {
                let name = args.first().cloned().unwrap_or_default();
                replies.push(self.take_reply(&name));
                self.journal.push(args);
            }
            Ok(replies)
        })
        .boxed()
    }

    fn get_db(&self) -> i64 {
        0
    }
}

async fn write_with(
    conn: &mut Scripted,
    keep_ttl: Option<bool>,
) -> Result<SetStringOutcome, String> {
    set_string_with_ttl_policy(conn, "app:cache:session:1", b"v2", keep_ttl).await
}

#[test]
fn an_absent_keep_ttl_flag_means_keep_the_ttl() {
    // The contract W3-E depends on: the GUI can stop sending the flag at all.
    assert_eq!(keep_ttl_policy(&serde_json::json!({})), None);
    assert_eq!(
        keep_ttl_policy(&serde_json::json!({ "keepTtl": true })),
        Some(true)
    );
    assert_eq!(
        keep_ttl_policy(&serde_json::json!({ "keep_ttl": false })),
        Some(false)
    );
    // A non-boolean value is not a silent "false": it falls back to the safe
    // default rather than dropping a live expiry.
    assert_eq!(
        keep_ttl_policy(&serde_json::json!({ "keepTtl": "false" })),
        None
    );
    assert_eq!(
        keep_ttl_policy(&serde_json::json!({ "keepTtl": null })),
        None
    );
}

#[tokio::test]
async fn default_write_uses_keepttl_and_never_probes_pttl() {
    let mut conn = Scripted::new();
    conn.push_ok("SET");
    let outcome = write_with(&mut conn, None).await.expect("write ok");
    assert_eq!(
        outcome,
        SetStringOutcome {
            ok: true,
            keep_ttl: true,
            keep_ttl_fallback: false,
        }
    );
    assert_eq!(
        conn.journal(),
        &[vec![
            "SET".to_string(),
            "app:cache:session:1".to_string(),
            "v2".to_string(),
            "KEEPTTL".to_string()
        ]]
    );
}

#[tokio::test]
async fn explicit_false_still_clears_the_expiry() {
    // Regression guard: "default = keep" must not swallow the opt-out.
    let mut conn = Scripted::new();
    conn.push_ok("SET");
    let outcome = write_with(&mut conn, Some(false)).await.expect("write ok");
    assert!(!outcome.keep_ttl);
    assert!(!outcome.keep_ttl_fallback);
    let issued = conn.journal();
    assert_eq!(issued.len(), 1, "no rescue round trips on the opt-out path");
    assert!(
        !issued[0]
            .iter()
            .any(|a| a.eq_ignore_ascii_case("KEEPTTL") || a.eq_ignore_ascii_case("PX")),
        "opt-out must send a bare SET, got {:?}",
        issued[0]
    );
}

#[tokio::test]
async fn rejected_keepttl_falls_back_to_pttl_plus_px() {
    let mut conn = Scripted::new();
    conn.push_err("SET", KEEP_TTL_REJECTED);
    conn.push_ok("SET");
    conn.push_int("PTTL", 4_321);
    let outcome = write_with(&mut conn, None)
        .await
        .expect("fallback write ok");
    assert!(outcome.keep_ttl);
    assert!(
        outcome.keep_ttl_fallback,
        "the rescue path must be reportable"
    );
    let issued = conn.journal();
    assert_eq!(issued.len(), 3, "SET / PTTL / SET, got {issued:?}");
    assert_eq!(issued[1], vec!["PTTL", "app:cache:session:1"]);
    assert_eq!(
        issued[2],
        vec!["SET", "app:cache:session:1", "v2", "PX", "4321"],
        "the retry must re-carry the value and the remaining lifetime"
    );
}

#[tokio::test]
async fn default_and_explicit_true_share_one_code_path() {
    for keep in [None, Some(true)] {
        let mut conn = Scripted::new();
        conn.push_err("SET", KEEP_TTL_REJECTED);
        conn.push_ok("SET");
        conn.push_int("PTTL", 999);
        let outcome = write_with(&mut conn, keep)
            .await
            .unwrap_or_else(|e| panic!("policy {keep:?} must survive rejection: {e}"));
        assert!(outcome.keep_ttl_fallback);
        assert_eq!(conn.journal()[2][3], "PX");
    }
}

#[tokio::test]
async fn non_positive_pttl_never_becomes_an_expiry() {
    // -1 = persistent key, -2 = key vanished between the failed SET and PTTL.
    for remaining in [-1i64, -2, 0] {
        let mut conn = Scripted::new();
        conn.push_err("SET", KEEP_TTL_REJECTED);
        conn.push_ok("SET");
        conn.push_int("PTTL", remaining);
        let outcome = write_with(&mut conn, None).await.expect("write ok");
        assert!(outcome.keep_ttl_fallback);
        let issued = conn.journal();
        assert_eq!(issued.len(), 3);
        assert_eq!(issued[1], vec!["PTTL", "app:cache:session:1"]);
        assert_eq!(
            issued[2],
            vec!["SET", "app:cache:session:1", "v2"],
            "PTTL {remaining} must not be turned into PX"
        );
    }
}

#[tokio::test]
async fn one_remaining_millisecond_is_still_an_expiry() {
    // One millisecond left is still a left-over expiry, so PX must be sent.
    let mut conn = Scripted::new();
    conn.push_err("SET", KEEP_TTL_REJECTED);
    conn.push_ok("SET");
    conn.push_int("PTTL", 1);
    let outcome = write_with(&mut conn, None).await.expect("write ok");
    assert!(outcome.keep_ttl_fallback);
    assert_eq!(
        conn.journal()[2],
        vec!["SET", "app:cache:session:1", "v2", "PX", "1"]
    );
}

#[tokio::test]
async fn when_the_rescue_also_fails_the_rejection_stays_visible() {
    let mut conn = Scripted::new();
    conn.push_err("SET", KEEP_TTL_REJECTED);
    conn.push_int("PTTL", 5_000);
    conn.push_err("SET", PLAIN_REJECTED);
    let err = write_with(&mut conn, None)
        .await
        .expect_err("both attempts failed");
    assert!(err.contains(KEEP_TTL_REJECTED), "got: {err}");
    assert!(err.contains(PLAIN_REJECTED), "got: {err}");
}

#[tokio::test]
async fn an_unavailable_pttl_reports_the_original_rejection() {
    // Old server / ACL: no error invented here, the KEEPTTL rejection is what
    // the user has to see, and no partial write is claimed.
    let mut conn = Scripted::new();
    conn.push_err("SET", KEEP_TTL_REJECTED);
    conn.push_err("PTTL", "sentinel-pttl-unavailable");
    let err = write_with(&mut conn, None)
        .await
        .expect_err("rescue unavailable");
    assert!(err.contains(KEEP_TTL_REJECTED), "got: {err}");
    assert!(
        !err.contains("PX"),
        "the failed rescue must not be reported as done, got: {err}"
    );
    assert_eq!(
        conn.journal().len(),
        2,
        "no blind retry SET after PTTL failed"
    );
}

#[tokio::test]
async fn opt_out_failures_are_returned_verbatim_without_retry() {
    let mut conn = Scripted::new();
    conn.push_err("SET", PLAIN_REJECTED);
    let err = write_with(&mut conn, Some(false))
        .await
        .expect_err("write rejected");
    assert!(err.contains(PLAIN_REJECTED), "got: {err}");
    assert_eq!(conn.journal().len(), 1);
}

#[test]
fn the_outcome_reports_camel_case_keys() {
    // The dispatch layer serialises this struct straight into the IPC payload.
    let value = serde_json::to_value(SetStringOutcome {
        ok: true,
        keep_ttl: true,
        keep_ttl_fallback: true,
    })
    .expect("serialisable");
    assert_eq!(value["ok"], serde_json::json!(true));
    assert_eq!(value["keepTtl"], serde_json::json!(true));
    assert_eq!(value["keepTtlFallback"], serde_json::json!(true));
    assert!(value.get("keep_ttl").is_none());
    assert!(value.get("keep_ttl_fallback").is_none());
}

// ---------------------------------------------------------------------------
// `[tester]` W3-C 第 1 轮补测（阶段 C）—— 只加断言，不改生产代码。
// ---------------------------------------------------------------------------

#[test]
fn test_tester_the_camel_case_spelling_wins_when_both_are_present() {
    // The command schema (commands.rs:199/214) names `keepTtl`; the snake_case
    // form is a convenience for headless callers. If a request carries both,
    // the documented spelling must be the one the write honours — otherwise a
    // Workflow step could clear a live expiry through a field nobody reads.
    assert_eq!(
        keep_ttl_policy(&serde_json::json!({ "keepTtl": true, "keep_ttl": false })),
        Some(true)
    );
    assert_eq!(
        keep_ttl_policy(&serde_json::json!({ "keepTtl": false, "keep_ttl": true })),
        Some(false)
    );
}

/// `[tester]` `redis-codec-write-BUG-004` 复现载体（今日为红，故 `#[ignore]`）。
///
/// 契约 C-4（同 crate `ops/write.rs` 的 `is_keepttl_keyword_rejection`，与
/// `decode` 侧另一套 C-1/C-2/C-3 编号无关）：回退的前提是
/// 「**被服务端拒绝**（旧版本 Redis / 代理）」，
/// `keepTtlFallback` 就是这一事件的可断言位。当前实现的分诊口径是
/// 「SET 报错了」而非「SET 因 KEEPTTL 报错」，于是 `WRONGTYPE` / `READONLY` /
/// 连接类失败也会走二次写入，并把「本服务器不支持 KEEPTTL」这个结论报给 UI（W3-E 已
/// 按此位决定是否提示）。`ops/write.rs` 的 C-3 分支（非 KEEPTTL 拒绝即透传）
/// 明确「传输/权限类失败仍按通用 IPC 错误抛出」。
#[ignore = "redis-codec-write-BUG-004: any SET error is classified as a KEEPTTL rejection"]
#[tokio::test]
async fn test_tester_a_non_keepttl_rejection_must_not_reissue_the_write() {
    let mut conn = Scripted::new();
    conn.push_err("SET", "sentinel-wrongtype");
    conn.push_int("PTTL", 5_000);
    conn.push_ok("SET");

    let result = write_with(&mut conn, None).await;
    assert!(
        result.is_err(),
        "a non-KEEPTTL rejection must be reported, not rescued: {result:?}"
    );
    assert_eq!(
        conn.journal().len(),
        1,
        "only the rejected SET may reach the server, got {:?}",
        conn.journal()
    );
}

/// `redis-codec-write-BUG-004` 绿色孪生体：非 KEEPTTL 的 SET 失败原样抛出、绝不补发第二条写入。
/// 不 push PTTL 回复 —— 回退一旦发生，`take_reply` 会直接 panic，回归无处可藏。
#[tokio::test]
async fn a_non_keyword_set_failure_is_thrown_without_rescue() {
    let mut conn = Scripted::new();
    conn.push_err("SET", "sentinel-wrongtype");

    let err = write_with(&mut conn, None)
        .await
        .expect_err("a failure that never mentioned KEEPTTL must propagate");
    assert!(err.contains("sentinel-wrongtype"), "got: {err}");
    assert_eq!(
        conn.journal().len(),
        1,
        "no PTTL/PX rescue for an unrelated failure, got {:?}",
        conn.journal()
    );
}

/// `redis-codec-write-BUG-004` 分类器的第二种受理形态：关键词没被回显、而是被报成 SET 的未知选项。
#[tokio::test]
async fn an_unknown_option_shape_is_also_a_keyword_refusal() {
    let mut conn = Scripted::new();
    conn.push_err("SET", "sentinel unknown option for 'set'");
    conn.push_int("PTTL", 5_000);
    conn.push_ok("SET");

    let outcome = write_with(&mut conn, None)
        .await
        .expect("the rescue must run for a keyword refusal");
    assert!(outcome.keep_ttl_fallback);
    assert_eq!(
        conn.journal().len(),
        3,
        "SET / PTTL / SET, got {:?}",
        conn.journal()
    );
}
