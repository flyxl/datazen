use super::*;
use crate::commands::driver_command::{execute_driver_command_impl, ExecuteDriverCommandRequest};
use crate::testing::{app_state::TestAppState, mock_driver::MockDriverOptions};

async fn run(
    test: &TestAppState,
    session: &str,
    command: &str,
    input: Value,
) -> Result<CommandResult, CommandError> {
    execute_driver_command_impl(
        &test.state,
        ExecuteDriverCommandRequest {
            db_session_id: Some(session.into()),
            driver_type: None,
            command: command.into(),
            input,
            database: None,
            schema: None,
        },
    )
    .await
}

#[tokio::test]
async fn reads_deduplicate_exact_identities_and_cache_distinct_schemas() {
    let test = TestAppState::with_options(MockDriverOptions {
        has_schema_level: true,
        ..Default::default()
    })
    .await;
    let (_, session) = test.save_and_connect("metadata-identities").await;
    let input = json!({ "relations": [
        { "database": "app", "schema": "public", "name": "users" },
        { "database": "app", "schema": "archive", "name": "users" },
        { "database": "app", "schema": "public", "name": "users" }
    ] });
    let result = run(&test, &session, "read_relation_columns", input.clone())
        .await
        .unwrap();
    let decoded: ReadColumnsOutput = serde_json::from_value(result.data).unwrap();
    assert_eq!(decoded.results.len(), 2);
    assert_eq!(test.mock.get_columns_calls(), 2);
    run(&test, &session, "read_relation_columns", input)
        .await
        .unwrap();
    assert_eq!(test.mock.get_columns_calls(), 2);
    assert!(test.mock.use_database_calls().is_empty());
}

#[tokio::test]
async fn invalid_target_fails_preflight_before_any_relation_read() {
    let test = TestAppState::with_options(MockDriverOptions {
        has_schema_level: true,
        ..Default::default()
    })
    .await;
    let (_, session) = test.save_and_connect("metadata-preflight").await;
    let error = run(
        &test,
        &session,
        "read_relation_columns",
        json!({ "relations": [
        { "database": "app", "schema": "public", "name": "users" },
        { "database": "app", "schema": null, "name": "orders" }
    ] }),
    )
    .await
    .unwrap_err();
    assert!(matches!(
        error,
        CommandError::Driver(_) | CommandError::Validation(_)
    ));
    assert_eq!(test.mock.get_columns_calls(), 0);
}

#[tokio::test]
async fn catalog_includes_empty_schemas_without_fake_relations() {
    let test = TestAppState::with_options(MockDriverOptions {
        has_schema_level: true,
        tables: vec![
            datazen_driver_api::TableInfo {
                name: "".into(),
                schema: Some("empty".into()),
                table_type: datazen_driver_api::TableType::Table,
                row_count: None,
            },
            datazen_driver_api::TableInfo {
                name: "users".into(),
                schema: Some("public".into()),
                table_type: datazen_driver_api::TableType::Table,
                row_count: Some(2),
            },
        ],
        ..Default::default()
    })
    .await;
    let (_, session) = test.save_and_connect("metadata-catalog").await;
    let result = run(
        &test,
        &session,
        "list_catalog",
        json!({ "database": "app", "schema": null }),
    )
    .await
    .unwrap();
    let catalog: ListCatalogOutput = serde_json::from_value(result.data).unwrap();
    assert_eq!(catalog.schemas, ["empty", "public"]);
    assert_eq!(catalog.relations.len(), 1);
    assert_eq!(catalog.relations[0].relation.name, "users");
}

#[tokio::test]
async fn full_schema_populates_columns_tier_for_the_same_identity() {
    let test = TestAppState::with_tables().await;
    let (_, session) = test.save_and_connect("metadata-full").await;
    let relation = json!({ "database": "app", "schema": null, "name": "users" });
    let result = run(
        &test,
        &session,
        "read_relation_schema",
        json!({ "relation": relation }),
    )
    .await
    .unwrap();
    assert_eq!(result.data["value"]["ref"]["database"], "app");
    run(
        &test,
        &session,
        "read_relation_columns",
        json!({ "relations": [relation] }),
    )
    .await
    .unwrap();
    assert_eq!(test.mock.get_columns_calls(), 0);
}

