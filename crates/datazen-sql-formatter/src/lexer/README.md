# Tokenizer reference and scope

The token rule order, per-dialect phrases and reserved/function/type tables
are ported from the pinned original MIT-licensed npm package sql-formatter
15.8.2. The Rust representation has no dependency or runtime script.
Read the adjoining third_party directory for source attributions. The
fixture tests retain original raw text, preceding whitespace and original
positions (the reference's UTF-16 positions projected to exact UTF-8 offsets).

The static grouped tables contain the already expanded phrase alternatives
from dist/esm/languages/{sql,sqlite,postgresql,mysql,mariadb,transactsql},
ranked in the Tokenizer.js category order and longest-first within a category.
The ASCII first-word index avoids scanning whole dictionaries for each token;
a match may include exact original whitespace within a phrase. Unicode
identifier classes use fixed disjoint Unicode 17 ranges for Alphabetic,
Mark and Decimal_Number, not ASCII approximation or a dynamically loaded
language. Quotes, comments and values are borrowed source byte ranges.

The first acceptance scope is common SQL/SQLite. Other listed dialect
tables, quoting and parameter rules are only lexical groundwork; these
alone do not prove actual formatter support for their statement families.
A protected sql-formatter-disable body is explicitly unsupported and the
caller must preserve the entire input. Malformed quote, comment or unrecognized
symbol is rejected rather than guessed. Runtime limits apply to input,
total tokens and periodic checks; a tokenization result does not imply a
statement is structurally supported by the layout layer.
