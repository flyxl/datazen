//! File-first storage for SQL favorites.
//!
//! Favorites live as one self-describing `.sql` file each, under a configurable
//! root (see `AppSettings::favorites_root`). The rationale, restated as the
//! implemented fact: the unit a user exchanges between devices must be
//! readable, diffable, and mergeable,
//! and a directory tree of text files is the only storage shape where all three
//! hold without writing a single line of sync code.
//!
//! The store keeps no authoritative index. A panel open recursively scans the
//! root, parses each file's front-matter ([`frontmatter`]), caches the result in
//! memory, and filters by `connectionId`. A few hundred files read instantly,
//! so there is no persistent index to drift out of sync with the directory.

pub mod frontmatter;
pub mod migrate;
pub mod ulid;

#[cfg(test)]
mod tests;

use std::path::{Path, PathBuf};
use std::sync::RwLock;

use chrono::{DateTime, TimeZone, Utc};

use super::models::FavoriteQuery;
use frontmatter::{FrontMatter, ParsedFile};
use ulid::Ulid;

/// Only this extension is read or written. AGENTS.md requires an extension
/// allow-list for anything that touches the user's file system.
const FAVORITE_EXTENSION: &str = "sql";

/// Sub-directory that receives deleted favorites instead of unlinking them.
///
/// A favorite is frequently the only copy of a query the user has (the SQL
/// editor keeps no persistence of its own), so "delete" must never mean "the
/// bytes are gone". See the plan §2.5 rule 1.
const TRASH_DIR: &str = ".trash";

/// Longest display title synthesized for a file with no `title` field.
const DERIVED_TITLE_MAX: usize = 60;

/// Bounds one walk of the favorites tree. Deeper trees are still fully loaded,
/// just reported so a pathological directory cannot silently hide data.
const MAX_SCAN_DEPTH: usize = 16;

#[derive(Debug, thiserror::Error)]
pub enum FavoritesError {
    #[error("Favorites root is not a directory: {0}")]
    RootNotADirectory(String),

    #[error("Favorite not found: {0}")]
    NotFound(String),

    #[error("Unsafe favorite identifier: {0}")]
    UnsafeId(String),

    #[error("Favorites IO error: {0}")]
    Io(String),

    #[error("Favorites migration error: {0}")]
    Migration(String),
}

/// Fields a caller supplies when saving a new favorite.
#[derive(Debug, Clone)]
pub struct NewFavorite {
    pub connection_id: String,
    pub title: String,
    pub sql: String,
    pub database: Option<String>,
    pub keyword: Option<String>,
}

/// Result of a directory scan: the parsed favorites plus anything unreadable.
///
/// Unreadable files are collected rather than dropped so the caller can log
/// them — a favorite the user hand-edited into an unparseable state must be
/// visible as a problem, not vanish.
#[derive(Debug, Default)]
pub struct ScanResult {
    pub favorites: Vec<FavoriteQuery>,
    pub skipped: Vec<String>,
}

/// Directory-backed favorites store. Cheap to construct, cheap to clone-share.
pub struct FavoritesStore {
    /// Re-writable so a settings change can repoint the store at a synced
    /// directory without a restart.
    root: RwLock<PathBuf>,
    /// `None` = not scanned yet. Invalidated by root changes and mutated in
    /// place by add/delete so a save does not cost a full re-read.
    cache: RwLock<Option<Vec<FavoriteQuery>>>,
}

impl FavoritesStore {
    /// Open (creating if absent) a favorites store rooted at `root`.
    pub fn open(root: &Path) -> Result<Self, FavoritesError> {
        create_root(root)?;
        Ok(Self {
            root: RwLock::new(root.to_path_buf()),
            cache: RwLock::new(None),
        })
    }

    /// Effective favorites root.
    pub fn root(&self) -> PathBuf {
        self.root
            .read()
            .map(|g| g.clone())
            .unwrap_or_else(|poisoned| poisoned.into_inner().clone())
    }

    /// Repoint at a new root. Any cached listing belongs to the old tree.
    pub fn set_root(&self, root: &Path) -> Result<(), FavoritesError> {
        create_root(root)?;
        let mut guard = self
            .root
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        *guard = root.to_path_buf();
        self.invalidate_cache();
        Ok(())
    }

    /// Drop the in-memory listing so the next read re-scans the directory.
    pub fn invalidate_cache(&self) {
        let mut guard = self
            .cache
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        *guard = None;
    }