#[tokio::test]
async fn metadata_envelope_target_is_rejected_instead_of_ignored() {
    let test = TestAppState::with_tables().await;
    let (_, session) = test.save_and_connect("metadata-envelope").await;
    let error = execute_driver_command_impl(
        &test.state,
        ExecuteDriverCommandRequest {
            db_session_id: Some(session),
            driver_type: None,
            command: "list_catalog".into(),
            input: json!({ "database": "app", "schema": null }),
            database: Some("other".into()),
            schema: None,
        },
    )
    .await
    .unwrap_err();
    assert!(matches!(error, CommandError::Validation(_)));
}

#[tokio::test]
async fn a_batch_preserves_successes_and_redacts_per_relation_failures() {
    let test = TestAppState::with_options(MockDriverOptions {
        column_errors_by_table: std::collections::HashMap::from([(
            "denied".into(),
            "postgres://root:hunter2@localhost/app".into(),
        )]),
        ..Default::default()
    })
    .await;
    let (_, session) = test.save_and_connect("metadata-partial").await;
    let result = run(
        &test,
        &session,
        "read_relation_columns",
        json!({ "relations": [
        { "database": "app", "schema": null, "name": "users" },
        { "database": "app", "schema": null, "name": "denied" }
    ] }),
    )
    .await
    .unwrap();
    assert_eq!(result.data["results"][0]["status"], "ok");
    assert_eq!(result.data["results"][1]["status"], "error");
    assert_eq!(result.data["results"][1]["ref"]["name"], "denied");
    assert_eq!(result.data["results"][1]["error"]["code"], "read-failed");
    assert!(!result.data.to_string().contains("hunter2"));
}

#[tokio::test]
async fn relation_refresh_invalidates_only_the_exact_schema_identity() {
    let test = TestAppState::with_options(MockDriverOptions {
        has_schema_level: true,
        ..Default::default()
    })
    .await;
    let (_, session) = test.save_and_connect("metadata-refresh").await;
    let input = json!({ "relations": [
        { "database": "app", "schema": "public", "name": "users" },
        { "database": "app", "schema": "archive", "name": "users" }
    ] });
    run(&test, &session, "read_relation_columns", input.clone())
        .await
        .unwrap();
    let before = test.state.schema_cache.generation();
    let result = run(
        &test,
        &session,
        "refresh_schema_metadata",
        json!({ "scope": {
        "kind": "relation", "relation": { "database": "app", "schema": "public", "name": "users" }
    } }),
    )
    .await
    .unwrap();
    assert!(result.data["revision"].as_u64().unwrap() > before);
    run(&test, &session, "read_relation_columns", input)
        .await
        .unwrap();
    assert_eq!(test.mock.get_columns_calls(), 3);
}

#[tokio::test]
async fn catalog_preserves_driver_owned_list_tables_command_results() {
    let test = TestAppState::with_options(MockDriverOptions {
        command_results: std::collections::HashMap::from([("list_tables".into(), json!({
            "tables": [{ "name": "custom_collection", "schema": null, "tableType": "table", "rowCount": null }]
        }))]), ..Default::default()
    }).await;
    let (_, session) = test.save_and_connect("metadata-driver-catalog").await;
    let result = run(
        &test,
        &session,
        "list_catalog",
        json!({ "database": "app", "schema": null }),
    )
    .await
    .unwrap();
    let catalog: ListCatalogOutput = serde_json::from_value(result.data).unwrap();
    assert_eq!(catalog.relations.len(), 1);
    assert_eq!(catalog.relations[0].relation.name, "custom_collection");
}

