//! Read and write Redis ZIP manifest v1 without extracting paths to disk.
use std::{
    collections::HashSet,
    fs::{File, OpenOptions},
    io::{Cursor, Read, Write},
    path::Path,
};

use base64::{engine::general_purpose::STANDARD, Engine};
use datazen_driver_api::{ConnectionHandle, DatabaseDriver};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use super::call;

const MAX_BYTES: u64 = 16 * 1024 * 1024;
const MAX_ENTRIES: usize = 128;
const MANIFEST: &str = "manifest.json";

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Manifest {
    schema_version: u32,
    db_index: u32,
    keys: Vec<Entry>,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Entry {
    key: String,
    ttl_seconds: i64,
    dump_file: String,
}

fn encode(db: u32, entries: &[Value]) -> Result<Vec<u8>, String> {
    let mut manifest = Manifest {
        schema_version: 1,
        db_index: db,
        keys: Vec::new(),
    };
    let mut total = 0;
    let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    for (i, row) in entries.iter().enumerate() {
        let key = row["key"]
            .as_str()
            .ok_or("Redis dump did not include a key.")?;
        let ttl_seconds = row["ttlSeconds"]
            .as_i64()
            .filter(|n| *n == -1 || (1..=31_536_000).contains(n))
            .ok_or("Cannot export an expired or invalid Redis key.")?;
        let bytes = STANDARD
            .decode(row["dumpBase64"].as_str().ok_or("Redis dump is missing.")?)
            .map_err(|_| "Invalid Redis dump.")?;
        total += bytes.len() as u64;
        if total > MAX_BYTES {
            return Err("Redis archive exceeds 16 MiB.".into());
        }
        let dump_file = format!("key_{i}.bin");
        writer
            .start_file(&dump_file, options)
            .map_err(|_| "Cannot create ZIP entry.")?;
        writer
            .write_all(&bytes)
            .map_err(|_| "Cannot write ZIP entry.")?;
        manifest.keys.push(Entry {
            key: key.into(),
            ttl_seconds,
            dump_file,
        });
    }
    writer
        .start_file(MANIFEST, options)
        .map_err(|_| "Cannot create ZIP manifest.")?;
    serde_json::to_writer(&mut writer, &manifest).map_err(|_| "Could not encode ZIP manifest.")?;
    writer
        .finish()
        .map(|cursor| cursor.into_inner())
        .map_err(|_| "Cannot finish Redis archive.".into())
}

fn decode(reader: impl Read + std::io::Seek) -> Result<Vec<Value>, String> {
    let mut zip = zip::ZipArchive::new(reader).map_err(|_| "Invalid Redis ZIP archive.")?;
    if zip.len() > MAX_ENTRIES + 1 {
        return Err("Redis archive has too many files.".into());
    }
    let manifest: Manifest = {
        let file = zip
            .by_name(MANIFEST)
            .map_err(|_| "Redis ZIP manifest is missing.")?;
        if file.size() > 256 * 1024 {
            return Err("Redis ZIP manifest is too large.".into());
        }
        serde_json::from_reader(file.take(256 * 1024)).map_err(|_| "Invalid Redis ZIP manifest.")?
    };
    if manifest.schema_version != 1 || manifest.keys.is_empty() || manifest.keys.len() > MAX_ENTRIES
    {
        return Err("Redis ZIP must contain between 1 and 128 version-1 entries.".into());
    }
    let mut key_names = HashSet::new();
    let mut file_names = HashSet::new();
    let mut total = 0_u64;
    let mut values = Vec::new();
    for row in manifest.keys {
        if !(1..=1024).contains(&row.key.len())
            || !(-1..=31_536_000).contains(&row.ttl_seconds)
            || row.ttl_seconds == 0
            || !row.dump_file.ends_with(".bin")
            || row.dump_file.contains(['/', '\\'])
            || !key_names.insert(row.key.clone())
            || !file_names.insert(row.dump_file.clone())
        {
            return Err("Invalid or duplicate Redis archive entries.".into());
        }
        let file = zip
            .by_name(&row.dump_file)
            .map_err(|_| "Redis dump file is missing.")?;
        if file.size() == 0 || file.size() > MAX_BYTES.saturating_sub(total) {
            return Err("Redis archive exceeds the supported dump size.".into());
        }
        let mut bytes = Vec::new();
        file.take(MAX_BYTES - total + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| "Could not read Redis dump.")?;
        total += bytes.len() as u64;
        if total > MAX_BYTES {
            return Err("Redis archive exceeds 16 MiB.".into());
        }
        values.push(json!({"key":row.key,"ttlSeconds":row.ttl_seconds,"dumpBase64":STANDARD.encode(&bytes)}));
    }
    Ok(values)
}

pub(super) async fn export(
    driver: &dyn DatabaseDriver,
    handle: &ConnectionHandle,
    db: u32,
    keys: &[String],
    path: &Path,
) -> Result<String, String> {
    if keys.is_empty() || keys.len() > MAX_ENTRIES {
        return Err("Choose between 1 and 128 Redis keys to export.".into());
    }
    let data = call(
        driver,
        handle,
        "dump_keys",
        json!({"dbIndex":db,"keys":keys}),
    )
    .await?;
    let rows = data["entries"]
        .as_array()
        .ok_or("Could not read Redis dumps.")?;
    let errors = data["errors"].as_array().map_or(0, Vec::len);
    if rows.len() != keys.len() || errors != 0 {
        return Err("Redis could not dump all selected keys; no archive was written.".into());
    }
    let bytes = encode(db, rows)?;
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|_| "Cannot create the archive. Choose a new writable file name.")?;
    file.write_all(&bytes)
        .map_err(|_| "Cannot write the Redis archive.")?;
    Ok(format!("Exported {} keys to ZIP.", rows.len()))
}

pub(super) async fn import(
    driver: &dyn DatabaseDriver,
    handle: &ConnectionHandle,
    db: u32,
    path: &Path,
) -> Result<String, String> {
    let file = File::open(path).map_err(|_| "Cannot open the Redis ZIP archive.")?;
    if file
        .metadata()
        .map_err(|_| "Cannot inspect the Redis archive.")?
        .len()
        > MAX_BYTES
    {
        return Err("Redis archive exceeds 16 MiB.".into());
    }
    let values = decode(file)?;
    let count = values.len();
    let data = call(
        driver,
        handle,
        "restore_keys",
        json!({"dbIndex":db,"entries":values,"replace":false}),
    )
    .await?;
    let restored = data["restored"]
        .as_u64()
        .ok_or("Redis did not confirm archive restore.")?;
    Ok(format!(
        "Restored {restored} of {count} keys. Existing keys were not overwritten."
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zip_v1_roundtrip_keeps_binary_and_expiry() {
        let input = vec![json!({"key":"abc","ttlSeconds":100,"dumpBase64":"AAECAw=="})];
        let zip = encode(1, &input).expect("encode");
        assert_eq!(decode(Cursor::new(zip)).expect("decode"), input);
    }
}
