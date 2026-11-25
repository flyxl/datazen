//! T060: the `AiCodeBlock` state and pure helpers (`ai_code_block.rs`
//! renders them).
//!
//! Everything here is host-independent: the SQL-dialect union, the
//! `isSqlCodeBlock` / `looksLikeSql` detection (`lib/aiMessageBlocks.ts`),
//! the header label, the `AiCodeBlockProps`, and the
//! [`AiCodeBlockState`] that holds React's `useState` pieces plus the
//! `useCopyFeedback` confirmation. The component in
//! [`super::ai_code_block`] only builds the tree from these and the
//! host's callbacks.

use std::rc::Rc;

/// How long the copied confirmation stays up before reverting to the copy
/// icon (React `COPIED_FEEDBACK_MS`).
#[allow(dead_code)] // reusable component; wired by the AiMessageContent track (T169)
pub const COPIED_FEEDBACK_MS: u64 = 1500;

/// React `SQL_DIALECTS` (`AiCodeBlock.tsx`): the seven dialects the dialect
/// select offers, in React's order.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[allow(dead_code)] // reusable component; wired by the AiMessageContent track (T169)
pub enum SqlDialect {
    /// `postgresql` — also the `sqlDialect` prop default.
    #[default]
    PostgreSql,
    /// `mysql`.
    MySql,
    /// `sqlite`.
    Sqlite,
    /// `mariadb`.
    MariaDb,
    /// `sqlserver`.
    SqlServer,
    /// `oracle`.
    Oracle,
    /// `clickhouse`.
    ClickHouse,
}

#[allow(dead_code)] // reusable component; wired by the AiMessageContent track (T169)
impl SqlDialect {
    /// Every dialect, in the select's order.
    pub const ALL: [SqlDialect; 7] = [
        SqlDialect::PostgreSql,
        SqlDialect::MySql,
        SqlDialect::Sqlite,
        SqlDialect::MariaDb,
        SqlDialect::SqlServer,
        SqlDialect::Oracle,
        SqlDialect::ClickHouse,
    ];

    /// The value React stores (`SelectOption.value`).
    pub fn value(self) -> &'static str {
        match self {
            Self::PostgreSql => "postgresql",
            Self::MySql => "mysql",
            Self::Sqlite => "sqlite",
            Self::MariaDb => "mariadb",
            Self::SqlServer => "sqlserver",
            Self::Oracle => "oracle",
            Self::ClickHouse => "clickhouse",
        }
    }

    /// The label React shows (`SelectOption.label`).
    pub fn label(self) -> &'static str {
        match self {
            Self::PostgreSql => "PostgreSQL",
            Self::MySql => "MySQL",
            Self::Sqlite => "SQLite",
            Self::MariaDb => "MariaDB",
            Self::SqlServer => "SQL Server",
            Self::Oracle => "Oracle",
            Self::ClickHouse => "ClickHouse",
        }
    }

    /// The stored value back to its dialect; `None` for a value React's
    /// list does not know.
    pub fn from_value(value: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|dialect| dialect.value() == value)
    }
}

/// React `SQL_KEYWORDS_RE` (`lib/aiMessageBlocks.ts`): the statement
/// starters that make a block SQL-looking, matched case-insensitively
/// against the first line.
const SQL_KEYWORDS: [&str; 21] = [
    "SELECT", "INSERT", "UPDATE", "DELETE", "WITH", "CREATE", "ALTER", "DROP", "EXPLAIN", "SHOW",
    "DESCRIBE", "PRAGMA", "TRUNCATE", "GRANT", "REVOKE", "BEGIN", "COMMIT", "ROLLBACK", "SET",
    "USE", "VACUUM",
];

/// React `isSqlCodeBlock` (`lib/aiMessageBlocks.ts`): a block is SQL when
/// its language tag is a SQL language (case-insensitive) or its first line
/// starts with a SQL keyword.
#[allow(dead_code)] // reusable component; wired by the AiMessageContent track (T169)
pub fn is_sql_code_block(language: &str, code: &str) -> bool {
    const SQL_LANGUAGES: [&str; 6] = [
        "sql",
        "mysql",
        "postgresql",
        "postgres",
        "sqlite",
        "mariadb",
    ];
    if SQL_LANGUAGES.contains(&language.to_ascii_lowercase().as_str()) {
        return true;
    }
    looks_like_sql(code)
}