    /// List favorites, optionally restricted to one connection.
    ///
    /// Returns newest first, matching the ordering the SQLite implementation
    /// used (`ORDER BY created_at DESC`) so the panel does not silently
    /// re-sort itself under users who already have a curated list.
    pub fn list(&self, connection_id: Option<&str>) -> Vec<FavoriteQuery> {
        let all = self.all();
        let mut out: Vec<FavoriteQuery> = all
            .iter()
            .filter(|f| connection_id.is_none_or(|cid| f.connection_id == cid))
            .cloned()
            .collect();
        sort_newest_first(&mut out);
        out
    }

    /// Every favorite on disk, scanning once and caching in memory.
    pub fn all(&self) -> Vec<FavoriteQuery> {
        let mut guard = self
            .cache
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some(cached) = guard.as_ref() {
            return cached.clone();
        }
        let result = scan(&self.root());
        if !result.skipped.is_empty() {
            for path in &result.skipped {
                tracing::warn!(path = %path, "Skipping unreadable favorite file");
            }
        }
        *guard = Some(result.favorites.clone());
        result.favorites
    }

    /// Write a new favorite and return it, including the id it was given.
    pub fn add(&self, draft: NewFavorite) -> Result<FavoriteQuery, FavoritesError> {
        let id = ulid::new_ulid().to_string();
        let now = Utc::now();
        let favorite = FavoriteQuery {
            id: id.clone(),
            connection_id: draft.connection_id,
            title: draft.title,
            sql: draft.sql,
            created_at: now,
            updated_at: Some(now),
            keyword: draft.keyword,
            database: draft.database,
            folder: None,
        };
        self.write_file(&favorite, &id)?;
        self.cache_push(favorite.clone());
        Ok(favorite)
    }

    /// Move a favorite into `.trash/`. The bytes survive; the panel stops
    /// listing the entry.
    pub fn delete(&self, id: &str) -> Result<(), FavoritesError> {
        let path = self.resolve_file(id)?;
        if !path.is_file() {
            return Err(FavoritesError::NotFound(id.to_string()));
        }
        let root = self.root();
        let trash_dir = root.join(TRASH_DIR);
        std::fs::create_dir_all(&trash_dir).map_err(|e| {
            FavoritesError::Io(format!(
                "cannot create trash dir {}: {e}",
                trash_dir.display()
            ))
        })?;
        let target = trash_dir.join(trash_file_name(id, Utc::now()));
        move_file(&path, &target)?;
        self.cache_remove(id);
        Ok(())
    }

    /// Render a favorite to its on-disk representation.
    pub fn render_file(favorite: &FavoriteQuery) -> String {
        let mut fm = FrontMatter::new();
        fm.set("title", &favorite.title);
        fm.set("connectionId", &favorite.connection_id);
        fm.set_some("database", favorite.database.as_deref());
        fm.set_some("keyword", favorite.keyword.as_deref());
        fm.set("createdAt", &favorite.created_at.to_rfc3339());
        fm.set_some(
            "updatedAt",
            favorite.updated_at.map(|t| t.to_rfc3339()).as_deref(),
        );
        ParsedFile {
            front_matter: fm,
            body: favorite.sql.clone(),
        }
        .render()
    }

    /// Write `favorite` under its own id, overwriting any existing file.
    ///
    /// Used by the SQLite migration, which owns the id it assigns. Because
    /// that id is derived from the legacy row, a retried export overwrites its
    /// own partial output instead of duplicating it.
    pub(super) fn import(&self, favorite: &FavoriteQuery) -> Result<(), FavoritesError> {
        self.write_file(favorite, &favorite.id)?;
        self.invalidate_cache();
        Ok(())
    }

    /// Write `favorite` under `id`, creating parent folders as needed.
    fn write_file(&self, favorite: &FavoriteQuery, id: &str) -> Result<(), FavoritesError> {
        let path = self.resolve_file(id)?;
        let parent = path.parent().ok_or_else(|| {
            FavoritesError::Io(format!("favorite path {} has no parent", path.display()))
        })?;
        std::fs::create_dir_all(parent)
            .map_err(|e| FavoritesError::Io(format!("cannot create {}: {e}", parent.display())))?;
        write_atomic(&path, Self::render_file(favorite).as_bytes())
    }

