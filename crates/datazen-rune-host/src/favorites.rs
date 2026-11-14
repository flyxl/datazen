//! The host's SQL favorites (T6): one self-describing `.sql` file per
//! favorite under the favorites root, the same file format the desktop app
//! reads and writes (`crates/datazen-desktop/src/store/favorites`): a
//! contiguous `-- key: value` front-matter block at the top, the statement
//! below it, so the file stays runnable as-is. The id is the file name's
//! stem — a ULID the store mints; a rename changes the `title` in the
//! front-matter, never the file; a delete moves the bytes into `.trash/`.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use datazen_ui_api::FavoriteSpec;

/// Only this extension is read or written.
const FAVORITE_EXTENSION: &str = "sql";
/// Deleted favorites land here instead of being unlinked: a favorite is
/// often the only copy of a query the user has.
const TRASH_DIR: &str = ".trash";
/// Bounds one walk of the favorites tree.
const MAX_SCAN_DEPTH: usize = 16;
/// Crockford base32: digits plus uppercase letters, minus `I`, `L`, `O`, `U`.
const ALPHABET: &[u8; 32] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";

/// The favorite's own metadata as the file carries it.
#[derive(Debug, Clone)]
struct StoredFavorite {
    spec: FavoriteSpec,
}

/// The favorites store: the root directory is the truth; every write lands
/// in a file, every read walks the tree.
pub struct FileFavorites {
    root: PathBuf,
    /// The last minted ULID, so ids inside one millisecond still increase.
    last_ulid: Mutex<[u8; 26]>,
}

impl FileFavorites {
    /// Open (and create) the favorites root. An unreadable root is an error;
    /// a fresh one is an empty store.
    pub fn open(root: &Path) -> Result<Self, String> {
        std::fs::create_dir_all(root).map_err(|error| {
            format!("cannot create the favorites root {}: {error}", root.display())
        })?;
        Ok(Self { root: root.to_owned(), last_ulid: Mutex::new([b'0'; 26]) })
    }

    /// Every favorite under the root, newest first (the ULID's high bits are
    /// the creation time, so file name order is creation order).
    pub fn list(&self) -> Vec<FavoriteSpec> {
        let mut out = Vec::new();
        let mut skipped = Vec::new();
        walk(&self.root, 0, &mut out, &mut skipped);
        out.sort_by(|a, b| b.spec.id.cmp(&a.spec.id));
        out.into_iter().map(|stored| stored.spec).collect()
    }

    /// Save a new favorite and return its id.
    pub fn add(&self, connection: &str, title: &str, sql: &str) -> Result<String, String> {
        let title = title.trim();
        if title.is_empty() {
            return Err("a favorite needs a title".into());
        }
        if sql.trim().is_empty() {
            return Err("a favorite needs a statement".into());
        }
        let id = self.mint_ulid();
        let file = render_favorite(connection, title, sql);
        let path = self.file_of(&id)?;
        write_atomic(&path, file.as_bytes())?;
        Ok(id)
    }

    /// Change a favorite's title. The file's name — its identity — does not
    /// change; the front-matter does.
    pub fn rename(&self, id: &str, title: &str) -> Result<(), String> {
        let title = title.trim();
        if title.is_empty() {
            return Err("a favorite needs a title".into());
        }
        let path = self.file_of(id)?;
        let content = std::fs::read_to_string(&path)
            .map_err(|error| format!("read {}: {error}", path.display()))?;
        let parsed = parse_file(&content);
        let connection = parsed.get("connectionId").unwrap_or_default().to_owned();
        let sql = parsed.body;
        let file = render_favorite(&connection, title, &sql);
        write_atomic(&path, file.as_bytes())
    }

