//! The host's SQL snippets (T3): one JSON file under the data directory,
//! written atomically. The store owns the same vocabulary the memory port
//! serves in the demo process — create refuses a duplicate name, update and
//! delete refuse an unknown one — so the two stores answer alike.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// The file's row shape. The UI vocabulary type stays serde-free; the store
/// owns its serialization and maps at the boundary.
#[derive(serde::Serialize, serde::Deserialize)]
struct SnippetRow {
    name: String,
    sql: String,
}

impl From<SnippetRow> for datazen_ui_api::Snippet {
    fn from(row: SnippetRow) -> Self {
        Self { name: row.name, sql: row.sql }
    }
}

impl From<&datazen_ui_api::Snippet> for SnippetRow {
    fn from(snippet: &datazen_ui_api::Snippet) -> Self {
        Self { name: snippet.name.clone(), sql: snippet.sql.clone() }
    }
}

/// The snippet store: a JSON array of `{name, sql}`, insertion order on disk.
pub struct FileSnippets {
    path: PathBuf,
    state: Mutex<Vec<datazen_ui_api::Snippet>>,
}

impl FileSnippets {
    pub fn open(path: &Path) -> Result<Self, String> {
        let rows = match std::fs::read_to_string(path) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Vec::new(),
            Err(error) => return Err(format!("read {}: {error}", path.display())),
            Ok(text) if text.trim().is_empty() => Vec::new(),
            Ok(text) => serde_json::from_str::<Vec<SnippetRow>>(&text)
                .map_err(|error| format!("parse {}: {error}", path.display()))?,
        };
        Ok(Self {
            path: path.to_owned(),
            state: Mutex::new(rows.into_iter().map(Into::into).collect()),
        })
    }

    fn persist(&self, snippets: &[datazen_ui_api::Snippet]) -> Result<(), String> {
        let rows: Vec<SnippetRow> = snippets.iter().map(Into::into).collect();
        let text = serde_json::to_string_pretty(&rows)
            .map_err(|error| format!("serialize the snippets: {error}"))?;
        let temp = self.path.with_extension("json.tmp");
        std::fs::write(&temp, text).map_err(|error| error.to_string())?;
        std::fs::rename(&temp, &self.path).map_err(|error| error.to_string())?;
        Ok(())
    }

    /// The snippets in insertion order.
    pub fn list(&self) -> Vec<datazen_ui_api::Snippet> {
        self.state.lock().expect("the snippets lock").clone()
    }

    /// Keep the same lock through validation, disk replacement, and
    /// publication. Failed writes must not change what list() or a retry sees.
    fn commit(
        &self,
        propose: impl FnOnce(&mut Vec<datazen_ui_api::Snippet>) -> Result<(), String>,
    ) -> Result<(), String> {
        let mut current = self.state.lock().expect("the snippets lock");
        let mut next = current.clone();
        propose(&mut next)?;
        self.persist(&next)?;
        *current = next;
        Ok(())
    }

    /// Create a snippet; an existing name is refused.
    pub fn save(&self, name: &str, sql: &str) -> Result<(), String> {
        if name.trim().is_empty() {
            return Err("snippet name is required".into());
        }
        self.commit(|next| {
            if next.iter().any(|snippet| snippet.name == name) {
                return Err(format!("a snippet named {name} already exists"));
            }
            next.push(datazen_ui_api::Snippet { name: name.to_owned(), sql: sql.to_owned() });
            Ok(())
        })
    }

    /// Change a snippet's SQL; an unknown name is refused.
    pub fn update(&self, name: &str, sql: &str) -> Result<(), String> {
        self.commit(|next| {
            let snippet = next.iter_mut()
                .find(|snippet| snippet.name == name)
                .ok_or_else(|| format!("no snippet named {name}"))?;
            snippet.sql = sql.to_owned();
            Ok(())
        })
    }

    /// Delete a snippet; an unknown name is refused.
    pub fn delete(&self, name: &str) -> Result<(), String> {
        self.commit(|next| {
            let before = next.len();
            next.retain(|snippet| snippet.name != name);
            if next.len() == before {
                return Err(format!("no snippet named {name}"));
            }
            Ok(())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_path(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("datazen-rune-snippets-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir.join("snippets.json")
    }

    #[test]
    fn snippets_round_trip_through_the_file() {
        let path = temp_path("round-trip");
        let store = FileSnippets::open(&path).unwrap();
        store.save("top customers", "SELECT * FROM customers ORDER BY spend DESC").unwrap();
        assert!(store.save("top customers", "SELECT 1").is_err(), "a duplicate name is refused");
        store.save("by city", "SELECT city, count(*) FROM customers GROUP BY city").unwrap();
        store.update("by city", "SELECT city, count(*) AS n FROM customers GROUP BY city").unwrap();
        assert!(store.update("no such", "SELECT 1").is_err(), "an unknown name is refused");

        // Through a fresh store over the same file: persistence.
        let store = FileSnippets::open(&path).unwrap();
        let snippets = store.list();
        assert_eq!(snippets.len(), 2);
        assert_eq!(snippets[0].name, "top customers");
        assert_eq!(
            snippets[1].sql,
            "SELECT city, count(*) AS n FROM customers GROUP BY city",
            "the update survived the reopen"
        );

        store.delete("top customers").unwrap();
        assert!(store.delete("top customers").is_err(), "a second delete is refused");
        let store = FileSnippets::open(&path).unwrap();
        assert_eq!(store.list().len(), 1, "the delete survived the reopen");
    }

    #[test]
    fn failed_create_update_and_delete_leave_the_live_and_disk_snapshots_unchanged() {
        let path = temp_path("failed-writes");
        let sentinel = b"[ { \"name\": \"Existing snippet\", \"sql\": \"SELECT 1;\" } ]\n";
        std::fs::write(&path, sentinel).unwrap();
        let store = FileSnippets::open(&path).unwrap();
        let original = store.list();
        assert_eq!(original.len(), 1);

        // A directory at the temporary file path blocks the actual write
        // portably, including under an account that ignores file modes.
        let obstruction = path.with_extension("json.tmp");
        std::fs::create_dir(&obstruction).unwrap();
        let assert_unchanged = || {
            assert_eq!(store.list(), original, "no phantom in-memory change");
            assert_eq!(std::fs::read(&path).unwrap().as_slice(), &sentinel[..], "original bytes preserved");
            assert_eq!(FileSnippets::open(&path).unwrap().list(), original);
        };
        assert!(store.save("New snippet", "SELECT 3;").is_err());
        assert_unchanged();
        assert!(store.update("Existing snippet", "SELECT 2;").is_err());
        assert_unchanged();
        assert!(store.delete("Existing snippet").is_err());
        assert_unchanged();

        std::fs::remove_dir(&obstruction).unwrap();
        store.save("New snippet", "SELECT 3;").unwrap();
        store.update("Existing snippet", "SELECT 2;").unwrap();
        let updated = store.list();
        assert_eq!(updated[0].sql, "SELECT 2;");
        assert_eq!(updated[1].name, "New snippet", "insertion order survives retry");
        assert_eq!(FileSnippets::open(&path).unwrap().list(), updated);
        store.delete("Existing snippet").unwrap();
        assert_eq!(store.list().as_slice(), &updated[1..]);
        assert_eq!(FileSnippets::open(&path).unwrap().list().as_slice(), &updated[1..]);
    }

    #[test]
    fn an_empty_or_missing_file_starts_empty() {
        let path = temp_path("empty");
        let store = FileSnippets::open(&path).unwrap();
        assert!(store.list().is_empty(), "a missing file is an empty store");
        std::fs::write(&path, "   ").unwrap();
        let store = FileSnippets::open(&path).unwrap();
        assert!(store.list().is_empty(), "a blank file is an empty store");
    }
}
