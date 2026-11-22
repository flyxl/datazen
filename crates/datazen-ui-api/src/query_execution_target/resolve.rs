use super::{effective_strategy, SqlExecutionStrategy};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StatementRange {
    pub from: usize,
    pub to: usize,
    pub content_from: usize,
    pub content_to: usize,
    pub index: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StatementTargetInfo {
    pub sql: String,
    pub from: usize,
    pub to: usize,
    pub from_line: u32,
    pub to_line: u32,
    pub index: u32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ResolveExecutionTargetResult {
    pub sql: String,
    pub strategy_used: &'static str,
    pub needs_ask: bool,
    pub statement_count: usize,
    pub current_statement: Option<StatementTargetInfo>,
    pub all_statements: Vec<StatementTargetInfo>,
    pub entire_script: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ExecutionTargetInput {
    pub doc: String,
    pub cursor_offset: usize,
    pub selection: String,
    pub strategy: SqlExecutionStrategy,
}

pub fn offset_to_line(source: &str, offset: usize) -> u32 {
    if offset == 0 {
        return 1;
    }
    let bounded = offset.min(source.chars().count());
    1 + source
        .chars()
        .take(bounded)
        .filter(|ch| *ch == '\n')
        .count() as u32
}

pub fn statement_target_info(source: &str, range: &StatementRange) -> StatementTargetInfo {
    let chars: Vec<char> = source.chars().collect();
    let from = range.from.min(chars.len());
    let to = range.to.min(chars.len()).max(from);
    StatementTargetInfo {
        sql: chars[from..to].iter().collect::<String>().trim().to_owned(),
        from: range.from,
        to: range.to,
        from_line: offset_to_line(source, range.content_from),
        to_line: offset_to_line(source, range.content_to),
        index: range.index,
    }
}

pub fn resolve_execution_target<B, V, F>(
    input: &ExecutionTargetInput,
    build_ranges: B,
    get_valid: V,
    find_at_cursor: F,
) -> ResolveExecutionTargetResult
where
    B: Fn(&str) -> Vec<StatementRange>,
    V: Fn(Vec<StatementRange>) -> Vec<StatementRange>,
    F: Fn(&str, usize, &[StatementRange]) -> Option<StatementRange>,
{
    let trimmed_selection = input.selection.trim();
    let entire_script = input.doc.trim().to_owned();
    let all_ranges = build_ranges(&input.doc);
    let valid_ranges = get_valid(all_ranges);
    let all_statements = valid_ranges
        .iter()
        .map(|range| statement_target_info(&input.doc, range))
        .collect();

    if !trimmed_selection.is_empty() {
        return ResolveExecutionTargetResult {
            sql: trimmed_selection.to_owned(),
            strategy_used: "selection",
            needs_ask: false,
            statement_count: 1,
            current_statement: None,
            all_statements,
            entire_script,
        };
    }

    if valid_ranges.is_empty() {
        return ResolveExecutionTargetResult {
            sql: entire_script.clone(),
            strategy_used: input.strategy.as_str(),
            needs_ask: false,
            statement_count: 0,
            current_statement: None,
            all_statements,
            entire_script,
        };
    }

    let current_range = find_at_cursor(&input.doc, input.cursor_offset, &valid_ranges)
        .or_else(|| valid_ranges.first().copied());
    let current_statement = current_range
        .as_ref()
        .map(|range| statement_target_info(&input.doc, range));

    let base = |sql: String,
                strategy_used: &'static str,
                needs_ask: bool|
     -> ResolveExecutionTargetResult {
        ResolveExecutionTargetResult {
            sql,
            strategy_used,
            needs_ask,
            statement_count: valid_ranges.len(),
            current_statement: current_statement.clone(),
            all_statements: all_statements.clone(),
            entire_script: entire_script.clone(),
        }
    };

    match input.strategy {
        SqlExecutionStrategy::EntireScript => base(entire_script.clone(), "entire_script", false),
        SqlExecutionStrategy::Ask => {
            let sql = current_statement
                .as_ref()
                .map(|statement| statement.sql.clone())
                .unwrap_or_else(|| entire_script.clone());
            base(sql, "ask", valid_ranges.len() > 1)
        }
        SqlExecutionStrategy::LargestStatement => {
            let current = match current_range {
                Some(range) => range,
                None => return base(entire_script.clone(), "largest_statement", false),
            };
            let directly_inside =
                input.cursor_offset >= current.from && input.cursor_offset <= current.to;
            if directly_inside {
                let sql = current_statement
                    .as_ref()
                    .filter(|statement| !statement.sql.is_empty())
                    .map(|statement| statement.sql.clone())
                    .unwrap_or_else(|| entire_script.clone());
                return base(sql, "largest_statement", false);
            }
            let largest = valid_ranges.iter().copied().fold(current, |best, range| {
                if (range.content_to - range.content_from) > (best.content_to - best.content_from) {
                    range
                } else {
                    best
                }
            });
            let largest_statement = statement_target_info(&input.doc, &largest);
            ResolveExecutionTargetResult {
                sql: largest_statement.sql.clone(),
                strategy_used: "largest_statement",
                needs_ask: false,
                statement_count: valid_ranges.len(),
                current_statement: Some(largest_statement),
                all_statements,
                entire_script,
            }
        }
        SqlExecutionStrategy::CurrentStatement => {
            let sql = current_statement
                .as_ref()
                .map(|statement| statement.sql.clone())
                .unwrap_or_else(|| entire_script.clone());
            base(sql, effective_strategy(false, input.strategy), false)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ranges() -> Vec<StatementRange> {
        vec![
            StatementRange {
                from: 0,
                to: 8,
                content_from: 0,
                content_to: 7,
                index: 0,
            },
            StatementRange {
                from: 9,
                to: 22,
                content_from: 9,
                content_to: 21,
                index: 1,
            },
        ]
    }

    fn input() -> ExecutionTargetInput {
        ExecutionTargetInput {
            doc: "select 1;\nselect 22;".to_owned(),
            cursor_offset: 12,
            selection: String::new(),
            strategy: SqlExecutionStrategy::CurrentStatement,
        }
    }

    fn resolve(item: &ExecutionTargetInput) -> ResolveExecutionTargetResult {
        resolve_execution_target(
            item,
            |_| ranges(),
            |ranges| ranges,
            |_, _, ranges| {
                ranges
                    .iter()
                    .find(|range| range.from <= 12 && 12 <= range.to)
                    .copied()
            },
        )
    }

    #[test]
    fn line_numbers_bounded_and_newline_aware() {
        assert_eq!(offset_to_line("abc", 0), 1);
        assert_eq!(offset_to_line("a\nb", 1), 1);
        assert_eq!(offset_to_line("a\nb", 2), 2);
        assert_eq!(offset_to_line("a\nb", 99), 2);
    }

    #[test]
    fn statement_info_trims_sql_and_uses_content_lines() {
        let info = statement_target_info("select 1;\nselect 22;", &ranges()[1]);
        assert_eq!(info.sql, "select 22;");
        assert_eq!(info.index, 1);
        assert_eq!(info.to_line, 2);
    }

    #[test]
    fn selection_has_absolute_precedence() {
        let mut item = input();
        item.selection = "  delete from t  ".to_owned();
        let result = resolve(&item);
        assert_eq!(result.sql, "delete from t");
        assert_eq!(result.strategy_used, "selection");
        assert_eq!(result.statement_count, 1);
        assert!(!result.needs_ask);
    }

    #[test]
    fn empty_valid_ranges_fall_back_to_the_trimmed_script() {
        let result =
            resolve_execution_target(&input(), |_| ranges(), |_| Vec::new(), |_, _, _| None);
        assert_eq!(result.sql, "select 1;\nselect 22;");
        assert_eq!(result.statement_count, 0);
        assert!(result.current_statement.is_none());
    }

    #[test]
    fn strategies_pick_current_entire_or_ask() {
        let current = resolve(&input());
        assert_eq!(current.sql, "select 22;");
        assert_eq!(current.strategy_used, "current_statement");
        assert_eq!(current.all_statements.len(), 2);

        let mut item = input();
        item.strategy = SqlExecutionStrategy::EntireScript;
        assert_eq!(resolve(&item).sql, "select 1;\nselect 22;");

        item.strategy = SqlExecutionStrategy::Ask;
        let asked = resolve(&item);
        assert!(asked.needs_ask);
        assert_eq!(asked.strategy_used, "ask");
    }

    #[test]
    fn largest_statement_uses_current_when_inside_else_biggest_content() {
        let mut item = input();
        item.cursor_offset = 12;
        item.strategy = SqlExecutionStrategy::LargestStatement;
        assert_eq!(resolve(&item).sql, "select 22;");

        let outside =
            resolve_execution_target(&item, |_| ranges(), |ranges| ranges, |_, _, _| None);
        assert_eq!(outside.sql, "select 22;");
    }
}
