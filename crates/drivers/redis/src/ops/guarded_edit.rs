//! Compare-and-edit for existing keys. The checks and mutation run together;
//! a key that expired or changed type cannot be silently recreated.

use redis::aio::ConnectionLike;

#[derive(Clone, Copy)]
pub enum Edit<'a> {
    String {
        expected: &'a str,
        value: &'a str,
    },
    Hash {
        field: &'a str,
        expected: &'a str,
        value: &'a str,
    },
    List {
        index: i64,
        expected: &'a str,
        value: &'a str,
    },
    HashAdd {
        field: &'a str,
        value: &'a str,
    },
    ListAppend {
        value: &'a str,
    },
    ZsetAdd {
        member: &'a str,
        score: &'a str,
    },
    HashRemove {
        field: &'a str,
        previous: &'a str,
    },
    ListRemoveHead {
        previous: &'a str,
    },
    SetRemove {
        member: &'a str,
    },
    ZsetRemove {
        member: &'a str,
        previous: &'a str,
    },
    SetAdd {
        member: &'a str,
    },
    Zset {
        member: &'a str,
        expected: &'a str,
        score: &'a str,
    },
    Stream {
        field: &'a str,
        value: &'a str,
    },
    StreamGroupCreate {
        group: &'a str,
    },
    Json {
        expected: &'a str,
        value: &'a str,
    },
    Create {
        value: &'a str,
    },
    Rename {
        target: &'a str,
    },
    Ttl {
        expected_type: &'a str,
        seconds: i64,
    },
}

