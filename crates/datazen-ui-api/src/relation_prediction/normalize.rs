//! Identifier and type normalization for foreign-key prediction.
//!
//! Ported from React `packages/frontend/src/lib/relationPrediction/normalize.ts`
//! (tasklist T033). Conventions differ far more than the textbook `{table}_id`
//! pattern, so names are folded before comparison: camelCase is split, case is
//! folded, and a table name is tried both as written and singularized. Two
//! deliberate limits, both consequences of "a wrong match is worse than no
//! match":
//!
//! - Singularization is conservative and never a gate. `people → person` and
//!   `address → address` are handled; anything not recognized is left alone. A
//!   missed match costs a manual click, an invented one costs a wrong query.
//! - Abbreviation guessing is not attempted at all. `usr_id` might mean
//!   `user_id` or `usage_id`, and there is no evidence to choose.
//!
//! ## Porting the regexes without a regex engine
//!
//! `normalize.ts` leans on four JavaScript regex idioms; this crate has no
//! regex dependency, so each is hand-rolled to the same observable semantics
//! (the derivation lives next to each helper):
//!
//! - [`split_identifier_words`] composes the two replacement passes (camelCase
//!   boundary, acronym boundary — each inserting a NUL separator) with the
//!   trailing split on NUL and on runs of non-alphanumeric chars, into a
//!   single left-to-right scan.
//! - [`strip_trailing_parenthesized`] is `/\(.*\)$/` with JavaScript's `.` never
//!   crossing a line terminator — the helper checks the exact condition the
//!   engine's backtracking would.
//! - `collapse_js_whitespace` is `/\s+/g → ' '` over the exact ECMAScript `\s`
//!   class (spelled out because Rust's `char::is_whitespace` disagrees with it
//!   on `\u{85}` and `\u{feff}`).
//! - The unsigned strips are one leading and one trailing literal replacement,
//!   so `strip_prefix`/`strip_suffix` are exact.
//!
//! Consumers: React calls these from `predictRelations.ts` (tasklist T082, not
//! yet ported) — this module is the engine's comparison vocabulary, landed
//! first so the schema adapter (this task) and the metadata adapter (T081) can
//! feed it.

/// Plural forms whose singular cannot be derived by a rule.
const IRREGULAR_SINGULARS: &[(&str, &str)] = &[
    ("people", "person"),
    ("children", "child"),
    ("men", "man"),
    ("women", "woman"),
    ("teeth", "tooth"),
    ("feet", "foot"),
    ("geese", "goose"),
    ("mice", "mouse"),
    ("oxen", "ox"),
    ("indices", "index"),
    ("matrices", "matrix"),
    ("vertices", "vertex"),
    ("criteria", "criterion"),
    ("phenomena", "phenomenon"),
    ("data", "data"),
];

/// Words that end in `s` but are not plural.
///
/// `alias_`/`status_` are dead entries carried over from React: the last word
/// of a [`split_identifier_words`] split never contains a separator, so an
/// underscored spelling can never match.
const SINGULAR_LOOKING: &[&str] = &[
    "address",
    "status",
    "business",
    "access",
    "class",
    "process",
    "analysis",
    "series",
    "species",
    "news",
    "boss",
    "alias",
    "alias_",
    "gas",
    "lens",
    "atlas",
    "campus",
    "corpus",
    "census",
    "plus",
    "bonus",
    "virus",
    "cactus",
    "focus",
    "radius",
    "surplus",
    "apparatus",
    "status_",
];