    /// Turn an id into a path inside the root.
    ///
    /// The id is the only user-influenced component of a favorites path, so it
    /// is validated against a strict allow-list: ASCII alphanumerics plus `-`
    /// and `_`, length bounded, and never `.` or `..`. The app itself only
    /// ever mints ULIDs, which fall inside this set — the wider set exists so
    /// a favorite the user renamed in Finder stays editable.
    fn resolve_file(&self, id: &str) -> Result<PathBuf, FavoritesError> {
        if !is_safe_stem(id) {
            return Err(FavoritesError::UnsafeId(id.to_string()));
        }
        Ok(self.root().join(id).with_extension(FAVORITE_EXTENSION))
    }

    fn cache_push(&self, favorite: FavoriteQuery) {
        let mut guard = self
            .cache
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some(cached) = guard.as_mut() {
            cached.retain(|f| f.id != favorite.id);
            cached.push(favorite);
        }
    }

    fn cache_remove(&self, id: &str) {
        let mut guard = self
            .cache
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some(cached) = guard.as_mut() {
            cached.retain(|f| f.id != id);
        }
    }
}

/// Create the favorites root if it is not there yet.
///
/// A path that exists and is *not* a directory gets its own error rather than a
/// generic IO one: it is the one configuration mistake worth naming in a log —
/// `favoritesRoot` pointed at a file, usually a stale `favorites` placeholder
/// from the JSON store this design replaced.
fn create_root(root: &Path) -> Result<(), FavoritesError> {
    if root.is_file() {
        return Err(FavoritesError::RootNotADirectory(
            root.display().to_string(),
        ));
    }
    std::fs::create_dir_all(root).map_err(|e| {
        FavoritesError::Io(format!(
            "cannot create favorites root {}: {e}",
            root.display()
        ))
    })
}

/// Newest first, with the ULID as a tie-break so equal timestamps keep a
/// stable order across reloads.
fn sort_newest_first(items: &mut [FavoriteQuery]) {
    items.sort_by(|a, b| {
        b.created_at
            .cmp(&a.created_at)
            .then_with(|| b.id.cmp(&a.id))
    });
}

/// Device names Windows refuses to use as a file name, with or without an
/// extension. On a synced folder these become un-openable on a colleague's
/// machine, so they are rejected on every platform rather than only on Windows.
const RESERVED_STEMS: [&str; 22] = [
    "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
    "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
];

/// Whether a file name stem is safe to turn into a path.
fn is_safe_stem(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 64
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        && !RESERVED_STEMS
            .iter()
            .any(|reserved| reserved.eq_ignore_ascii_case(s))
}

/// Recursively read every `.sql` favorite under `root`.
pub fn scan(root: &Path) -> ScanResult {
    let mut out = ScanResult::default();
    walk(root, 0, None, &mut out);
    sort_newest_first(&mut out.favorites);
    out
}

fn walk(dir: &Path, depth: usize, rel: Option<&str>, out: &mut ScanResult) {
    if depth > MAX_SCAN_DEPTH {
        out.skipped.push(dir.display().to_string());
        return;
    }
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        // A missing directory is an empty store, not a failure.
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return,
        Err(e) => {
            out.skipped.push(format!("{}: {e}", dir.display()));
            return;
        }
    };

    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(name) = name.to_str() else { continue };
        // Dot entries carry no favorites: `.trash`, and the `.{name}.{pid}..tmp`
        // files an interrupted atomic write can leave behind.
        if name.starts_with('.') {
            continue;
        }
        let path = entry.path();
        // `symlink_metadata` — never follow links. A symlink pointing at an
        // ancestor would otherwise make this walk loop forever, and following
        // one would read files the user never put in their favorites root.
        let Ok(meta) = entry.path().symlink_metadata() else {
            continue;
        };

        if meta.is_dir() {
            let child_rel = match rel {
                Some(parent) => format!("{parent}/{name}"),
                None => name.to_string(),
            };
            walk(&path, depth + 1, Some(&child_rel), out);
            continue;
        }
        if !meta.is_file() || path.extension().and_then(|e| e.to_str()) != Some(FAVORITE_EXTENSION)
        {
            continue;
        }
        match read_favorite(&path, rel.as_deref()) {
            Ok(favorite) => out.favorites.push(favorite),
            Err(e) => out.skipped.push(format!("{}: {e}", path.display())),
        }
    }
}

