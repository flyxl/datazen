mod filter;
mod keywords;

#[cfg(test)]
mod tests;

pub use filter::{
    filter_completions_by_kind, filter_keywords_by_kind, CompletionContextInput, CompletionOption,
    CompletionOutcome, CompletionResult, CompletionSourceFn, COLUMN_CONTEXT_ALLOWED_KEYWORDS,
    TABLE_CONTEXT_ALLOWED_KEYWORDS,
};
pub use keywords::{
    detect_alias_dot_context, extract_qualifier_parts, infer_sql_completion_kind,
    is_table_context, last_bare_sql_keyword, last_bare_sql_keyword_with_offset, SqlCompletionKind,
};

fn apply_keywords(result: CompletionResult, kind: SqlCompletionKind) -> CompletionResult {
    CompletionResult {
        options: filter_keywords_by_kind(&result.options, kind),
        ..result
    }
}

fn apply_schema(result: CompletionResult, kind: SqlCompletionKind) -> CompletionResult {
    CompletionResult {
        options: filter_completions_by_kind(&result.options, kind),
        ..result
    }
}

fn transform(
    raw: CompletionOutcome,
    kind: SqlCompletionKind,
    apply: fn(CompletionResult, SqlCompletionKind) -> CompletionResult,
) -> CompletionOutcome {
    match raw {
        CompletionOutcome::Direct(None) => CompletionOutcome::Direct(None),
        CompletionOutcome::Direct(Some(result)) => CompletionOutcome::Direct(Some(apply(result, kind))),
        CompletionOutcome::Deferred(task) => {
            CompletionOutcome::Deferred(Box::new(move || task().map(|result| apply(result, kind))))
        }
    }
}

pub fn contextual_keyword_completion(source: CompletionSourceFn) -> CompletionSourceFn {
    Box::new(move |context| {
        if detect_alias_dot_context(&context.text_before_cursor).0 != "none" {
            return CompletionOutcome::Direct(None);
        }
        let kind = infer_sql_completion_kind(&context.text_before_cursor);
        transform(source(context), kind, apply_keywords)
    })
}

pub fn contextual_schema_completion(inner: CompletionSourceFn) -> CompletionSourceFn {
    Box::new(move |context| {
        if detect_alias_dot_context(&context.text_before_cursor).0 != "none" {
            return CompletionOutcome::Direct(None);
        }
        let text = context.text_before_cursor.clone();
        let raw = inner(context);
        let kind = infer_sql_completion_kind(&text);
        transform(raw, kind, apply_schema)
    })
}