    /// Delete a favorite: the bytes move into `.trash/`, the panel stops
    /// listing the entry.
    pub fn delete(&self, id: &str) -> Result<(), String> {
        let path = self.file_of(id)?;
        if !path.is_file() {
            return Err(format!("no favorite {id}"));
        }
        let trash_dir = self.root.join(TRASH_DIR);
        std::fs::create_dir_all(&trash_dir)
            .map_err(|error| format!("cannot create {}: {error}", trash_dir.display()))?;
        let now = chrono::Utc::now();
        let stamp = now.format("%Y-%m-%dT%H-%M-%S");
        let target = trash_dir
            .join(format!("{stamp}-{:03}__{id}.{FAVORITE_EXTENSION}", now.timestamp_subsec_millis()));
        std::fs::rename(&path, &target)
            .map_err(|error| format!("cannot move {}: {error}", path.display()))
    }

    /// The file a favorite id names. The id is the only caller-influenced
    /// path component, so it gets the desktop's strict allow-list.
    fn file_of(&self, id: &str) -> Result<PathBuf, String> {
        if !is_safe_stem(id) {
            return Err(format!("unsafe favorite id {id}"));
        }
        Ok(self.root.join(format!("{id}.{FAVORITE_EXTENSION}")))
    }

    /// Mint a ULID: the 48-bit millisecond timestamp in the high bits, 80
    /// random bits below, monotonic inside one millisecond. The randomness
    /// comes from a v4 UUID's bytes — the host already links the crate, and
    /// the format is the desktop's.
    fn mint_ulid(&self) -> String {
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_millis() as u64)
            .unwrap_or(0)
            .min((1u64 << 48) - 1);
        let mut guard = self.last_ulid.lock().expect("the ULID lock");
        loop {
            let uuid = *uuid::Uuid::new_v4().as_bytes();
            let mut random = [0u8; 10];
            random.copy_from_slice(&uuid[..10]);
            let id = encode_ulid(now_ms, &random);
            // Strictly greater than the last one issued in the same
            // millisecond; a fresh millisecond always wins.
            let bytes = id.as_bytes().try_into().expect("26 characters");
            if bytes > *guard {
                *guard = bytes;
                return id;
            }
        }
    }
}

/// Encode timestamp and randomness as the canonical 26-character Crockford
/// base32 ULID.
fn encode_ulid(timestamp_ms: u64, randomness: &[u8; 10]) -> String {
    let mut out = String::with_capacity(26);
    let ts = u128::from(timestamp_ms);
    for i in (0..10).rev() {
        out.push(ALPHABET[((ts >> (5 * i)) & 0x1F) as usize] as char);
    }
    let mut rnd = 0u128;
    for byte in randomness {
        rnd = (rnd << 8) | u128::from(*byte);
    }
    for i in (0..16).rev() {
        out.push(ALPHABET[((rnd >> (5 * i)) & 0x1F) as usize] as char);
    }
    out
}

/// The desktop's id allow-list: ASCII alphanumerics plus `-` and `_`, length
/// bounded — the app only mints ULIDs, the wider set keeps a hand-renamed
/// file editable.
fn is_safe_stem(stem: &str) -> bool {
    !stem.is_empty()
        && stem.len() <= 64
        && stem
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        && !stem.eq_ignore_ascii_case(TRASH_DIR)
}

/// One walk of the favorites tree. Unreadable files are skipped, not fatal.
fn walk(dir: &Path, depth: usize, out: &mut Vec<StoredFavorite>, skipped: &mut Vec<String>) {
    if depth > MAX_SCAN_DEPTH {
        skipped.push(dir.display().to_string());
        return;
    }
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') {
            continue; // `.trash` and dotfiles are not favorites
        }
        if path.is_dir() {
            walk(&path, depth + 1, out, skipped);
            continue;
        }
        if path.extension().and_then(|ext| ext.to_str()) != Some(FAVORITE_EXTENSION) {
            continue;
        }
        let stem = path.file_stem().and_then(|stem| stem.to_str()).unwrap_or("");
        if !is_safe_stem(stem) {
            continue;
        }
        let content = match std::fs::read_to_string(&path) {
            Ok(content) => content,
            Err(_) => {
                skipped.push(path.display().to_string());
                continue;
            }
        };
        let parsed = parse_file(&content);
        let connection = parsed.get("connectionId").unwrap_or_default().to_owned();
        let title = parsed
            .get("title")
            .filter(|title| !title.trim().is_empty())
            .map(str::to_owned)
            .unwrap_or_else(|| stem.to_owned());
        out.push(StoredFavorite {
            spec: FavoriteSpec {
                id: stem.to_owned(),
                connection,
                title,
                sql: parsed.body,
            },
        });
    }
}

