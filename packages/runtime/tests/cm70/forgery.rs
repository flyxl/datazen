//! A5：伪造 `issuedAt` / `keyVersion` 一律拒绝。
//!
//! 关键不是「拒绝」，而是**拒绝得足够早**：伪造的请求必须死在令牌闸门上，
//! 一次都碰不到端口、也碰不到账本。否则「拒绝」可能只是账本恰好没有这条记录
//! 造成的副作用，安全语义就落空了。
//!
//! 令牌结构：`cm70.<keyVersion>.<hex(payload)>.<hex(MAC)>`，
//! MAC 覆盖 `cm70.<keyVersion>.<hex(payload)>`——前缀与版本号都在签名覆盖范围内，
//! 所以改版本号、改签发时刻、改到期时刻都会让 MAC 对不上。

use crate::gateway_fixtures as fx;
use crate::{err, token_reason, write_once};
use datazen_runtime::gateway::{
    GatewayError, GrantRegistry, KeyVersion, SubmissionTokenGuard, TokenKeyring,
};

/// 把令牌切成 `段.段.段.段`。
fn segments(token: &str) -> Vec<&str> {
    token.split('.').collect()
}

/// 只换第 `index` 段，其余原样拼回。
fn with_segment(token: &str, index: usize, value: &str) -> String {
    let mut parts = segments(token);
    assert_eq!(parts.len(), 4, "令牌必须是四段，实际是 {parts:?}");
    parts[index] = value;
    parts.join(".")
}

/// payload 段是 `|` 分隔字段的 **hex**——先解回文本再动它。
fn from_hex(hex: &str) -> String {
    assert!(hex.len() % 2 == 0, "hex 长度必须是偶数：{hex:?}");
    let bytes: Vec<u8> = (0..hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).expect("hex 解码失败"))
        .collect();
    String::from_utf8(bytes).expect("payload 解码后必须是 UTF-8")
}

fn to_hex(text: &str) -> String {
    text.as_bytes().iter().map(|b| format!("{b:02x}")).collect()
}

/// payload 段解出来的字段：`nonce|operation|ownerEpoch|issuedAt|expiresAt`。
fn payload_fields(token: &str) -> Vec<String> {
    from_hex(&segments(token)[2])
        .split('|')
        .map(|piece| piece.to_owned())
        .collect()
}

/// 改掉 payload 里第 `field` 个字段，原样重编回去。
///
/// **不重算 MAC**——这正是伪造的定义。MAC 覆盖的是 payload 段本身，
/// 所以字段一改签名就对不上，测试要看的正是这一点。
fn with_payload_field(token: &str, field: usize, value: &str) -> String {
    let mut fields = payload_fields(token);
    assert!(
        field < fields.len(),
        "payload 字段越界：{field} ≥ {}",
        fields.len()
    );
    fields[field] = value.to_owned();
    with_segment(token, 2, &to_hex(&fields.join("|")))
}

/// 声称签发时刻被改到签名覆盖之外 ⇒ 拒绝。
#[tokio::test]
async fn a_forged_issued_at_is_refused_before_it_reaches_the_port() {
    let h = fx::token_harness();
    let genuine = fx::issue_token(&h.tokens, 1_000);
    let forged = with_payload_field(&genuine, 3, &(1_000_000_000_u64).to_string());

    let error = err(h
        .harness
        .gateway
        .accept(
            &fx::principal(),
            fx::request_with_key(fx::REVISION, &forged),
        )
        .await);
    assert_eq!(token_reason(&error), "submissionTokenSignatureInvalid");
    assert_eq!(
        h.harness.port.session_view_calls(),
        0,
        "伪造令牌必须在碰端口之前就死"
    );
    assert_eq!(h.harness.port.execute_calls(), 0);
    assert_eq!(h.store.read_calls(), 0, "伪造令牌不得查账本");
}

/// 把签发时刻改到「未来」同样拒绝。
///
/// 反向也要钉：只防「改早」不防「改晚」，攻击者就能用一个自称未来签发、
/// 有效期随之被拉长的令牌重放。
#[tokio::test]
async fn a_forged_issued_at_in_the_future_is_refused() {
    let h = fx::token_harness();
    let genuine = fx::issue_token(&h.tokens, 1_000);
    let forged = with_payload_field(&genuine, 3, &(1_000_000_000_u64).to_string());
    assert_ne!(forged, genuine);

    let error = err(h
        .harness
        .gateway
        .accept(
            &fx::principal(),
            fx::request_with_key(fx::REVISION, &forged),
        )
        .await);
    assert_eq!(token_reason(&error), "submissionTokenSignatureInvalid");
    assert_eq!(h.harness.port.execute_calls(), 0);
}

/// 把到期时刻拉长 ⇒ 拒绝（否则过期重放就能靠改一个字段复活）。
#[tokio::test]
async fn a_forged_expiry_that_outlives_the_real_one_is_refused() {
    let h = fx::token_harness();
    let genuine = fx::issue_token(&h.tokens, 1_000);
    let forged = with_payload_field(&genuine, 4, &u64::MAX.to_string());

    let error = err(h
        .harness
        .gateway
        .accept(
            &fx::principal(),
            fx::request_with_key(fx::REVISION, &forged),
        )
        .await);
    assert_eq!(token_reason(&error), "submissionTokenSignatureInvalid");
    assert_eq!(h.harness.port.execute_calls(), 0);
}

