//! Insert the authored built-ins into the active Native Query draft.
//!
//! This is the narrow action shared with React: insert resolved defaults and
//! select the first numbered field. It is not a CodeMirror tabstop session or
//! a parser for user storage; plain saved SQL keeps its existing path.

use std::ops::Range;

use super::SqlEditor;

fn append(out: &mut String, s: &str, indent: &str) {
    for ch in s.chars() {
        out.push(ch);
        if ch == '\n' {
            out.push_str(indent);
        }
    }
}

fn resolve_authored_template(mut source: &str, indent: &str) -> Option<(String, Range<usize>)> {
    let mut text = String::new();
    let mut first: Option<(u32, Range<usize>)> = None;
    let mut last = 0;
    while let Some((literal, marked)) = source.split_once("${") {
        append(&mut text, literal, indent);
        let (field, rest) = marked.split_once('}')?;
        let Some((number, default)) = field.split_once(':') else {
            let terminal = field.parse::<u32>().ok()?;
            let (_, initial) = first?;
            return (rest.is_empty() && terminal > last).then_some((text, initial));
        };
        let number = number.parse::<u32>().ok()?;
        if number == 0 || default.is_empty() || default.contains(['$', '{', '}', '\n']) {
            return None;
        }
        let start = text.len();
        text.push_str(default);
        if first.as_ref().is_none_or(|(low, _)| number < *low) {
            first = Some((number, start..text.len()));
        }
        last = last.max(number);
        source = rest;
    }
    None // The seven authored templates always end at a terminal field.
}

pub(crate) fn insert(draft: &mut SqlEditor, template: &str) -> bool {
    if draft.composition().is_some() {
        return false;
    }
    let sel = draft.selection().main();
    let from = sel.from();
    if draft.doc().get(from..sel.to()).is_none() {
        return false;
    }
    let Some(prefix) = draft.doc().get(..from) else { return false; };
    let line_start = prefix.rfind('\n').map_or(0, |i| i + 1);
    let line = draft.doc()[line_start..].split('\n').next().unwrap_or_default();
    let indent: String = line.chars().take_while(|ch| matches!(ch, ' ' | '\t')).collect();
    let Some((text, first)) = resolve_authored_template(template, &indent) else { return false; };

    // A single host-model edit, never a query job or settings write. Capture
    // the old document *and* ranges before choosing the first numbered field.
    draft.push_history();
    draft.replace_range(from, sel.to(), &text);
    draft.set_main_selection(from + first.start, from + first.end);
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::sql_editor::snippets::builtin_sql_snippets;

    #[test]
    fn builtin_defaults_and_initial_ranges_are_the_exact_authored_templates() {
        let expected = [
            ("select-all","SELECT *\nFROM table_name\nWHERE condition;","table_name"),
            ("select-columns","SELECT columns\nFROM table_name;","table_name"),
            ("insert","INSERT INTO table_name (columns)\nVALUES (values);","table_name"),
            ("update","UPDATE table_name\nSET column = value\nWHERE condition;","table_name"),
            ("delete","DELETE FROM table_name\nWHERE condition;","table_name"),
            ("join","JOIN table_name ON table_name.id = other_table.fk_id","table_name"),
            ("count","SELECT COUNT(1) AS total\nFROM table_name;","table_name"),
        ];
        let authored = builtin_sql_snippets();
        assert_eq!(authored.len(), expected.len());
        for (item,(id,sql,field)) in authored.iter().zip(expected) {
            let (got, first) = resolve_authored_template(&item.template, "").expect(id);
            assert_eq!((item.id.as_str(),got.as_str(),&got[first]), (id,sql,field));
        }
        let (indented, first) = resolve_authored_template(&authored[1].template, "  ").unwrap();
        assert_eq!(indented, "SELECT columns\n  FROM table_name;");
        assert_eq!(first.start, indented.find("table_name").unwrap());
    }
}