/// A parsed `.sql` favorite: the front-matter fields plus the statement body.
struct ParsedFavorite {
    fields: Vec<(String, String)>,
    body: String,
}

impl ParsedFavorite {
    fn get(&self, key: &str) -> Option<&str> {
        self.fields
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.as_str())
    }
}

/// Split a favorite file into its `-- key: value` block and its SQL body.
/// The block is the contiguous run of metadata lines at the very top; the
/// first line that is not one ends it. Values are escaped so they can never
/// break out of their line.
fn parse_file(content: &str) -> ParsedFavorite {
    let content = content.strip_prefix('\u{feff}').unwrap_or(content);
    let mut fields = Vec::new();
    let mut consumed = 0usize;
    for line in content.split_inclusive('\n') {
        match parse_metadata_line(line) {
            Some((key, value)) => {
                fields.retain(|(k, _)| k != &key);
                fields.push((key, value));
            }
            None => break,
        }
        consumed += 1;
    }
    let body = content
        .split_inclusive('\n')
        .skip(consumed)
        .collect::<String>();
    ParsedFavorite { fields, body }
}

/// Render a favorite to its on-disk form: front-matter block, statement, one
/// trailing newline.
fn render_favorite(connection: &str, title: &str, sql: &str) -> String {
    let mut out = String::new();
    let mut line = |key: &str, value: &str| {
        out.push_str("-- ");
        out.push_str(key);
        out.push_str(": ");
        out.push_str(&escape_value(value));
        out.push('\n');
    };
    line("title", title);
    line("connectionId", connection);
    let now = chrono::Utc::now().to_rfc3339();
    line("createdAt", &now);
    line("updatedAt", &now);
    out.push_str(sql);
    if !out.ends_with('\n') {
        out.push('\n');
    }
    out
}

/// Parse one `-- key: value` line; `None` means the line is not metadata.
fn parse_metadata_line(line: &str) -> Option<(String, String)> {
    let trimmed = line.trim_end_matches(['\r', '\n']);
    let rest = trimmed.strip_prefix("--")?;
    let rest = rest.trim_start_matches([' ', '\t']);
    let (key, value) = rest.split_once(':')?;
    let key = key.trim();
    if key.is_empty() || !is_metadata_key(key) {
        return None;
    }
    Some((key.to_string(), unescape_value(value.trim())))
}

/// Metadata keys are plain identifiers; anything else is user prose.
fn is_metadata_key(key: &str) -> bool {
    let mut chars = key.chars();
    matches!(chars.next(), Some(c) if c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

/// Escape a value so it can never break out of its `-- key: value` line.
fn escape_value(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for ch in value.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            other => out.push(other),
        }
    }
    out
}

/// The escape's inverse: a lone backslash is user text and stays verbatim.
fn unescape_value(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let mut chars = value.chars();
    while let Some(ch) = chars.next() {
        if ch != '\\' {
            out.push(ch);
            continue;
        }
        match chars.next() {
            Some('n') => out.push('\n'),
            Some('r') => out.push('\r'),
            Some('\\') => out.push('\\'),
            Some(other) => {
                out.push('\\');
                out.push(other);
            }
            None => out.push('\\'),
        }
    }
    out
}

