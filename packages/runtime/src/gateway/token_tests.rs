//! CM-70 提交令牌的单元测试。
//!
//! 三条主线：
//!
//! 1. **伪造拒绝**（CM-70「伪造 issuedAt/keyVersion」）——令牌里的每一个字节都在
//!    MAC 覆盖范围内，改一个 nibble 就断；版本先查密钥再验签，未知版本**不降级**。
//! 2. **过期拒绝**——只认服务端时钟。
//! 3. **授予已删 → 仍拒绝**——这是协调者点名要的否定用例：令牌签名完好、尚未过期，
//!    但它的授予已经被删掉，此时重放必须被墓碑挡下，否则会一路走到账本的 `Miss`
//!    然后被真的执行一遍。
//!
//! 所有时间都是常量纳秒，没有真实等待。

use std::sync::Arc;

use super::*;
use crate::connection::{Counter, ExecutionId};
use crate::gateway::idempotency::IdempotencyScope;
use crate::gateway::retention::GrantRegistry;
use crate::gateway::token::token_digest;

const HOUR: u64 = 60 * 60 * 1_000_000_000;
const TTL: u64 = 6 * HOUR;
const SECRET: &[u8] = b"cm70-test-signing-key";

fn keyring() -> Arc<TokenKeyring> {
    TokenKeyring::single(SECRET.to_vec()).shared()
}

fn guard(keyring: Arc<TokenKeyring>) -> Arc<SubmissionTokenGuard> {
    SubmissionTokenGuard::shared(keyring, GrantRegistry::shared())
}

/// 测试侧**独立实现**的十六进制编解码。不复用 `token` 模块里那一份——
/// 用被测代码来解析被测数据，测试就在替实现背书了。
fn hex_decode(text: &str) -> Option<Vec<u8>> {
    if text.len() % 2 != 0 {
        return None;
    }
    text.as_bytes()
        .chunks_exact(2)
        .map(|pair| {
            let hi = char::from(pair[0]).to_digit(16)?;
            let lo = char::from(pair[1]).to_digit(16)?;
            Some(((hi << 4) | lo) as u8)
        })
        .collect()
}

fn hex_encode(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(char::from_digit(u32::from(byte >> 4), 16).unwrap_or('0'));
        out.push(char::from_digit(u32::from(byte & 0x0f), 16).unwrap_or('0'));
    }
    out
}

/// 取出令牌第 `index` 段（`0`=前缀 `1`=版本 `2`=载荷 `3`=MAC）。
fn segment(token: &str, index: usize) -> String {
    token.split('.').nth(index).unwrap_or_default().to_owned()
}

fn set_segment(token: &str, index: usize, value: &str) -> String {
    let mut parts: Vec<String> = token.split('.').map(str::to_owned).collect();
    parts[index] = value.to_owned();
    parts.join(".")
}

fn scope(key: &str) -> IdempotencyScope {
    IdempotencyScope::new(
        crate::connection::DbSessionId::new("dbs_cm70_0001"),
        Counter::new(7),
        key,
    )
}

fn issue_at(guard: &SubmissionTokenGuard, issued_at: u64) -> String {
    guard
        .issue(
            SubmissionOperation::ExecuteInSession,
            Some(Counter::new(7)),
            issued_at,
            TTL,
        )
        .unwrap_or_else(|err| panic!("签发不该失败：{err:?}"))
}

// ── 签发与验签的正路 ──────────────────────────────────────────────────────

#[test]
fn a_fresh_token_verifies_and_carries_server_registered_times() {
    let guard = guard(keyring());
    let token = issue_at(&guard, 1_000);

    let admission = guard
        .admit(&token, 1_000, Some(Counter::new(7)))
        .unwrap_or_else(|rejection| panic!("刚签的令牌不该被拒：{rejection:?}"));
    let payload = admission.payload();

    assert_eq!(payload.issued_at_nanos(), 1_000);
    assert_eq!(payload.expires_at_nanos(), 1_000 + TTL);
    assert_eq!(payload.operation(), SubmissionOperation::ExecuteInSession);
    assert_eq!(payload.owner_runtime_epoch(), Some(Counter::new(7)));
    assert_eq!(admission.digest(), token_digest(&token));
}

