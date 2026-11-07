//! 语言包 ZIP 的两遍安检解包、只检不装的预检（inspect）与原子安装。
//!
//! 流程与 wapp 安装器同构（`wapps::install`）：解到 `{packs_dir}/
//! .staging-{uuid}` → 全量校验 → 原子 rename 到 `{packs_dir}/{code}`
//! （旧装备份为 `{code}.old.bak`，新包就位后删除）。inspect 把包物化进
//! 系统临时目录跑同一套规则，不触碰 `{packs_dir}`。
//!
//! 两遍解包（fail-closed）：第一遍只读 ZIP 目录表逐条安检——路径穿越/
//! 绝对路径拒（`datazen-infra::path_safety`）、符号链接拒、隐藏文件拒、
//! 根层白名单文件名拒、目录条目拒、单条目与整包压缩比超 100 拒（ZIP
//! 炸弹）；第二遍才写盘，写盘读取器强制实际字节数不超过条目声明值。
//! 除压缩比外**无内容数量上限**（用户裁决 Q5：键数、译文长度、包体积
//! 均不设限）——条目白名单只有 14 个合法名字，结构上不可能塞进海量条目。

use std::collections::{BTreeMap, HashSet};
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use uuid::Uuid;
use zip::ZipArchive;

use datazen_infra::path_safety::{
    validate_theme_zip_path, validate_zip_entry_path, MAX_COMPRESSION_RATIO,
};

use super::manifest::{
    validate_message_key, validate_message_value, LanguagePackManifest, LANGUAGE_PACK_DOMAINS,
    MANIFEST_FILE,
};
use super::summary::LanguagePackDomainCount;

const STAGING_PREFIX: &str = ".staging-";
const BACKUP_SUFFIX: &str = ".old.bak";
const INSPECT_PREFIX: &str = ".datazen-langpack-inspect-";

/// 预检结果：manifest + 各域译文条数（安装对话框展示用）。
#[derive(Debug, Clone)]
pub struct LanguagePackInspection {
    pub manifest: LanguagePackManifest,
    pub domains: Vec<LanguagePackDomainCount>,
}

/// 安装一个语言包 ZIP 到 `{packs_dir}/{manifest.code}/`。
///
/// 同 code 重装 = 旧目录先备份为 `{code}.old.bak`、新目录就位后删除备份；
/// 任一步失败都会尝试恢复旧包并把 staging 清干净（零残留）。返回
/// inspection（manifest + 各域条数），宿主用它登记共享注册表。
pub fn install_from_zip(
    zip_path: &Path,
    packs_dir: &Path,
) -> Result<LanguagePackInspection, String> {
    if !zip_path.is_file() {
        return Err(format!(
            "language pack file not found: {}",
            zip_path.display()
        ));
    }
    fs::create_dir_all(packs_dir).map_err(|e| format!("create language packs dir: {e}"))?;

    let staging = staging_dir(packs_dir);
    let result = (|| -> Result<LanguagePackInspection, String> {
        extract_language_pack_zip(zip_path, &staging)?;
        finalize_staged_package(&staging, packs_dir)
    })();

    let _ = fs::remove_dir_all(&staging);
    result
}

/// 只检不装：把 ZIP 物化进临时目录跑同一套规则，返回预览信息；成功与
/// 失败都会清理临时目录，`{packs_dir}` 永不被触碰。
pub fn inspect_language_pack_file(zip_path: &Path) -> Result<LanguagePackInspection, String> {
    if !zip_path.is_file() {
        return Err(format!(
            "language pack file not found: {}",
            zip_path.display()
        ));
    }
    let staging = std::env::temp_dir().join(format!("{INSPECT_PREFIX}{}", Uuid::new_v4()));
    let result = (|| -> Result<LanguagePackInspection, String> {
        extract_language_pack_zip(zip_path, &staging)?;
        validate_pack_dir(&staging)
    })();
    let _ = fs::remove_dir_all(&staging);
    result
}

fn staging_dir(packs_dir: &Path) -> PathBuf {
    packs_dir.join(format!("{STAGING_PREFIX}{}", Uuid::new_v4()))
}

fn zip_bomb_err(message: impl Into<String>) -> String {
    message.into()
}

fn ratio_exceeds_limit(uncompressed: u64, compressed: u64, max_ratio: u64) -> bool {
    if compressed == 0 {
        return uncompressed > 0;
    }
    (uncompressed as u128) > (compressed as u128) * max_ratio as u128
}

