//! Port of
//! `packages/frontend/src/windows/schema-diff/schemaDiffObjectIdentity.ts`
//! (T363): the stable identity key of a unified schema-migration object.
//!
//! The React file is a single function, `schemaDiffObjectIdentityKey`: it
//! folds an object identity into `JSON.stringify([kind, schema ?? null,
//! name, signature ?? null, targetSchema ?? null, targetName ?? null])`.
//! The key is what keeps two same-named objects apart — a PostgreSQL
//! routine overload (same schema/name, different `signature`) and a
//! trigger attached to a different relation (same schema/name, different
//! `targetName`) must never collapse into one selection entry.
//!
//! ## What maps where
//!
//! - `SchemaDiffObjectIdentity` — the type alias whose single source is
//!   `@datazen/backend-client`'s `SchemaDiffDatabaseObject` (driver-api's
//!   `DatabaseObject` is the exact wire twin) — becomes
//!   [`SchemaDiffObjectIdentityVm`]. `datazen-ui` does not depend on the
//!   client crate and `datazen-ui-api`'s `DatabaseObject` is the
//!   frontend-narrowed twin (no `signature`/`targetSchema`/`targetName`,
//!   no `view` kind), so the wire shape lands here as a `*Vm` projection
//!   — the deploy panel's pattern; the Rust host converts into it (the
//!   T379 `commands/schemaDiff` seam).
//! - `SchemaDiffObjectKind` (`'view' | DatabaseObjectKind`) becomes
//!   [`SchemaDiffObjectKindVm`], the closed six-literal union carrying
//!   its wire spelling.
//! - `schemaDiffObjectIdentityKey` becomes
//!   [`schema_diff_object_identity_key`]: serde_json's compact array
//!   encoding — the `metadata_session_key` convention — is
//!   byte-for-byte `JSON.stringify` for these primitives (`"`/`\`
//!   escaped, `\b\t\n\f\r` short escapes, other control chars as
//!   lowercase `\u00xx`, non-ASCII raw UTF-8), and `?? null` is
//!   `Option::None` → `null`. In TypeScript an absent optional and an
//!   explicit `null` optional are different inputs that the `?? null`
//!   folds together; Rust's `Option::None` is that one folded value.
//!
//! ## Keys
//!
//! The React file has no `data-testid` values. The derived key itself is
//! the contract: the compact JSON array string, in the field order kind,
//! schema, name, signature, targetSchema, targetName. Its consumers —
//! `useSchemaDiffUnifiedObjects` (selection filter/restore/toggle/
//! select-all), `SchemaDiffUnifiedObjectsPicker` (row keys) and
//! `useSchemaDiffSavedSetups` (parsed-config duplicate rejection) — are
//! later tasks of this window's track and must keep calling this one
//! function, never a re-derived format.

// The serde derives ride the clipboard-config wire (T377's
// SchemaDiffConfigJson carries identity lists).
use serde::{Deserialize, Serialize};

/// React `SchemaDiffObjectKind` = `'view' | DatabaseObjectKind` — the
/// object categories a unified plan can address beyond tables (tables
/// themselves travel as the plan's `tables` string list, not identities).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(dead_code)] // reusable port; wired by the schema-diff wizard track
pub enum SchemaDiffObjectKindVm {
    View,
    Function,
    Procedure,
    Trigger,
    Sequence,
    Type,
}

impl serde::Serialize for SchemaDiffObjectKindVm {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.wire_str())
    }
}

impl<'de> serde::Deserialize<'de> for SchemaDiffObjectKindVm {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        Self::from_wire_str(&value)
            .ok_or_else(|| serde::de::Error::custom(format!("unknown schema-diff object kind: {value}")))
    }
}

#[allow(dead_code)] // reusable port; wired by the schema-diff wizard track
impl SchemaDiffObjectKindVm {
    /// The wire spelling (the `as_str` twin of the ui-api kind enums).
    pub fn wire_str(self) -> &'static str {
        match self {
            Self::View => "view",
            Self::Function => "function",
            Self::Procedure => "procedure",
            Self::Trigger => "trigger",
            Self::Sequence => "sequence",
            Self::Type => "type",
        }
    }

    /// The wire spelling's parse twin (serde + host conversion use it).
    pub fn from_wire_str(value: &str) -> Option<Self> {
        match value {
            "view" => Some(Self::View),
            "function" => Some(Self::Function),
            "procedure" => Some(Self::Procedure),
            "trigger" => Some(Self::Trigger),
            "sequence" => Some(Self::Sequence),
            "type" => Some(Self::Type),
            _ => None,
        }
    }
}