#[test]
fn every_issue_gets_its_own_nonce_even_with_identical_inputs() {
    let guard = guard(keyring());
    let first = issue_at(&guard, 1_000);
    let second = issue_at(&guard, 1_000);
    assert_ne!(first, second, "同一进程内两次签发不得产出同一条令牌");
}

#[test]
fn the_default_ttl_is_twenty_four_hours() {
    assert_eq!(DEFAULT_TTL_NANOS, 24 * HOUR);
}

#[test]
fn issuing_without_the_owner_epoch_the_operation_requires_is_refused() {
    let keyring = keyring();
    assert_eq!(
        keyring.issue(SubmissionOperation::ExecuteInSession, None, 0, TTL,),
        Err(TokenIssueError::OwnerEpochRequired(
            SubmissionOperation::ExecuteInSession
        )),
        "执行类令牌不绑 owner 代次就是废令牌，不该签出来"
    );
    assert_eq!(
        keyring.issue(SubmissionOperation::SetContext, None, 0, TTL),
        Err(TokenIssueError::OwnerEpochRequired(
            SubmissionOperation::SetContext
        ))
    );
    // `open` 时 owner 尚不存在，不要求代次。
    assert!(keyring
        .issue(SubmissionOperation::OpenSession, None, 0, TTL)
        .is_ok());
}

#[test]
fn issuing_fails_loudly_when_the_current_version_has_no_secret() {
    let keyring = TokenKeyring::rotate(Vec::new(), KeyVersion::new(4));
    assert_eq!(
        keyring.issue(SubmissionOperation::OpenSession, None, 0, TTL),
        Err(TokenIssueError::UnknownCurrentKeyVersion)
    );
}

#[test]
fn an_expiry_that_would_overflow_the_nanosecond_space_is_refused() {
    let guard = guard(keyring());
    assert_eq!(
        guard.issue(
            SubmissionOperation::ExecuteInSession,
            Some(Counter::new(7)),
            u64::MAX,
            1,
        ),
        Err(TokenIssueError::ExpiryOverflow)
    );
}

// ── 伪造拒绝 ──────────────────────────────────────────────────────────────

/// **伪造 issuedAt**：改载荷里的时间，其余原样。MAC 覆盖整个载荷，所以必断。
#[test]
fn a_forged_issued_at_is_rejected_even_though_the_rest_of_the_token_is_intact() {
    let guard = guard(keyring());
    let token = issue_at(&guard, 1_000);

    let payload_text =
        String::from_utf8(hex_decode(&segment(&token, 2)).unwrap_or_default()).unwrap_or_default();
    let fields: Vec<&str> = payload_text.split('|').collect();
    assert_eq!(fields.len(), 5, "载荷结构变了，先查实现再改测试");
    assert_eq!(fields[3], "1000", "签发时刻确实在载荷里");

    // 换一个**合法**的时间戳：不是乱码，是攻击者真正会写的东西。
    let forged_text = format!(
        "{}|{}|{}|{}|{}",
        fields[0], fields[1], fields[2], 0, fields[4]
    );
    let forged = set_segment(&token, 2, &hex_encode(forged_text.as_bytes()));

    assert_eq!(
        guard.admit(&forged, 1_000, Some(Counter::new(7))),
        Err(TokenRejection::SignatureMismatch),
        "改 issuedAt 就该断——MAC 覆盖载荷"
    );
}

#[test]
fn a_forged_expiry_that_outlives_the_real_one_is_rejected() {
    let guard = guard(keyring());
    let token = issue_at(&guard, 1_000);
    let payload_text =
        String::from_utf8(hex_decode(&segment(&token, 2)).unwrap_or_default()).unwrap_or_default();
    let fields: Vec<&str> = payload_text.split('|').collect();
    let forged_text = format!(
        "{}|{}|{}|{}|{}",
        fields[0],
        fields[1],
        fields[2],
        fields[3],
        u64::MAX.to_string()
    );
    let forged = set_segment(&token, 2, &hex_encode(forged_text.as_bytes()));
    assert_eq!(
        guard.admit(&forged, 10 * TTL, Some(Counter::new(7))),
        Err(TokenRejection::SignatureMismatch)
    );
}