/// 把绑定的 owner epoch 改成别的会话 ⇒ 拒绝。
#[tokio::test]
async fn a_forged_owner_epoch_is_refused() {
    let h = fx::token_harness();
    let genuine = fx::issue_token(&h.tokens, 1_000);
    let forged = with_payload_field(&genuine, 2, &u64::MAX.to_string());

    let error = err(h
        .harness
        .gateway
        .accept(
            &fx::principal(),
            fx::request_with_key(fx::REVISION, &forged),
        )
        .await);
    assert_eq!(token_reason(&error), "submissionTokenSignatureInvalid");
    assert_eq!(h.harness.port.execute_calls(), 0);
}

/// 把操作类型换掉 ⇒ 拒绝。
#[tokio::test]
async fn a_forged_operation_is_refused() {
    let h = fx::token_harness();
    let genuine = fx::issue_token(&h.tokens, 1_000);
    let forged = with_payload_field(&genuine, 1, "99");

    let error = err(h
        .harness
        .gateway
        .accept(
            &fx::principal(),
            fx::request_with_key(fx::REVISION, &forged),
        )
        .await);
    assert_eq!(token_reason(&error), "submissionTokenSignatureInvalid");
    assert_eq!(h.harness.port.execute_calls(), 0);
}

/// 换 `keyVersion` ⇒ 拒绝，且**不许降级**。
///
/// 版本号在签名覆盖范围内，所以「改版本号换一个已知密钥」骗不过 MAC；
/// 「改版本号换一个未知密钥」死在 `unknownKeyVersion` 上。两种都要拒。
#[tokio::test]
async fn relabelling_the_key_version_is_refused_and_never_downgraded() {
    let h = fx::token_harness();
    let genuine = fx::issue_token(&h.tokens, 1_000);
    let forged = with_segment(&genuine, 1, "9");

    let error = err(h
        .harness
        .gateway
        .accept(
            &fx::principal(),
            fx::request_with_key(fx::REVISION, &forged),
        )
        .await);
    assert_eq!(token_reason(&error), "submissionTokenKeyVersionUnknown");
    assert_eq!(h.harness.port.execute_calls(), 0);
    assert_eq!(h.store.read_calls(), 0);
}

/// 把版本号改成一个**存在**的密钥（轮转后的旧密钥）也一样拒。
///
/// 这是更危险的一种：版本号合法、密钥存在，只因为它不在当前密钥手里。
#[tokio::test]
async fn relabelling_onto_a_real_older_key_version_is_still_refused() {
    // 密钥环里**同时**留着 1 与 2，当前签发用 2。
    let keyring = TokenKeyring::rotate(
        vec![
            (KeyVersion::new(1), fx::SIGNING_KEY.to_vec()),
            (KeyVersion::new(2), b"cm70-rotated-key".to_vec()),
        ],
        KeyVersion::new(2),
    )
    .shared();
    let tokens = SubmissionTokenGuard::shared(keyring, GrantRegistry::shared());
    let h = fx::token_harness_with(tokens);
    let genuine = fx::issue_token(&h.tokens, 1_000);
    assert_eq!(segments(&genuine)[1], "2", "夹具前提：令牌由版本 2 签发");

    let forged = with_segment(&genuine, 1, "1");
    let error = err(h
        .harness
        .gateway
        .accept(
            &fx::principal(),
            fx::request_with_key(fx::REVISION, &forged),
        )
        .await);
    assert_eq!(token_reason(&error), "submissionTokenSignatureInvalid");
    assert_eq!(h.harness.port.execute_calls(), 0);
}

/// 连前缀一起换 ⇒ 拒绝。
#[tokio::test]
async fn a_relabelled_prefix_is_refused() {
    let h = fx::token_harness();
    let genuine = fx::issue_token(&h.tokens, 1_000);
    let forged = format!("cm69{}", &genuine[genuine.find('.').unwrap()..]);

    let error = err(h
        .harness
        .gateway
        .accept(
            &fx::principal(),
            fx::request_with_key(fx::REVISION, &forged),
        )
        .await);
    assert_eq!(token_reason(&error), "submissionTokenMalformed");
    assert_eq!(h.harness.port.execute_calls(), 0);
}

/// MAC 段被改 ⇒ 拒绝。
#[tokio::test]
async fn a_swapped_mac_is_refused() {
    let h = fx::token_harness();
    let genuine = fx::issue_token(&h.tokens, 1_000);
    let forged = with_segment(&genuine, 3, "00");

    let error = err(h
        .harness
        .gateway
        .accept(
            &fx::principal(),
            fx::request_with_key(fx::REVISION, &forged),
        )
        .await);
    assert_eq!(token_reason(&error), "submissionTokenSignatureInvalid");
    assert_eq!(h.harness.port.execute_calls(), 0);
}

