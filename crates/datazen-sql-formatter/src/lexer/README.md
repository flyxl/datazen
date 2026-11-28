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

The reference matrix exercises selected SQL, SQLite, PostgreSQL, MySQL,
MariaDB, and T-SQL paths. Lexer recognition alone does not prove support for
additional statements. A protected sql-formatter-disable body is retained
as one raw borrowed token through the first literal enable marker (or EOF);
it is not parsed or reindented. A malformed unprotected quote, comment, or
unrecognized symbol is rejected rather than guessed. Runtime limits apply
to input, total tokens, and periodic checks.