// ===========================================================================
// CM-01 baseline — 空 namespace 字符串 / 缺字段请求必须返回参数错误。
//
// Spec: `docs/architecture/platform/connection-management.md:834-838`（CM-01，H/F）
//   前置：所有 DTO 已定义；Counter 使用大于 `2^53` 的十进制值。
//   步骤：Rust/TS 往返序列化；编译时把 connectionId newtype 传入 session API
//         的负例；传空 namespace 字符串。
//   断言：计数不丢精度；Rust 负例编译失败；空值/缺字段请求返回参数错误；
//         null 按 namespaceShape 与操作需求解释。
//
// 本块只覆盖 CM-01 的最后两条断言中 Host 侧可测的部分（"空值/缺字段请求返回
// 参数错误" + "null 按操作需求解释"），因为：
//   * "计数不丢精度"要求 `Counter` 这类 ID/计数 newtype；仓里不存在这种类型，
//     建它就是改生产代码。
//   * "Rust 负例编译失败"要求 `ConnectionId` / `DbSessionId` newtype 让两个 ID
//     在类型上不可互换，同样是改生产代码；且仓里既无 `trybuild` 也无任何
//     `compile_fail` 夹具，落地还得引入新依赖。
// 两者都超出「本轮 P0 只加夹具与测试」的范围，故不在此伪造断言。
//
// 落地形态：CM-01 是 (H/F) 用例，Host 侧可观测的参数层就是元数据请求的解码
// 入口。`CatalogInput` / `SchemaInput` / `ColumnsInput` / `RefreshInput` 都带
// `deny_unknown_fields` 且字段无 `#[serde(default)]`，`decode`（schema_metadata.rs:113）
// 把 serde 失败统一包成 `CommandError::Validation("Invalid metadata request: …")`；
// `validate_relation`（:118-144）再把空串 database/schema/name 包成
// `CommandError::Validation`。即"缺字段"与"空 namespace"都在到达驱动之前被参数层
// 拦下——这正是 CM-01 要固定下来的行为。
//
// 本块全部为绿测：今天 Host 在这两类输入上的行为已经符合 CM-01，无需配
// `#[ignore]` 的目标行为伴随测。
// ===========================================================================

/// 取参数错误正文，让断言落在"参数错误"这一类而不是任意失败上。
fn validation(error: CommandError) -> String {
    match error {
        CommandError::Validation(message) => message,
        other => panic!("CM-01: 期望 CommandError::Validation, got {other:?}"),
    }
}

/// 一个声明了 schema 层级的 Host fake：只有它，"空 schema 是错、null 是错"
/// 这两条规则才会和 listing 的"null 合法"形成对照。
async fn schema_aware(name: &str) -> (TestAppState, String) {
    let test = TestAppState::with_options(MockDriverOptions {
        has_schema_level: true,
        tables: vec![datazen_driver_api::TableInfo {
            name: "users".into(),
            schema: Some("public".into()),
            table_type: datazen_driver_api::TableType::Table,
            row_count: Some(1),
        }],
        ..Default::default()
    })
    .await;
    let (_, session) = test.save_and_connect(name).await;
    (test, session)
}

