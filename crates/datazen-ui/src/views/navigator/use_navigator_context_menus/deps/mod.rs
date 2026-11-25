//! Private verbatim ports of the libraries `useNavigatorContextMenus.ts`
//! consumes, kept inside the navigator until their formal shard tasks land.
//! One file per React source, so the future move is a directory rename:
//!
//! | file | React source | formal task |
//! | --- | --- | --- |
//! | [`native_menu`] | `lib/nativeContextMenu.ts` (`NativeMenuItemDef`, driver-sdk shape) | shard 2, T567 |
//! | [`main_menu`] | `lib/mainWindowContextMenu.ts` | shard 2, T555 |
//! | [`schema_tree_menu`] | `lib/schemaTreeContextMenu.ts` | shard 2, T599 |
//! | [`connection_url`] | `lib/buildConnectionUrl.ts` | shard 2, T340 |
//!
//! `formatGroupLabel`, `shouldUseMultiDatabaseTree`, `quoteRelationName`,
//! `extractErrorMessage` and `resolveDropDatabaseFallback` — the
//! `navigator/utils.ts` and `connectionGroups.ts` exports this file reads —
//! live in `build_flat_rows::deps` (T570/T578's port of the same React
//! sources; removal stage T582/T394).

pub mod connection_url;
pub mod main_menu;
pub mod native_menu;
pub mod schema_tree_menu;
