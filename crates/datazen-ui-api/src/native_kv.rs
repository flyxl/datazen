//! Native Redis workbench protocol; console text is validated by the host.

use serde::Deserialize;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NativeKvDriver {
    Redis,
}

pub fn native_kv_driver(database_type: &str) -> Option<NativeKvDriver> {
    match database_type {
        "redis" => Some(NativeKvDriver::Redis),
        _ => None,
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KvRead {
    pub connection_id: String,
    pub session_generation: u32,
    pub request_id: u64,
    pub db_index: Option<u32>,
    pub kind: KvReadKind,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum KvReadKind {
    Scan {
        cursor: u64,
        pattern: String,
        key_type: RedisKeyType,
        persistent: bool,
    },
    Folder {
        prefix: String,
        cursor: u64,
        key_type: RedisKeyType,
    },
    Detail {
        key: String,
    },
    Edit {
        key: String,
        change: KvChange,
    },
    Tool(KvTool),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum KvReply {
    Page(KvPage),
    Folder(KvFolderPage),
    Detail(KvDetail),
    Edited,
    Tool(String),
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct KvPage {
    pub cursor: u64,
    pub keys: Vec<KvKey>,
    pub truncated: bool,
    #[serde(skip)]
    pub writable: bool,
    #[serde(skip)]
    pub db_index: u32,
    #[serde(skip)]
    pub databases: Vec<u32>,
    #[serde(skip)]
    pub cluster: bool,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct KvFolderPage {
    pub children: Vec<KvFolderChild>,
    pub cursor: u64,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum KvFolderChild {
    Folder { prefix: String },
    Key { key: String },
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum RedisKeyType {
    #[default]
    Any,
    String,
    Hash,
    List,
    Set,
    SortedSet,
    Stream,
}

impl RedisKeyType {
    pub fn token(self) -> Option<&'static str> {
        match self {
            Self::Any => None,
            Self::String => Some("string"),
            Self::Hash => Some("hash"),
            Self::List => Some("list"),
            Self::Set => Some("set"),
            Self::SortedSet => Some("zset"),
            Self::Stream => Some("stream"),
        }
    }

    pub fn next(self) -> Self {
        match self {
            Self::Any => Self::String,
            Self::String => Self::Hash,
            Self::Hash => Self::List,
            Self::List => Self::Set,
            Self::Set => Self::SortedSet,
            Self::SortedSet => Self::Stream,
            Self::Stream => Self::Any,
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct KvKey {
    pub key: String,
    pub key_type: String,
    pub ttl: i64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KvDetail {
    pub key_type: String,
    pub ttl: i64,
    pub preview: String,
    pub string_edit: Option<String>,
    pub hash_fields: Vec<(String, String)>,
    pub collection_edits: Vec<(String, KvCollectionEdit, String)>,
}

/// Existing-value edits only. The worker checks profile permissions again.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum KvChange {
    String {
        expected: String,
        value: String,
    },
    Hash {
        field: String,
        expected: String,
        value: String,
    },
    Collection {
        target: KvCollectionEdit,
        expected: String,
        value: String,
    },
    Ttl {
        expected_type: String,
        seconds: i64,
    },
}

pub const KV_EDIT_MAX_BYTES: usize = 4096;
pub const KV_TTL_MAX_SECONDS: i64 = 31_536_000;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum RedisValueCodec {
    #[default]
    MessagePack,
    Base64,
    Gzip,
    Zlib,
    Deflate,
}

impl RedisValueCodec {
    pub fn token(self) -> &'static str {
        match self {
            Self::MessagePack => "msgpack",
            Self::Base64 => "base64",
            Self::Gzip => "gzip",
            Self::Zlib => "zlib",
            Self::Deflate => "deflate",
        }
    }

    pub fn next(self) -> Self {
        match self {
            Self::MessagePack => Self::Base64,
            Self::Base64 => Self::Gzip,
            Self::Gzip => Self::Zlib,
            Self::Zlib => Self::Deflate,
            Self::Deflate => Self::MessagePack,
        }
    }
}

/// Native console command catalog. The host independently validates each request.
pub const REDIS_CONSOLE_READ_COMMANDS: &[&str] = &[
    "DBSIZE",
    "EXISTS",
    "GET",
    "HEXISTS",
    "HGET",
    "HLEN",
    "INFO",
    "LINDEX",
    "LLEN",
    "PING",
    "PTTL",
    "SCARD",
    "SISMEMBER",
    "STRLEN",
    "TIME",
    "TTL",
    "TYPE",
    "XLEN",
    "ZCARD",
    "ZSCORE",
];

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum KvTool {
    Console(String),
    ConsoleBatch(Vec<String>),
    Overview,
    Info,
    SlowLog,
    MemorySample,
    ClusterNodes,
    KeyProperties(String),
    DecodeString {
        key: String,
        codec: RedisValueCodec,
    },
    StreamGroups(String),
    StreamConsumers {
        key: String,
        group: String,
    },
    StreamPending {
        key: String,
        group: String,
    },
    StreamCreateGroup {
        key: String,
        group: String,
    },
    StreamAck {
        key: String,
        group: String,
        entry: String,
    },
    SearchStart {
        query: String,
        pattern: String,
    },
    SearchNext,
    SearchStop,
    MonitorStart,
    MonitorRead,
    MonitorStop,
    Subscribe {
        channels: String,
        patterns: String,
    },
    Messages,
    Publish {
        channel: String,
        message: String,
    },
    Unsubscribe,
    Export {
        keys: Vec<String>,
        path: std::path::PathBuf,
    },
    Import {
        path: std::path::PathBuf,
    },
    CreateString {
        key: String,
        value: String,
    },
    RenameKey {
        key: String,
        target: String,
    },
    DeleteMarked {
        keys: Vec<String>,
        confirmation: String,
    },
    DeleteKey {
        key: String,
        confirmation: String,
    },
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum RedisTopology {
    #[default]
    Standalone,
    Sentinel,
    Cluster,
}

impl RedisTopology {
    pub fn label(self) -> &'static str {
        match self {
            Self::Standalone => "standalone",
            Self::Sentinel => "sentinel",
            Self::Cluster => "cluster",
        }
    }

    pub fn next(self) -> Self {
        match self {
            Self::Standalone => Self::Sentinel,
            Self::Sentinel => Self::Cluster,
            Self::Cluster => Self::Standalone,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RedisConnectionOptions {
    pub topology: RedisTopology,
    pub sentinel_master: String,
    pub nodes: String,
    pub tls_ca: String,
    pub tls_cert: String,
    pub tls_key: String,
    pub tls: bool,
    pub read_only: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum KvCollectionEdit {
    ListIndex(usize),
    ListAppend,
    ListRemoveHead(String),
    HashAdd,
    HashRemove { field: String, previous: String },
    SetAdd,
    SetRemove(String),
    SortedSetScore(String),
    SortedSetAdd,
    SortedSetRemove { member: String, score: String },
    StreamAppend,
    JsonRoot,
}