/// Split an identifier into lowercase words.
///
/// `UserID` → `['user','id']`, `user_id` → `['user','id']`, `userID` → same.
/// Runs of capitals are treated as one acronym (`userID` → `user` + `id`,
/// `HTTPServer` → `http` + `server`).
///
/// The two regex passes compose into one scan because their boundaries never
/// interact: a camelCase boundary needs a lowercase/digit immediately before
/// an uppercase, an acronym boundary needs an uppercase immediately before an
/// uppercase whose next char is lowercase, and no single position can satisfy
/// both — so checking both conditions per character (against the *original*
/// neighbors) reproduces the composed replacements exactly. Every char
/// outside `[A-Za-z0-9]` is a separator (quoted and bracket-quoted
/// identifiers included, since the JS classes are ASCII-only), and separator
/// runs yield no empty words.
pub fn split_identifier_words(value: &str) -> Vec<String> {
    let chars: Vec<char> = value.chars().collect();
    let mut words: Vec<String> = Vec::new();
    let mut current = String::new();
    for (i, &c) in chars.iter().enumerate() {
        if !c.is_ascii_alphanumeric() {
            // `[^A-Za-z0-9]+`: runs of separators split once, empties drop.
            if !current.is_empty() {
                words.push(std::mem::take(&mut current));
            }
            continue;
        }
        let prev = if i > 0 { Some(chars[i - 1]) } else { None };
        let boundary = match prev {
            // camelCase boundary: lower/digit immediately followed by upper.
            Some(p) => {
                (p.is_ascii_lowercase() || p.is_ascii_digit()) && c.is_ascii_uppercase()
                    // acronym boundary: upper immediately followed by
                    // upper+lower (`HTTPServer` → `HTTP` + `Server`).
                    || p.is_ascii_uppercase()
                        && c.is_ascii_uppercase()
                        && chars.get(i + 1).is_some_and(|&n| n.is_ascii_lowercase())
            }
            None => false,
        };
        if boundary && !current.is_empty() {
            words.push(std::mem::take(&mut current));
        }
        // Words only ever contain ASCII alphanumerics, so ASCII folding is JS
        // `toLowerCase` here.
        current.push(c.to_ascii_lowercase());
    }
    if !current.is_empty() {
        words.push(current);
    }
    words
}

/// Canonical comparison form: words joined by `_`.
pub fn normalize_identifier(value: &str) -> String {
    split_identifier_words(value).join("_")
}

/// Singularize the last word of a table name.
///
/// Only the last word is touched (`order_items` → `order_item`), and words
/// that merely look plural are left alone. Returns the input unchanged when
/// unsure — including when the name has no words at all, where the original
/// spelling (punctuation and case included) is handed back verbatim.
pub fn singularize(table_name: &str) -> String {
    let mut words = split_identifier_words(table_name);
    let Some(last) = words.last_mut() else {
        // No words: React returns the input string itself, unchanged.
        return table_name.to_string();
    };
    if let Some(irregular) = irregular_singular(last) {
        *last = irregular.to_string();
        return words.join("_");
    }
    if is_singular_looking(last) {
        return words.join("_");
    }
    // Words are ASCII-only (see split_identifier_words), so byte lengths
    // equal JS `.length` and the slice cuts below are char-safe.
    if last.ends_with("ies") && last.len() > 4 {
        // `categories` → `category`: drop `ies`, restore the `y`.
        *last = format!("{}y", &last[..last.len() - 3]);
        return words.join("_");
    }
    if last.ends_with("ses")
        || last.ends_with("xes")
        || last.ends_with("zes")
        || last.ends_with("ches")
        || last.ends_with("shes")
    {
        // `buses`/`boxes`/`watches`/`bushes` → drop the `es`.
        *last = last[..last.len() - 2].to_string();
        return words.join("_");
    }
    if last.ends_with('s') && !last.ends_with("ss") && last.len() > 3 {
        *last = last[..last.len() - 1].to_string();
        return words.join("_");
    }
    words.join("_")
}

fn irregular_singular(word: &str) -> Option<&'static str> {
    // A linear scan over a 15-entry literal keeps the table readable, like
    // React's record.
    IRREGULAR_SINGULARS
        .iter()
        .find(|(plural, _)| *plural == word)
        .map(|(_, singular)| *singular)
}

fn is_singular_looking(word: &str) -> bool {
    SINGULAR_LOOKING.iter().any(|entry| *entry == word)
}