/// **伪造 keyVersion**：把版本号改成一个密钥环里没有的版本。
/// 判据必须是 `UnknownKeyVersion` 而不是 `SignatureMismatch`——前者证明版本先查、
/// 没有把未知版本降级塞进验签路径。
#[test]
fn an_unknown_key_version_is_rejected_and_never_downgraded() {
    let guard = guard(keyring());
    let token = issue_at(&guard, 1_000);
    let forged = set_segment(&token, 1, "7");

    assert_eq!(
        guard.admit(&forged, 1_000, Some(Counter::new(7))),
        Err(TokenRejection::UnknownKeyVersion),
        "未知版本必须单独判掉：不能拿已知版本的密钥去试"
    );
}

/// 轮换期间 v1/v2 两把密钥都在手上。把 v1 的令牌改标成 v2，
/// 只有「拿 v1 的密钥回退试一遍」的实现才会放行。
#[test]
fn a_token_relabelled_with_a_known_version_is_still_rejected() {
    let keyring = TokenKeyring::rotate(
        vec![
            (KeyVersion::new(1), SECRET.to_vec()),
            (KeyVersion::new(2), b"second-generation-key".to_vec()),
        ],
        KeyVersion::new(1),
    )
    .shared();
    let guard = SubmissionTokenGuard::shared(keyring, GrantRegistry::shared());
    let token = issue_at(&guard, 1_000);
    let relabelled = set_segment(&token, 1, "2");

    assert_eq!(
        guard.admit(&relabelled, 1_000, Some(Counter::new(7))),
        Err(TokenRejection::SignatureMismatch),
        "版本 2 的密钥是真的，但 MAC 是版本 1 签的——不许回退去试 v1"
    );
}

#[test]
fn a_token_signed_under_the_previous_key_still_verifies_during_rotation() {
    let old = TokenKeyring::single(SECRET.to_vec()).shared();
    let token = old
        .issue(
            SubmissionOperation::ExecuteInSession,
            Some(Counter::new(7)),
            1_000,
            TTL,
        )
        .unwrap_or_else(|err| panic!("签发不该失败：{err:?}"));

    let rotated = TokenKeyring::rotate(
        vec![
            (KeyVersion::new(1), SECRET.to_vec()),
            (KeyVersion::new(2), b"second-generation-key".to_vec()),
        ],
        KeyVersion::new(2),
    );
    assert_eq!(
        rotated.current_version(),
        KeyVersion::new(2),
        "轮换后只用新版本签发"
    );
    assert!(
        rotated.verify(&token, 1_000, Some(Counter::new(7))).is_ok(),
        "轮换窗口内旧令牌仍须能验签，否则在途请求会全炸"
    );
}

#[test]
fn a_token_signed_by_a_different_secret_is_rejected() {
    let mine = guard(keyring());
    let theirs = guard(TokenKeyring::single(b"a-completely-different-key").shared());
    let token = issue_at(&theirs, 1_000);

    assert_eq!(
        mine.admit(&token, 1_000, Some(Counter::new(7))),
        Err(TokenRejection::SignatureMismatch)
    );
    assert!(
        theirs.admit(&token, 1_000, Some(Counter::new(7))).is_ok(),
        "同一把密钥在两端都能过——否则上面的断言可能只是密钥环配错了"
    );
}

