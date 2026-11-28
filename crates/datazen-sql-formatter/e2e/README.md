# Formatter public-process acceptance

This is an external executable test consumer for the formatter crate's public Rust API.
It reads SQL from stdin, applies the named options, and emits the exact resulting bytes.
A formatter error exits 1, writes the crate's public diagnostic to stderr, and emits no
stdout. These replays complement, but are not, the Native Query UI tests.
`format_sql.rs` is not a Cargo target or a shipped executable. Node is only used as
an external test oracle, never linked to the formatter or this consumer.

## Run against the Native build's exact instrumented library

Use the same rustc and target as for the selectively instrumented Native build.
The following command assumes a Linux ELF target. Set `FORMATTER_RLIB` to the exact
formatter artifact from the Native build (the workspace coverage rustc wrapper
can retain it), and set `REFERENCE` to the installed project-locked
`sql-formatter` 15.8.2 package directory. The harness verifies the browser
bundle's pinned checksum before comparing that bundle and the CJS entry point.

```sh
e2e=crates/datazen-sql-formatter/e2e
out=$(mktemp -d)
target=$(rustc -vV | sed -n 's/^host: //p')
profiler=$(find "$(rustc --print sysroot)/lib/rustlib/$target/lib" \
  -maxdepth 1 -name 'libprofiler_builtins-*.rlib' -print -quit)
test -s "$FORMATTER_RLIB" && test -s "$profiler" || exit 1
rustc --edition=2021 --extern datazen_sql_formatter="$FORMATTER_RLIB" \
  -C 'link-arg=-Wl,-u,__llvm_profile_runtime' -C "link-arg=$profiler" \
  "$e2e/format_sql.rs" -o "$out/format-sql"
python3 -B "$e2e/public_process.py" "$out/format-sql" \
  --reference "$REFERENCE" --output "$out/conformance"
python3 -B "$e2e/output_bounds.py" "$out/format-sql" \
  --reference "$REFERENCE" --output "$out/output-bounds"
python3 -B "$e2e/incomplete_drafts.py" "$out/format-sql" --output "$out/incomplete-drafts"
```

Do not use any profiles from a failing or incomplete batch. Both positive-case
scripts record their accepted raw inputs in `accepted-profile-list`.
The refusal script's `passing.json` records the accepted profile filenames
and the exact observed error for each accepted case. Inputs are never executed.
SQL Server is not part of the subprocess E2E corpus; only the separately
designated SQL Server unit cases may supplement its production-line coverage.

When combining these replays with Native UI results, verify the library and Native
executable hashes and merge raw profiles with the matching toolchain. Export
against the same Native ELF and confirm the same ten production source files
and unchanged line denominator. Report Native-only, subprocess-only, and combined
E2E results separately. If additionally merging targeted SQL Server unit
profiles, label that total as mixed coverage, not E2E coverage.