/// React `SchemaDiffObjectIdentity` (= backend-client's
/// `SchemaDiffDatabaseObject`), the six fields the key reads. The
/// optionals are React's `?: string | null`; absent and explicit `null`
/// are the same value after the key's `?? null` normalization.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[allow(dead_code)] // reusable port; wired by the schema-diff wizard track
pub struct SchemaDiffObjectIdentityVm {
    pub kind: SchemaDiffObjectKindVm,
    /// Null when the engine has no schema level.
    pub schema: Option<String>,
    pub name: String,
    /// PostgreSQL routine identity arguments — disambiguates overloaded
    /// functions/procedures without putting SQL syntax into `name`.
    pub signature: Option<String>,
    /// Schema of the relation a trigger is attached to, when the driver
    /// exposes it.
    pub target_schema: Option<String>,
    /// Name of the relation a trigger is attached to, when the driver
    /// exposes it.
    pub target_name: Option<String>,
}

/// React `schemaDiffObjectIdentityKey` (`schemaDiffObjectIdentity.ts:4`):
/// `JSON.stringify([kind, schema ?? null, name, signature ?? null,
/// targetSchema ?? null, targetName ?? null])`.
///
/// serde_json's compact array output matches `JSON.stringify` byte for
/// byte on these primitives; `unwrap_or_default` is the inert branch — a
/// `json!` array of strings and nulls always serializes.
#[allow(dead_code)] // reusable port; wired by the schema-diff wizard track
pub fn schema_diff_object_identity_key(object: &SchemaDiffObjectIdentityVm) -> String {
    serde_json::to_string(&serde_json::json!([
        object.kind.wire_str(),
        object.schema.as_deref(),
        object.name.as_str(),
        object.signature.as_deref(),
        object.target_schema.as_deref(),
        object.target_name.as_deref(),
    ]))
    .unwrap_or_default()
}

#[doc(hidden)]  // exposed for integration tests
pub mod tests {
    use super::*;

    fn view(name: &str) -> SchemaDiffObjectIdentityVm {
        SchemaDiffObjectIdentityVm {
            kind: SchemaDiffObjectKindVm::View,
            schema: None,
            name: name.to_owned(),
            signature: None,
            target_schema: None,
            target_name: None,
        }
    }

    fn function(schema: &str, name: &str, signature: &str) -> SchemaDiffObjectIdentityVm {
        SchemaDiffObjectIdentityVm {
            kind: SchemaDiffObjectKindVm::Function,
            schema: Some(schema.to_owned()),
            name: name.to_owned(),
            signature: Some(signature.to_owned()),
            target_schema: None,
            target_name: None,
        }
    }

    fn trigger(target_name: &str) -> SchemaDiffObjectIdentityVm {
        SchemaDiffObjectIdentityVm {
            kind: SchemaDiffObjectKindVm::Trigger,
            schema: Some("public".to_owned()),
            name: "audit_row".to_owned(),
            signature: None,
            target_schema: Some("public".to_owned()),
            target_name: Some(target_name.to_owned()),
        }
    }

    /// React test 1: overloaded routines (same name, different signature)
    /// and relation-bound triggers (same name, different target relation)
    /// stay distinct — the reason the key exists.
    #[test]
    fn overloads_and_relation_bound_triggers_stay_distinct() {
        let integer_routine = function("public", "lookup_order", "integer");
        let text_routine = function("public", "lookup_order", "text");
        assert_ne!(
            schema_diff_object_identity_key(&integer_routine),
            schema_diff_object_identity_key(&text_routine)
        );
        let orders_trigger = trigger("orders");
        let users_trigger = trigger("users");
        assert_ne!(
            schema_diff_object_identity_key(&orders_trigger),
            schema_diff_object_identity_key(&users_trigger)
        );
    }

    /// React test 2: absent optional identity fields key the same as
    /// explicit `null` fields. TypeScript needs the `?? null` fold for
    /// that; `Option::None` already is the folded value — and the exact
    /// wire form is pinned so the fold stays observable.
    #[test]
    fn absent_optionals_normalize_like_nulls() {
        assert_eq!(
            schema_diff_object_identity_key(&view("orders_view")),
            r#"["view",null,"orders_view",null,null,null]"#
        );
    }