/// React `looksLikeSql`: the first line, trimmed, starts with a SQL
/// keyword followed by a word boundary. The leading alphabetic run is the
/// candidate word, so `SELECTED` is not `SELECT`, and a digit or `_`
/// immediately after the run defeats the boundary, exactly as `\b` does.
#[allow(dead_code)] // reusable component; wired by the AiMessageContent track (T169)
pub fn looks_like_sql(code: &str) -> bool {
    let first_line = code.lines().next().unwrap_or("");
    let trimmed = first_line.trim_start();
    let run_end = trimmed
        .find(|c: char| !c.is_ascii_alphabetic())
        .unwrap_or(trimmed.len());
    let word = &trimmed[..run_end];
    if word.is_empty() {
        return false;
    }
    if trimmed[run_end..]
        .chars()
        .next()
        .is_some_and(|c| c.is_ascii_alphanumeric() || c == '_')
    {
        return false;
    }
    SQL_KEYWORDS.contains(&word.to_ascii_uppercase().as_str())
}

/// React's header label: the language tag as given, else `sql` for a
/// SQL-looking block, else `code`.
#[allow(dead_code)] // reusable component; wired by the AiMessageContent track (T169)
pub fn code_block_label<'a>(language: &'a str, is_sql: bool) -> &'a str {
    if !language.is_empty() {
        language
    } else if is_sql {
        "sql"
    } else {
        "code"
    }
}

/// The label as rendered: React uppercases it with CSS (`uppercase`).
#[allow(dead_code)] // reusable component; wired by the AiMessageContent track (T169)
pub fn display_label(language: &str, is_sql: bool) -> String {
    code_block_label(language, is_sql).to_uppercase()
}

/// The language `onRunCode` receives: React passes `language || 'sql'`,
/// even for a block that is only SQL-looking through its content.
#[allow(dead_code)] // reusable component; wired by the AiMessageContent track (T169)
pub fn run_language<'a>(language: &'a str) -> &'a str {
    if language.is_empty() {
        "sql"
    } else {
        language
    }
}

/// React's `useState` pieces plus the `useCopyFeedback` confirmation, kept
/// in the app's state `S` because Rune has no component-local state.
#[derive(Clone, Debug)]
#[allow(dead_code)] // reusable component; wired by the AiMessageContent track (T169)
pub struct AiCodeBlockState {
    /// React `fullscreen`.
    pub fullscreen: bool,
    /// React `selectedDialect`, seeded from the `sqlDialect` prop.
    pub selected_dialect: SqlDialect,
    /// Whether the dialect listbox is open.
    pub dialect_open: bool,
    /// React `copied`: the confirmation is showing.
    pub copied: bool,
    /// Monotonic id of the most recent copy request; `useCopyFeedback`'s
    /// `requestIdRef`, so a late failure or timer cannot clear a newer
    /// confirmation.
    pub copy_request: u64,
}

impl Default for AiCodeBlockState {
    fn default() -> Self {
        Self {
            fullscreen: false,
            selected_dialect: SqlDialect::default(),
            dialect_open: false,
            copied: false,
            copy_request: 0,
        }
    }
}

#[allow(dead_code)] // reusable component; wired by the AiMessageContent track (T169)
impl AiCodeBlockState {
    /// Seeds the dialect from the `sqlDialect` prop; an unknown value
    /// falls back to PostgreSQL (see the module docs).
    pub fn new(sql_dialect: &str) -> Self {
        Self {
            selected_dialect: SqlDialect::from_value(sql_dialect).unwrap_or_default(),
            ..Self::default()
        }
    }

    /// React `toggleFullscreen`: flips `fullscreen`.
    pub fn toggle_fullscreen(&mut self) {
        self.fullscreen = !self.fullscreen;
    }

    /// React `handleDialectChange`: picks a dialect; the listbox closes
    /// in the same press (the kit select closes on choose).
    pub fn set_dialect(&mut self, dialect: SqlDialect) {
        self.selected_dialect = dialect;
        self.dialect_open = false;
    }

    /// The dialect trigger's press: opens or closes the listbox.
    pub fn toggle_dialect_open(&mut self) {
        self.dialect_open = !self.dialect_open;
    }

    /// Presses outside and Escape close the listbox.
    pub fn close_dialect(&mut self) {
        self.dialect_open = false;
    }

    /// React `copy`: the optimistic mark. Sets the confirmation, arms a
    /// fresh full window (a second click owns a new request id), and
    /// returns that id for the host's failure / expiry report.
    pub fn copy_pressed(&mut self) -> u64 {
        self.copy_request += 1;
        self.copied = true;
        self.copy_request
    }

    /// The `COPIED_FEEDBACK_MS` timer firing: clears the confirmation
    /// only when `request` is still the most recent one. Returns whether
    /// it cleared.
    pub fn expire_copy_feedback(&mut self, request: u64) -> bool {
        if request == self.copy_request {
            self.copied = false;
            true
        } else {
            false
        }
    }