/// Key column names that carry no domain meaning.
///
/// `id` is excluded from the same-name pattern: every table's surrogate key is
/// called `id`, so matching on it would relate every table to every other.
pub fn is_generic_key_name(key_column: &str) -> bool {
    let normalized = normalize_identifier(key_column);
    matches!(normalized.as_str(), "id" | "uuid" | "pk")
}

/// Type families that must agree for a relationship to be plausible.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TypeFamily {
    Numeric,
    String,
    Uuid,
    Temporal,
    Boolean,
    Binary,
    Json,
    Other,
}

/// React's `TYPE_FAMILY_PATTERNS` as exact-match spelling lists, in the same
/// order (`uuid` first, then numeric, boolean, temporal, json, binary,
/// string). The alternatives are disjoint across lists, so the order cannot
/// change any answer — it is kept for review against the React source.
const UUID_TYPE_SPELLINGS: &[&str] = &["uuid", "guid"];

const NUMERIC_TYPE_SPELLINGS: &[&str] = &[
    "int",
    "int2",
    "int4",
    "int8",
    "integer",
    "bigint",
    "smallint",
    "tinyint",
    "mediumint",
    "serial",
    "bigserial",
    "smallserial",
    "number",
    "numeric",
    "decimal",
    "dec",
    "real",
    "double",
    "double precision",
    "float",
    "float4",
    "float8",
    "money",
    // Dead entry, preserved from React: the `unsigned` strip runs before the
    // lookup, so `unsigned big int` folds to `big int` and never matches
    // (SQLite's spelling falls to `Other`, exactly as React wrote it).
    "unsigned big int",
];

const BOOLEAN_TYPE_SPELLINGS: &[&str] = &["bool", "boolean", "bit"];

const TEMPORAL_TYPE_SPELLINGS: &[&str] = &[
    "date",
    "time",
    "timetz",
    "timestamp",
    "timestamptz",
    "datetime",
    "datetime2",
    "smalldatetime",
    "interval",
    "year",
];

const JSON_TYPE_SPELLINGS: &[&str] = &["json", "jsonb"];

const BINARY_TYPE_SPELLINGS: &[&str] = &["bytea", "blob", "binary", "varbinary", "image", "raw"];

const STRING_TYPE_SPELLINGS: &[&str] = &[
    "char",
    "character",
    "nchar",
    "varchar",
    "nvarchar",
    "varchar2",
    "nvarchar2",
    "text",
    "ntext",
    "tinytext",
    "mediumtext",
    "longtext",
    "clob",
    "nclob",
    "citext",
    "string",
    "enum",
    "set",
];

/// Reduce a driver-reported type string to a comparison family.
///
/// Lengths, precision and unsigned flags are dropped (`varchar(32)` →
/// `string`), and every numeric type shares one family so a `bigint` key
/// referencing an `integer` key still matches — a common, intentional schema
/// shape.
///
/// React's pipeline order is kept exactly: trim → lowercase → strip a trailing
/// parenthesized suffix → collapse whitespace → trim → strip the unsigned
/// markers → lookup. Note the trim *between* the collapse and the unsigned
/// strip — [`normalize_data_type`] does not have it, and the one input family
/// the two pipelines disagree on is pinned by a test.
pub fn type_family(data_type: &str) -> TypeFamily {
    let lowered = trim_js_whitespace(data_type).to_lowercase();
    let stripped = strip_trailing_parenthesized(&lowered);
    let collapsed = collapse_js_whitespace(&stripped);
    let trimmed = trim_js_whitespace(&collapsed);
    let unsigned = strip_unsigned_markers(trimmed);
    family_of(unsigned)
}

/// Canonical form used for the exact-type bonus.
///
/// React's pipeline: trim → lowercase → strip the trailing parenthesized
/// suffix → collapse whitespace → strip the unsigned markers → trim. The
/// final trim (not an intermediate one before the unsigned strip) is what
/// separates this from [`type_family`]'s order.
pub fn normalize_data_type(data_type: &str) -> String {
    let lowered = trim_js_whitespace(data_type).to_lowercase();
    let stripped = strip_trailing_parenthesized(&lowered);
    let collapsed = collapse_js_whitespace(&stripped);
    let unsigned = strip_unsigned_markers(&collapsed);
    trim_js_whitespace(unsigned).to_string()
}