/// 完全不是令牌的字符串 ⇒ 拒绝。
#[tokio::test]
async fn a_plain_string_is_not_accepted_as_a_token() {
    let h = fx::token_harness();
    for candidate in [
        "idem-contract",
        "cm70",
        "cm70.1",
        "cm70.1.zzzz.00",
        "a.b.c.d",
    ] {
        let error = err(h
            .harness
            .gateway
            .accept(
                &fx::principal(),
                fx::request_with_key(fx::REVISION, candidate),
            )
            .await);
        assert!(
            matches!(error, GatewayError::SubmissionTokenRejected { .. }),
            "候选 {candidate:?} 本该被令牌层拒绝，实际是 {error:?}"
        );
    }
    assert_eq!(h.harness.port.execute_calls(), 0, "一个都不许下发");
    assert_eq!(h.store.read_calls(), 0, "一个都不许查账本");

    // 空幂等键走的是请求校验，比令牌层更早——两者都不许放行。
    let empty = err(h
        .harness
        .gateway
        .accept(&fx::principal(), fx::request_with_key(fx::REVISION, ""))
        .await);
    assert!(
        matches!(empty, GatewayError::InvalidRequest { .. }),
        "空键应在请求校验就被拒，实际是 {empty:?}"
    );
    assert_eq!(h.harness.port.execute_calls(), 0);
}

/// 真令牌仍然通行——确认上面那些不是因为「令牌层坏了」而拒。
#[tokio::test]
async fn the_genuine_token_still_wins_against_all_those_forgeries() {
    let h = fx::token_harness();
    let genuine = fx::issue_token(&h.tokens, 1_000);
    for forged in [
        with_payload_field(&genuine, 3, "1"),
        with_payload_field(&genuine, 4, &u64::MAX.to_string()),
        with_segment(&genuine, 1, "9"),
        with_segment(&genuine, 3, "00"),
    ] {
        let error = err(h
            .harness
            .gateway
            .accept(
                &fx::principal(),
                fx::request_with_key(fx::REVISION, &forged),
            )
            .await);
        assert!(!token_reason(&error).is_empty());
    }
    assert_eq!(h.harness.port.execute_calls(), 0);

    // 真令牌必须能一路走到驱动——否则上面那些拒绝可能只是「全都不通」。
    write_once(&h, fx::request_with_key(fx::REVISION, &genuine)).await;
    assert_eq!(h.harness.port.execute_calls(), 1);
}

/// 拒绝理由逐条不同，且都不含令牌原文。
///
/// 机器可读理由是给调用方分流用的；顺带钉住「理由里不回显令牌」，
/// 免得令牌随错误流进日志。
#[tokio::test]
async fn every_forgery_reason_is_machine_readable_and_leak_free() {
    let h = fx::token_harness();
    let genuine = fx::issue_token(&h.tokens, 1_000);
    let forged_expiry = with_payload_field(&genuine, 4, &u64::MAX.to_string());

    let expired = err(h
        .harness
        .gateway
        .accept(
            &fx::principal(),
            fx::request_with_key(fx::REVISION, &forged_expiry),
        )
        .await);
    let unknown_version = err(h
        .harness
        .gateway
        .accept(
            &fx::principal(),
            fx::request_with_key(fx::REVISION, &with_segment(&genuine, 1, "9")),
        )
        .await);
    let malformed = err(h
        .harness
        .gateway
        .accept(&fx::principal(), fx::request_with_key(fx::REVISION, "cm70"))
        .await);

    let reasons = [
        token_reason(&expired),
        token_reason(&unknown_version),
        token_reason(&malformed),
    ];
    assert_eq!(
        reasons,
        [
            "submissionTokenSignatureInvalid",
            "submissionTokenKeyVersionUnknown",
            "submissionTokenMalformed",
        ]
    );

    // 可持久化形态里同样不得出现令牌原文。
    for error in [&expired, &unknown_version, &malformed] {
        let json = error.to_persistable_json().to_string();
        assert!(!json.contains(&genuine), "拒绝理由里混进了令牌原文：{json}");
        assert!(json.contains("submissionTokenRejected"));
    }
}

/// 夹具自证：签发出来的确实是四段带签名的令牌。
#[test]
fn the_fixture_token_is_well_formed() {
    let keyring = TokenKeyring::single(fx::SIGNING_KEY.to_vec()).shared();
    let tokens = SubmissionTokenGuard::shared(keyring, GrantRegistry::shared());
    let token = fx::issue_token(&tokens, 1_000);
    assert_eq!(segments(&token).len(), 4);
    assert_eq!(segments(&token)[0], "cm70");
    assert_eq!(segments(&token)[1], "1");
    assert_eq!(payload_fields(&token).len(), 5, "载荷必须是五个字段");
    assert_eq!(
        payload_fields(&token)[1],
        "execute",
        "夹具发的是会话内执行令牌"
    );
    assert_eq!(segments(&token)[3].len(), 64, "HMAC-SHA256 是 32 字节");
}