#[test]
fn every_malformed_shape_is_rejected_before_any_signature_work() {
    let guard = guard(keyring());
    let token = issue_at(&guard, 1_000);
    let payload = segment(&token, 2);
    let mac = segment(&token, 3);
    let candidates = [
        String::new(),
        "cm70".to_owned(),
        "cm70.1".to_owned(),
        format!("cm70.1.{payload}"),
        format!("other.1.{payload}.{mac}"),
        format!("cm70.x.{payload}.{mac}"),
        format!("cm70.1.{payload}.{mac}.extra"),
        format!("cm70.1.{}..", &payload[..payload.len() - 1]),
    ];
    for candidate in candidates {
        assert_eq!(
            guard.admit(&candidate, 1_000, Some(Counter::new(7))),
            Err(TokenRejection::Malformed),
            "畸形令牌没有被拒：{candidate}"
        );
    }

    // `zzzz` 是**合法**十六进制，所以它过了解析、死在 MAC 上。
    // 理由不同，但结果同样是拒——这里把差别也钉住，免得哪天解析器放宽了结构。
    assert_eq!(
        guard.admit(&format!("cm70.1.zzzz.{mac}"), 1_000, Some(Counter::new(7))),
        Err(TokenRejection::SignatureMismatch)
    );
}

#[test]
fn every_rejection_carries_a_distinct_machine_readable_reason() {
    let reasons = [
        TokenRejection::Malformed,
        TokenRejection::UnknownKeyVersion,
        TokenRejection::SignatureMismatch,
        TokenRejection::Expired,
        TokenRejection::OwnerEpochMismatch,
        TokenRejection::Tombstoned,
    ];
    for rejection in reasons {
        let reason = rejection.reason();
        assert!(!reason.is_empty(), "{rejection:?} 没有机器可读理由");
        assert!(reason.starts_with("submissionToken"), "{reason} 命名不一致");
    }
    let mut unique: Vec<&str> = reasons.iter().map(|r| r.reason()).collect();
    unique.sort_unstable();
    unique.dedup();
    assert_eq!(unique.len(), reasons.len(), "两种拒绝共用了一个理由字符串");
}

// ── 过期 ──────────────────────────────────────────────────────────────────

#[test]
fn a_token_stops_being_usable_exactly_at_its_expiry() {
    let guard = guard(keyring());
    let token = issue_at(&guard, 1_000);
    let expires_at = 1_000 + TTL;

    assert!(guard
        .admit(&token, expires_at - 1, Some(Counter::new(7)))
        .is_ok());
    assert_eq!(
        guard.admit(&token, expires_at, Some(Counter::new(7))),
        Err(TokenRejection::Expired),
        "到期那一刻就作废，不给宽限"
    );
    assert_eq!(
        guard.admit(&token, expires_at + HOUR, Some(Counter::new(7))),
        Err(TokenRejection::Expired)
    );
}

#[test]
fn a_tombstone_does_not_hide_an_expired_token_reason() {
    // 墓碑先查，但墓碑剪掉之后过期判定接手——两条拒绝路径都得通。
    let guard = guard(keyring());
    let token = issue_at(&guard, 1_000);
    let admission = guard
        .admit(&token, 1_000, Some(Counter::new(7)))
        .unwrap_or_else(|rejection| panic!("刚签的令牌不该被拒：{rejection:?}"));
    guard.register(
        &admission,
        &ExecutionId::new("e1"),
        &scope("cm70.a3.expired"),
    );
    let retired_at = 1_000 + TTL + RETENTION_AFTER_EXPIRY_NANOS;
    let report = guard.sweep(retired_at);
    assert_eq!(report.deleted.len(), 1);
    assert_eq!(report.tombstones_pruned, 0, "这趟刚立的墓碑不被自己剪掉");

    // 墓碑确实挡过一次重放。
    assert_eq!(
        guard.admit(&token, retired_at, Some(Counter::new(7))),
        Err(TokenRejection::Tombstoned)
    );

    // 墓碑剪掉之后由过期判定接手，不会重新开口子。
    let report = guard.sweep(retired_at);
    assert_eq!(report.tombstones_pruned, 1);
    assert_eq!(
        guard.admit(&token, retired_at, Some(Counter::new(7))),
        Err(TokenRejection::Expired),
        "墓碑剪掉之后由过期判定接手，不会重新开口子"
    );
}

// ── owner 代次（A7） ──────────────────────────────────────────────────────