    /// `?? null` normalizes only null/undefined: an empty-string optional
    /// is a real value and must not collapse onto `null`.
    #[test]
    fn empty_string_optionals_stay_distinct_from_null() {
        let empty_schema = SchemaDiffObjectIdentityVm {
            schema: Some(String::new()),
            ..view("orders_view")
        };
        assert_eq!(
            schema_diff_object_identity_key(&empty_schema),
            r#"["view","","orders_view",null,null,null]"#
        );
        assert_ne!(
            schema_diff_object_identity_key(&empty_schema),
            schema_diff_object_identity_key(&view("orders_view"))
        );
    }

    /// The key is `JSON.stringify` byte for byte: compact separators,
    /// double-quoted strings, `null` for the optionals.
    #[test]
    fn key_format_is_json_stringify() {
        assert_eq!(
            schema_diff_object_identity_key(&function("public", "lookup_order", "integer")),
            r#"["function","public","lookup_order","integer",null,null]"#
        );
        assert_eq!(
            schema_diff_object_identity_key(&trigger("orders")),
            r#"["trigger","public","audit_row",null,"public","orders"]"#
        );
    }

    /// Every identity field participates: changing any one of the six
    /// slots changes the key, and the six single-field mutations stay
    /// pairwise distinct (field order is part of the contract).
    #[test]
    fn every_field_participates() {
        let base = function("public", "lookup_order", "integer");
        let variants = [
            SchemaDiffObjectIdentityVm {
                kind: SchemaDiffObjectKindVm::Procedure,
                ..base.clone()
            },
            SchemaDiffObjectIdentityVm {
                schema: Some("sales".to_owned()),
                ..base.clone()
            },
            SchemaDiffObjectIdentityVm {
                name: "lookup_customer".to_owned(),
                ..base.clone()
            },
            SchemaDiffObjectIdentityVm {
                signature: Some("text".to_owned()),
                ..base.clone()
            },
            SchemaDiffObjectIdentityVm {
                target_schema: Some("public".to_owned()),
                ..base.clone()
            },
            SchemaDiffObjectIdentityVm {
                target_name: Some("orders".to_owned()),
                ..base
            },
        ];
        let base_key = schema_diff_object_identity_key(&SchemaDiffObjectIdentityVm {
            kind: SchemaDiffObjectKindVm::Function,
            schema: Some("public".to_owned()),
            name: "lookup_order".to_owned(),
            signature: Some("integer".to_owned()),
            target_schema: None,
            target_name: None,
        });
        let keys: std::collections::HashSet<String> = variants
            .iter()
            .map(schema_diff_object_identity_key)
            .collect();
        assert!(!keys.contains(&base_key));
        assert_eq!(keys.len(), variants.len());
    }

    /// The closed union's six kinds wire-spell to the React literals and
    /// never collide for one name.
    #[test]
    fn the_six_kinds_are_distinct() {
        let kinds = [
            (SchemaDiffObjectKindVm::View, "view"),
            (SchemaDiffObjectKindVm::Function, "function"),
            (SchemaDiffObjectKindVm::Procedure, "procedure"),
            (SchemaDiffObjectKindVm::Trigger, "trigger"),
            (SchemaDiffObjectKindVm::Sequence, "sequence"),
            (SchemaDiffObjectKindVm::Type, "type"),
        ];
        let mut keys = std::collections::HashSet::new();
        for (kind, wire) in kinds {
            assert_eq!(kind.wire_str(), wire);
            keys.insert(schema_diff_object_identity_key(&SchemaDiffObjectIdentityVm {
                kind,
                schema: None,
                name: "thing".to_owned(),
                signature: None,
                target_schema: None,
                target_name: None,
            }));
        }
        assert_eq!(keys.len(), 6);
    }

    /// JSON escaping matches `JSON.stringify` exactly: quotes and
    /// backslashes escape, the five control short-escapes apply, other
    /// control chars become lowercase `\u00xx`, non-ASCII stays raw
    /// UTF-8 — and escaping is injective, so names differing only in
    /// escapable characters never share a key.
    #[test]
    fn json_escaping_matches_json_stringify() {
        let key_of = |name: &str| schema_diff_object_identity_key(&view(name));
        assert_eq!(key_of("a\"b\\c"), r#"["view",null,"a\"b\\c",null,null,null]"#);
        assert_eq!(key_of("row\t1\n"), r#"["view",null,"row\t1\n",null,null,null]"#);
        assert_eq!(key_of("a\u{1}b"), r#"["view",null,"a\u0001b",null,null,null]"#);
        assert_eq!(key_of("订单视图"), r#"["view",null,"订单视图",null,null,null]"#);
        assert_ne!(key_of("weird\"name"), key_of("weird\\name"));
        assert_ne!(key_of("weird\"name"), key_of("weird\\\"name"));
    }
}