#[tokio::test]
async fn cm01_blank_namespace_is_a_parameter_error_not_a_default() {
    let (test, session) = schema_aware("cm01-blank-namespace").await;

    // 空 namespace 字符串：不得被静默当成"未指定 schema"而回落到驱动默认库。
    for blank in ["", " ", "\t", "\n  "] {
        let error = run(
            &test,
            &session,
            "read_relation_schema",
            json!({ "relation": { "database": "app", "schema": blank, "name": "users" } }),
        )
        .await
        .expect_err("CM-01: 空 namespace 必须被参数层拒绝");
        assert_eq!(
            validation(error),
            "Schema must be null or a non-blank name",
            "CM-01: 空白 schema 不得当作缺省 schema, blank={blank:?}"
        );
    }

    // 批量入口走同一个校验，行为必须一致，不能只有单条入口拦得住。
    let error = run(
        &test,
        &session,
        "read_relation_columns",
        json!({ "relations": [
        { "database": "app", "schema": "public", "name": "users" },
        { "database": "app", "schema": "", "name": "users" }
    ] }),
    )
    .await
    .expect_err("CM-01: 批量入口也必须拒绝空 namespace");
    assert_eq!(validation(error), "Schema must be null or a non-blank name");

    // 空串 database / name 同属参数层。
    for (relation, expected) in [
        (
            json!({ "database": "", "schema": "public", "name": "users" }),
            "Database is required",
        ),
        (
            json!({ "database": "  ", "schema": "public", "name": "users" }),
            "Database is required",
        ),
        (
            json!({ "database": "app", "schema": "public", "name": "" }),
            "Relation name is required",
        ),
    ] {
        let error = run(
            &test,
            &session,
            "read_relation_schema",
            json!({ "relation": relation }),
        )
        .await
        .expect_err("CM-01: 空串字段必须被参数层拒绝");
        assert_eq!(validation(error), expected, "CM-01: {relation}");
    }

    // list_catalog 与 refresh 的空串 database 同样在参数层被拒。
    let error = run(
        &test,
        &session,
        "list_catalog",
        json!({ "database": "", "schema": null }),
    )
    .await
    .expect_err("CM-01: list_catalog 的空 database 必须被拒");
    assert_eq!(validation(error), "Database is required");

    let error = run(
        &test,
        &session,
        "refresh_schema_metadata",
        json!({ "scope": { "kind": "database", "database": "  " } }),
    )
    .await
    .expect_err("CM-01: refresh 的空 database 必须被拒");
    assert_eq!(validation(error), "Database is required");

    // 对照组：同一形状的非空请求是成功的，说明上面拒的是"空"而不是"这条命令"。
    run(
        &test,
        &session,
        "read_relation_schema",
        json!({ "relation": { "database": "app", "schema": "public", "name": "users" } }),
    )
    .await
    .expect("CM-01: 合法请求必须仍然成功，否则上面的拒绝不是形状校验");
}

#[tokio::test]
async fn cm01_missing_required_field_is_a_parameter_error() {
    let (test, session) = schema_aware("cm01-missing-field").await;

    // 参数层有两道闸，两道都必须给出参数错误并指名出错字段：
    //   1. Command JSON Schema 的 `required`（driver-api `validate_command_input`），
    //      产出 "Command '<id>' input is missing required field '<field>'"；
    //   2. serde 解码（schema_metadata.rs 的 `decode`），产出
    //      "Invalid metadata request: <serde 原因>"。
    // 第 1 道只看顶层 required，relation 内部的字段由第 2 道兜住——两道都要覆盖。
    for (command, input, expected) in [
        (
            "list_catalog",
            json!({ "schema": null }),
            "Command 'list_catalog' input is missing required field 'database'",
        ),
        (
            "list_catalog",
            json!({ "database": "app" }),
            "Command 'list_catalog' input is missing required field 'schema'",
        ),
        (
            "read_relation_columns",
            json!({}),
            "Command 'read_relation_columns' input is missing required field 'relations'",
        ),
        (
            "refresh_schema_metadata",
            json!({}),
            "Command 'refresh_schema_metadata' input is missing required field 'scope'",
        ),
        (
            "read_relation_schema",
            json!({}),
            "Command 'read_relation_schema' input is missing required field 'relation'",
        ),
        (
            "read_relation_schema",
            json!({ "relation": { "database": "app", "schema": "public" } }),
            "Invalid metadata request: missing field `name`",
        ),
        // 多余字段按 deny_unknown_fields 拒绝——目标维度不被静默吞掉。
        (
            "list_catalog",
            json!({ "database": "app", "schema": null, "target": "other" }),
            "Invalid metadata request: unknown field `target`, expected `database` or `schema`",
        ),
        // scope 的取值空间是封闭的：未知 kind 不能被当成 session 级刷新。
        (
            "refresh_schema_metadata",
            json!({ "scope": { "kind": "everything" } }),
            "Invalid metadata request: unknown variant `everything`, expected one of \
             `session`, `database`, `relation`",
        ),
    ] {
        let error = run(&test, &session, command, input.clone())
            .await
            .expect_err("CM-01: 缺字段/多余字段必须返回参数错误");
        assert_eq!(validation(error), expected, "CM-01: {command} / {input}");
    }

    // 形状错误（顶层不是对象）同样在参数层被拒。
    let error = run(&test, &session, "list_catalog", json!("app"))
        .await
        .expect_err("CM-01: input 必须是对象，否则无法定位目标");
    assert_eq!(
        validation(error),
        "Command 'list_catalog' input must be an object"
    );
}

