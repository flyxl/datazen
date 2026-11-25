//! Bounded native views and guarded edits for aggregate values.
use datazen_driver_api::{ConnectionHandle, DatabaseDriver};
use serde_json::{json, Value};

use super::{call, clip, editable, KvCollectionEdit};

type CollectionEdits = Vec<(String, KvCollectionEdit, String)>;

pub(super) async fn preview(
    driver: &dyn DatabaseDriver,
    handle: &ConnectionHandle,
    db: u32,
    key: &str,
    kind: &str,
) -> Result<(String, CollectionEdits), String> {
    let (command, args) = match kind {
        "list" => (
            "list_range",
            json!({"dbIndex":db,"key":key,"start":0,"stop":15}),
        ),
        "set" => (
            "set_scan",
            json!({"dbIndex":db,"key":key,"cursor":0,"count":16}),
        ),
        "zset" => (
            "zset_scan",
            json!({"dbIndex":db,"key":key,"cursor":0,"count":16}),
        ),
        "stream" => (
            "xrange",
            json!({"dbIndex":db,"key":key,"start":"-","end":"+","count":16}),
        ),
        "ReJSON-RL" => (
            "json_get",
            json!({"dbIndex":db,"key":key,"path":"$","raw":true}),
        ),
        _ => return Err("Unexpected Redis value type.".into()),
    };
    let mut data = call(driver, handle, command, args).await?;
    let mut edits = Vec::new();
    match kind {
        "list" => {
            if let Some(items) = data["items"].as_array() {
                for (i, item) in items.iter().take(16).enumerate() {
                    if let Some(value) = item.as_str().filter(|v| editable(v)) {
                        edits.push((
                            format!("Edit index {i}"),
                            KvCollectionEdit::ListIndex(i),
                            value.into(),
                        ));
                    }
                }
            }
            edits.push((
                "Append list item".into(),
                KvCollectionEdit::ListAppend,
                String::new(),
            ));
            if let Some(first) = data["items"]
                .get(0)
                .and_then(Value::as_str)
                .filter(|v| editable(v))
            {
                edits.push((
                    format!("Remove list head {} (type REMOVE)", clip(first)),
                    KvCollectionEdit::ListRemoveHead(first.into()),
                    String::new(),
                ));
            }
        }
        "set" => {
            edits.push(("Add member".into(), KvCollectionEdit::SetAdd, String::new()));
            for member in data["members"].as_array().into_iter().flatten().take(16) {
                if let Some(member) = member.as_str().filter(|v| editable(v)) {
                    edits.push((
                        format!("Remove Set member {} (type REMOVE)", clip(member)),
                        KvCollectionEdit::SetRemove(member.into()),
                        String::new(),
                    ));
                }
            }
        }
        "zset" => {
            if let Some(items) = data["members"].as_array() {
                for item in items.iter().take(16) {
                    if let (Some(member), Some(score)) =
                        (item["member"].as_str(), item["score"].as_f64())
                    {
                        if editable(member) && score.is_finite() {
                            edits.push((
                                format!("Edit score: {}", clip(member)),
                                KvCollectionEdit::SortedSetScore(member.into()),
                                score.to_string(),
                            ));
                        }
                    }
                }
            }
            for item in data["members"].as_array().into_iter().flatten().take(16) {
                if let (Some(member), Some(score)) =
                    (item["member"].as_str(), item["score"].as_f64())
                {
                    if editable(member) && score.is_finite() {
                        edits.push((
                            format!("Remove ZSet member {} (type REMOVE)", clip(member)),
                            KvCollectionEdit::SortedSetRemove {
                                member: member.into(),
                                score: score.to_string(),
                            },
                            String::new(),
                        ));
                    }
                }
            }
            edits.push((
                "Add member as member=score".into(),
                KvCollectionEdit::SortedSetAdd,
                String::new(),
            ));
        }
        "stream" => edits.push((
            "Append field=value".into(),
            KvCollectionEdit::StreamAppend,
            String::new(),
        )),
        "ReJSON-RL" => {
            if let Some(value) = data["rawText"].as_str().filter(|v| editable(v)) {
                edits.push((
                    "Edit JSON document".into(),
                    KvCollectionEdit::JsonRoot,
                    value.into(),
                ));
            }
            data = data["value"].take();
        }
        _ => {}
    }
    Ok((
        clip(&serde_json::to_string_pretty(&data).unwrap_or_default()),
        edits,
    ))
}