fn family_of(value: &str) -> TypeFamily {
    if UUID_TYPE_SPELLINGS.iter().any(|spelling| *spelling == value) {
        return TypeFamily::Uuid;
    }
    if NUMERIC_TYPE_SPELLINGS.iter().any(|spelling| *spelling == value) {
        return TypeFamily::Numeric;
    }
    if BOOLEAN_TYPE_SPELLINGS.iter().any(|spelling| *spelling == value) {
        return TypeFamily::Boolean;
    }
    if TEMPORAL_TYPE_SPELLINGS.iter().any(|spelling| *spelling == value) {
        return TypeFamily::Temporal;
    }
    if JSON_TYPE_SPELLINGS.iter().any(|spelling| *spelling == value) {
        return TypeFamily::Json;
    }
    if BINARY_TYPE_SPELLINGS.iter().any(|spelling| *spelling == value) {
        return TypeFamily::Binary;
    }
    if STRING_TYPE_SPELLINGS.iter().any(|spelling| *spelling == value) {
        return TypeFamily::String;
    }
    TypeFamily::Other
}

/// The exact `\s` class of ECMAScript regexes (and of
/// `String.prototype.trim`): TAB/LF/VT/FF/CR/SP, NBSP, ZWNBSP (`\u{feff}`),
/// the Zs separators, and the line/paragraph separators.
///
/// Rust's `char::is_whitespace` differs from it on exactly `\u{85}` (Rust
/// folds it, JS does not) and `\u{feff}` (JS folds it, Rust does not), so the
/// class is spelled out instead of trusting the trait.
fn is_js_whitespace(c: char) -> bool {
    matches!(
        c,
        '\u{0009}'..='\u{000d}'
            | '\u{0020}'
            | '\u{00a0}'
            | '\u{1680}'
            | '\u{2000}'..='\u{200a}'
            | '\u{2028}'
            | '\u{2029}'
            | '\u{202f}'
            | '\u{205f}'
            | '\u{3000}'
            | '\u{feff}'
    )
}

/// Line terminators a JS regex `.` never crosses (without the `s` flag).
fn is_js_line_terminator(c: char) -> bool {
    matches!(c, '\n' | '\r' | '\u{2028}' | '\u{2029}')
}

fn trim_js_whitespace(value: &str) -> &str {
    value.trim_matches(is_js_whitespace)
}

/// `/\s+/g → ' '`: every run of whitespace becomes one space, runs at the
/// very start/end included (React's chain trims afterwards, not here).
fn collapse_js_whitespace(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let mut in_run = false;
    for c in value.chars() {
        if is_js_whitespace(c) {
            if !in_run {
                out.push(' ');
                in_run = true;
            }
        } else {
            out.push(c);
            in_run = false;
        }
    }
    out
}

/// `/\(.*\)$/ → ''`: remove a trailing parenthesized suffix
/// (`varchar(32)` → `varchar`).
///
/// The engine semantics: the match must run from some `(` to the *final* `)`
/// at the very end of the string, with `.*` covering everything between — and
/// `.` never crosses a line terminator, so a `(` whose tail crosses one cannot
/// start a match (the engine then retries at later `(`s; the *first* usable
/// one wins, so `numeric(10)(2)` loses both groups). No `(` or no trailing `)`
/// means no match and the value passes through unchanged.
fn strip_trailing_parenthesized(value: &str) -> String {
    if !value.ends_with(')') {
        return value.to_string();
    }
    // `(` and `)` are ASCII, so `len() - 1` is the final `)`'s byte index and
    // a char boundary.
    let tail_start = value.len() - 1;
    for (index, c) in value.char_indices() {
        if c != '(' {
            continue;
        }
        // ASCII `(` → `index + 1` is the next char boundary.
        if value[index + 1..tail_start].chars().any(is_js_line_terminator) {
            continue;
        }
        return value[..index].to_string();
    }
    value.to_string()
}

