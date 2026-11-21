//! Pure port of `packages/frontend/src/lib/importMapping.ts` (DBR-01).
//!
//! The mapping planner is deterministic and domain-free, so it is translated
//! verbatim instead of being handed to the host: the dialog needs the same
//! entries/conflicts/unmapped/untargeted plan to decide `canStart` and to draw
//! the mapping table. The *parse* side (CSV/JSON/XLSX) stays host-owned — the
//! host feeds `ImportParseResult` into [`crate::views::import_dialog::ImportDialogVm`].
//!
//! React's `matchedBy` bookkeeping is not read by the wizard view (only
//! `source` / `target` are), so it is omitted here rather than carried as dead
//! state. Removal stage: none — this is a permanent pure helper of the port.

use super::{MappingConflict, MappingEntry, MappingMode, MappingPlan};

/// React `planImportMapping`. Overrides win over every automatic rule; the
/// three modes (auto / name / position) run in the same order as the source.
pub(crate) fn plan_import_mapping(
    source_columns: &[String],
    target_columns: &[String],
    mode: MappingMode,
    overrides: &[(String, Option<String>)],
) -> MappingPlan {
    let mut entries: Vec<MappingEntry> = Vec::new();
    let mut used_targets: Vec<String> = Vec::new();
    let mark_used = |target: &str, used: &mut Vec<String>| {
        if !used.iter().any(|name| name == target) {
            used.push(target.to_owned());
        }
    };

    // 1. Manual overrides first — the user's word is final (a null override
    //    means "don't import this column", represented as None).
    for source in source_columns {
        let Some((_, target)) = overrides.iter().find(|(name, _)| name == source) else {
            continue;
        };
        let target = target.clone();
        if let Some(name) = target.as_deref() {
            mark_used(name, &mut used_targets);
        }
        entries.push(MappingEntry {
            source: source.clone(),
            target,
        });
    }

    // 2. Name matching (auto and name modes): case-insensitive, and the
    //    recorded target is the real spelled name. Two sources claiming one
    //    target both keep the claim; the conflict detector below flags it.
    if matches!(mode, MappingMode::Auto | MappingMode::Name) {
        for source in source_columns {
            if entries.iter().any(|entry| entry.source == *source) {
                continue;
            }
            let found = target_columns
                .iter()
                .find(|target| target.to_lowercase() == source.to_lowercase());
            if let Some(target) = found {
                mark_used(target, &mut used_targets);
                entries.push(MappingEntry {
                    source: source.clone(),
                    target: Some(target.clone()),
                });
            }
        }
    }

    // 3. Positional fill (auto and position modes): unmatched sources take the
    //    next still-free target in target order.
    if matches!(mode, MappingMode::Auto | MappingMode::Position) {
        let free: Vec<&String> = target_columns
            .iter()
            .filter(|target| !used_targets.iter().any(|name| name == *target))
            .collect();
        let mut free_index = 0usize;
        for source in source_columns {
            if entries.iter().any(|entry| entry.source == *source) {
                continue;
            }
            let target = free.get(free_index).copied();
            free_index += 1;
            if let Some(name) = target {
                mark_used(name, &mut used_targets);
            }
            entries.push(MappingEntry {
                source: source.clone(),
                target: target.cloned(),
            });
        }
    }

    // 4. Anything still unassigned (name-mode leftovers) lands unmapped.
    for source in source_columns {
        if entries.iter().any(|entry| entry.source == *source) {
            continue;
        }
        entries.push(MappingEntry {
            source: source.clone(),
            target: None,
        });
    }

    // 5. Conflict detection: two or more active sources on one target.
    let mut writers: Vec<(String, Vec<String>)> = Vec::new();
    for entry in &entries {
        let Some(target) = entry.target.as_deref() else {
            continue;
        };
        if let Some((_, sources)) = writers.iter_mut().find(|(name, _)| name == target) {
            sources.push(entry.source.clone());
        } else {
            writers.push((target.to_owned(), vec![entry.source.clone()]));
        }
    }
    let conflicts: Vec<MappingConflict> = writers
        .into_iter()
        .filter(|(_, sources)| sources.len() > 1)
        .map(|(target, sources)| MappingConflict { target, sources })
        .collect();

    let unmapped: Vec<String> = entries
        .iter()
        .filter(|entry| entry.target.is_none())
        .map(|entry| entry.source.clone())
        .collect();
    let untargeted: Vec<String> = target_columns
        .iter()
        .filter(|target| !used_targets.iter().any(|name| name == *target))
        .cloned()
        .collect();

    // Keep entries in source order — the mapping table mirrors the file.
    entries.sort_by_key(|entry| {
        source_columns
            .iter()
            .position(|source| source == &entry.source)
            .unwrap_or(usize::MAX)
    });

    MappingPlan {
        entries,
        conflicts,
        unmapped,
        untargeted,
    }
}

/// React `mappingIsExecutable`: no conflicts and at least one mapped column.
pub(crate) fn mapping_is_executable(plan: &MappingPlan) -> bool {
    plan.conflicts.is_empty() && plan.entries.iter().any(|entry| entry.target.is_some())
}