fn check_compression_ratio(
    uncompressed: u64,
    compressed: u64,
    max_ratio: u64,
    context: &str,
) -> Result<(), String> {
    if ratio_exceeds_limit(uncompressed, compressed, max_ratio) {
        return Err(zip_bomb_err(format!(
            "zip bomb: compression ratio exceeded ({context})"
        )));
    }
    Ok(())
}

/// 拒绝点前缀的路径分量：包内不允许任何隐藏文件。
fn reject_hidden_components(entry_name: &str) -> Result<(), String> {
    for component in entry_name.split('/') {
        if component.is_empty() || component == "." || component == ".." {
            continue;
        }
        if component.starts_with('.') {
            return Err(format!("hidden file not allowed in package: {entry_name}"));
        }
    }
    Ok(())
}

/// 条目名必须精确等于 `manifest.json` 或 13 个白名单域名之一，且只在
/// ZIP 根层（无子目录）。
fn check_pack_file_name(entry_name: &str) -> Result<(), String> {
    if entry_name.contains('/') {
        return Err(format!(
            "invalid language pack: nested paths are not allowed: {entry_name}"
        ));
    }
    if entry_name == MANIFEST_FILE {
        return Ok(());
    }
    if let Some(stem) = entry_name.strip_suffix(".json") {
        if LANGUAGE_PACK_DOMAINS.contains(&stem) {
            return Ok(());
        }
    }
    Err(format!(
        "invalid language pack: unexpected file `{entry_name}`"
    ))
}

/// 读取器：实际解出的字节数不得超过条目声明的未压缩大小（防「声明小、
/// 实际大」的撒谎条目）。
struct LimitedZipReader<'a, R: Read> {
    inner: R,
    entry_remaining: u64,
    _marker: std::marker::PhantomData<&'a ()>,
}

impl<R: Read> Read for LimitedZipReader<'_, R> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let n = self.inner.read(buf)?;
        if n == 0 {
            return Ok(0);
        }
        let n = n as u64;
        if n > self.entry_remaining {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "zip bomb: entry exceeded declared uncompressed size",
            ));
        }
        self.entry_remaining -= n;
        Ok(n as usize)
    }
}

/// 两遍安检解包。第一遍名称/配额安检（逐条目），第二遍有界写盘。
fn extract_language_pack_zip(zip_path: &Path, dest: &Path) -> Result<(), String> {
    fs::create_dir_all(dest).map_err(|e| format!("create staging dir: {e}"))?;
    let file = File::open(zip_path).map_err(|e| format!("open language pack zip: {e}"))?;
    let mut archive = ZipArchive::new(file).map_err(|e| format!("read language pack zip: {e}"))?;

    let entry_count = archive.len();
    let mut seen: HashSet<String> = HashSet::new();
    let mut total_uncompressed: u64 = 0;
    let mut total_compressed: u64 = 0;

    // 第一遍：名称安全、白名单、重复、压缩比（只读目录表，不写盘）。
    for i in 0..entry_count {
        let entry = archive
            .by_index(i)
            .map_err(|e| format!("read zip entry {i}: {e}"))?;
        let entry_name = entry.name().to_string();
        if entry.is_dir() || entry_name.ends_with('/') {
            return Err(format!(
                "invalid language pack: directory entries are not allowed: {entry_name}"
            ));
        }
        validate_theme_zip_path(&entry_name)?;
        reject_hidden_components(&entry_name)?;
        if entry.is_symlink() {
            return Err(format!("symlink zip entry not allowed: {entry_name}"));
        }
        check_pack_file_name(&entry_name)?;
        if !seen.insert(entry_name.clone()) {
            return Err(format!(
                "invalid language pack: duplicate file `{entry_name}`"
            ));
        }
        let uncompressed = entry.size();
        let compressed = entry.compressed_size();
        check_compression_ratio(
            uncompressed,
            compressed,
            MAX_COMPRESSION_RATIO,
            &format!("entry `{entry_name}`"),
        )?;
        total_uncompressed = total_uncompressed
            .checked_add(uncompressed)
            .ok_or_else(|| zip_bomb_err("zip bomb: uncompressed size overflow"))?;
        total_compressed = total_compressed
            .checked_add(compressed)
            .ok_or_else(|| zip_bomb_err("zip bomb: compressed size overflow"))?;
    }
    if total_compressed > 0 {
        check_compression_ratio(
            total_uncompressed,
            total_compressed,
            MAX_COMPRESSION_RATIO,
            "archive total",
        )?;
    }

    // 第二遍：有界写盘（平面布局，所有文件落在 dest 根层）。
    for i in 0..entry_count {
        let mut entry = archive
            .by_index(i)
            .map_err(|e| format!("read zip entry {i}: {e}"))?;
        let entry_name = entry.name().to_string();
        let rel = validate_zip_entry_path(&entry_name)
            .map_err(|e| format!("invalid zip entry `{entry_name}`: {e}"))?;
        let out_path = dest.join(&rel);
        let declared = entry.size();
        let mut out = File::create(&out_path).map_err(|e| format!("write `{entry_name}`: {e}"))?;
        let mut limited = LimitedZipReader {
            inner: &mut entry,
            entry_remaining: declared,
            _marker: std::marker::PhantomData,
        };
        std::io::copy(&mut limited, &mut out)
            .map_err(|e| format!("extract `{entry_name}`: {e}"))?;
        out.flush()
            .map_err(|e| format!("flush `{entry_name}`: {e}"))?;
    }
    Ok(())
}