/// `^unsigned ` and ` unsigned$` strips, once each (React's `replace` has no
/// `g` flag), applied to the already-normalized spelling.
fn strip_unsigned_markers(value: &str) -> &str {
    let once = value.strip_prefix("unsigned ").unwrap_or(value);
    once.strip_suffix(" unsigned").unwrap_or(once)
}

#[cfg(test)]
mod tests {
    use super::*;

    // The React suite for these functions lives in
    // `relationPrediction/__tests__/predictRelations.test.ts`
    // (`describe('normalize')`) — its cases are carried here; predictRelations
    // itself is tasklist T082.

    #[test]
    fn splits_camel_snake_and_acronym_identifiers_the_same_way() {
        assert_eq!(split_identifier_words("UserID"), vec!["user", "id"]);
        assert_eq!(split_identifier_words("user_id"), vec!["user", "id"]);
        assert_eq!(split_identifier_words("HTTPServer"), vec!["http", "server"]);
        assert_eq!(normalize_identifier("userId"), "user_id");
    }

    #[test]
    fn acronym_boundaries_break_before_the_last_capital_of_a_run() {
        // An acronym run followed by lowercase breaks before its last
        // capital (`HTTPs` → `htt` + `ps`); `UserIDs` composes both rules
        // (camel boundary first, then the acronym rule inside the tail);
        // `aBCd` breaks between the two capitals; a capital before a digit
        // does not (`AB1c` stays one word).
        assert_eq!(split_identifier_words("HTTPs"), vec!["htt", "ps"]);
        assert_eq!(split_identifier_words("UserIDs"), vec!["user", "i", "ds"]);
        assert_eq!(split_identifier_words("aBCd"), vec!["a", "b", "cd"]);
        assert_eq!(split_identifier_words("AB1c"), vec!["ab1c"]);
        assert_eq!(normalize_identifier("OrderID2"), "order_id2");
    }

    #[test]
    fn digits_open_boundaries_like_lowercase() {
        assert_eq!(split_identifier_words("a1B2"), vec!["a1", "b2"]);
        assert_eq!(split_identifier_words("2"), vec!["2"]);
        // All-caps is one acronym word: no boundary rule fires without a
        // lowercase (or digit) somewhere in the run.
        assert_eq!(normalize_identifier("USERID"), "userid");
    }

    #[test]
    fn separators_split_and_empty_words_are_dropped() {
        // JS `[^A-Za-z0-9]` treats every non-ASCII-alphanumeric char as a
        // separator — quoted and bracket-quoted identifiers included — and
        // runs of them produce no empty words.
        assert_eq!(split_identifier_words("\"Users\""), vec!["users"]);
        assert_eq!(split_identifier_words("`order items`"), vec!["order", "items"]);
        assert_eq!(split_identifier_words("[dbo].[users]"), vec!["dbo", "users"]);
        assert_eq!(split_identifier_words("public.users"), vec!["public", "users"]);
        assert_eq!(split_identifier_words("café_x"), vec!["caf", "x"]);
        assert!(split_identifier_words("").is_empty());
        assert!(split_identifier_words("___").is_empty());
        assert_eq!(normalize_identifier(""), "");
        assert_eq!(normalize_identifier("__--__"), "");
        assert_eq!(normalize_identifier("__user--ID__"), "user_id");
    }

    #[test]
    fn singularizes_regular_and_irregular_plurals() {
        assert_eq!(singularize("users"), "user");
        assert_eq!(singularize("order_items"), "order_item");
        assert_eq!(singularize("categories"), "category");
        assert_eq!(singularize("people"), "person");
    }

    #[test]
    fn every_irregular_plural_folds_to_its_listed_singular() {
        // Table-driven over the whole literal so a dropped entry cannot slip
        // through review.
        for (plural, singular) in IRREGULAR_SINGULARS {
            assert_eq!(singularize(plural), *singular, "irregular {plural}");
        }
    }

