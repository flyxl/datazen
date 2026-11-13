---
name: bump-version
description: >-
  Update the DataZen application version number across all files and trigger a
  GitHub release rebuild. Use when the user asks to change, bump, or update the
  app version number.
---

# Bump Version

Run the bump script to update the app version sources, regenerate `Cargo.lock`,
commit, tag, and push to `origin` in one step.

## Usage

```bash
.cursor/skills/bump-version/scripts/bump.sh <new-version>
```

The version can include a leading `v` (it is stripped automatically).

Examples:

```bash
.cursor/skills/bump-version/scripts/bump.sh 0.1.0
.cursor/skills/bump-version/scripts/bump.sh v1.2.3
```

## What the Script Does

1. Reads the current version from `package.json`.
2. Updates bare semver in `package.json`, `crates/datazen-desktop/Cargo.toml`,
   and `crates/datazen-desktop/tauri.conf.json`.
3. Runs `cargo check -p datazen_desktop --lib` to regenerate `Cargo.lock`.
4. Runs the version, CI docs, structure-editor, and i18n release guards, then
   checks the diff.
5. Commits the version sources, lockfile, and bump skill as
   `chore: bump version to vX.Y.Z`.
6. Creates git tag `vX.Y.Z` and pushes `main` and the tag to `origin` to trigger
   the GitHub release build. If a `gitee` remote is configured, it also pushes
   there.

## Adding New Version References

If the app version gains another source, add its update pattern and path to the script.
