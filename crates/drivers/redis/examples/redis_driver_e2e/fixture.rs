//! Real-server driver journeys. Run via the E2E fixture, never against a user DB.

#[path = "access.rs"]
mod access;
#[path = "codec.rs"]
mod codec;
#[path = "connections.rs"]
mod connections;
#[path = "extra.rs"]
mod extra;
#[path = "guarded.rs"]
mod guarded;
#[path = "protocol.rs"]
mod protocol;
#[path = "resource.rs"]
mod resource;
#[path = "search.rs"]
mod search;
#[path = "topology.rs"]
mod topology;
#[path = "validation.rs"]
mod validation;

use datazen_driver_api::{ConnectionConfig, ConnectionHandle, DatabaseDriver};
use datazen_driver_redis::factories;
use serde_json::{json, Value};

struct Journey<'a> {
    driver: &'a dyn DatabaseDriver,
    handle: &'a ConnectionHandle,
    oracle: redis::aio::MultiplexedConnection,
}

impl Journey<'_> {
    async fn command(&self, name: &str, payload: Value) -> Value {
        self.driver
            .execute_command(self.handle, name, payload)
            .await
            .unwrap_or_else(|error| panic!("{name}: {error}"))
            .data
    }

    async fn read<T: redis::FromRedisValue>(&mut self, name: &str, args: &[&str]) -> T {
        redis::cmd(name)
            .arg(args)
            .query_async(&mut self.oracle)
            .await
            .unwrap_or_else(|error| panic!("independent {name}: {error}"))
    }

    async fn catalog(&mut self) {
        let commands = self.driver.command_definitions();
        assert!(commands
            .iter()
            .any(|command| command.id == "type_distribution"));
        assert!(commands.iter().any(|command| command.id == "dump_keys"));
        assert_eq!(self.driver.driver_type(), "redis");
        assert_eq!(self.driver.default_port(), Some(6379));
        assert_eq!(self.driver.default_host(), Some("127.0.0.1"));
        assert!(self
            .driver
            .get_databases(self.handle)
            .await
            .unwrap()
            .contains(&"db1".into()));
        self.command(
            "set_string",
            json!({"key":"contract:catalog","value":"written"}),
        )
        .await;
        let names = self
            .driver
            .get_tables(self.handle, "db0", None)
            .await
            .unwrap();
        assert!(names.iter().any(|table| table.name == "contract:catalog"));
        let schema = self
            .driver
            .get_table_schema(self.handle, "contract:catalog", "db0", None)
            .await
            .unwrap();
        assert!(!schema.columns.is_empty());
        assert!(!self
            .driver
            .get_server_info(self.handle)
            .await
            .unwrap()
            .server_version
            .is_empty());
        let result = self
            .driver
            .query(self.handle, "GET contract:catalog")
            .await
            .unwrap();
        assert_eq!(result.rows.len(), 1);
        let results = self
            .driver
            .query_multi(self.handle, "PING\nGET contract:catalog\nGET", None)
            .await
            .unwrap();
        assert_eq!(results.results.len(), 3);
        assert!(self.driver.cancel_query(self.handle).await.is_err());
        let dbs = self.command("db_sizes", json!({})).await;
        assert!(dbs.as_array().is_some_and(|rows| rows.len() >= 2));
    }

    async fn keys(&mut self) {
        let key = "contract:binary";
        let result = self
            .command(
                "set_string_raw",
                json!({"key":key,"dataB64":"AP8HAA==","keepTtl":false}),
            )
            .await;
        assert_eq!(result["ok"], true);
        let data: Vec<u8> = self.read("GET", &[key]).await;
        assert_eq!(data, [0_u8, 255, 7, 0]);
        let response = self.command("get_key_raw", json!({"key":key})).await;
        assert_eq!(response["keyType"], "string");
        assert_eq!(response["rawB64"], "AP8HAA==");
        let plain = self
            .command("get_key", json!({"key":"contract:catalog"}))
            .await;
        assert!(plain.is_object());
        self.command("set_ttl", json!({"key":key,"ttlSeconds":80}))
            .await;
        assert!((1..=80).contains(&self.read::<i64>("TTL", &[key]).await));
        self.command("rename", json!({"key":key,"newKey":"contract:renamed"}))
            .await;
        assert_eq!(self.read::<u64>("EXISTS", &["contract:renamed"]).await, 1);
        let probe = self
            .command("key_probe", json!({"key":"contract:renamed"}))
            .await;
        assert!(probe.is_object());
        let live = self
            .command("count_matching", json!({"pattern":"contract:*","count":64}))
            .await;
        assert!(live.is_object());
        let folders = self
            .command(
                "list_children",
                json!({"prefix":"","count":64,"budget":128}),
            )
            .await;
        assert!(folders["children"]
            .as_array()
            .unwrap()
            .iter()
            .any(|c| c["prefix"] == "contract:"));
        for id in ["a", "b"] {
            self.command(
                "set_string",
                json!({"key":format!("contract:old:{id}"),"value":id}),
            )
            .await;
        }
        self.command(
            "batch_set_ttl",
            json!({"keys":["contract:old:a","contract:old:b"],"ttlSeconds":90}),
        )
        .await;
        assert!(self.read::<i64>("TTL", &["contract:old:a"]).await > 0);
        self.command(
            "batch_rename_prefix",
            json!({"oldPrefix":"contract:old:","newPrefix":"contract:new:"}),
        )
        .await;
        assert_eq!(
            self.read::<u64>("EXISTS", &["contract:new:a", "contract:new:b"])
                .await,
            2
        );
        self.command("batch_delete_pattern", json!({"pattern":"contract:new:*"}))
            .await;
        assert_eq!(
            self.read::<u64>("EXISTS", &["contract:new:a", "contract:new:b"])
                .await,
            0
        );
    }

    async fn aggregates(&mut self) {
        let hash = "contract:hash";
        self.command("hash_set", json!({"key":hash,"field":"a","value":"one"}))
            .await;
        let view = self
            .command(
                "hash_scan",
                json!({"key":hash,"matchPattern":"a","count":8}),
            )
            .await;
        assert_eq!(view["entries"][0]["value"], "one");
        self.command("hash_del", json!({"key":hash,"fields":["a"]}))
            .await;
        assert_eq!(self.read::<u64>("HEXISTS", &[hash, "a"]).await, 0);

        let list = "contract:list";
        self.command(
            "list_push",
            json!({"key":list,"side":"right","values":["a","b","a"]}),
        )
        .await;
        self.command("list_set", json!({"key":list,"index":1,"value":"c"}))
            .await;
        self.command("list_index", json!({"key":list,"index":1}))
            .await;
        self.command("list_rem", json!({"key":list,"value":"a","count":1}))
            .await;
        self.command("list_pop", json!({"key":list,"side":"right"}))
            .await;
        assert_eq!(
            self.read::<Vec<String>>("LRANGE", &[list, "0", "-1"]).await,
            ["c"]
        );

        let set = "contract:set";
        self.command("set_add", json!({"key":set,"members":["a","b"]}))
            .await;
        let items = self
            .command("set_scan", json!({"key":set,"matchPattern":"b","count":8}))
            .await;
        assert_eq!(items["members"], json!(["b"]));
        self.command("set_remove", json!({"key":set,"members":["b"]}))
            .await;
        assert_eq!(self.read::<Vec<String>>("SMEMBERS", &[set]).await, ["a"]);

        let zset = "contract:zset";
        self.command(
            "zset_add",
            json!({"key":zset,"members":[{"member":"a","score":1},{"member":"b","score":2}]}),
        )
        .await;
        let ranked = self
            .command(
                "zset_scan",
                json!({"key":zset,"matchPattern":"b","count":8}),
            )
            .await;
        assert_eq!(ranked["members"][0]["score"].as_f64(), Some(2.0));
        self.command("zset_remove", json!({"key":zset,"members":["a"]}))
            .await;
        assert_eq!(
            self.read::<Vec<String>>("ZRANGE", &[zset, "0", "-1"]).await,
            ["b"]
        );
    }

    async fn observe(&mut self) {
        let info = self
            .command(
                "info_filtered",
                json!({"section":"server","search":"redis_version"}),
            )
            .await;
        assert!(info.to_string().contains("redis_version"));
        let unknown = self
            .command(
                "info_filtered",
                json!({"section":"nonexistent","search":"fixture"}),
            )
            .await;
        assert_eq!(unknown["sections"], json!([]));
        let missing = self
            .command("key_object_info", json!({"key":"contract:does-not-exist"}))
            .await;
        assert_eq!(missing["missing"], true);
        assert_eq!(missing["ttlMs"], -2);
        let memory = self
            .command("memory_usage_key", json!({"key":"contract:does-not-exist"}))
            .await;
        assert_eq!(memory["bytes"], 0);
        assert_eq!(memory["error"], "key does not exist");

        assert_eq!(
            self.read::<String>("CONFIG", &["SET", "maxmemory-policy", "allkeys-lfu"])
                .await,
            "OK"
        );
        let lfu = self
            .command("key_object_info", json!({"key":"contract:catalog"}))
            .await;
        assert_eq!(lfu["type"], "string");
        assert_eq!(lfu["idleSeconds"], Value::Null);
        assert_eq!(
            lfu["freq"],
            self.read::<u64>("OBJECT", &["FREQ", "contract:catalog"])
                .await
        );
        assert_eq!(
            self.read::<String>("CONFIG", &["SET", "maxmemory-policy", "noeviction"])
                .await,
            "OK"
        );

        let distribution = self
            .command("type_distribution", json!({"sampleLimit":64}))
            .await;
        assert!(distribution.is_object());
        assert!(self.command("modules_list", json!({})).await.is_array());
        assert!(self
            .command("memory_usage_key", json!({"key":"contract:zset"}))
            .await
            .is_object());
        let result = self
            .command(
                "exec",
                json!({"commands":"GET contract:catalog\nPING\nGET\n"}),
            )
            .await;
        let rows = result["results"].as_array().unwrap();
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[1]["value"], "PONG");
        assert_eq!(rows[2]["ok"], false);
        // Exercise the real slow-log lifecycle, not just its parser.
        assert_eq!(
            self.read::<String>("CONFIG", &["SET", "slowlog-log-slower-than", "0"])
                .await,
            "OK"
        );
        assert_eq!(
            self.read::<String>("GET", &["contract:catalog"]).await,
            "written"
        );
        assert_eq!(
            self.read::<String>("CONFIG", &["SET", "slowlog-log-slower-than", "-1"])
                .await,
            "OK"
        );
        let slow = self.command("slowlog_get", json!({"count":10})).await;
        assert!(slow.to_string().contains("contract:catalog"), "{slow}");
        assert!(self
            .driver
            .execute_command(self.handle, "slowlog_reset", json!({"confirm":false}))
            .await
            .is_err());
        assert!(self.read::<u64>("SLOWLOG", &["LEN"]).await > 0);
        assert_eq!(
            self.command("slowlog_reset", json!({"confirm":true})).await["ok"],
            true
        );
        assert_eq!(self.read::<u64>("SLOWLOG", &["LEN"]).await, 0);
        assert_eq!(
            self.command("slowlog_get", json!({"count":0})).await,
            json!([])
        );
        let status = self.command("pubsub_stats", json!({})).await;
        assert_eq!(status["totalMessages"], 0);
        assert_eq!(
            self.command("pubsub_list_subscriptions", json!({})).await["subscriptions"],
            json!([])
        );
    }

    async fn stream(&mut self) {
        let key = "contract:stream";
        self.command("xadd", json!({"key":key,"fields":{"entry":"payload"}}))
            .await;
        self.command(
            "xgroup_create",
            json!({"key":key,"group":"g","startId":"0"}),
        )
        .await;
        self.command("stream_lag", json!({"key":key,"group":"g"}))
            .await;
        let overview = self.command("stream_overview", json!({"key":key})).await;
        assert!(overview.is_object());
        self.command("xgroup_destroy", json!({"key":key,"group":"g"}))
            .await;
        let groups = self.command("xinfo_groups", json!({"key":key})).await;
        assert_eq!(groups, json!([]));
    }
}