    /// A failed clipboard write: rolls the confirmation back, only when
    /// `request` is still the most recent one — a late failure cannot
    /// erase a newer, successful confirmation. Returns whether it
    /// rolled back.
    pub fn fail_copy(&mut self, request: u64) -> bool {
        if request == self.copy_request {
            self.copied = false;
            true
        } else {
            false
        }
    }
}

/// React `AiCodeBlockProps` (`AiCodeBlock.tsx`); the callbacks live on the
/// [`AiCodeBlock`] builder, not here, so a block can be described without
/// naming its host.
#[derive(Clone, Debug)]
#[allow(dead_code)] // reusable component; wired by the AiMessageContent track (T169)
pub struct AiCodeBlockProps {
    /// React `language`: the fenced block's tag, possibly empty.
    pub language: String,
    /// React `code`: the block's text.
    pub code: String,
    /// React `sqlDialect` (default `postgresql`): seeds
    /// [`AiCodeBlockState::selected_dialect`].
    pub sql_dialect: SqlDialect,
    /// React `showActions` (default `true`).
    pub show_actions: bool,
    /// React `isStreaming` (default `false`).
    pub is_streaming: bool,
}

#[allow(dead_code)] // reusable component; wired by the AiMessageContent track (T169)
impl Default for AiCodeBlockProps {
    /// An empty block with React's defaults.
    fn default() -> Self {
        Self::new("", "")
    }
}

#[allow(dead_code)] // reusable component; wired by the AiMessageContent track (T169)
impl AiCodeBlockProps {
    /// A block with React's defaults: `postgresql` dialect, actions
    /// shown, not streaming.
    pub fn new(language: impl Into<String>, code: impl Into<String>) -> Self {
        Self {
            language: language.into(),
            code: code.into(),
            sql_dialect: SqlDialect::default(),
            show_actions: true,
            is_streaming: false,
        }
    }

    /// React `sqlDialect`.
    pub fn sql_dialect(mut self, dialect: SqlDialect) -> Self {
        self.sql_dialect = dialect;
        self
    }

    /// React `showActions`.
    pub fn show_actions(mut self, show: bool) -> Self {
        self.show_actions = show;
        self
    }

    /// React `isStreaming`.
    pub fn is_streaming(mut self, streaming: bool) -> Self {
        self.is_streaming = streaming;
        self
    }
}

/// Applies a dialect choice from the kit select: a known value picks the
/// dialect, an unknown one leaves it alone (the kit only offers known
/// values, so this guards a stale stored value rather than a row the
/// listbox can show); the listbox closes either way.
#[allow(dead_code)] // reusable component; wired by the AiMessageContent track (T169)
pub fn apply_dialect_choice<S>(
    set_dialect: SetDialectFn<S>,
    close_dialect: CloseDialectFn<S>,
    s: &mut S,
    value: &str,
) {
    if let Some(dialect) = SqlDialect::from_value(value) {
        set_dialect(s, dialect);
    }
    close_dialect(s);
}

/// Reads the block's state out of the app's `S` (a clone: the message
/// wiring addresses one slot per rendered block, which a bare fn pointer
/// cannot express — see `ai_message_content.rs`).
#[allow(dead_code)] // reusable component; wired by the AiMessageContent track (T169)
pub type ReadCodeBlockStateFn<S> = Rc<dyn Fn(&S) -> AiCodeBlockState>;

/// Flips `fullscreen` on the app's `S`.
#[allow(dead_code)] // reusable component; wired by the AiMessageContent track (T169)
pub type ToggleFullscreenFn<S> = Rc<dyn Fn(&mut S)>;

/// Picks a dialect on the app's `S`.
#[allow(dead_code)] // reusable component; wired by the AiMessageContent track (T169)
pub type SetDialectFn<S> = Rc<dyn Fn(&mut S, SqlDialect)>;

/// Opens or closes the dialect listbox on the app's `S`.
#[allow(dead_code)] // reusable component; wired by the AiMessageContent track (T169)
pub type ToggleDialectOpenFn<S> = Rc<dyn Fn(&mut S)>;

/// Closes the dialect listbox on the app's `S`.
#[allow(dead_code)] // reusable component; wired by the AiMessageContent track (T169)
pub type CloseDialectFn<S> = Rc<dyn Fn(&mut S)>;

/// The optimistic copy mark on the app's `S`; returns the request id.
#[allow(dead_code)] // reusable component; wired by the AiMessageContent track (T169)
pub type CopyPressedFn<S> = Rc<dyn Fn(&mut S) -> u64>;