    #[test]
    fn leaves_words_that_merely_look_plural_alone() {
        // `address` is not the plural of `addres`; guessing here would invent
        // names.
        assert_eq!(singularize("address"), "address");
        assert_eq!(singularize("status"), "status");
        assert_eq!(singularize("business"), "business");
        assert_eq!(singularize("analysis"), "analysis");
    }

    #[test]
    fn every_separator_free_singular_looking_word_survives_unchanged() {
        for word in SINGULAR_LOOKING.iter().filter(|w| !w.contains('_')) {
            assert_eq!(singularize(word), *word, "singular-looking {word}");
        }
    }

    #[test]
    fn the_underscored_singular_looking_entries_are_unreachable_as_words() {
        // `alias_`/`status_` are dead entries in React too: the last word of
        // a split never contains a separator. Folding them through anyway
        // lands on the separator-free spelling (and drops the underscore) —
        // the documented behavior, not a match.
        assert_eq!(singularize("alias_"), "alias");
        assert_eq!(singularize("status_"), "status");
    }

    #[test]
    fn singularize_folds_case_and_separators_through_the_split() {
        // Words come back lowercased and joined on `_`, so the input casing
        // and the original separator spelling both normalize away.
        assert_eq!(singularize("OrderItems"), "order_item");
        assert_eq!(singularize("order items"), "order_item");
        assert_eq!(singularize("Alias"), "alias");
    }

    #[test]
    fn singularize_tails_each_rule() {
        assert_eq!(singularize("cities"), "city"); // ies → y
        assert_eq!(singularize("boxes"), "box"); // xes
        assert_eq!(singularize("watches"), "watch"); // ches
        assert_eq!(singularize("bushes"), "bush"); // shes
        assert_eq!(singularize("buses"), "bus"); // ses
        assert_eq!(singularize("classes"), "class"); // ses beats the ss guard
        assert_eq!(singularize("series"), "series"); // singular-looking beats ies
        assert_eq!(singularize("ties"), "tie"); // 4-letter ies → the s rule
        assert_eq!(singularize("bus"), "bus"); // too short for the s rule
        assert_eq!(singularize("data"), "data"); // identity mapping
    }

    #[test]
    fn singularize_without_words_returns_the_input_verbatim() {
        // Empty/separator-only names have no words, so React hands back the
        // input unchanged — punctuation included, no folding.
        assert_eq!(singularize(""), "");
        assert_eq!(singularize("!!!"), "!!!");
        assert_eq!(singularize("-"), "-");
    }

    #[test]
    fn generic_key_names_match_after_normalization() {
        assert!(is_generic_key_name("id"));
        assert!(is_generic_key_name("UUID"));
        assert!(is_generic_key_name("pk"));
        assert!(is_generic_key_name("Pk"));
        // Separators fold away in the canonical form, so these are generic.
        assert!(is_generic_key_name("id_"));
        assert!(is_generic_key_name("ID "));
        assert!(!is_generic_key_name("user_id"));
        assert!(!is_generic_key_name(""));
    }

    #[test]
    fn groups_type_spellings_into_families() {
        assert_eq!(type_family("INT"), TypeFamily::Numeric);
        assert_eq!(type_family("bigint"), TypeFamily::Numeric);
        assert_eq!(type_family("NUMERIC(10,2)"), TypeFamily::Numeric);
        assert_eq!(type_family("varchar(32)"), TypeFamily::String);
        assert_eq!(type_family("uuid"), TypeFamily::Uuid);
        assert_eq!(type_family("timestamptz"), TypeFamily::Temporal);
    }

    #[test]
    fn every_listed_type_spelling_maps_to_its_family() {
        // Table-driven over the spelling lists (skipping the one dead entry
        // React carries) so a dropped alternative cannot slip through review.
        let lists: &[(&[&str], TypeFamily)] = &[
            (UUID_TYPE_SPELLINGS, TypeFamily::Uuid),
            (NUMERIC_TYPE_SPELLINGS, TypeFamily::Numeric),
            (BOOLEAN_TYPE_SPELLINGS, TypeFamily::Boolean),
            (TEMPORAL_TYPE_SPELLINGS, TypeFamily::Temporal),
            (JSON_TYPE_SPELLINGS, TypeFamily::Json),
            (BINARY_TYPE_SPELLINGS, TypeFamily::Binary),
            (STRING_TYPE_SPELLINGS, TypeFamily::String),
        ];
        for (spellings, family) in lists {
            for spelling in spellings.iter().filter(|s| **s != "unsigned big int") {
                assert_eq!(type_family(spelling), *family, "spelling {spelling}");
            }
        }
    }