#[tokio::main(flavor = "current_thread")]
pub async fn run_cli(mut args: impl Iterator<Item = String>) {
    let port: u16 = args
        .next()
        .expect("fixture port")
        .parse()
        .expect("integer port");
    let proof = args.next().expect("disposable fixture nonce");
    let json = args.next();
    assert!(args.next().is_none() && !proof.is_empty());
    assert!(json.as_deref().is_none_or(|arg| arg == "json"));
    let redis = redis::Client::open(format!("redis://127.0.0.1:{port}/0")).unwrap();
    let mut oracle = redis.get_multiplexed_async_connection().await.unwrap();
    let observed: Option<String> = redis::cmd("GET")
        .arg("contract:fixture")
        .query_async(&mut oracle)
        .await
        .unwrap();
    assert_eq!(
        observed.as_deref(),
        Some(proof.as_str()),
        "Use only the fresh E2E fixture."
    );
    let cfg: ConnectionConfig = serde_json::from_value(json!({
        "id":"real-redis-driver-e2e","name":"Real Redis E2E","databaseType":"redis",
        "host":"127.0.0.1","port":port,"database":"0","connectionTimeout":4,
    }))
    .unwrap();
    let factory = factories()[0];
    assert_eq!(factory.driver_id(), "redis");
    let provider = factory
        .resource_provider()
        .expect("registered Redis resource provider");
    let driver = factory.create();
    let server = driver.test_connection(&cfg).await.unwrap();
    assert!(!server.server_version.is_empty());
    let handle = driver.connect(&cfg).await.unwrap();
    let mut suite = Journey {
        driver: driver.as_ref(),
        handle: &handle,
        oracle,
    };
    suite.catalog().await;
    suite.keys().await;
    suite.aggregates().await;
    suite.observe().await;
    suite.stream().await;
    extra::archive_and_metadata(&mut suite).await;
    if json.is_some() {
        extra::json_document(&mut suite).await;
    }
    codec::run(&mut suite).await;
    search::run(&mut suite).await;
    protocol::run(&mut suite).await;
    guarded::run(&mut suite, json.is_some()).await;
    validation::run(&mut suite).await;
    connections::run(&mut suite, &cfg).await;
    access::authenticated(&mut suite, &cfg, &proof).await;
    resource::run(provider.as_ref(), &cfg, port).await;
    if let (Ok(sentinel), Ok(cluster)) = (
        std::env::var("REDIS_E2E_SENTINEL_PORT"),
        std::env::var("REDIS_E2E_CLUSTER_PORT"),
    ) {
        topology::run(
            driver.as_ref(),
            &cfg,
            port,
            sentinel.parse().unwrap(),
            cluster.parse().unwrap(),
            &proof,
        )
        .await;
    }
    access::flush_guard(&mut suite, &proof).await;
    driver.disconnect(handle).await.unwrap();
    println!(
        "PASS: real Redis driver catalog, exact/binary keys, aggregates, observations, Streams, real resource sessions, codecs, ACL and disposal guards"
    );
}