/// Parse one file into a favorite, or explain why it cannot be one.
fn read_favorite(path: &Path, folder: Option<&str>) -> Result<FavoriteQuery, FavoritesError> {
    let content = std::fs::read_to_string(path)
        .map_err(|e| FavoritesError::Io(format!("cannot read {}: {e}", path.display())))?;
    let parsed = ParsedFile::parse(&content);
    let fm = &parsed.front_matter;

    let stem = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or_default()
        .to_string();

    Ok(FavoriteQuery {
        id: stem.clone(),
        title: resolve_title(fm, &parsed.body, &stem),
        connection_id: fm.get_non_empty("connectionId").unwrap_or("").to_string(),
        sql: parsed.body,
        created_at: resolve_created_at(fm, &stem, path),
        updated_at: parse_timestamp(fm.get("updatedAt")),
        keyword: fm.get_non_empty("keyword").map(str::to_string),
        database: fm.get_non_empty("database").map(str::to_string),
        folder: folder.map(str::to_string),
    })
}

/// `title` when present, else the first meaningful line of SQL, else the file
/// name. Only ever affects display — reading never rewrites the file.
fn resolve_title(fm: &FrontMatter, body: &str, stem: &str) -> String {
    if let Some(title) = fm.get_non_empty("title") {
        return title.to_string();
    }
    let first = body
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty() && !l.starts_with("--"))
        .unwrap_or(stem);
    let collapsed: String = first.split_whitespace().collect::<Vec<_>>().join(" ");
    if collapsed.is_empty() {
        stem.to_string()
    } else {
        collapsed.chars().take(DERIVED_TITLE_MAX).collect()
    }
}

/// `createdAt` when present and valid, else the ULID timestamp encoded in the
/// file name, else the file's modification time.
fn resolve_created_at(fm: &FrontMatter, stem: &str, path: &Path) -> DateTime<Utc> {
    if let Some(at) = parse_timestamp(fm.get("createdAt")) {
        return at;
    }
    if let Ok(ulid) = Ulid::parse(stem) {
        if let Some(at) = Utc
            .timestamp_millis_opt(i64::try_from(ulid.timestamp_ms()).unwrap_or(i64::MAX))
            .single()
        {
            return at;
        }
    }
    std::fs::metadata(path)
        .and_then(|m| m.modified())
        .map(DateTime::<Utc>::from)
        .unwrap_or_else(|_| DateTime::<Utc>::from(std::time::UNIX_EPOCH))
}

fn parse_timestamp(raw: Option<&str>) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(raw?)
        .ok()
        .map(|t| t.with_timezone(&Utc))
}

/// `2026-08-18T10-00-00__01J8XK2M9Q7B4F.sql` — sortable by deletion time,
/// still carrying the id so the original name is recoverable from the trash.
fn trash_file_name(id: &str, at: DateTime<Utc>) -> String {
    let stamp = at.format("%Y-%m-%dT%H-%M-%S").to_string();
    let millis = at.timestamp_subsec_millis();
    format!("{stamp}-{millis:03}__{id}.{FAVORITE_EXTENSION}")
}

/// Write `content` to `path` via a temp file and a rename, so a crash leaves
/// either the old file or the new one — never a half-written favorite.
pub fn write_atomic(path: &Path, content: &[u8]) -> Result<(), FavoritesError> {
    use std::sync::atomic::{AtomicU64, Ordering};
    static SEQ: AtomicU64 = AtomicU64::new(0);

    let parent = path.parent().ok_or_else(|| {
        FavoritesError::Io(format!("path {} has no parent directory", path.display()))
    })?;
    let file_name = path
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| FavoritesError::Io(format!("invalid file name in {}", path.display())))?;
    let seq = SEQ.fetch_add(1, Ordering::Relaxed);
    let tmp = parent.join(format!(".{file_name}.{}.{seq}.tmp", std::process::id()));

    std::fs::write(&tmp, content)
        .map_err(|e| FavoritesError::Io(format!("cannot write {}: {e}", tmp.display())))?;
    match std::fs::rename(&tmp, path) {
        Ok(()) => Ok(()),
        Err(e) => {
            // The temp file must not outlive a failed write.
            let _ = std::fs::remove_file(&tmp);
            Err(FavoritesError::Io(format!(
                "cannot replace {}: {e}",
                path.display()
            )))
        }
    }
}

/// Rename, falling back to copy+delete if the two paths end up on different
/// file systems (a favorites root moved onto a mounted volume).
fn move_file(from: &Path, to: &Path) -> Result<(), FavoritesError> {
    match std::fs::rename(from, to) {
        Ok(()) => Ok(()),
        Err(_) => {
            std::fs::copy(from, to).map_err(|e| {
                FavoritesError::Io(format!(
                    "cannot move {} to {}: {e}",
                    from.display(),
                    to.display()
                ))
            })?;
            std::fs::remove_file(from)
                .map_err(|e| FavoritesError::Io(format!("cannot remove {}: {e}", from.display())))
        }
    }
}