    #[test]
    fn unsigned_markers_strip_before_the_family_lookup() {
        assert_eq!(type_family("unsigned int"), TypeFamily::Numeric);
        assert_eq!(type_family("bigint unsigned"), TypeFamily::Numeric);
        assert_eq!(type_family("DOUBLE PRECISION"), TypeFamily::Numeric);
        assert_eq!(type_family("bool"), TypeFamily::Boolean);
        assert_eq!(type_family("bit"), TypeFamily::Boolean);
        assert_eq!(type_family("jsonb"), TypeFamily::Json);
        assert_eq!(type_family("bytea"), TypeFamily::Binary);
        assert_eq!(type_family("text"), TypeFamily::String);
    }

    #[test]
    fn unlisted_and_malformed_spellings_fall_to_other() {
        assert_eq!(type_family(""), TypeFamily::Other);
        assert_eq!(type_family("weird"), TypeFamily::Other);
        // The dead-entry quirk: the unsigned strip runs before the lookup,
        // so SQLite's `unsigned big int` folds to `big int`, misses the
        // table, and lands in `Other` — exactly as React behaves.
        assert_eq!(type_family("unsigned big int"), TypeFamily::Other);
    }

    #[test]
    fn whitespace_and_parens_normalize_before_the_lookup() {
        assert_eq!(type_family("  varchar(32)  "), TypeFamily::String);
        assert_eq!(type_family("varchar (32)"), TypeFamily::String);
        assert_eq!(type_family("decimal(10, 2)"), TypeFamily::Numeric);
        assert_eq!(type_family("double  precision"), TypeFamily::Numeric);
        assert_eq!(type_family("int unsigned "), TypeFamily::Numeric);
    }

    #[test]
    fn type_family_and_normalize_data_type_keep_their_own_pipeline_orders() {
        // A faithful quirk: typeFamily trims between the whitespace collapse
        // and the unsigned strip, normalizeDataType only trims at the very
        // end — so a trailing marker left next to a stripped `(...)` suffix
        // stays in normalizeDataType's output while typeFamily loses it.
        let spelling = "varchar unsigned (32)";
        assert_eq!(type_family(spelling), TypeFamily::String);
        assert_eq!(normalize_data_type(spelling), "varchar unsigned");
    }

    #[test]
    fn a_line_terminator_inside_the_parens_blocks_the_strip() {
        // JS `.` never crosses a line terminator, so `\(.*\)$` cannot match
        // when one sits inside the parentheses: the suffix survives verbatim
        // (whitespace-collapsed). A terminator *before* the `(` does not
        // block anything.
        let blocked = "a(\n1)";
        assert_eq!(normalize_data_type(blocked), "a( 1)");
        assert_eq!(type_family(blocked), TypeFamily::Other);
        assert_eq!(normalize_data_type("numeric\n(10,2)"), "numeric");
    }

    #[test]
    fn normalize_data_type_folds_case_parens_and_whitespace() {
        assert_eq!(normalize_data_type("INT"), "int");
        assert_eq!(normalize_data_type("VarChar(32)"), "varchar");
        assert_eq!(normalize_data_type("  decimal(10,2)  "), "decimal");
        assert_eq!(normalize_data_type("unsigned int"), "int");
        assert_eq!(normalize_data_type("int unsigned"), "int");
        assert_eq!(normalize_data_type("DOUBLE  PRECISION"), "double precision");
        assert_eq!(normalize_data_type("unsigned"), "unsigned");
        assert_eq!(normalize_data_type(""), "");
    }
}