#[tokio::test]
async fn cm01_omitted_relation_schema_is_reported_as_a_driver_error_today() {
    let (test, session) = schema_aware("cm01-omitted-schema").await;

    // 已观测行为（不是目标行为）：RelationRef.schema 是 `Option<String>` 且没有
    // `#[serde(default)]`，serde 仍然把"键缺失"解成 `None`，与显式 `schema: null`
    // 无法区分。于是 Host 走的是 `validate_relation` 末尾的
    // `validate_schema_target(..., ExactSchema)`，报出来的是驱动侧配置错误，
    // 不是 CM-01 要求的参数错误。
    //
    // 这是本用例集里唯一一处"缺字段没有变成参数错误"的缺口：顶层 required 由
    // `validate_command_input` 兜住，非 Option 的嵌套字段由 serde 兜住，
    // 只有"省略一个 Option 字段"这条路落到了驱动侧。目标行为由下面的
    // `#[ignore]` 伴随测固定，本轮不改生产代码。
    for command in ["read_relation_schema", "read_relation_columns"] {
        let input = if command == "read_relation_schema" {
            json!({ "relation": { "database": "app", "name": "users" } })
        } else {
            json!({ "relations": [{ "database": "app", "name": "users" }] })
        };
        let error = run(&test, &session, command, input.clone())
            .await
            .expect_err("CM-01: 省略 schema 无法定位单个 relation，必然被拒");
        match error {
            CommandError::Driver(inner) => assert_eq!(
                inner.to_string(),
                "Invalid configuration: driver 'postgres' has a schema level: an explicit \
                 schema is required (database 'app')"
            ),
            other => panic!("CM-01: 已观测行为是驱动侧配置错误, {command} / {input} → {other:?}"),
        }
    }

    // 无论走哪条路径，都不得落到"按默认库执行"。
    assert_eq!(test.mock.get_columns_calls(), 0);
    assert_eq!(test.mock.get_schema_calls(), 0);
}

/// 目标行为：省略 `schema` 键与显式 `null` 必须可区分，前者按缺字段报参数错误。
///
/// `#[ignore]`：本轮 P0 只加夹具与测试，不改生产解码路径（plan §4 P0 回滚规则）。
/// 该行为一旦落地，取消 `#[ignore]` 即成为常规回归测。
#[tokio::test]
#[ignore = "CM-01 目标行为：省略 Option 字段应报参数错误，待 Host 解码路径区分缺省与显式 null"]
async fn cm01_omitted_relation_schema_should_be_a_parameter_error() {
    let (test, session) = schema_aware("cm01-omitted-schema-target").await;

    for command in ["read_relation_schema", "read_relation_columns"] {
        let input = if command == "read_relation_schema" {
            json!({ "relation": { "database": "app", "name": "users" } })
        } else {
            json!({ "relations": [{ "database": "app", "name": "users" }] })
        };
        let error = run(&test, &session, command, input.clone())
            .await
            .expect_err("CM-01: 省略 schema 必须被参数层拒绝");
        let message = validation(error);
        assert!(
            message.starts_with("Invalid metadata request:") && message.contains("schema"),
            "CM-01: 省略 schema 应报缺字段参数错误, {command} / {input} → {message}"
        );
    }
}