/// Write `content` to `path` via a temp file and a rename, so a crash leaves
/// either the old file or the new one, never a torn one.
fn write_atomic(path: &Path, content: &[u8]) -> Result<(), String> {
    let temp = path.with_extension("sql.tmp");
    std::fs::write(&temp, content).map_err(|error| error.to_string())?;
    std::fs::rename(&temp, path).map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_root(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "datazen-rune-favorites-{tag}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn the_file_shape_is_the_desktops() {
        let root = temp_root("shape");
        let store = FileFavorites::open(&root).unwrap();
        let id = store.add("live", "top spenders", "SELECT * FROM users").unwrap();

        let path = root.join(format!("{id}.sql"));
        let body = std::fs::read_to_string(&path).unwrap();
        assert!(body.starts_with("-- title: top spenders\n"), "title first: {body}");
        assert!(body.contains("-- connectionId: live\n"), "connection: {body}");
        assert!(
            body.lines().take(4).all(|line| line.starts_with("-- ")),
            "the metadata block is contiguous at the top: {body}"
        );
        assert!(body.ends_with("SELECT * FROM users\n"), "the statement and one trailing newline: {body}");

        // The id is a 26-character Crockford ULID: file name order is
        // creation order.
        assert_eq!(id.len(), 26);
        assert!(id.bytes().all(|b| ALPHABET.contains(&b)), "Crockford base32: {id}");

        // Two saves inside one millisecond mint increasing ids.
        let second = store.add("live", "another", "SELECT 1").unwrap();
        assert!(second > id, "monotonic inside a millisecond: {id} -> {second}");
    }

    #[test]
    fn list_rename_delete_round_trip_and_the_file_stays() {
        let root = temp_root("round-trip");
        let store = FileFavorites::open(&root).unwrap();
        let id = store.add("live", "first title", "SELECT 1").unwrap();
        assert_eq!(store.list().len(), 1);

        // Rename: the title moves, the file name does not.
        store.rename(&id, "renamed").unwrap();
        let path = root.join(format!("{id}.sql"));
        assert!(path.is_file(), "the id file is still there");
        assert!(!root.join("renamed.sql").exists(), "no second file appeared");
        let listed = store.list();
        assert_eq!(listed[0].title, "renamed");
        assert_eq!(listed[0].sql, "SELECT 1\n", "the statement survived the rename");
        assert_eq!(listed[0].id, id);

        // A fresh store over the same root reads the same favorite.
        let store = FileFavorites::open(&root).unwrap();
        assert_eq!(store.list()[0].title, "renamed");

        // Delete: the bytes move to trash, the listing stops.
        store.delete(&id).unwrap();
        assert!(store.list().is_empty());
        let trash = root.join(TRASH_DIR);
        let trashed: Vec<_> = std::fs::read_dir(&trash)
            .unwrap()
            .flatten()
            .map(|entry| entry.path())
            .collect();
        assert_eq!(trashed.len(), 1, "the bytes survive in trash");
        let trashed_name = trashed[0].file_name().unwrap().to_string_lossy().into_owned();
        assert!(
            trashed_name.ends_with(&format!("__{id}.sql")),
            "the trash entry carries the id: {trashed_name}"
        );
        let bytes = std::fs::read_to_string(&trashed[0]).unwrap();
        assert!(bytes.contains("renamed"), "the trashed file keeps the content");
    }

    #[test]
    fn a_hand_written_file_loads_verbatim() {
        let root = temp_root("hand-written");
        std::fs::create_dir_all(&root).unwrap();
        // No front-matter, and a comment line where metadata could be: the
        // body is the whole file.
        std::fs::write(root.join("plain.sql"), "-- just a note\nSELECT 2\n").unwrap();
        // A title with a line break must not spill into the statement.
        let store = FileFavorites::open(&root).unwrap();
        store.add("live", "odd\\nnot a newline", "SELECT 3").unwrap();

        let listed = store.list();
        let plain = listed.iter().find(|f| f.id == "plain").expect("plain.sql lists");
        assert_eq!(plain.sql, "-- just a note\nSELECT 2\n");
        assert_eq!(plain.title, "plain", "a file with no title derives it from the stem");
        let odd = listed.iter().find(|f| f.title == "odd\\nnot a newline").expect("the escaped title lists");
        assert_eq!(odd.sql, "SELECT 3\n");
    }

    #[test]
    fn unsafe_ids_are_refused() {
        let root = temp_root("unsafe");
        let store = FileFavorites::open(&root).unwrap();
        assert!(store.delete("../escape").is_err());
        assert!(store.rename("no such", "x").is_err());
        assert!(store.add("live", " ", "SELECT 1").is_err(), "a blank title is refused");
        assert!(store.add("live", "t", "  ").is_err(), "a blank statement is refused");
    }
}
