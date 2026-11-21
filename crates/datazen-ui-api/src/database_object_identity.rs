#[cfg(test)]
mod tests;

use crate::DatabaseObject;
use serde_json::json;

pub fn database_object_identity_key(object: &DatabaseObject) -> String {
    let payload = json!([
        object.kind.as_str(),
        &object.schema,
        &object.name,
        &object.signature,
        &object.target_schema,
        &object.target_name,
    ]);
    serde_json::to_string(&payload).unwrap_or_default()
}