#[tokio::test]
async fn cm01_null_namespace_follows_the_declared_shape() {
    // schema=null 在 listing 上是合法的：它表示"该库的全部 schema"。
    let (aware, session) = schema_aware("cm01-null-shape").await;
    run(
        &aware,
        &session,
        "list_catalog",
        json!({ "database": "app", "schema": null }),
    )
    .await
    .expect("CM-01: listing 的 null schema 是合法的");

    // 但在解析单个 relation 上，同一个 null 对声明了 schema 层级的驱动就是
    // 缺参——这正是 CM-01 "null 按 namespaceShape 与操作需求解释" 的一半。
    let error = run(
        &aware,
        &session,
        "read_relation_schema",
        json!({ "relation": { "database": "app", "schema": null, "name": "users" } }),
    )
    .await
    .expect_err("CM-01: 解析单个 relation 时 null schema 不足以定位");
    match error {
        CommandError::Driver(inner) => assert!(
            inner.to_string().contains("explicit schema is required"),
            "CM-01: 错误要说明缺的是显式 schema, got: {inner}"
        ),
        other => panic!("CM-01: 期望 CommandError::Driver, got {other:?}"),
    }

    // 另一半：无 schema 层级的驱动上，同一个 null 是它唯一合法的形状。
    let schemaless = TestAppState::with_tables().await;
    let (_, flat_session) = schemaless.save_and_connect("cm01-null-flat").await;
    run(
        &schemaless,
        &flat_session,
        "read_relation_schema",
        json!({ "relation": { "database": "app", "schema": null, "name": "users" } }),
    )
    .await
    .expect("CM-01: 无 schema 层级的驱动上 null schema 合法");
    let error = run(
        &schemaless,
        &flat_session,
        "read_relation_schema",
        json!({ "relation": { "database": "app", "schema": "public", "name": "users" } }),
    )
    .await
    .expect_err("CM-01: 无 schema 层级的驱动不得接受 schema");
    assert!(
        rejection_text(&error).contains("no schema level"),
        "CM-01: 错误要说明该驱动没有 schema 层级"
    );
}

/// 参数层拒绝的正文：参数错误与驱动侧的目标声明错误都算"被拦下"，
/// 这里统一取可断言的文字，不让测试去依赖错误枚举的具体分派。
fn rejection_text(error: &CommandError) -> String {
    match error {
        CommandError::Validation(message) => message.clone(),
        CommandError::Driver(inner) => inner.to_string(),
        other => panic!("CM-01: 期望参数层拒绝, got {other:?}"),
    }
}

#[tokio::test]
async fn cm01_rejected_metadata_requests_never_reach_the_driver() {
    let (test, session) = schema_aware("cm01-no-driver-touch").await;

    for (command, input) in [
        ("list_catalog", json!({ "database": "", "schema": null })),
        ("list_catalog", json!({ "schema": null })),
        (
            "read_relation_schema",
            json!({ "relation": { "database": "app", "schema": "", "name": "users" } }),
        ),
        (
            "read_relation_columns",
            json!({ "relations": [
            { "database": "app", "schema": " ", "name": "users" }
        ] }),
        ),
        ("refresh_schema_metadata", json!({})),
    ] {
        run(&test, &session, command, input.clone())
            .await
            .expect_err("CM-01: 这批请求都应当被参数层拒绝");
    }

    // 参数层拦下的代价必须是 0：一次驱动调用都不该发生。
    assert_eq!(test.mock.get_columns_calls(), 0, "CM-01: 不得读列");
    assert_eq!(test.mock.get_schema_calls(), 0, "CM-01: 不得读表结构");
    assert_eq!(test.mock.query_calls(), 0, "CM-01: 不得发查询");
    assert_eq!(test.mock.execute_calls(), 0, "CM-01: 不得执行语句");
    assert!(test.mock.use_database_calls().is_empty(), "CM-01: 不得切库");
}
