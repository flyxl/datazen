use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ImportMappingMode {
    Auto,
    Name,
    Position,
}

impl ImportMappingMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Name => "name",
            Self::Position => "position",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ImportMatchKind {
    Name,
    Position,
    Manual,
    None,
}

impl ImportMatchKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Name => "name",
            Self::Position => "position",
            Self::Manual => "manual",
            Self::None => "none",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImportMappingEntry {
    pub source: String,
    pub target: Option<String>,
    pub matched_by: ImportMatchKind,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImportMappingConflict {
    pub target: String,
    pub sources: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImportMappingPlan {
    pub entries: Vec<ImportMappingEntry>,
    pub conflicts: Vec<ImportMappingConflict>,
    pub unmapped: Vec<String>,
    pub untargeted: Vec<String>,
}

pub struct PlanImportMappingInput<'a> {
    pub source_columns: &'a [String],
    pub target_columns: &'a [String],
    pub mode: ImportMappingMode,
    pub overrides: &'a BTreeMap<String, Option<String>>,
}

pub fn plan_import_mapping(input: &PlanImportMappingInput<'_>) -> ImportMappingPlan {
    let mut entries: Vec<ImportMappingEntry> = Vec::new();
    let mut used_targets: BTreeSet<String> = BTreeSet::new();

    for source in input.source_columns {
        let Some(target) = input.overrides.get(source) else {
            continue;
        };
        let target = target.clone();
        entries.push(ImportMappingEntry {
            source: source.clone(),
            target: target.clone(),
            matched_by: ImportMatchKind::Manual,
        });
        if let Some(target) = target {
            used_targets.insert(target);
        }
    }

    let mut target_by_lower: BTreeMap<String, String> = BTreeMap::new();
    for target in input.target_columns {
        target_by_lower.entry(target.to_lowercase()).or_insert_with(|| target.clone());
    }

    if matches!(input.mode, ImportMappingMode::Auto | ImportMappingMode::Name) {
        for source in input.source_columns {
            if entries.iter().any(|entry| entry.source == *source) {
                continue;
            }
            let Some(target) = target_by_lower.get(&source.to_lowercase()).cloned() else {
                continue;
            };
            entries.push(ImportMappingEntry {
                source: source.clone(),
                target: Some(target.clone()),
                matched_by: ImportMatchKind::Name,
            });
            used_targets.insert(target);
        }
    }

    if matches!(input.mode, ImportMappingMode::Auto | ImportMappingMode::Position) {
        let free_targets: Vec<String> = input
            .target_columns
            .iter()
            .filter(|target| !used_targets.contains(*target))
            .cloned()
            .collect();
        let mut free_index = 0;
        for source in input.source_columns {
            if entries.iter().any(|entry| entry.source == *source) {
                continue;
            }
            let target = free_targets.get(free_index).cloned();
            free_index += 1;
            if let Some(target) = target.as_ref() {
                used_targets.insert(target.clone());
            }
            entries.push(ImportMappingEntry {
                source: source.clone(),
                matched_by: if target.is_some() { ImportMatchKind::Position } else { ImportMatchKind::None },
                target,
            });
        }
    }

    for source in input.source_columns {
        if entries.iter().any(|entry| entry.source == *source) {
            continue;
        }
        entries.push(ImportMappingEntry {
            source: source.clone(),
            target: None,
            matched_by: ImportMatchKind::None,
        });
    }

    let mut writers: Vec<(String, Vec<String>)> = Vec::new();
    for entry in &entries {
        let Some(target) = entry.target.as_ref() else {
            continue;
        };
        if let Some((_, sources)) = writers.iter_mut().find(|(name, _)| name == target) {
            sources.push(entry.source.clone());
        } else {
            writers.push((target.clone(), vec![entry.source.clone()]));
        }
    }
    let conflicts: Vec<ImportMappingConflict> = writers
        .into_iter()
        .filter(|(_, sources)| sources.len() > 1)
        .map(|(target, sources)| ImportMappingConflict { target, sources })
        .collect();

    let unmapped: Vec<String> = entries
        .iter()
        .filter(|entry| entry.target.is_none())
        .map(|entry| entry.source.clone())
        .collect();
    let untargeted: Vec<String> = input
        .target_columns
        .iter()
        .filter(|target| !used_targets.contains(*target))
        .cloned()
        .collect();

    entries.sort_by_key(|entry| {
        input
            .source_columns
            .iter()
            .position(|source| source == &entry.source)
            .unwrap_or(usize::MAX)
    });

    ImportMappingPlan { entries, conflicts, unmapped, untargeted }
}

pub fn mapping_is_executable(plan: &ImportMappingPlan) -> bool {
    plan.conflicts.is_empty() && plan.entries.iter().any(|entry| entry.target.is_some())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| (*value).to_owned()).collect()
    }

    fn plan(source: &[&str], target: &[&str], mode: ImportMappingMode) -> ImportMappingPlan {
        plan_import_mapping(&PlanImportMappingInput {
            source_columns: &names(source),
            target_columns: &names(target),
            mode,
            overrides: &BTreeMap::new(),
        })
    }

    #[test]
    fn mode_and_match_literals() {
        assert_eq!(ImportMappingMode::Auto.as_str(), "auto");
        assert_eq!(ImportMappingMode::Name.as_str(), "name");
        assert_eq!(ImportMappingMode::Position.as_str(), "position");
        assert_eq!(ImportMatchKind::Name.as_str(), "name");
        assert_eq!(ImportMatchKind::Position.as_str(), "position");
        assert_eq!(ImportMatchKind::Manual.as_str(), "manual");
        assert_eq!(ImportMatchKind::None.as_str(), "none");
    }

    #[test]
    fn name_mode_records_the_real_target_spelling() {
        let result = plan(&["ID", "name"], &["id", "Name"], ImportMappingMode::Name);
        assert_eq!(result.entries[0].target.as_deref(), Some("id"));
        assert_eq!(result.entries[0].matched_by, ImportMatchKind::Name);
        assert_eq!(result.entries[1].target.as_deref(), Some("Name"));
        assert!(result.unmapped.is_empty());
        assert!(result.untargeted.is_empty());
    }

    #[test]
    fn position_mode_fills_in_target_order() {
        let result = plan(&["a", "b"], &["x", "y", "z"], ImportMappingMode::Position);
        assert_eq!(result.entries[0].target.as_deref(), Some("x"));
        assert_eq!(result.entries[1].target.as_deref(), Some("y"));
        assert_eq!(result.untargeted, vec!["z"]);
        let short = plan(&["a", "b", "c"], &["x"], ImportMappingMode::Position);
        assert_eq!(short.entries[2].target, None);
        assert_eq!(short.unmapped, vec!["b", "c"]);
    }

    #[test]
    fn auto_mode_matches_names_then_fills_the_remaining_slots() {
        let result = plan(&["b", "a", "c"], &["A", "B", "C"], ImportMappingMode::Auto);
        assert_eq!(result.entries[0].target.as_deref(), Some("B"));
        assert_eq!(result.entries[0].matched_by, ImportMatchKind::Name);
        assert_eq!(result.entries[1].target.as_deref(), Some("A"));
        assert_eq!(result.entries[2].target.as_deref(), Some("C"));
    }

    #[test]
    fn manual_overrides_win_and_null_means_do_not_import() {
        let mut overrides = BTreeMap::new();
        overrides.insert("a".to_owned(), Some("z".to_owned()));
        overrides.insert("b".to_owned(), None);
        let result = plan_import_mapping(&PlanImportMappingInput {
            source_columns: &names(&["a", "b"]),
            target_columns: &names(&["x", "z"]),
            mode: ImportMappingMode::Auto,
            overrides: &overrides,
        });
        assert_eq!(result.entries[0].target.as_deref(), Some("z"));
        assert_eq!(result.entries[0].matched_by, ImportMatchKind::Manual);
        assert_eq!(result.entries[1].target, None);
        assert_eq!(result.entries[1].matched_by, ImportMatchKind::Manual);
        assert_eq!(result.unmapped, vec!["b"]);
        assert_eq!(result.untargeted, vec!["x"]);
    }

    #[test]
    fn two_sources_claiming_one_target_is_a_conflict_not_a_silent_drop() {
        let result = plan(&["id", "ID"], &["id"], ImportMappingMode::Name);
        assert_eq!(result.conflicts.len(), 1);
        assert_eq!(result.conflicts[0].target, "id");
        assert_eq!(result.conflicts[0].sources, vec!["id", "ID"]);
        assert_eq!(result.entries.len(), 2);
        assert!(!mapping_is_executable(&result));
    }

    #[test]
    fn a_manual_target_shared_with_an_automatic_one_still_conflicts() {
        let mut overrides = BTreeMap::new();
        overrides.insert("a".to_owned(), Some("x".to_owned()));
        let result = plan_import_mapping(&PlanImportMappingInput {
            source_columns: &names(&["a", "x"]),
            target_columns: &names(&["x"]),
            mode: ImportMappingMode::Auto,
            overrides: &overrides,
        });
        assert_eq!(result.conflicts[0].sources, vec!["a", "x"]);
    }

    #[test]
    fn executable_requires_at_least_one_target_and_no_conflicts() {
        assert!(!mapping_is_executable(&plan(&[], &["x"], ImportMappingMode::Auto)));
        assert!(mapping_is_executable(&plan(&["a"], &["x"], ImportMappingMode::Position)));
    }

    #[test]
    fn entries_stay_in_source_order_even_when_matched_out_of_order() {
        let result = plan(&["z", "a"], &["a", "z"], ImportMappingMode::Name);
        assert_eq!(result.entries[0].source, "z");
        assert_eq!(result.entries[1].source, "a");
    }

    #[test]
    fn duplicate_source_names_are_preempted_after_the_first_entry() {
        let result = plan(&["a", "a"], &["x", "y"], ImportMappingMode::Position);
        assert_eq!(result.entries.len(), 1);
        assert_eq!(result.untargeted, vec!["y"]);
    }
}