pub async fn edit<C: ConnectionLike + Send>(
    conn: &mut C,
    key: &str,
    edit: Edit<'_>,
) -> Result<bool, String> {
    let (script, args) = match &edit {
        Edit::String { expected, value } => (
            "if redis.call('TYPE',KEYS[1]).ok ~= 'string' or redis.call('GET',KEYS[1]) ~= ARGV[1] then return 0 end; redis.call('SET',KEYS[1],ARGV[2],'XX','KEEPTTL'); return 1",
            vec![expected.to_string(), value.to_string()],
        ),
        Edit::Hash { field, expected, value } => (
            "if redis.call('TYPE',KEYS[1]).ok ~= 'hash' or redis.call('HGET',KEYS[1],ARGV[1]) ~= ARGV[2] then return 0 end; redis.call('HSET',KEYS[1],ARGV[1],ARGV[3]); return 1",
            vec![field.to_string(), expected.to_string(), value.to_string()],
        ),
        Edit::List { index, expected, value } => (
            "if redis.call('TYPE',KEYS[1]).ok ~= 'list' or redis.call('LINDEX',KEYS[1],ARGV[1]) ~= ARGV[2] then return 0 end; redis.call('LSET',KEYS[1],ARGV[1],ARGV[3]); return 1",
            vec![index.to_string(), expected.to_string(), value.to_string()],
        ),
        Edit::HashAdd { field, value } => (
            "if redis.call('TYPE',KEYS[1]).ok ~= 'hash' then return 0 end; return redis.call('HSETNX',KEYS[1],ARGV[1],ARGV[2])",
            vec![field.to_string(), value.to_string()],
        ),
        Edit::ListAppend { value } => (
            "if redis.call('TYPE',KEYS[1]).ok ~= 'list' then return 0 end; redis.call('RPUSHX',KEYS[1],ARGV[1]); return 1",
            vec![value.to_string()],
        ),
        Edit::ZsetAdd { member, score } => (
            "if redis.call('TYPE',KEYS[1]).ok ~= 'zset' then return 0 end; return redis.call('ZADD',KEYS[1],'NX',ARGV[2],ARGV[1])",
            vec![member.to_string(), score.to_string()],
        ),
        Edit::HashRemove { field, previous } => (
            "if redis.call('TYPE',KEYS[1]).ok ~= 'hash' or redis.call('HLEN',KEYS[1]) < 2 or redis.call('HGET',KEYS[1],ARGV[1]) ~= ARGV[2] then return 0 end; return redis.call('HDEL',KEYS[1],ARGV[1])",
            vec![field.to_string(), previous.to_string()],
        ),
        Edit::ListRemoveHead { previous } => (
            "if redis.call('TYPE',KEYS[1]).ok ~= 'list' or redis.call('LLEN',KEYS[1]) < 2 or redis.call('LINDEX',KEYS[1],0) ~= ARGV[1] then return 0 end; redis.call('LPOP',KEYS[1]); return 1",
            vec![previous.to_string()],
        ),
        Edit::SetRemove { member } => (
            "if redis.call('TYPE',KEYS[1]).ok ~= 'set' or redis.call('SCARD',KEYS[1]) < 2 then return 0 end; return redis.call('SREM',KEYS[1],ARGV[1])",
            vec![member.to_string()],
        ),
        Edit::ZsetRemove { member, previous } => (
            "if redis.call('TYPE',KEYS[1]).ok ~= 'zset' or redis.call('ZCARD',KEYS[1]) < 2 then return 0 end; local score=redis.call('ZSCORE',KEYS[1],ARGV[1]); if not score or tonumber(score) ~= tonumber(ARGV[2]) then return 0 end; return redis.call('ZREM',KEYS[1],ARGV[1])",
            vec![member.to_string(), previous.to_string()],
        ),
        Edit::SetAdd { member } => (
            "if redis.call('TYPE',KEYS[1]).ok ~= 'set' then return 0 end; redis.call('SADD',KEYS[1],ARGV[1]); return 1",
            vec![member.to_string()],
        ),
        Edit::Zset { member, expected, score } => (
            "if redis.call('TYPE',KEYS[1]).ok ~= 'zset' then return 0 end; local score=redis.call('ZSCORE',KEYS[1],ARGV[1]); if not score or tonumber(score) ~= tonumber(ARGV[2]) then return 0 end; redis.call('ZADD',KEYS[1],'XX',ARGV[3],ARGV[1]); return 1",
            vec![member.to_string(), expected.to_string(), score.to_string()],
        ),
        Edit::Stream { field, value } => (
            "if redis.call('TYPE',KEYS[1]).ok ~= 'stream' then return 0 end; redis.call('XADD',KEYS[1],'NOMKSTREAM','*',ARGV[1],ARGV[2]); return 1",
            vec![field.to_string(), value.to_string()],
        ),
        Edit::StreamGroupCreate { group } => (
            "if redis.call('TYPE',KEYS[1]).ok ~= 'stream' then return 0 end; local result=redis.pcall('XGROUP','CREATE',KEYS[1],ARGV[1],'0'); if result.err then if string.sub(result.err,1,9) == 'BUSYGROUP' then return 0 else return result end end; return 1",
            vec![group.to_string()],
        ),
        Edit::Json { expected, value } => (
            "if redis.call('TYPE',KEYS[1]).ok ~= 'ReJSON-RL' or redis.call('JSON.GET',KEYS[1]) ~= ARGV[1] then return 0 end; redis.call('JSON.SET',KEYS[1],'$',ARGV[2],'XX'); return 1",
            vec![expected.to_string(), value.to_string()],
        ),
        Edit::Create { value } => (
            "if redis.call('SET',KEYS[1],ARGV[1],'NX') then return 1 else return 0 end",
            vec![value.to_string()],
        ),
        Edit::Rename { target } => {
            // This initial operation deliberately refuses cross-slot changes
            // on Redis Cluster; Redis itself classifies and rejects them.
            return redis::cmd("RENAMENX").arg(key).arg(target)
                .query_async::<bool>(conn).await.map_err(|error| error.to_string());
        }
        Edit::Ttl { expected_type, seconds } => {
            if (*seconds <= 0 && *seconds != -1) || *expected_type == "none" || expected_type.is_empty() {
                return Err("TTL requires an existing type and positive seconds or -1.".into());
            }
            (
                "if redis.call('TYPE',KEYS[1]).ok ~= ARGV[1] then return 0 end; if ARGV[2] == '-1' then redis.call('PERSIST',KEYS[1]); else redis.call('EXPIRE',KEYS[1],ARGV[2]); end; return 1",
                vec![expected_type.to_string(), seconds.to_string()],
            )
        }
    };
    redis::cmd("EVAL")
        .arg(script)
        .arg(1)
        .arg(key)
        .arg(args)
        .query_async::<u8>(conn)
        .await
        .map(|code| code == 1)
        .map_err(|error| error.to_string())
}
