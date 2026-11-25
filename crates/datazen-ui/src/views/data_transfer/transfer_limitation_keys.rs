//! The data-transfer limitation keys, ported from React
//! `packages/frontend/src/windows/data-transfer/transferLimitationKeys.ts`
//! (tasklist T500).
//!
//! The transfer window renders one sentence per key through `t()`, in this
//! order — the exact order the React `as const` tuple fixes. The sentences
//! themselves live in the zh-CN sync pack (`transfer.limitations.*`), so
//! this port pins the key spellings and the order, nothing else.

/// `TRANSFER_LIMITATION_KEYS`: the five sentences the transfer window
/// shows about what this version cannot do.
pub const TRANSFER_LIMITATION_KEYS: [&str; 5] = [
    "transfer.limitations.noViews",
    "transfer.limitations.noFkIndexes",
    "transfer.limitations.crossDialect",
    "transfer.limitations.baseTables",
    "transfer.limitations.noResume",
];
