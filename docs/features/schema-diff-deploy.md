# Schema Diff Deploy

User-facing guides (also shown in the in-app **使用说明** / User Guide window):

- Chinese: [schema-diff-guide.md](schema-diff-guide.zh-CN.md)
- English: [schema-diff-guide.en.md](schema-diff-guide.en.md)

Architecture: [architecture/backend/schema-diff.md](../architecture/backend/schema-diff.md).

## Summary

**Compare → Plan → Review → Deploy** with **source = desired**.

| Default | Behavior |
|---------|----------|
| Additive-only | ADD COLUMN / widen nullability / CREATE INDEX |
| Destructive | Requires checkbox + typing `DEPLOY` |
| PG / SQLite | Can run in a transaction → `rolled_back` on failure |
| MySQL | Auto-commit DDL → `mixed` on partial failure |
| Cross-dialect | Types via sync IR; unsupported → warning + skip |