pub(super) fn edit_args(
    db: u32,
    key: &str,
    target: &KvCollectionEdit,
    expected: &str,
    value: &str,
) -> Result<(&'static str, Value), String> {
    Ok(match target {
        KvCollectionEdit::ListIndex(index) if *index < 16 => (
            "list_set",
            json!({"dbIndex":db,"key":key,"index":index,"expectedValue":expected,"value":value}),
        ),
        KvCollectionEdit::HashRemove { field, previous }
            if value == "REMOVE" && editable(field) && editable(previous) =>
        {
            (
                "hash_del",
                json!({"dbIndex":db,"key":key,"fields":[field],"guardedField":field,"expectedValue":previous}),
            )
        }
        KvCollectionEdit::ListRemoveHead(previous) if value == "REMOVE" && editable(previous) => (
            "list_pop",
            json!({"dbIndex":db,"key":key,"side":"left","expectedHead":previous}),
        ),
        KvCollectionEdit::SetRemove(member) if value == "REMOVE" && editable(member) => (
            "set_remove",
            json!({"dbIndex":db,"key":key,"members":[member],"guardedMember":member}),
        ),
        KvCollectionEdit::SortedSetRemove { member, score }
            if value == "REMOVE"
                && editable(member)
                && score.parse::<f64>().is_ok_and(f64::is_finite) =>
        {
            (
                "zset_remove",
                json!({"dbIndex":db,"key":key,"members":[member],"guardedMember":member,"expectedScore":score}),
            )
        }
        KvCollectionEdit::HashRemove { .. }
        | KvCollectionEdit::ListRemoveHead(_)
        | KvCollectionEdit::SetRemove(_)
        | KvCollectionEdit::SortedSetRemove { .. } => {
            return Err("Type REMOVE to confirm this entry.".into())
        }
        KvCollectionEdit::ListAppend => (
            "list_push",
            json!({"dbIndex":db,"key":key,"appendExistingOnly":true,"appendValue":value,"side":"right","values":[value]}),
        ),
        KvCollectionEdit::HashAdd => {
            let (field, value) = value
                .split_once('=')
                .filter(|(f, _)| !f.is_empty())
                .ok_or("Enter a Hash field as field=value.")?;
            (
                "hash_set",
                json!({"dbIndex":db,"key":key,"createFieldOnly":true,"field":field,"value":value}),
            )
        }
        KvCollectionEdit::SortedSetAdd => {
            let (member, score) = value
                .split_once('=')
                .filter(|(member, _)| !member.is_empty())
                .ok_or("Enter a sorted-set entry as member=score.")?;
            let score = score
                .trim()
                .parse::<f64>()
                .ok()
                .filter(|score| score.is_finite())
                .ok_or("Enter a finite sorted-set score.")?;
            (
                "zset_add",
                json!({"dbIndex":db,"key":key,"createMemberOnly":true,"existingMember":member,
                    "newScore":score.to_string(),"members":[{"member":member,"score":score}]}),
            )
        }
        KvCollectionEdit::SetAdd => (
            "set_add",
            json!({"dbIndex":db,"key":key,"members":[value],"existingMember":value}),
        ),
        KvCollectionEdit::SortedSetScore(member) if editable(member) => {
            let score = value
                .trim()
                .parse::<f64>()
                .ok()
                .filter(|v| v.is_finite())
                .ok_or("Enter a finite sorted-set score.")?;
            (
                "zset_add",
                json!({"dbIndex":db,"key":key,"members":[{"member":member,"score":score}],
                "expectedScore":expected,"existingMember":member,"newScore":score.to_string()}),
            )
        }
        KvCollectionEdit::StreamAppend => {
            let (field, value) = value
                .split_once('=')
                .filter(|(f, _)| !f.is_empty())
                .ok_or("Enter a stream entry as field=value.")?;
            (
                "xadd",
                json!({"dbIndex":db,"key":key,"fields":{(field):value},"existingField":field,"fieldValue":value}),
            )
        }
        KvCollectionEdit::JsonRoot => {
            let _: Value =
                serde_json::from_str(value).map_err(|_| "Enter a valid JSON document.")?;
            (
                "json_set",
                json!({"dbIndex":db,"key":key,"path":"$","expectedDocument":expected,"value":value}),
            )
        }
        _ => return Err("Invalid existing Redis collection value.".into()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_collection_entries_require_exact_fields_and_finite_scores() {
        for value in ["", "member", "=2", "member=nan", "member=inf"] {
            assert!(edit_args(0, "z", &KvCollectionEdit::SortedSetAdd, "", value).is_err());
        }
        let (cmd, args) =
            edit_args(2, "z", &KvCollectionEdit::SortedSetAdd, "", "member=1.5").unwrap();
        assert_eq!(cmd, "zset_add");
        assert_eq!(args["createMemberOnly"], true);
        assert_eq!(args["dbIndex"], 2);
        assert!(edit_args(0, "h", &KvCollectionEdit::HashAdd, "", "=value").is_err());
    }
}