#[test]
fn an_owner_restart_makes_every_token_from_the_previous_epoch_stale() {
    let guard = guard(keyring());
    let token = issue_at(&guard, 1_000);
    assert!(guard.admit(&token, 1_000, Some(Counter::new(7))).is_ok());

    // owner 重启：代次从 7 变成 8，令牌本身完好、也还没过期。
    assert_eq!(
        guard.admit(&token, 1_000, Some(Counter::new(8))),
        Err(TokenRejection::OwnerEpochMismatch)
    );
    assert_eq!(
        guard.admit(&token, 1_000 + TTL - 1, Some(Counter::new(8))),
        Err(TokenRejection::OwnerEpochMismatch),
        "旧令牌不能靠「还没过期」熬过 owner 重启"
    );
}

#[test]
fn a_token_bound_to_no_epoch_cannot_be_presented_to_an_owned_session() {
    let guard = guard(keyring());
    let token = guard
        .issue(SubmissionOperation::OpenSession, None, 1_000, TTL)
        .unwrap_or_else(|err| panic!("签发不该失败：{err:?}"));

    assert!(guard.admit(&token, 1_000, None).is_ok());
    assert_eq!(
        guard.admit(&token, 1_000, Some(Counter::new(7))),
        Err(TokenRejection::OwnerEpochMismatch),
        "没绑代次的令牌不能送进一个有 owner 的会话"
    );
}

// ── 授予已删仍拒绝（A4 的否定用例） ────────────────────────────────────────

/// **协调者点名的否定用例。**
///
/// 状态：令牌的 MAC 完好、**尚未过期**，但它的授予已经被删掉。
/// 若闸门只看签名和过期，这条重放会一路走到 CM-54 账本；账本里那条记录
/// 也已经不在了，于是 `Miss` → 真的执行一遍——CM-70 明令禁止。
/// 所以第 0 步必须先查墓碑。
#[test]
fn a_token_whose_grant_was_already_deleted_is_rejected_before_expiry() {
    let guard = guard(keyring());
    let token = issue_at(&guard, 1_000);
    let expires_at = 1_000 + TTL;
    let digest = token_digest(&token);

    let admission = guard
        .admit(&token, 1_000, Some(Counter::new(7)))
        .unwrap_or_else(|rejection| panic!("刚签的令牌不该被拒：{rejection:?}"));
    guard.register(&admission, &ExecutionId::new("e1"), &scope("cm70.a4.gone"));
    assert!(guard.registry().grant(&digest).is_some());

    // 「有人把删除提前调用了」：授予没了，墓碑立起来。
    let now = 1_000 + HOUR;
    assert!(now < expires_at, "必须仍在有效期内，否则这条测的其实是过期");
    assert!(guard.registry().force_retire(&digest, now));
    assert!(guard.registry().grant(&digest).is_none());

    // 重放：令牌自己完全没过期，签名完好——仍必须被拒。
    assert_eq!(
        guard.admit(&token, now, Some(Counter::new(7))),
        Err(TokenRejection::Tombstoned),
        "记录被提前删掉之后，重放必须死在闸门第 0 步，不能走到账本的 Miss"
    );
    // 授权登记的同一条作用域此刻已经查不到授予了——这正是「真的执行」的入口。
    assert!(guard.registry().grant(&digest).is_none());
}

#[test]
fn registering_the_same_digest_again_keeps_the_tombstone_irrelevant() {
    // 重登记清掉墓碑是防御性的：闸门第 0 步先查墓碑，能走到 register 的令牌
    // 必然不在墓碑表里。这里只固定「重登记不会让登记计数错乱」。
    let guard = guard(keyring());
    let token = issue_at(&guard, 1_000);
    let admission = guard
        .admit(&token, 1_000, Some(Counter::new(7)))
        .unwrap_or_else(|rejection| panic!("刚签的令牌不该被拒：{rejection:?}"));
    for key in ["k1", "k2"] {
        guard.register(&admission, &ExecutionId::new("e1"), &scope(key));
    }
    assert_eq!(guard.registry().len(), 1, "同一摘要只占一个位置");
}