/// 校验一个已解开的包目录：manifest 全量校验 + 逐域文件校验。返回预览
/// 信息（manifest + 各域条数）。
///
/// 跨域键冲突**合法**：内置字典自身就存在跨域重复键（en 的
/// `common.aiNotConfigured` 同时在 ai 与 core 域），运行时注册表的
/// `Object.assign` 合并语义是「后注册者胜」、内置全量合并按域文件字母序
/// 展开——包读取沿同一裁决（`manager::read_messages` 按字母序、后者胜），
/// 安装与内置对同一份源数据产出同一个生效值。
pub(crate) fn validate_pack_dir(dir: &Path) -> Result<LanguagePackInspection, String> {
    let manifest_path = dir.join(MANIFEST_FILE);
    if !manifest_path.is_file() {
        return Err("invalid language pack: missing manifest.json".to_string());
    }
    let manifest_text =
        fs::read_to_string(&manifest_path).map_err(|e| format!("read manifest.json: {e}"))?;
    let manifest: LanguagePackManifest = serde_json::from_str(&manifest_text)
        .map_err(|e| format!("invalid language pack manifest: {e}"))?;
    manifest.validate()?;

    let mut domains: Vec<LanguagePackDomainCount> = Vec::new();
    let mut entries: Vec<PathBuf> = fs::read_dir(dir)
        .map_err(|e| format!("read pack dir: {e}"))?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .collect();
    entries.sort();
    for path in entries {
        let name = match path.file_name().and_then(|n| n.to_str()) {
            Some(name) => name.to_string(),
            None => return Err("invalid language pack: non-UTF-8 file name".to_string()),
        };
        if name == MANIFEST_FILE {
            continue;
        }
        if !path.is_file() {
            return Err(format!(
                "invalid language pack: unexpected directory entry `{name}`"
            ));
        }
        let stem = name.strip_suffix(".json").unwrap_or(&name);
        if !LANGUAGE_PACK_DOMAINS.contains(&stem) {
            return Err(format!("invalid language pack: unexpected file `{name}`"));
        }
        let text = fs::read_to_string(&path).map_err(|e| format!("read `{name}`: {e}"))?;
        let map: BTreeMap<String, String> = serde_json::from_str(&text)
            .map_err(|e| format!("invalid language pack: invalid domain file `{name}`: {e}"))?;
        for (key, value) in &map {
            validate_message_key(key)?;
            validate_message_value(key, value)?;
        }
        domains.push(LanguagePackDomainCount {
            domain: stem.to_string(),
            message_count: map.len(),
        });
    }
    Ok(LanguagePackInspection { manifest, domains })
}

fn finalize_staged_package(
    staging: &Path,
    packs_dir: &Path,
) -> Result<LanguagePackInspection, String> {
    let inspection = validate_pack_dir(staging)?;
    let manifest = inspection.manifest.clone();
    let target = packs_dir.join(&manifest.code);
    let backup = packs_dir.join(format!("{}{BACKUP_SUFFIX}", manifest.code));

    if target.exists() {
        let _ = fs::remove_dir_all(&backup);
        fs::rename(&target, &backup).map_err(|e| format!("backup existing language pack: {e}"))?;
    }
    if let Err(e) = fs::rename(staging, &target) {
        // 安装失败：尽力恢复旧包。
        if backup.exists() {
            let _ = fs::rename(&backup, &target);
        }
        return Err(format!("install language pack: {e}"));
    }
    if backup.exists() {
        let _ = fs::remove_dir_all(&backup);
    }
    Ok(inspection)
}

#[cfg(test)]
#[path = "install/tests.rs"]
mod tests;
